//! Orquestração do gerenciamento de serviços. Separada em duas fases para que a
//! CLI, a TUI e o MCP compartilhem a mesma lógica:
//!
//! 1. `planejar` não altera nada. Resolve o alvo, aplica o escudo sobre toda a
//!    cadeia de dependências e calcula os passos.
//! 2. `executar` aplica o plano: exige elevação, envia cada passo ao SCM,
//!    espera o estado final de cada um e devolve um `Relatorio`.
//!
//! Quem chama decide sobre confirmação e grava a auditoria com `registrar`.

use super::models::{
    Acao, Espera, Passo, Resolucao, StartType, Transicao, ordem, passo_espera, resolver_alvo,
    valida_transicao,
};
use super::scm;
use super::shield::{Decisao, ServiceShield};
use crate::audit;
use crate::modules::support::{SvcState, list_services};
use std::collections::HashMap;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

const INTERVALO_ESPERA: Duration = Duration::from_millis(500);

#[derive(Debug, Clone)]
pub struct Opcoes {
    /// Recusa a ação se houver dependentes ativos, em vez de pará-los.
    pub sem_dependentes: bool,
    pub timeout: Duration,
}

/// Um passo do plano: que serviço, e o que fazer com ele.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassoServico {
    pub servico: String,
    pub passo: Passo,
}

/// Plano calculado sem efeitos colaterais. Só `executar` altera o sistema.
#[derive(Debug, Clone)]
pub struct Plano {
    pub acao: Acao,
    pub alvo: String,
    pub passos: Vec<PassoServico>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resultado {
    Concluido,
    DryRun,
    /// Quem chama não confirmou o plano.
    Cancelado,
    NoOp(String),
    Recusado(String),
    Falhou(String),
}

impl Resultado {
    /// Rótulo curto para a trilha de auditoria.
    pub fn rotulo(&self) -> &'static str {
        match self {
            Resultado::Concluido => "ok",
            Resultado::DryRun => "dry-run",
            Resultado::Cancelado => "cancelado",
            Resultado::NoOp(_) => "noop",
            Resultado::Recusado(_) => "recusado",
            Resultado::Falhou(_) => "erro",
        }
    }

    pub fn mensagem(&self) -> &str {
        match self {
            Resultado::Concluido | Resultado::DryRun | Resultado::Cancelado => "",
            Resultado::NoOp(m) | Resultado::Recusado(m) | Resultado::Falhou(m) => m,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Relatorio {
    pub acao: Acao,
    pub alvo: String,
    pub resultado: Resultado,
    pub passos: Vec<String>,
}

impl Relatorio {
    fn sem_plano(acao: Acao, alvo: &str, resultado: Resultado) -> Self {
        Self { acao, alvo: alvo.to_string(), resultado, passos: Vec::new() }
    }
}

/// Texto de um passo para exibição e auditoria.
pub fn descreve(ps: &PassoServico) -> String {
    let verbo = match ps.passo {
        Passo::Parar => "parar",
        Passo::Iniciar => "iniciar",
        Passo::Pausar => "pausar",
        Passo::Continuar => "continuar",
    };
    format!("{} {}", verbo, ps.servico)
}

/// Fecho transitivo de dependentes ativos: `antes[x]` são os dependentes de `x`.
fn mapa_dependentes(alvo: &str) -> Result<HashMap<String, Vec<String>>, String> {
    let mut mapa = HashMap::new();
    let mut fila = vec![alvo.to_string()];
    while let Some(no) = fila.pop() {
        if mapa.contains_key(&no) {
            continue;
        }
        let filhos = scm::dependentes_ativos(&no)?;
        fila.extend(filhos.iter().filter(|f| !mapa.contains_key(*f)).cloned());
        mapa.insert(no, filhos);
    }
    Ok(mapa)
}

/// Fecho transitivo de dependências (o que o alvo precisa para subir).
fn mapa_dependencias(alvo: &str) -> Result<HashMap<String, Vec<String>>, String> {
    let mut mapa = HashMap::new();
    let mut fila = vec![alvo.to_string()];
    while let Some(no) = fila.pop() {
        if mapa.contains_key(&no) {
            continue;
        }
        let deps = scm::dependencias(&no)?;
        fila.extend(deps.iter().filter(|d| !mapa.contains_key(*d)).cloned());
        mapa.insert(no, deps);
    }
    Ok(mapa)
}

/// Calcula o plano para `acao` sobre `consulta`. `Err` carrega o relatório de
/// quem não deve ser executado: alvo inexistente, ambíguo, ação inválida para o
/// estado, no-op, ou bloqueio pelo escudo.
pub fn planejar(
    acao: Acao,
    consulta: &str,
    opts: &Opcoes,
    escudo: &ServiceShield,
) -> Result<Plano, Relatorio> {
    // `recusado`: política ou validação. `falha`: erro ao ler o SCM ou montar a cadeia.
    let recusado = |msg: String| Relatorio::sem_plano(acao, consulta, Resultado::Recusado(msg));
    let falha = |msg: String| Relatorio::sem_plano(acao, consulta, Resultado::Falhou(msg));

    let servicos = list_services().map_err(|e| falha(e))?;
    let alvo = match resolver_alvo(consulta, &servicos) {
        Resolucao::Unico(nome) => nome,
        Resolucao::Ambiguo(nomes) => {
            return Err(recusado(format!("consulta ambígua, use o nome exato: {}", nomes.join(", "))));
        }
        Resolucao::NaoEncontrado => return Err(recusado("serviço não encontrado".to_string())),
    };

    // Escudo antes de qualquer consulta ao SCM: serviço crítico é bloqueado mesmo
    // sem permissão para ler seu estado.
    if let Decisao::Bloquear(m) = escudo.check(&alvo, acao) {
        return Err(recusado(m));
    }

    let atual = scm::estado(&alvo).map_err(|e| falha(e))?;
    let passos_alvo = match valida_transicao(atual, acao) {
        Transicao::NoOp(m) => {
            return Err(Relatorio::sem_plano(acao, &alvo, Resultado::NoOp(m.to_string())));
        }
        Transicao::Recusar(m) => return Err(recusado(m)),
        Transicao::Executar(passos) => passos,
    };

    let mut passos: Vec<PassoServico> = Vec::new();

    match acao {
        Acao::Criar(_) => return Err(recusado("criação não passa por planejar; use criar".to_string())),
        Acao::MudarInicializacao(_) => {}
        Acao::Start => {
            // Sobe primeiro o que o alvo precisa, na ordem certa. Só mexe no que está parado.
            let deps = mapa_dependencias(&alvo).map_err(|e| falha(e))?;
            let ordem_deps = ordem(&alvo, &deps).map_err(|e| falha(e))?;
            for nome in ordem_deps.iter().filter(|n| **n != alvo) {
                match scm::estado(nome).map_err(|e| falha(e))? {
                    SvcState::Stopped => passos.push(PassoServico {
                        servico: nome.clone(),
                        passo: Passo::Iniciar,
                    }),
                    SvcState::Running => {}
                    _ => return Err(recusado(format!("dependência '{}' está em transição ou pausada", nome))),
                }
            }
            passos.extend(passos_alvo.iter().map(|p| PassoServico { servico: alvo.clone(), passo: *p }));
        }
        Acao::Stop | Acao::Restart => {
            // Para os dependentes ativos antes do alvo. Com `sem_dependentes`, recusa.
            let antes = mapa_dependentes(&alvo).map_err(|e| falha(e))?;
            let cadeia = ordem(&alvo, &antes).map_err(|e| falha(e))?;
            let dependentes: Vec<&String> = cadeia.iter().filter(|n| **n != alvo).collect();

            if opts.sem_dependentes && !dependentes.is_empty() {
                let lista = dependentes.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
                return Err(recusado(format!("há dependentes ativos: {}", lista)));
            }

            // Escudo sobre toda a cadeia: uma recusa cancela a operação inteira.
            let mut bloqueios = Vec::new();
            for dep in &dependentes {
                if let Decisao::Bloquear(m) = escudo.check(dep, Acao::Stop) {
                    bloqueios.push(m);
                }
            }
            if !bloqueios.is_empty() {
                return Err(recusado(bloqueios.join("; ")));
            }

            for dep in &dependentes {
                passos.push(PassoServico { servico: (*dep).clone(), passo: Passo::Parar });
            }
            passos.extend(passos_alvo.iter().map(|p| PassoServico { servico: alvo.clone(), passo: *p }));
            if acao == Acao::Restart {
                // Depois de religar o alvo, religa os dependentes na ordem inversa da parada.
                for dep in dependentes.iter().rev() {
                    passos.push(PassoServico { servico: (*dep).clone(), passo: Passo::Iniciar });
                }
            }
        }
        Acao::Pause => {
            // Pausar não é cascateado: com dependentes ativos, recusa.
            let dependentes = scm::dependentes_ativos(&alvo).map_err(|e| falha(e))?;
            if !dependentes.is_empty() {
                return Err(recusado(format!("pausar exige dependentes parados: {}", dependentes.join(", "))));
            }
            passos.extend(passos_alvo.iter().map(|p| PassoServico { servico: alvo.clone(), passo: *p }));
        }
        Acao::Continue => {
            passos.extend(passos_alvo.iter().map(|p| PassoServico { servico: alvo.clone(), passo: *p }));
        }
    }

    Ok(Plano { acao, alvo, passos })
}

/// Relatório de um plano que não foi executado, só simulado.
pub fn simulacao(plano: &Plano) -> Relatorio {
    Relatorio {
        acao: plano.acao,
        alvo: plano.alvo.clone(),
        resultado: Resultado::DryRun,
        passos: textos_do_plano(plano),
    }
}

fn textos_do_plano(plano: &Plano) -> Vec<String> {
    let mut linhas: Vec<String> = plano.passos.iter().map(descreve).collect();
    if let Acao::MudarInicializacao(tipo) = plano.acao {
        linhas.push(format!("mudar início de {} para {}", plano.alvo, tipo.label()));
    }
    linhas
}

/// Espera o serviço atingir `esperado`, consultando o SCM até o limite.
fn esperar(nome: &str, esperado: SvcState, limite: Duration) -> Result<(), String> {
    let inicio = Instant::now();
    loop {
        let atual = scm::estado(nome)?;
        match passo_espera(atual, esperado, inicio.elapsed(), limite) {
            Espera::Pronto => return Ok(()),
            Espera::Timeout => {
                return Err(format!(
                    "tempo esgotado aguardando '{}' (estado atual {})",
                    nome,
                    atual.label()
                ));
            }
            Espera::Aguardar => thread::sleep(INTERVALO_ESPERA),
        }
    }
}

/// Aplica o plano. Bloqueia em esperas, então a TUI deve chamar em thread própria.
pub fn executar(plano: &Plano, opts: &Opcoes) -> Relatorio {
    let mut rel = Relatorio {
        acao: plano.acao,
        alvo: plano.alvo.clone(),
        resultado: Resultado::Concluido,
        passos: Vec::new(),
    };

    if !scm::e_elevado() {
        rel.resultado = Resultado::Recusado("requer executar como Administrador".to_string());
        return rel;
    }

    for ps in &plano.passos {
        let esperado = ps.passo.estado_esperado();
        // Passo já satisfeito (ex.: dependente que parou junto com o pai): pula sem erro.
        let ja_ok = matches!(scm::estado(&ps.servico), Ok(e) if e == esperado);
        if !ja_ok {
            if let Err(m) = scm::controlar(&ps.servico, ps.passo) {
                rel.resultado = Resultado::Falhou(format!("{}: {}", descreve(ps), m));
                return rel;
            }
            if let Err(m) = esperar(&ps.servico, esperado, opts.timeout) {
                rel.resultado = Resultado::Falhou(format!("{}: {}", descreve(ps), m));
                return rel;
            }
        }
        rel.passos.push(descreve(ps));
    }

    if let Acao::MudarInicializacao(tipo) = plano.acao {
        match scm::mudar_inicializacao(&plano.alvo, tipo) {
            Ok(()) => rel.passos.push(format!("início de {} = {}", plano.alvo, tipo.label())),
            Err(m) => rel.resultado = Resultado::Falhou(m),
        }
    }
    rel
}

/// Cria um serviço Win32 novo. Não passa pelo escudo: o nome precisa ser inédito.
pub fn criar(nome: &str, exibicao: &str, exe: &str, tipo: StartType) -> Relatorio {
    let acao = Acao::Criar(tipo);
    let mut rel = Relatorio::sem_plano(acao, nome, Resultado::Concluido);
    let falha = |m: String| Relatorio::sem_plano(acao, nome, Resultado::Falhou(m));

    if let Err(m) = super::models::nome_valido(nome) {
        return falha(m);
    }
    if !scm::e_elevado() {
        return Relatorio::sem_plano(acao, nome, Resultado::Recusado("requer executar como Administrador".to_string()));
    }
    let binario = match super::models::valida_caminho_criacao(exe) {
        Ok(b) => b,
        Err(m) => return falha(m),
    };
    let exibicao = if exibicao.trim().is_empty() { nome } else { exibicao };
    match scm::criar(nome, exibicao, &binario, tipo) {
        Ok(()) => rel.passos.push(format!("criar {} apontando para {}", nome, binario)),
        Err(m) => rel.resultado = Resultado::Falhou(m),
    }
    rel
}

/// Grava uma linha na trilha de auditoria para o relatório. `consulta` é o que o
/// usuário digitou, para registrar também os pedidos que não acharam alvo.
pub fn registrar(arquivo: &Path, consulta: &str, rel: &Relatorio) -> std::io::Result<()> {
    let acao = format!("servico.{}", rel.acao.rotulo());
    let msg = format!(
        "alvo={} consulta={} passos=[{}] {}",
        rel.alvo,
        consulta,
        rel.passos.join("; "),
        rel.resultado.mensagem()
    );
    audit::registrar(arquivo, &acao, rel.resultado.rotulo(), msg.trim_end())
}

pub fn listar() -> Result<Vec<super::models::SvcDetail>, String> {
    scm::listar_detalhes()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn descreve_passo_em_portugues() {
        let ps = PassoServico { servico: "Spooler".into(), passo: Passo::Parar };
        assert_eq!(descreve(&ps), "parar Spooler");
    }

    #[test]
    fn resultado_mapeia_rotulo_de_auditoria() {
        assert_eq!(Resultado::Recusado("x".into()).rotulo(), "recusado");
        assert_eq!(Resultado::DryRun.rotulo(), "dry-run");
        assert_eq!(Resultado::Concluido.rotulo(), "ok");
    }
}
