//! Plano e execução de cópia. O plano é calculado antes, mostrado ao usuário e só então executado.
//! Regras: nunca sobrescreve (conflito vira `nome (2)`), não segue links nem junções,
//! recusa copiar uma pasta para dentro dela mesma.

use super::listing::{ATTR_REPARSE, atributos};
use super::names::nome_livre;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct ItemCopia {
    pub origem: PathBuf,
    pub destino: PathBuf,
    pub eh_pasta: bool,
    pub bytes: u64,
}

#[derive(Debug, Clone, Default)]
pub struct PlanoCopia {
    pub itens: Vec<ItemCopia>,
    pub total_bytes: u64,
    /// Quantos itens da raiz ganharam sufixo `(2)` por conflito.
    pub renomeados: usize,
    /// Links, junções e itens ilegíveis que ficaram de fora, com o motivo.
    pub ignorados: Vec<String>,
}

fn eh_reparse(p: &Path) -> bool {
    std::fs::symlink_metadata(p)
        .map(|m| m.file_type().is_symlink() || atributos(&m) & ATTR_REPARSE != 0)
        .unwrap_or(false)
}

/// Calcula o que seria copiado para `destino`. Lê a árvore de origem: pode demorar,
/// então **chame de uma thread de fundo**.
pub fn planejar(fontes: &[PathBuf], destino: &Path) -> Result<PlanoCopia, String> {
    if fontes.is_empty() {
        return Err("nada selecionado para copiar".into());
    }
    if !destino.is_dir() {
        return Err(format!("o destino não é uma pasta: {}", destino.display()));
    }
    let destino_canon =
        std::fs::canonicalize(destino).map_err(|e| format!("destino ilegível: {e}"))?;

    let mut plano = PlanoCopia::default();
    let mut usados: HashSet<String> = HashSet::new();

    for fonte in fontes {
        let md = std::fs::symlink_metadata(fonte)
            .map_err(|e| format!("origem ilegível ({}): {e}", fonte.display()))?;
        if eh_reparse(fonte) {
            plano.ignorados.push(format!("{} (link ou junção não é copiado)", fonte.display()));
            continue;
        }
        let nome = fonte
            .file_name()
            .ok_or_else(|| format!("origem sem nome: {}", fonte.display()))?
            .to_string_lossy()
            .to_string();

        if md.is_dir() {
            let fonte_canon = std::fs::canonicalize(fonte)
                .map_err(|e| format!("origem ilegível ({}): {e}", fonte.display()))?;
            if destino_canon.starts_with(&fonte_canon) {
                return Err(format!("o destino está dentro da pasta de origem: {}", fonte.display()));
            }
        }

        let livre = nome_livre(
            |n| destino.join(n).exists() || usados.contains(&n.to_lowercase()),
            &nome,
        );
        if livre != nome {
            plano.renomeados += 1;
        }
        usados.insert(livre.to_lowercase());
        let raiz_destino = destino.join(&livre);

        if !md.is_dir() {
            plano.total_bytes += md.len();
            plano.itens.push(ItemCopia {
                origem: fonte.clone(),
                destino: raiz_destino,
                eh_pasta: false,
                bytes: md.len(),
            });
            continue;
        }

        plano.itens.push(ItemCopia {
            origem: fonte.clone(),
            destino: raiz_destino.clone(),
            eh_pasta: true,
            bytes: 0,
        });
        let mut it = WalkDir::new(fonte).min_depth(1).follow_links(false).into_iter();
        while let Some(ent) = it.next() {
            let ent = match ent {
                Ok(e) => e,
                Err(e) => {
                    plano.ignorados.push(e.to_string());
                    continue;
                }
            };
            if eh_reparse(ent.path()) {
                plano.ignorados.push(format!("{} (link ou junção)", ent.path().display()));
                if ent.file_type().is_dir() {
                    it.skip_current_dir();
                }
                continue;
            }
            let Ok(rel) = ent.path().strip_prefix(fonte) else { continue };
            let alvo = raiz_destino.join(rel);
            if ent.file_type().is_dir() {
                plano.itens.push(ItemCopia {
                    origem: ent.path().to_path_buf(),
                    destino: alvo,
                    eh_pasta: true,
                    bytes: 0,
                });
            } else if ent.file_type().is_file() {
                let bytes = ent.metadata().map(|m| m.len()).unwrap_or(0);
                plano.total_bytes += bytes;
                plano.itens.push(ItemCopia {
                    origem: ent.path().to_path_buf(),
                    destino: alvo,
                    eh_pasta: false,
                    bytes,
                });
            }
        }
    }

    if plano.itens.is_empty() {
        return Err("nada que possa ser copiado (só links ou itens ilegíveis)".into());
    }
    Ok(plano)
}

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

/// Contadores lidos pela interface enquanto a cópia roda em outra thread.
#[derive(Debug, Default)]
pub struct Progresso {
    pub bytes: AtomicU64,
    pub itens: AtomicUsize,
    pub cancelar: AtomicBool,
}

#[derive(Debug, Default)]
pub struct ResultadoCopia {
    pub copiados: usize,
    pub bytes: u64,
    pub erros: Vec<String>,
    pub cancelado: bool,
}

/// Executa o plano. Confere o cancelamento **entre itens**.
// ponytail: um arquivo muito grande não é interrompido no meio (std::fs::copy).
// Se isso incomodar, trocar por cópia em blocos conferindo `cancelar` a cada bloco.
pub fn executar(plano: &PlanoCopia, prog: &Progresso) -> ResultadoCopia {
    let mut r = ResultadoCopia::default();
    for item in &plano.itens {
        if prog.cancelar.load(Ordering::Relaxed) {
            r.cancelado = true;
            break;
        }
        // Defesa contra corrida: o destino pode ter surgido depois do plano
        if item.destino.exists() {
            r.erros.push(format!("{}: já existe, não foi sobrescrito", item.destino.display()));
            continue;
        }
        let resultado = if item.eh_pasta {
            std::fs::create_dir_all(&item.destino).map(|_| 0u64)
        } else {
            std::fs::copy(&item.origem, &item.destino)
        };
        match resultado {
            Ok(bytes) => {
                r.copiados += 1;
                r.bytes += bytes;
                prog.bytes.fetch_add(bytes, Ordering::Relaxed);
                prog.itens.fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => r.erros.push(format!("{}: {e}", item.origem.display())),
        }
    }
    r
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Cria `<temp>/cli_admin_cp_<nome>/{origem,destino}` e devolve (origem, destino).
    fn cenario(nome: &str) -> (PathBuf, PathBuf) {
        let raiz = std::env::temp_dir().join(format!("cli_admin_cp_{nome}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&raiz);
        std::fs::create_dir_all(raiz.join("origem")).unwrap();
        std::fs::create_dir_all(raiz.join("destino")).unwrap();
        (raiz.join("origem"), raiz.join("destino"))
    }

    fn limpar(p: &Path) {
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn arquivo_simples() {
        let (o, d) = cenario("simples");
        std::fs::write(o.join("a.txt"), b"hello").unwrap();
        let p = planejar(&[o.join("a.txt")], &d).unwrap();
        assert_eq!(p.itens.len(), 1);
        assert_eq!(p.total_bytes, 5);
        assert_eq!(p.renomeados, 0);
        assert_eq!(p.itens[0].destino, d.join("a.txt"));
        limpar(&o);
    }

    #[test]
    fn conflito_vira_nome_2() {
        let (o, d) = cenario("conflito");
        std::fs::write(o.join("a.txt"), b"novo").unwrap();
        std::fs::write(d.join("a.txt"), b"antigo").unwrap();
        let p = planejar(&[o.join("a.txt")], &d).unwrap();
        assert_eq!(p.itens[0].destino, d.join("a (2).txt"));
        assert_eq!(p.renomeados, 1);
        limpar(&o);
    }

    #[test]
    fn dois_com_o_mesmo_nome_nao_colidem() {
        let (o, d) = cenario("mesmo_nome");
        std::fs::create_dir_all(o.join("x")).unwrap();
        std::fs::write(o.join("a.txt"), b"1").unwrap();
        std::fs::write(o.join("x").join("a.txt"), b"2").unwrap();
        let p = planejar(&[o.join("a.txt"), o.join("x").join("a.txt")], &d).unwrap();
        assert_ne!(p.itens[0].destino, p.itens[1].destino);
        limpar(&o);
    }

    #[test]
    fn pasta_recursiva() {
        let (o, d) = cenario("recursiva");
        std::fs::create_dir_all(o.join("p").join("sub")).unwrap();
        std::fs::write(o.join("p").join("x.txt"), b"xx").unwrap();
        std::fs::write(o.join("p").join("sub").join("y.txt"), b"yyy").unwrap();
        let p = planejar(&[o.join("p")], &d).unwrap();
        assert_eq!(p.itens.len(), 4, "p, x.txt, sub, y.txt");
        assert_eq!(p.total_bytes, 5);
        assert!(p.itens[0].eh_pasta, "a pasta raiz vem antes dos filhos");
        limpar(&o);
    }

    #[test]
    fn destino_dentro_da_origem_e_recusado() {
        let (o, _d) = cenario("dentro");
        std::fs::create_dir_all(o.join("p").join("sub")).unwrap();
        let r = planejar(&[o.join("p")], &o.join("p").join("sub"));
        assert!(r.unwrap_err().contains("dentro"));
        limpar(&o);
    }

    #[test]
    fn sem_fontes_ou_destino_invalido() {
        let (o, d) = cenario("invalidos");
        assert!(planejar(&[], &d).is_err());
        std::fs::write(o.join("f.txt"), b"x").unwrap();
        assert!(planejar(&[o.join("f.txt")], &o.join("f.txt")).is_err(), "destino é arquivo");
        assert!(planejar(&[o.join("nao_existe")], &d).is_err());
        limpar(&o);
    }

    #[cfg(windows)]
    #[test]
    fn juncao_e_ignorada_e_nao_seguida() {
        let (o, d) = cenario("juncao");
        std::fs::create_dir_all(o.join("p").join("real")).unwrap();
        std::fs::write(o.join("p").join("real").join("dado.bin"), b"12345").unwrap();
        let link = o.join("p").join("atalho");
        let ok = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(o.join("p").join("real"))
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "mklink /J falhou; o teste precisa criar a junção");
        let p = planejar(&[o.join("p")], &d).unwrap();
        assert_eq!(p.ignorados.len(), 1, "a junção deve ser listada como ignorada: {:?}", p.ignorados);
        assert!(p.itens.iter().all(|i| !i.origem.starts_with(&link)), "nada dentro da junção entra no plano");
        // dado.bin entra uma única vez (via 'real'), não duas
        assert_eq!(p.itens.iter().filter(|i| i.origem.ends_with("dado.bin")).count(), 1);
        limpar(&o);
    }

    #[test]
    fn executar_copia_o_conteudo_e_conta() {
        let (o, d) = cenario("exec");
        std::fs::create_dir_all(o.join("p").join("sub")).unwrap();
        std::fs::write(o.join("p").join("x.txt"), b"xx").unwrap();
        std::fs::write(o.join("p").join("sub").join("y.txt"), b"yyy").unwrap();
        let plano = planejar(&[o.join("p")], &d).unwrap();
        let prog = Progresso::default();
        let r = executar(&plano, &prog);
        assert!(r.erros.is_empty(), "{:?}", r.erros);
        assert!(!r.cancelado);
        assert_eq!(r.copiados, 4);
        assert_eq!(r.bytes, 5);
        assert_eq!(std::fs::read(d.join("p").join("sub").join("y.txt")).unwrap(), b"yyy");
        assert_eq!(prog.bytes.load(Ordering::Relaxed), 5);
        limpar(&o);
    }

    #[test]
    fn cancelado_antes_de_comecar_nao_copia_nada() {
        let (o, d) = cenario("cancel");
        std::fs::write(o.join("a.txt"), b"hello").unwrap();
        let plano = planejar(&[o.join("a.txt")], &d).unwrap();
        let prog = Progresso::default();
        prog.cancelar.store(true, Ordering::Relaxed);
        let r = executar(&plano, &prog);
        assert!(r.cancelado);
        assert_eq!(r.copiados, 0);
        assert!(!d.join("a.txt").exists());
        limpar(&o);
    }

    #[test]
    fn nunca_sobrescreve_mesmo_se_o_destino_surgir_depois_do_plano() {
        let (o, d) = cenario("corrida");
        std::fs::write(o.join("a.txt"), b"novo").unwrap();
        let plano = planejar(&[o.join("a.txt")], &d).unwrap();
        // o arquivo aparece entre o plano e a execução
        std::fs::write(d.join("a.txt"), b"antigo").unwrap();
        let r = executar(&plano, &Progresso::default());
        assert_eq!(r.copiados, 0);
        assert_eq!(r.erros.len(), 1);
        assert_eq!(std::fs::read(d.join("a.txt")).unwrap(), b"antigo");
        limpar(&o);
    }

    #[test]
    fn falha_em_um_item_nao_aborta_os_outros() {
        let (o, d) = cenario("falha");
        std::fs::write(o.join("a.txt"), b"1").unwrap();
        std::fs::write(o.join("b.txt"), b"2").unwrap();
        let plano = planejar(&[o.join("a.txt"), o.join("b.txt")], &d).unwrap();
        std::fs::remove_file(o.join("a.txt")).unwrap(); // some antes de copiar
        let r = executar(&plano, &Progresso::default());
        assert_eq!(r.erros.len(), 1);
        assert_eq!(r.copiados, 1);
        assert!(d.join("b.txt").exists());
        limpar(&o);
    }
}
