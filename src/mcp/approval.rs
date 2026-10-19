//! Planos de ação em disco. O agente cria o plano; **um humano**, em outro terminal, o aprova
//! (`cli_admin_v3 mcp aprovar <id>`); o servidor o consome uma única vez. Em disco porque a
//! aprovação acontece em outro processo.

use super::policy::fnv1a;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static CONTADOR: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plano {
    pub id: String,
    pub ferramenta: String,
    pub args_hash: u64,
    pub payload: Value,
    pub descricao: String,
    pub criado_ms: u64,
    pub validade_ms: u64,
    pub aprovado: bool,
    pub usado: bool,
}

pub fn agora_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn hash_args(args: &Value) -> u64 {
    // serde_json ordena as chaves de objeto (BTreeMap): a serialização é estável.
    fnv1a(args.to_string().as_bytes())
}

/// Só letras, dígitos, '-' e '_': impede `..` e separadores de caminho no id.
fn id_valido(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn arquivo(dir: &Path, id: &str) -> Result<PathBuf, String> {
    if !id_valido(id) {
        return Err("id de plano inválido".into());
    }
    Ok(dir.join(format!("{id}.json")))
}

fn salvar(dir: &Path, p: &Plano) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("pasta de planos: {e}"))?;
    let destino = arquivo(dir, &p.id)?;
    let temporario = destino.with_extension("tmp");
    let texto = serde_json::to_string_pretty(p).map_err(|e| e.to_string())?;
    std::fs::write(&temporario, texto).map_err(|e| e.to_string())?;
    std::fs::rename(&temporario, &destino).map_err(|e| e.to_string())
}

fn carregar(dir: &Path, id: &str) -> Result<Plano, String> {
    let texto = std::fs::read_to_string(arquivo(dir, id)?).map_err(|_| "plano não encontrado".to_string())?;
    serde_json::from_str(&texto).map_err(|_| "plano corrompido".to_string())
}

pub fn criar(
    dir: &Path,
    ferramenta: &str,
    args: &Value,
    payload: Value,
    descricao: &str,
    agora_ms: u64,
    validade_s: u64,
) -> Result<Plano, String> {
    let plano = Plano {
        id: format!("p{}-{}", agora_ms, CONTADOR.fetch_add(1, Ordering::Relaxed)),
        ferramenta: ferramenta.to_string(),
        args_hash: hash_args(args),
        payload,
        descricao: descricao.to_string(),
        criado_ms: agora_ms,
        validade_ms: validade_s * 1000,
        aprovado: false,
        usado: false,
    };
    salvar(dir, &plano)?;
    Ok(plano)
}

fn expirado(p: &Plano, agora_ms: u64) -> bool {
    agora_ms.saturating_sub(p.criado_ms) > p.validade_ms
}

/// Planos ainda válidos, não aprovados e não usados (o que um humano precisa revisar).
pub fn listar_pendentes(dir: &Path, agora_ms: u64) -> Vec<Plano> {
    let Ok(lista) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<Plano> = lista
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|t| serde_json::from_str::<Plano>(&t).ok())
        .filter(|p| !p.aprovado && !p.usado && !expirado(p, agora_ms))
        .collect();
    v.sort_by_key(|p| p.criado_ms);
    v
}

pub fn aprovar(dir: &Path, id: &str, agora_ms: u64) -> Result<Plano, String> {
    let mut p = carregar(dir, id)?;
    if p.usado {
        return Err("o plano já foi usado".into());
    }
    if expirado(&p, agora_ms) {
        return Err("o plano expirou".into());
    }
    p.aprovado = true;
    salvar(dir, &p)?;
    Ok(p)
}

/// Marca como usado **antes** de devolver: no máximo uma execução, mesmo se algo falhar depois.
pub fn consumir(dir: &Path, id: &str, ferramenta: &str, args: &Value, agora_ms: u64) -> Result<Plano, String> {
    let mut p = carregar(dir, id)?;
    if p.usado {
        return Err("o plano já foi usado".into());
    }
    if expirado(&p, agora_ms) {
        return Err("o plano expirou".into());
    }
    if !p.aprovado {
        return Err("aguardando aprovação humana: rode 'cli_admin_v3 mcp aprovar <id>' em outro terminal".into());
    }
    if p.ferramenta != ferramenta || p.args_hash != hash_args(args) {
        return Err("os argumentos diferem dos do plano aprovado".into());
    }
    p.usado = true;
    salvar(dir, &p)?;
    Ok(p)
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::json;

    fn pasta(nome: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cli_admin_plan_{nome}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn novo(d: &Path) -> Plano {
        criar(d, "processo_encerrar", &json!({"pids":[10]}), json!({"alvos":[10]}), "encerrar 10", 1_000, 300).unwrap()
    }

    #[test]
    fn caminho_feliz_criar_aprovar_consumir() {
        let d = pasta("feliz");
        let p = novo(&d);
        assert!(!p.aprovado);
        assert_eq!(listar_pendentes(&d, 2_000).len(), 1);
        aprovar(&d, &p.id, 2_000).unwrap();
        let c = consumir(&d, &p.id, "processo_encerrar", &json!({"pids":[10]}), 3_000).unwrap();
        assert_eq!(c.payload, json!({"alvos":[10]}));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn sem_aprovacao_nao_consome() {
        let d = pasta("sem_aprov");
        let p = novo(&d);
        let e = consumir(&d, &p.id, "processo_encerrar", &json!({"pids":[10]}), 2_000).unwrap_err();
        assert!(e.contains("aprova"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn uso_unico() {
        let d = pasta("uso_unico");
        let p = novo(&d);
        aprovar(&d, &p.id, 2_000).unwrap();
        consumir(&d, &p.id, "processo_encerrar", &json!({"pids":[10]}), 3_000).unwrap();
        let e = consumir(&d, &p.id, "processo_encerrar", &json!({"pids":[10]}), 3_100).unwrap_err();
        assert!(e.contains("já foi usado"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn expira() {
        let d = pasta("expira");
        let p = novo(&d); // validade: 300 s a partir de 1_000 ms
        aprovar(&d, &p.id, 2_000).unwrap();
        let e = consumir(&d, &p.id, "processo_encerrar", &json!({"pids":[10]}), 1_000 + 300_001).unwrap_err();
        assert!(e.contains("expirou"), "{e}");
        assert!(aprovar(&d, &p.id, 1_000 + 300_001).is_err(), "não se aprova plano expirado");
        assert!(listar_pendentes(&d, 1_000 + 300_001).is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn argumentos_diferentes_do_plano_sao_recusados() {
        let d = pasta("hash");
        let p = novo(&d);
        aprovar(&d, &p.id, 2_000).unwrap();
        let e = consumir(&d, &p.id, "processo_encerrar", &json!({"pids":[999]}), 3_000).unwrap_err();
        assert!(e.contains("diferem"), "{e}");
        // e a ferramenta também precisa ser a mesma
        let e = consumir(&d, &p.id, "outra", &json!({"pids":[10]}), 3_000).unwrap_err();
        assert!(e.contains("diferem") || e.contains("ferramenta"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn id_com_caminho_nao_escapa_da_pasta() {
        let d = pasta("traversal");
        std::fs::create_dir_all(&d).unwrap();
        for ruim in ["..\\..\\x", "../x", "a/b", "a b", ""] {
            assert!(aprovar(&d, ruim, 1).is_err(), "deveria recusar id {ruim:?}");
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn plano_inexistente() {
        let d = pasta("inexistente");
        std::fs::create_dir_all(&d).unwrap();
        assert!(aprovar(&d, "p123-1", 1).unwrap_err().contains("não encontrado"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
