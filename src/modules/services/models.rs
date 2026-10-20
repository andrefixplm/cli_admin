//! Tipos e regras puras do gerenciamento de serviços. Nada aqui chama o Win32,
//! para que as decisões (transições, espera, ordem de dependências, resolução
//! de alvo, validação de caminho) possam ser testadas sem o SCM.

use crate::modules::support::{SvcInfo, SvcState};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration;

/// Tipo de inicialização de um serviço (como o `sc config start=`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartType {
    Automatico,
    AutomaticoAtrasado,
    Manual,
    Desabilitado,
    Desconhecido,
}

impl StartType {
    pub fn label(&self) -> &'static str {
        match self {
            StartType::Automatico => "AUTOMÁTICO",
            StartType::AutomaticoAtrasado => "AUTO (ATRASADO)",
            StartType::Manual => "MANUAL",
            StartType::Desabilitado => "DESABILITADO",
            StartType::Desconhecido => "DESCONHECIDO",
        }
    }

    /// Aceita os nomes usados na CLI: auto, atrasado, manual, desabilitado.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "auto" | "automatico" | "automático" => Some(StartType::Automatico),
            "atrasado" | "auto-atrasado" => Some(StartType::AutomaticoAtrasado),
            "manual" => Some(StartType::Manual),
            "desabilitado" | "disabled" => Some(StartType::Desabilitado),
            _ => None,
        }
    }
}

/// Detalhes de um serviço, para exibição e para decisões de controle.
#[derive(Debug, Clone)]
pub struct SvcDetail {
    pub name: String,
    pub display_name: String,
    pub state: SvcState,
    pub start_type: StartType,
    pub binary_path: String,
    pub account: String,
}

/// Ação pedida pelo usuário ou pelo plano.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acao {
    Start,
    Stop,
    Restart,
    Pause,
    Continue,
    MudarInicializacao(StartType),
    Criar(StartType),
}

impl Acao {
    /// Rótulo usado na trilha de auditoria (`servico.<rotulo>`).
    pub fn rotulo(&self) -> &'static str {
        match self {
            Acao::Start => "start",
            Acao::Stop => "stop",
            Acao::Restart => "restart",
            Acao::Pause => "pause",
            Acao::Continue => "continue",
            Acao::MudarInicializacao(_) => "set-start",
            Acao::Criar(_) => "criar",
        }
    }
}

/// Uma chamada ao SCM dentro de uma ação.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Passo {
    Parar,
    Iniciar,
    Pausar,
    Continuar,
}

impl Passo {
    /// Estado que o serviço deve atingir depois do passo.
    pub fn estado_esperado(&self) -> SvcState {
        match self {
            Passo::Parar => SvcState::Stopped,
            Passo::Iniciar | Passo::Continuar => SvcState::Running,
            Passo::Pausar => SvcState::Paused,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transicao {
    /// Nada a fazer; o motivo vai para o relatório.
    NoOp(&'static str),
    /// Ação inválida para o estado atual.
    Recusar(String),
    /// Passos a executar em ordem. Vazio quando a ação não mexe no estado.
    Executar(Vec<Passo>),
}

/// Decide quais passos executar para `acao` a partir do estado `atual`.
pub fn valida_transicao(atual: SvcState, acao: Acao) -> Transicao {
    use SvcState::*;
    let transitorio = || Transicao::Recusar("serviço em transição, tente de novo".to_string());
    match acao {
        Acao::Start => match atual {
            Stopped => Transicao::Executar(vec![Passo::Iniciar]),
            Running => Transicao::NoOp("já está em execução"),
            Paused => Transicao::Recusar("serviço pausado: use 'continue'".to_string()),
            Other(_) => transitorio(),
        },
        Acao::Stop => match atual {
            Running | Paused => Transicao::Executar(vec![Passo::Parar]),
            Stopped => Transicao::NoOp("já está parado"),
            Other(_) => transitorio(),
        },
        Acao::Restart => match atual {
            Running | Paused => Transicao::Executar(vec![Passo::Parar, Passo::Iniciar]),
            Stopped => Transicao::Executar(vec![Passo::Iniciar]),
            Other(_) => transitorio(),
        },
        Acao::Pause => match atual {
            Running => Transicao::Executar(vec![Passo::Pausar]),
            Paused => Transicao::NoOp("já está pausado"),
            _ => Transicao::Recusar("só é possível pausar um serviço em execução".to_string()),
        },
        Acao::Continue => match atual {
            Paused => Transicao::Executar(vec![Passo::Continuar]),
            Running => Transicao::NoOp("já está em execução"),
            _ => Transicao::Recusar("só é possível continuar um serviço pausado".to_string()),
        },
        // Tipo de inicialização e criação não dependem do estado de execução.
        Acao::MudarInicializacao(_) | Acao::Criar(_) => Transicao::Executar(Vec::new()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Espera {
    Pronto,
    Aguardar,
    Timeout,
}

/// Decide o que fazer depois de consultar o estado durante a espera.
pub fn passo_espera(atual: SvcState, esperado: SvcState, decorrido: Duration, limite: Duration) -> Espera {
    if atual == esperado {
        Espera::Pronto
    } else if decorrido >= limite {
        Espera::Timeout
    } else {
        Espera::Aguardar
    }
}

/// Ordem de execução que termina em `alvo`. `antes[x]` lista os nós que devem
/// vir antes de `x`. Para parar um serviço, `antes` são os dependentes; para
/// iniciar, são as dependências. Ciclo vira erro.
pub fn ordem(alvo: &str, antes: &HashMap<String, Vec<String>>) -> Result<Vec<String>, String> {
    fn visita(
        no: &str,
        antes: &HashMap<String, Vec<String>>,
        feitos: &mut HashSet<String>,
        em_visita: &mut HashSet<String>,
        saida: &mut Vec<String>,
    ) -> Result<(), String> {
        if feitos.contains(no) {
            return Ok(());
        }
        if !em_visita.insert(no.to_string()) {
            return Err(format!("ciclo de dependência envolvendo '{}'", no));
        }
        if let Some(vizinhos) = antes.get(no) {
            for v in vizinhos {
                visita(v, antes, feitos, em_visita, saida)?;
            }
        }
        em_visita.remove(no);
        feitos.insert(no.to_string());
        saida.push(no.to_string());
        Ok(())
    }

    let mut feitos = HashSet::new();
    let mut em_visita = HashSet::new();
    let mut saida = Vec::new();
    visita(alvo, antes, &mut feitos, &mut em_visita, &mut saida)?;
    Ok(saida)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolucao {
    Unico(String),
    Ambiguo(Vec<String>),
    NaoEncontrado,
}

/// Nome exato vence. Senão, substring em nome ou nome de exibição; mais de um
/// resultado é recusado como ambíguo.
pub fn resolver_alvo(consulta: &str, servicos: &[SvcInfo]) -> Resolucao {
    if let Some(s) = servicos.iter().find(|s| s.name.eq_ignore_ascii_case(consulta)) {
        return Resolucao::Unico(s.name.clone());
    }
    let q = consulta.to_lowercase();
    let hits: Vec<String> = servicos
        .iter()
        .filter(|s| s.name.to_lowercase().contains(&q) || s.display_name.to_lowercase().contains(&q))
        .map(|s| s.name.clone())
        .collect();
    match hits.len() {
        0 => Resolucao::NaoEncontrado,
        1 => Resolucao::Unico(hits[0].clone()),
        _ => Resolucao::Ambiguo(hits),
    }
}

/// Nome de serviço: não vazio, até 256 caracteres, sem barras nem espaços nas pontas.
pub fn nome_valido(nome: &str) -> Result<(), String> {
    if nome.is_empty() || nome.len() > 256 {
        return Err("nome de serviço deve ter de 1 a 256 caracteres".to_string());
    }
    if nome.contains('/') || nome.contains('\\') {
        return Err("nome de serviço não pode conter '/' ou '\\'".to_string());
    }
    if nome.trim() != nome {
        return Err("nome de serviço não pode começar ou terminar com espaço".to_string());
    }
    Ok(())
}

/// Valida o executável de um serviço novo e devolve o `binPath` pronto para o SCM.
/// Caminho com espaço vai entre aspas, evitando o caminho sem aspas.
pub fn valida_caminho_criacao(caminho: &str) -> Result<String, String> {
    let p = Path::new(caminho);
    if !p.is_absolute() {
        return Err("caminho do executável deve ser absoluto".to_string());
    }
    if caminho.contains('"') {
        return Err("caminho do executável não pode conter aspas".to_string());
    }
    let extensao = p.extension().map(|e| e.to_string_lossy().to_lowercase());
    if extensao.as_deref() != Some("exe") {
        return Err("o executável do serviço deve ser um arquivo .exe".to_string());
    }
    if !p.is_file() {
        return Err(format!("executável não encontrado: {}", caminho));
    }
    Ok(if caminho.contains(' ') {
        format!("\"{}\"", caminho)
    } else {
        caminho.to_string()
    })
}

/// Texto legível para um código de erro Win32 do SCM.
pub fn mensagem_win32(codigo: u32) -> String {
    let texto = match codigo {
        5 => "acesso negado: requer executar como Administrador",
        1041 => "o serviço não pode aceitar esse controle agora",
        1051 => "há serviços dependentes em execução",
        1053 => "o serviço não respondeu a tempo",
        1056 => "o serviço já está em execução",
        1057 => "conta do serviço inválida",
        1060 => "o serviço não existe",
        1062 => "o serviço não está ativo",
        1068 => "um serviço do qual ele depende não pôde ser iniciado",
        1072 => "o serviço está marcado para exclusão",
        1073 => "já existe um serviço com este nome",
        1078 => "nome de exibição já em uso",
        _ => return format!("erro Win32 {}", codigo),
    };
    format!("{} (código {})", texto, codigo)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn svc(nome: &str, exibicao: &str) -> SvcInfo {
        SvcInfo {
            name: nome.to_string(),
            display_name: exibicao.to_string(),
            state: SvcState::Stopped,
        }
    }

    #[test]
    fn start_type_aceita_nomes_da_cli() {
        assert_eq!(StartType::parse("auto"), Some(StartType::Automatico));
        assert_eq!(StartType::parse("Atrasado"), Some(StartType::AutomaticoAtrasado));
        assert_eq!(StartType::parse("DESABILITADO"), Some(StartType::Desabilitado));
        assert_eq!(StartType::parse("xpto"), None);
    }

    #[test]
    fn transicoes_por_estado() {
        use SvcState::*;
        assert_eq!(valida_transicao(Stopped, Acao::Start), Transicao::Executar(vec![Passo::Iniciar]));
        assert_eq!(valida_transicao(Running, Acao::Start), Transicao::NoOp("já está em execução"));
        assert!(matches!(valida_transicao(Paused, Acao::Start), Transicao::Recusar(_)));
        assert_eq!(valida_transicao(Running, Acao::Stop), Transicao::Executar(vec![Passo::Parar]));
        assert_eq!(valida_transicao(Paused, Acao::Stop), Transicao::Executar(vec![Passo::Parar]));
        assert_eq!(valida_transicao(Stopped, Acao::Stop), Transicao::NoOp("já está parado"));
        assert_eq!(
            valida_transicao(Running, Acao::Restart),
            Transicao::Executar(vec![Passo::Parar, Passo::Iniciar])
        );
        assert_eq!(valida_transicao(Stopped, Acao::Restart), Transicao::Executar(vec![Passo::Iniciar]));
        assert_eq!(valida_transicao(Running, Acao::Pause), Transicao::Executar(vec![Passo::Pausar]));
        assert!(matches!(valida_transicao(Stopped, Acao::Pause), Transicao::Recusar(_)));
        assert_eq!(valida_transicao(Paused, Acao::Continue), Transicao::Executar(vec![Passo::Continuar]));
        assert!(matches!(valida_transicao(Running, Acao::Continue), Transicao::NoOp(_)));
        assert!(matches!(valida_transicao(Other(4), Acao::Stop), Transicao::Recusar(_)));
        assert_eq!(
            valida_transicao(Stopped, Acao::MudarInicializacao(StartType::Manual)),
            Transicao::Executar(Vec::new())
        );
    }

    #[test]
    fn espera_pronto_aguarda_e_expira() {
        let lim = Duration::from_secs(30);
        assert_eq!(
            passo_espera(SvcState::Running, SvcState::Running, Duration::ZERO, lim),
            Espera::Pronto
        );
        assert_eq!(
            passo_espera(SvcState::Other(2), SvcState::Running, Duration::from_secs(5), lim),
            Espera::Aguardar
        );
        assert_eq!(
            passo_espera(SvcState::Other(2), SvcState::Running, Duration::from_secs(30), lim),
            Espera::Timeout
        );
    }

    #[test]
    fn ordem_parada_poe_dependentes_antes_do_alvo() {
        // B e C dependem de A; D depende de B. Parar A exige parar D, B, C e só então A.
        let mut dependentes: HashMap<String, Vec<String>> = HashMap::new();
        dependentes.insert("A".into(), vec!["B".into(), "C".into()]);
        dependentes.insert("B".into(), vec!["D".into()]);
        let r = ordem("A", &dependentes).unwrap();
        assert_eq!(r.last().map(String::as_str), Some("A"));
        let pos = |n: &str| r.iter().position(|x| x == n).unwrap();
        assert!(pos("D") < pos("B"));
        assert!(pos("B") < pos("A"));
        assert!(pos("C") < pos("A"));
        assert_eq!(r.len(), 4);
    }

    #[test]
    fn ordem_detecta_ciclo() {
        let mut m: HashMap<String, Vec<String>> = HashMap::new();
        m.insert("A".into(), vec!["B".into()]);
        m.insert("B".into(), vec!["A".into()]);
        assert!(ordem("A", &m).unwrap_err().contains("ciclo"));
    }

    #[test]
    fn resolver_prefere_nome_exato_e_recusa_ambiguidade() {
        let lista = vec![svc("Spooler", "Fila de impressão"), svc("Spooler2", "Outro"), svc("Dhcp", "Cliente DHCP")];
        assert_eq!(resolver_alvo("spooler", &lista), Resolucao::Unico("Spooler".into()));
        assert_eq!(resolver_alvo("dhcp", &lista), Resolucao::Unico("Dhcp".into()));
        assert_eq!(resolver_alvo("cliente", &lista), Resolucao::Unico("Dhcp".into()));
        assert_eq!(
            resolver_alvo("spool", &lista),
            Resolucao::Ambiguo(vec!["Spooler".into(), "Spooler2".into()])
        );
        assert_eq!(resolver_alvo("zzz", &lista), Resolucao::NaoEncontrado);
    }

    #[test]
    fn nome_de_servico_valida_caracteres() {
        assert!(nome_valido("Spooler").is_ok());
        assert!(nome_valido("").is_err());
        assert!(nome_valido("a/b").is_err());
        assert!(nome_valido("a\\b").is_err());
        assert!(nome_valido(" espaço").is_err());
    }

    #[test]
    fn caminho_de_criacao_exige_absoluto_exe_existente_e_cita_espacos() {
        let dir = std::env::temp_dir().join(format!("cli_admin_svc_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("com espaco")).unwrap();
        let exe = dir.join("com espaco").join("servico.exe");
        std::fs::write(&exe, b"x").unwrap();
        let caminho = exe.to_string_lossy().to_string();

        let bin = valida_caminho_criacao(&caminho).unwrap();
        assert_eq!(bin, format!("\"{}\"", caminho));

        assert!(valida_caminho_criacao("relativo\\servico.exe").is_err());
        assert!(valida_caminho_criacao(&dir.join("x.txt").to_string_lossy()).is_err());
        assert!(valida_caminho_criacao(&dir.join("nao_existe.exe").to_string_lossy()).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mensagem_win32_traduz_codigos_conhecidos() {
        assert!(mensagem_win32(5).contains("Administrador"));
        assert!(mensagem_win32(1060).contains("não existe"));
        assert_eq!(mensagem_win32(9999), "erro Win32 9999");
    }
}
