//! Um servidor MCP iniciado como administrador faria todas as chamadas do agente rodarem como
//! administrador. Por isso ele recusa iniciar elevado, salvo pedido explícito.

/// `true` quando o processo está elevado. `net session` só funciona em processo elevado.
/// Limite conhecido: depende do serviço "Servidor" do Windows; se ele estiver parado,
/// a detecção pode dar falso "não elevado".
pub fn is_admin() -> bool {
    std::process::Command::new("net")
        .arg("session")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn decidir_inicio(admin: bool, permitir_elevado: bool) -> Result<(), String> {
    if admin && !permitir_elevado {
        return Err("recusado: o servidor MCP não deve rodar como administrador. \
                    Abra um terminal comum, ou use --permitir-elevado se tiver certeza."
            .into());
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn recusa_elevado_sem_flag() {
        assert!(decidir_inicio(true, false).unwrap_err().contains("administrador"));
    }

    #[test]
    fn aceita_usuario_comum_e_elevado_com_flag_explicita() {
        assert!(decidir_inicio(false, false).is_ok());
        assert!(decidir_inicio(true, true).is_ok());
    }
}
