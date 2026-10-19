//! Utilitários de texto para a interface e a saída de terminal.
//!
//! Regra: nunca fatiar `String` por índice de byte. Um caractere acentuado ocupa dois ou mais
//! bytes em UTF-8, e fatiar no meio dele causa panic. Estas funções trabalham por caractere.

/// Corta `texto` em no máximo `max` caracteres. Quando corta, termina com `…`.
pub fn truncar(texto: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if texto.chars().count() <= max {
        return texto.to_string();
    }
    let mut out: String = texto.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Barra horizontal proporcional: ocupa `valor / maximo` da `largura`, com `█` (cheio) e `░` (vazio).
/// Sempre devolve exatamente `largura` caracteres; `valor` acima do máximo é limitado.
pub fn barra(valor: u64, maximo: u64, largura: usize) -> String {
    let maximo = maximo.max(1);
    let cheio = (((valor as f64 / maximo as f64) * largura as f64).round() as usize).min(largura);
    format!("{}{}", "█".repeat(cheio), "░".repeat(largura - cheio))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn texto_curto_nao_muda() {
        assert_eq!(truncar("chrome.exe", 28), "chrome.exe");
    }

    #[test]
    fn corta_e_marca_com_reticencias() {
        assert_eq!(truncar("abcdefghij", 5), "abcd…");
    }

    #[test]
    fn nao_quebra_caractere_acentuado() {
        // Com fatiamento por byte, `&s[..4]` cairia no meio do "ç" e causaria panic.
        let s = "açãoçãoçãoçãoçãoção.exe";
        let cortado = truncar(s, 4);
        assert_eq!(cortado.chars().count(), 4);
        assert!(cortado.ends_with('…'));
    }

    #[test]
    fn barra_proporcional() {
        assert_eq!(barra(100, 100, 10), "██████████");
        assert_eq!(barra(50, 100, 10), "█████░░░░░");
        assert_eq!(barra(0, 100, 10), "░░░░░░░░░░");
    }

    #[test]
    fn barra_sempre_tem_a_largura_pedida() {
        for valor in [0u64, 1, 7, 99, 100, 5_000] {
            assert_eq!(barra(valor, 100, 24).chars().count(), 24, "valor={valor}");
        }
        // máximo zero não divide por zero
        assert_eq!(barra(0, 0, 8).chars().count(), 8);
    }

    #[test]
    fn limite_zero_devolve_vazio() {
        assert_eq!(truncar("qualquer", 0), "");
    }
}
