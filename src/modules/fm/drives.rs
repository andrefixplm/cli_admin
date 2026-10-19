//! Letras de unidade presentes. Usa `GetLogicalDrives` (uma máscara de bits): **não toca a
//! mídia**, então não trava com unidade de rede lenta nem com leitor vazio.

#[cfg(windows)]
pub fn unidades() -> Vec<String> {
    use windows::Win32::Storage::FileSystem::GetLogicalDrives;
    // SAFETY: a função não recebe ponteiros e só lê o estado do sistema.
    let mascara = unsafe { GetLogicalDrives() };
    (0..26u32)
        .filter(|i| mascara & (1 << i) != 0)
        .map(|i| format!("{}:\\", (b'A' + i as u8) as char))
        .collect()
}

#[cfg(not(windows))]
pub fn unidades() -> Vec<String> {
    vec!["/".to_string()]
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn inclui_a_unidade_do_temp() {
        let temp = std::env::temp_dir().to_string_lossy().to_uppercase();
        let raiz: String = temp.chars().take(3).collect();
        assert!(
            unidades().iter().any(|u| u.to_uppercase() == raiz),
            "unidades={:?} temp={temp}",
            unidades()
        );
    }
}
