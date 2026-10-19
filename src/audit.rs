//! Trilha de auditoria: uma linha por evento, 5 campos separados por TAB:
//! quando, usuário, ação, resultado, mensagem. Quebras de linha e tabs viram espaço.
//! Quem chama é responsável por **não** passar segredos na mensagem.

use std::io::Write;
use std::path::Path;

pub const ARQUIVO_PADRAO: &str = "audit/auditoria.log";

fn limpar(s: &str) -> String {
    s.replace(['\t', '\r', '\n'], " ")
}

pub fn formatar(quando: &str, usuario: &str, acao: &str, resultado: &str, msg: &str) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}",
        limpar(quando),
        limpar(usuario),
        limpar(acao),
        limpar(resultado),
        limpar(msg)
    )
}

/// Acrescenta uma linha ao arquivo, criando as pastas que faltarem.
pub fn registrar(arquivo: &Path, acao: &str, resultado: &str, msg: &str) -> std::io::Result<()> {
    if let Some(pai) = arquivo.parent() {
        std::fs::create_dir_all(pai)?;
    }
    let quando = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let usuario = std::env::var("USERNAME").unwrap_or_else(|_| "desconhecido".into());
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(arquivo)?;
    writeln!(f, "{}", formatar(&quando, &usuario, acao, resultado, msg))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn linha_tem_cinco_campos_e_sem_quebras() {
        let l = formatar("2026-10-17 10:00:00", "andre", "fm.copiar", "ok", "a\tb\nc");
        assert_eq!(l.split('\t').count(), 5);
        assert!(!l.contains('\n'));
        assert!(l.ends_with("a b c"));
    }

    #[test]
    fn registrar_acrescenta_linhas_e_cria_pastas() {
        let dir = std::env::temp_dir().join(format!("cli_admin_audit_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let arq = dir.join("sub").join("a.log");
        registrar(&arq, "x", "ok", "1").unwrap();
        registrar(&arq, "x", "ok", "2").unwrap();
        assert_eq!(std::fs::read_to_string(&arq).unwrap().lines().count(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
