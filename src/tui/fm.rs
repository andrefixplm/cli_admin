//! Estado e teclas da aba Arquivos.
//! **Nenhuma E/S de disco roda na thread da interface**: listar, planejar, copiar e criar pasta
//! rodam em threads de fundo e são consultados por `poll()` a cada tick de 150 ms.

use crate::audit;
use crate::modules::fm::{self, FmEntry, FmSort, PlanoCopia, Progresso, ResultadoCopia, executar, nome_valido, planejar};
use crate::modules::support::{bytes_to_human, volume_info};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::widgets::TableState;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::Instant;

/// Folga exigida além dos bytes do plano ao conferir o espaço livre do destino.
const MARGEM_ESPACO: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FmModo {
    Normal,
    Filtro,
    Caminho,
    NovaPasta,
}

/// Roda `f` em outra thread e devolve o canal do resultado.
pub(crate) fn em_segundo_plano<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> mpsc::Receiver<T> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx
}

struct Carga {
    alvo: PathBuf,
    rx: mpsc::Receiver<Result<Vec<FmEntry>, String>>,
}

/// Plano pedido, à espera de ser calculado em segundo plano.
pub struct Confirmacao {
    pub plano: PlanoCopia,
    pub destino: PathBuf,
}

struct Planejando {
    rx: mpsc::Receiver<(Result<PlanoCopia, String>, Option<u64>)>,
    destino: PathBuf,
}

struct CopiaAtiva {
    prog: Arc<Progresso>,
    rx: mpsc::Receiver<ResultadoCopia>,
    total_itens: usize,
    total_bytes: u64,
    destino: PathBuf,
    inicio: Instant,
}

struct CriandoPasta {
    rx: mpsc::Receiver<Result<PathBuf, String>>,
}

/// Fotografia do progresso para a tela.
pub struct ProgressoVista {
    pub itens: usize,
    pub total_itens: usize,
    pub bytes: u64,
    pub total_bytes: u64,
    pub segundos: u64,
}

pub struct FmState {
    pub dir: PathBuf,
    pub em_unidades: bool,
    todas: Vec<FmEntry>,
    pub visiveis: Vec<FmEntry>,
    pub cursor: usize,
    pub tabela: TableState,
    pub marcados: HashSet<PathBuf>,
    /// Itens guardados com `y`, à espera do `p`.
    pub area: Vec<PathBuf>,
    pub ordem: FmSort,
    pub mostrar_ocultos: bool,
    pub filtro: String,
    pub modo: FmModo,
    /// Texto do campo ativo (caminho ou nome da nova pasta).
    pub entrada: String,
    pub erro: Option<String>,
    /// Mensagem curta para a barra de status do programa (a TUI a consome).
    pub mensagem: Option<String>,
    /// Modal de confirmação da cópia, enquanto aberto.
    pub confirmacao: Option<Confirmacao>,
    /// Erros e itens que ficaram de fora da última cópia.
    pub relatorio: Vec<String>,
    pub(crate) auditoria: Option<PathBuf>,
    // Substituir `carga` descarta o receptor anterior: resposta antiga nunca vale.
    carga: Option<Carga>,
    planejando: Option<Planejando>,
    copia: Option<CopiaAtiva>,
    criando: Option<CriandoPasta>,
}

impl FmState {
    pub fn novo(inicio: PathBuf, auditoria: Option<PathBuf>) -> Self {
        let mut s = FmState {
            dir: inicio.clone(),
            em_unidades: false,
            todas: Vec::new(),
            visiveis: Vec::new(),
            cursor: 0,
            tabela: TableState::default(),
            marcados: HashSet::new(),
            area: Vec::new(),
            ordem: FmSort::Nome,
            mostrar_ocultos: false,
            filtro: String::new(),
            modo: FmModo::Normal,
            entrada: String::new(),
            erro: None,
            mensagem: None,
            confirmacao: None,
            relatorio: Vec::new(),
            auditoria,
            carga: None,
            planejando: None,
            copia: None,
            criando: None,
        };
        s.abrir(inicio);
        s
    }

    /// Pede a listagem de `alvo` em segundo plano. Só troca de pasta se a leitura der certo.
    pub fn abrir(&mut self, alvo: PathBuf) {
        let caminho = alvo.clone();
        let rx = em_segundo_plano(move || fm::listar(&caminho).map_err(|e| e.to_string()));
        self.carga = Some(Carga { alvo, rx });
    }

    pub fn carregando(&self) -> bool {
        self.carga.is_some()
    }

    /// Progresso da cópia em andamento, ou `None` se não há cópia.
    pub fn progresso(&self) -> Option<ProgressoVista> {
        self.copia.as_ref().map(|c| ProgressoVista {
            itens: c.prog.itens.load(Ordering::Relaxed),
            total_itens: c.total_itens,
            bytes: c.prog.bytes.load(Ordering::Relaxed),
            total_bytes: c.total_bytes,
            segundos: c.inicio.elapsed().as_secs(),
        })
    }

    fn reaplicar(&mut self) {
        fm::ordenar(&mut self.todas, self.ordem);
        self.visiveis = fm::visiveis(&self.todas, self.mostrar_ocultos, &self.filtro);
        self.cursor = self.cursor.min(self.visiveis.len().saturating_sub(1));
        self.tabela.select(if self.visiveis.is_empty() { None } else { Some(self.cursor) });
    }

    fn mostrar_unidades(&mut self) {
        self.em_unidades = true;
        self.todas = fm::unidades()
            .into_iter()
            .map(|u| FmEntry {
                nome: u.clone(),
                caminho: PathBuf::from(&u),
                eh_pasta: true,
                eh_link: false,
                bytes: 0,
                modificado: None,
                oculto: false,
            })
            .collect();
        self.marcados.clear();
        self.filtro.clear();
        self.cursor = 0;
        self.reaplicar();
    }

    /// Consulta o que está em segundo plano. Chamado a cada tick da interface.
    pub fn poll(&mut self) {
        let resposta = match &self.carga {
            Some(c) => Some(c.rx.try_recv()),
            None => None,
        };
        match resposta {
            Some(Ok(Ok(entradas))) => {
                if let Some(c) = self.carga.take() {
                    self.dir = c.alvo;
                }
                self.em_unidades = false;
                self.todas = entradas;
                self.erro = None;
                self.marcados.clear();
                self.cursor = 0;
                self.reaplicar();
            }
            Some(Ok(Err(e))) => {
                self.carga = None;
                self.erro = Some(e);
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => self.carga = None,
            Some(Err(mpsc::TryRecvError::Empty)) | None => {}
        }

        // plano de cópia
        let plano = self.planejando.as_ref().map(|p| p.rx.try_recv());
        match plano {
            Some(Ok((Ok(plano), livre))) => {
                let destino = self.planejando.take().map(|p| p.destino).unwrap_or_default();
                match livre {
                    Some(l) if plano.total_bytes + MARGEM_ESPACO > l => {
                        self.mensagem = Some(format!(
                            "Sem espaço: precisa de {} e há {} livres.",
                            bytes_to_human(plano.total_bytes + MARGEM_ESPACO),
                            bytes_to_human(l)
                        ));
                    }
                    _ => self.confirmacao = Some(Confirmacao { plano, destino }),
                }
            }
            Some(Ok((Err(e), _))) => {
                self.planejando = None;
                self.mensagem = Some(format!("Não foi possível copiar: {e}"));
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => self.planejando = None,
            Some(Err(mpsc::TryRecvError::Empty)) | None => {}
        }

        // cópia em andamento
        let fim = self.copia.as_ref().map(|c| c.rx.try_recv());
        match fim {
            Some(Ok(r)) => {
                let destino = self.copia.take().map(|c| c.destino).unwrap_or_default();
                let resumo = format!(
                    "{} item(ns), {}, {} erro(s){}",
                    r.copiados,
                    bytes_to_human(r.bytes),
                    r.erros.len(),
                    if r.cancelado { ", CANCELADA" } else { "" }
                );
                self.auditar("fm.copiar", if r.erros.is_empty() { "ok" } else { "com erros" }, &resumo);
                self.mensagem = Some(format!("Cópia concluída: {resumo}"));
                self.relatorio = r.erros;
                self.abrir(destino); // recarrega para mostrar o que chegou
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => self.copia = None,
            Some(Err(mpsc::TryRecvError::Empty)) | None => {}
        }

        // criação de pasta
        let nova = self.criando.as_ref().map(|c| c.rx.try_recv());
        match nova {
            Some(Ok(Ok(caminho))) => {
                self.criando = None;
                self.auditar("fm.criar_pasta", "ok", &caminho.display().to_string());
                self.mensagem = Some(format!("Pasta criada: {}", caminho.display()));
                let dir = self.dir.clone();
                self.abrir(dir);
            }
            Some(Ok(Err(e))) => {
                self.criando = None;
                self.erro = Some(format!("Não foi possível criar a pasta: {e}"));
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => self.criando = None,
            Some(Err(mpsc::TryRecvError::Empty)) | None => {}
        }
    }

    fn sob_cursor(&self) -> Option<&FmEntry> {
        self.visiveis.get(self.cursor)
    }

    pub fn entrar(&mut self) {
        let alvo = match self.sob_cursor() {
            Some(e) if e.eh_pasta => e.caminho.clone(),
            _ => return,
        };
        self.abrir(alvo);
    }

    pub fn subir(&mut self) {
        if self.em_unidades {
            return;
        }
        match self.dir.parent() {
            Some(p) if !p.as_os_str().is_empty() => self.abrir(p.to_path_buf()),
            _ => self.mostrar_unidades(),
        }
    }

    fn mover(&mut self, delta: isize) {
        if self.visiveis.is_empty() {
            return;
        }
        let max = self.visiveis.len() - 1;
        self.cursor = (self.cursor as isize + delta).clamp(0, max as isize) as usize;
        self.tabela.select(Some(self.cursor));
    }

    fn alternar_marca(&mut self) {
        if let Some(e) = self.sob_cursor() {
            let c = e.caminho.clone();
            if !self.marcados.remove(&c) {
                self.marcados.insert(c);
            }
            self.mover(1);
        }
    }

    fn auditar(&self, acao: &str, resultado: &str, msg: &str) {
        if let Some(arq) = &self.auditoria {
            let _ = audit::registrar(arq, acao, resultado, msg);
        }
    }

    fn guardar(&mut self) {
        let itens: Vec<PathBuf> = if self.marcados.is_empty() {
            self.sob_cursor().map(|e| vec![e.caminho.clone()]).unwrap_or_default()
        } else {
            self.marcados.iter().cloned().collect()
        };
        if itens.is_empty() || self.em_unidades {
            self.mensagem = Some("Nada para guardar aqui.".into());
            return;
        }
        self.mensagem = Some(format!(
            "{} item(ns) guardado(s). Vá à pasta de destino e aperte [p].",
            itens.len()
        ));
        self.area = itens;
    }

    fn colar(&mut self) {
        if self.area.is_empty() {
            self.mensagem = Some("Nada guardado: marque e aperte [y] antes.".into());
            return;
        }
        if self.em_unidades || self.planejando.is_some() || self.copia.is_some() {
            self.mensagem = Some("Escolha uma pasta de destino (ou espere a operação atual).".into());
            return;
        }
        let (fontes, destino) = (self.area.clone(), self.dir.clone());
        let d2 = destino.clone();
        let rx = em_segundo_plano(move || {
            let livre = volume_info(&d2).map(|v| v.free);
            (planejar(&fontes, &d2), livre)
        });
        self.planejando = Some(Planejando { rx, destino });
        self.mensagem = Some("Calculando o plano de cópia...".into());
    }

    fn confirmar_copia(&mut self) {
        let Some(c) = self.confirmacao.take() else { return };
        let prog = Arc::new(Progresso::default());
        let (total_itens, total_bytes) = (c.plano.itens.len(), c.plano.total_bytes);
        let (p2, plano) = (Arc::clone(&prog), c.plano);
        let rx = em_segundo_plano(move || executar(&plano, &p2));
        self.auditar(
            "fm.copiar",
            "iniciado",
            &format!("{} item(ns), {} para {}", total_itens, bytes_to_human(total_bytes), c.destino.display()),
        );
        self.copia = Some(CopiaAtiva { prog, rx, total_itens, total_bytes, destino: c.destino, inicio: Instant::now() });
    }

    fn tecla_confirmacao(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => self.confirmar_copia(),
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                self.confirmacao = None;
                self.mensagem = Some("Cópia cancelada.".into());
            }
            _ => {}
        }
    }

    fn criar_pasta(&mut self) {
        let nome = self.entrada.trim().to_string();
        if let Err(e) = nome_valido(&nome) {
            self.erro = Some(e); // o campo continua aberto para corrigir
            return;
        }
        self.erro = None;
        let alvo = self.dir.join(&nome);
        let rx = em_segundo_plano(move || {
            std::fs::create_dir(&alvo).map(|_| alvo.clone()).map_err(|e| e.to_string())
        });
        self.criando = Some(CriandoPasta { rx });
        self.modo = FmModo::Normal;
        self.entrada.clear();
    }

    /// `true` quando a aba precisa receber **todas** as teclas (campo de texto ou modal aberto),
    /// para que digitar `q` ou `5` não saia do programa nem troque de aba.
    pub fn captura_teclado(&self) -> bool {
        self.modo != FmModo::Normal || self.confirmacao.is_some()
    }

    pub fn tecla(&mut self, key: KeyEvent) {
        if self.confirmacao.is_some() {
            return self.tecla_confirmacao(key);
        }
        match self.modo {
            FmModo::Normal => self.tecla_normal(key),
            _ => self.tecla_campo(key),
        }
    }

    fn tecla_campo(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                if self.modo == FmModo::Filtro {
                    self.filtro.clear();
                    self.reaplicar();
                }
                self.modo = FmModo::Normal;
                self.entrada.clear();
            }
            KeyCode::Enter => match self.modo {
                FmModo::Filtro => self.modo = FmModo::Normal,
                FmModo::Caminho => {
                    let alvo = PathBuf::from(self.entrada.trim());
                    self.modo = FmModo::Normal;
                    self.entrada.clear();
                    self.abrir(alvo);
                }
                FmModo::NovaPasta => self.criar_pasta(),
                FmModo::Normal => {}
            },
            KeyCode::Backspace => {
                if self.modo == FmModo::Filtro {
                    self.filtro.pop();
                    self.reaplicar();
                } else {
                    self.entrada.pop();
                }
            }
            KeyCode::Char(c) => {
                if self.modo == FmModo::Filtro {
                    self.filtro.push(c);
                    self.reaplicar();
                } else {
                    self.entrada.push(c);
                }
            }
            _ => {}
        }
    }

    fn tecla_normal(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.mover(-1),
            KeyCode::Down | KeyCode::Char('j') => self.mover(1),
            KeyCode::PageUp => self.mover(-10),
            KeyCode::PageDown => self.mover(10),
            KeyCode::Home | KeyCode::Char('g') => self.mover(isize::MIN / 2),
            KeyCode::End | KeyCode::Char('G') => self.mover(isize::MAX / 2),
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => self.entrar(),
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Backspace => self.subir(),
            KeyCode::Char(' ') => self.alternar_marca(),
            KeyCode::Char('.') => {
                self.mostrar_ocultos = !self.mostrar_ocultos;
                self.reaplicar();
            }
            KeyCode::Char('s') => {
                self.ordem = self.ordem.proximo();
                self.reaplicar();
                self.mensagem = Some(format!("Ordenado por {}.", self.ordem.rotulo()));
            }
            KeyCode::Char('/') => {
                self.modo = FmModo::Filtro;
            }
            KeyCode::Char(':') => {
                self.modo = FmModo::Caminho;
                self.entrada = self.dir.display().to_string();
            }
            KeyCode::Char('y') => self.guardar(),
            KeyCode::Char('p') => self.colar(),
            KeyCode::Char('n') => {
                if self.em_unidades {
                    self.mensagem = Some("Entre numa unidade antes de criar pasta.".into());
                } else {
                    self.modo = FmModo::NovaPasta;
                    self.entrada.clear();
                    self.erro = None;
                }
            }
            KeyCode::Char('r') => {
                if !self.em_unidades {
                    let d = self.dir.clone();
                    self.abrir(d);
                }
            }
            KeyCode::Esc => {
                if let Some(c) = &self.copia {
                    c.prog.cancelar.store(true, Ordering::Relaxed);
                    self.mensagem = Some("Cancelando a cópia (termina o item atual)...".into());
                } else {
                    self.marcados.clear();
                    self.filtro.clear();
                    self.reaplicar();
                }
            }
            _ => {}
        }
    }
}
