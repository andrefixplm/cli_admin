//! Leitura de uma pasta para o explorador. Itens ilegíveis são pulados, nunca derrubam a lista.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

const ATTR_OCULTO: u32 = 0x2;
const ATTR_SISTEMA: u32 = 0x4;
/// Link simbólico ou junção. Para a **cópia**, nunca é seguido.
pub const ATTR_REPARSE: u32 = 0x400;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FmEntry {
    pub nome: String,
    pub caminho: PathBuf,
    /// Pode ser aberta na navegação (inclui link para pasta).
    pub eh_pasta: bool,
    /// Link simbólico ou junção: aparece na lista, mas a cópia o ignora.
    pub eh_link: bool,
    pub bytes: u64,
    pub modificado: Option<SystemTime>,
    pub oculto: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FmSort {
    Nome,
    Tamanho,
    Data,
}

impl FmSort {
    pub fn proximo(self) -> Self {
        match self {
            FmSort::Nome => FmSort::Tamanho,
            FmSort::Tamanho => FmSort::Data,
            FmSort::Data => FmSort::Nome,
        }
    }

    pub fn rotulo(self) -> &'static str {
        match self {
            FmSort::Nome => "nome",
            FmSort::Tamanho => "tamanho",
            FmSort::Data => "data",
        }
    }
}

#[cfg(windows)]
pub(super) fn atributos(md: &std::fs::Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt;
    md.file_attributes()
}

#[cfg(not(windows))]
pub(super) fn atributos(_md: &std::fs::Metadata) -> u32 {
    0
}

/// Lê `dir`. Pode demorar em unidade de rede: **chame de uma thread de fundo**.
pub fn listar(dir: &Path) -> std::io::Result<Vec<FmEntry>> {
    let mut saida = Vec::new();
    for ent in std::fs::read_dir(dir)? {
        let Ok(ent) = ent else { continue };
        // No Windows, DirEntry::metadata não segue links
        let Ok(md) = ent.metadata() else { continue };
        let attr = atributos(&md);
        let nome = ent.file_name().to_string_lossy().to_string();
        let caminho = ent.path();
        let eh_link = md.file_type().is_symlink() || attr & ATTR_REPARSE != 0;
        let eh_pasta = if eh_link {
            std::fs::metadata(&caminho).map(|m| m.is_dir()).unwrap_or(false)
        } else {
            md.is_dir()
        };
        let oculto = attr & (ATTR_OCULTO | ATTR_SISTEMA) != 0
            || (cfg!(not(windows)) && nome.starts_with('.'));
        saida.push(FmEntry {
            nome,
            caminho,
            eh_pasta,
            eh_link,
            bytes: if md.is_file() { md.len() } else { 0 },
            modificado: md.modified().ok(),
            oculto,
        });
    }
    Ok(saida)
}

/// Pastas primeiro; depois pela ordem pedida (tamanho e data em ordem decrescente).
pub fn ordenar(v: &mut [FmEntry], ordem: FmSort) {
    v.sort_by(|a, b| {
        let por_nome = || a.nome.to_lowercase().cmp(&b.nome.to_lowercase());
        b.eh_pasta.cmp(&a.eh_pasta).then_with(|| match ordem {
            FmSort::Nome => por_nome(),
            FmSort::Tamanho => b.bytes.cmp(&a.bytes).then_with(por_nome),
            FmSort::Data => b.modificado.cmp(&a.modificado).then_with(por_nome),
        })
    });
}

/// Itens que aparecem na tela: respeita ocultos e o filtro por texto.
pub fn visiveis(todas: &[FmEntry], mostrar_ocultos: bool, filtro: &str) -> Vec<FmEntry> {
    let f = filtro.to_lowercase();
    todas
        .iter()
        .filter(|e| (mostrar_ocultos || !e.oculto) && (f.is_empty() || e.nome.to_lowercase().contains(&f)))
        .cloned()
        .collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    fn pasta_de_teste(nome: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cli_admin_fm_{nome}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("zeta")).unwrap();
        std::fs::write(d.join("b.txt"), vec![0u8; 10]).unwrap();
        std::fs::write(d.join("A.txt"), vec![0u8; 100]).unwrap();
        d
    }

    #[test]
    fn lista_e_marca_pastas_e_tamanhos() {
        let d = pasta_de_teste("lista");
        let mut v = listar(&d).unwrap();
        ordenar(&mut v, FmSort::Nome);
        let nomes: Vec<&str> = v.iter().map(|e| e.nome.as_str()).collect();
        assert_eq!(nomes, ["zeta", "A.txt", "b.txt"], "pastas primeiro, depois por nome sem diferenciar caixa");
        assert!(v[0].eh_pasta && !v[1].eh_pasta);
        assert_eq!(v[1].bytes, 100);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn ordena_por_tamanho_decrescente() {
        let d = pasta_de_teste("tam");
        let mut v = listar(&d).unwrap();
        ordenar(&mut v, FmSort::Tamanho);
        let nomes: Vec<&str> = v.iter().map(|e| e.nome.as_str()).collect();
        assert_eq!(nomes, ["zeta", "A.txt", "b.txt"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn filtro_por_texto_sem_diferenciar_caixa() {
        let d = pasta_de_teste("filtro");
        let v = listar(&d).unwrap();
        let r = visiveis(&v, true, "B.T");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].nome, "b.txt");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[cfg(windows)]
    #[test]
    fn oculto_some_sem_mostrar_ocultos() {
        let d = pasta_de_teste("oculto");
        let alvo = d.join("b.txt");
        let ok = std::process::Command::new("attrib").arg("+h").arg(&alvo).status().unwrap().success();
        assert!(ok, "attrib +h falhou");
        let v = listar(&d).unwrap();
        assert!(visiveis(&v, false, "").iter().all(|e| e.nome != "b.txt"));
        assert!(visiveis(&v, true, "").iter().any(|e| e.nome == "b.txt"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn pasta_inexistente_devolve_erro() {
        assert!(listar(Path::new(r"Z:\nao\existe\mesmo")).is_err());
    }

    #[test]
    fn ciclo_de_ordenacao() {
        assert_eq!(FmSort::Nome.proximo(), FmSort::Tamanho);
        assert_eq!(FmSort::Tamanho.proximo(), FmSort::Data);
        assert_eq!(FmSort::Data.proximo(), FmSort::Nome);
    }
}
