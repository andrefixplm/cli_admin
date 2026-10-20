//! Escudo de serviços críticos. Espaço de nomes próprio: não reutiliza o
//! `ProcessShield`, porque nome de serviço e nome de processo não se misturam.
//!
//! Regra: parar, reiniciar, pausar ou desabilitar um serviço crítico é bloqueado.
//! Iniciar e continuar são permitidos, porque bloquear a subida pode deixar a
//! máquina tão inutilizável quanto bloquear a parada.

use super::models::{Acao, StartType};
use std::collections::HashSet;

/// Serviços sem os quais o Windows não funciona direito (RPC, COM, logon, eventos,
/// WMI, filtragem de Bluetooth/BFE, Netlogon e o servidor SMB).
pub const CRITICOS: &[&str] = &[
    "RpcSs",
    "RpcEptMapper",
    "DcomLaunch",
    "LSM",
    "SamSs",
    "EventLog",
    "Winmgmt",
    "BFE",
    "Netlogon",
    "LanmanServer",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decisao {
    Permitir,
    Bloquear(String),
}

pub struct ServiceShield {
    protegidos: HashSet<String>,
}

impl ServiceShield {
    /// `extras` vem de `[services].protected_names` no config.toml.
    pub fn new(extras: &[String]) -> Self {
        let protegidos = CRITICOS
            .iter()
            .map(|s| s.to_string())
            .chain(extras.iter().cloned())
            .map(|s| s.to_lowercase())
            .collect();
        Self { protegidos }
    }

    pub fn e_protegido(&self, nome: &str) -> bool {
        self.protegidos.contains(&nome.to_lowercase())
    }

    pub fn check(&self, nome: &str, acao: Acao) -> Decisao {
        if !self.e_protegido(nome) {
            return Decisao::Permitir;
        }
        match acao {
            Acao::Start | Acao::Continue => Decisao::Permitir,
            Acao::MudarInicializacao(tipo) if tipo != StartType::Desabilitado => Decisao::Permitir,
            _ => Decisao::Bloquear(format!("serviço '{}' é crítico para o sistema", nome)),
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn critico_bloqueia_parar_reiniciar_pausar_e_desabilitar() {
        let s = ServiceShield::new(&[]);
        for acao in [Acao::Stop, Acao::Restart, Acao::Pause] {
            assert!(matches!(s.check("RpcSs", acao), Decisao::Bloquear(_)));
        }
        assert!(matches!(
            s.check("EventLog", Acao::MudarInicializacao(StartType::Desabilitado)),
            Decisao::Bloquear(_)
        ));
    }

    #[test]
    fn critico_permite_iniciar_continuar_e_mudar_para_tipo_nao_desabilitado() {
        let s = ServiceShield::new(&[]);
        assert_eq!(s.check("RpcSs", Acao::Start), Decisao::Permitir);
        assert_eq!(s.check("RpcSs", Acao::Continue), Decisao::Permitir);
        assert_eq!(
            s.check("RpcSs", Acao::MudarInicializacao(StartType::Manual)),
            Decisao::Permitir
        );
    }

    #[test]
    fn nome_nao_critico_e_livre() {
        let s = ServiceShield::new(&[]);
        assert_eq!(s.check("Spooler", Acao::Stop), Decisao::Permitir);
        assert_eq!(s.check("Spooler", Acao::Restart), Decisao::Permitir);
    }

    #[test]
    fn comparacao_ignora_caixa_e_respeita_extras_do_config() {
        let s = ServiceShield::new(&["MeuAgente".to_string()]);
        assert!(s.e_protegido("rpcss"));
        assert!(s.e_protegido("meuagente"));
        assert!(matches!(s.check("MEUAGENTE", Acao::Stop), Decisao::Bloquear(_)));
        assert!(!s.e_protegido("Outro"));
    }
}
