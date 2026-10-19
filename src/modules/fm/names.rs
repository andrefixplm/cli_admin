//! Regras de nome do Windows para criar pastas e para nunca sobrescrever na cópia.

const RESERVADOS: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Valida o nome de **um** item (sem separadores de caminho).
pub fn nome_valido(nome: &str) -> Result<(), String> {
    if nome.is_empty() {
        return Err("o nome está vazio".into());
    }
    if nome.chars().count() > 255 {
        return Err("nome longo demais (máximo 255 caracteres)".into());
    }
    if nome.ends_with(' ') || nome.ends_with('.') {
        return Err("o nome não pode terminar com espaço ou ponto".into());
    }
    if let Some(c) = nome.chars().find(|c| c.is_control() || "<>:\"/\\|?*".contains(*c)) {
        return Err(format!("caractere inválido: {c:?}"));
    }
    let base = nome.split('.').next().unwrap_or("").to_uppercase();
    if RESERVADOS.contains(&base.as_str()) {
        return Err(format!("nome reservado do Windows: {base}"));
    }
    Ok(())
}

/// Primeiro nome livre: `nome`, depois `nome (2).ext`, `nome (3).ext`...
/// `existe` diz se um nome já está ocupado (o Windows não diferencia maiúsculas).
pub fn nome_livre(existe: impl Fn(&str) -> bool, nome: &str) -> String {
    if !existe(nome) {
        return nome.to_string();
    }
    let (base, ext) = match nome.rfind('.') {
        Some(i) if i > 0 => (&nome[..i], &nome[i..]),
        _ => (nome, ""),
    };
    (2..)
        .map(|n| format!("{base} ({n}){ext}"))
        .find(|candidato| !existe(candidato))
        .expect("sempre existe um número livre")
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn nomes_validos() {
        assert!(nome_valido("Relatórios 2026").is_ok());
        assert!(nome_valido("a.b.c").is_ok());
        assert!(nome_valido(&"x".repeat(255)).is_ok());
    }

    #[test]
    fn nomes_invalidos() {
        for ruim in ["", "a:b", "a/b", "a\\b", "a|b", "a*b", "a?b", "a\"b", "pasta.", "pasta ", "..", "x\u{7}y"] {
            assert!(nome_valido(ruim).is_err(), "deveria recusar {ruim:?}");
        }
        assert!(nome_valido(&"x".repeat(256)).is_err());
    }

    #[test]
    fn nomes_reservados_do_windows() {
        for r in ["CON", "con", "NUL", "COM1", "lpt9", "con.txt", "Aux.log"] {
            assert!(nome_valido(r).is_err(), "deveria recusar {r:?}");
        }
        assert!(nome_valido("CONTA").is_ok());
        assert!(nome_valido("COM10").is_ok());
    }

    #[test]
    fn nome_livre_sem_conflito_nao_muda() {
        assert_eq!(nome_livre(|_| false, "a.txt"), "a.txt");
    }

    #[test]
    fn nome_livre_numera_antes_da_extensao() {
        let ocupados = ["a.txt", "a (2).txt"];
        assert_eq!(nome_livre(|n| ocupados.contains(&n), "a.txt"), "a (3).txt");
    }

    #[test]
    fn nome_livre_sem_extensao_e_com_ponto_inicial() {
        assert_eq!(nome_livre(|n| n == "pasta", "pasta"), "pasta (2)");
        assert_eq!(nome_livre(|n| n == ".env", ".env"), ".env (2)");
    }
}
