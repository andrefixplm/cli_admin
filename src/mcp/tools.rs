//! Ferramentas do MCP como funções sobre dados, sem depender do SDK. A coleta (sysinfo, sockets)
//! fica a cargo de quem chama; aqui só se decide o que sai, já redigido, sanitizado e limitado.

use super::policy::{caminho_permitido, redigir_linha_de_comando, sanitizar_campo};
use crate::config::McpConfig;
use crate::modules::net::NetConnection;
use crate::modules::process::ProcessItem;
use crate::modules::process::killer::separar_protegidos;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const TETO_ITENS: usize = 200;

pub fn processos_listar(
    procs: &[ProcessItem],
    filtro: Option<&str>,
    ordenar: Option<&str>,
    limite: usize,
    offset: usize,
    cfg: &McpConfig,
) -> Value {
    let f = filtro.map(|s| s.to_lowercase());
    let mut v: Vec<&ProcessItem> = procs
        .iter()
        .filter(|p| f.as_ref().is_none_or(|f| p.name.to_lowercase().contains(f.as_str())))
        .collect();
    match ordenar.unwrap_or("ram") {
        "cpu" => v.sort_by(|a, b| b.cpu_pct.total_cmp(&a.cpu_pct)),
        "nome" => v.sort_by_key(|p| p.name.to_lowercase()),
        _ => v.sort_by(|a, b| b.memory_bytes.cmp(&a.memory_bytes)),
    }
    let total = v.len();
    let limite = limite.clamp(1, TETO_ITENS);
    let itens: Vec<Value> = v
        .iter()
        .skip(offset)
        .take(limite)
        .map(|p| {
            let cmd = if cfg.expor_linha_de_comando {
                Value::String(sanitizar_campo(&redigir_linha_de_comando(&p.cmd_line), cfg.max_campo))
            } else {
                Value::Null
            };
            json!({
                "pid": p.pid,
                "nome": sanitizar_campo(&p.name, cfg.max_campo),
                "cpu_pct": (p.cpu_pct * 10.0).round() / 10.0,
                "memoria_mb": (p.memory_mb() * 10.0).round() / 10.0,
                "usuario": sanitizar_campo(&p.user, cfg.max_campo),
                "caminho": sanitizar_campo(&p.exe_path, cfg.max_campo),
                "linha_de_comando": cmd,
                "protegido": p.is_protected,
            })
        })
        .collect();
    let proxima = if offset + itens.len() < total { json!(offset + itens.len()) } else { Value::Null };
    json!({ "total": total, "offset": offset, "limite": limite, "proxima": proxima, "itens": itens })
}

pub fn portas_listar(
    conns: &[NetConnection],
    so_escuta: bool,
    filtro: Option<&str>,
    limite: usize,
    offset: usize,
    cfg: &McpConfig,
) -> Value {
    let f = filtro.map(|s| s.to_lowercase());
    let v: Vec<&NetConnection> = conns
        .iter()
        .filter(|c| !so_escuta || c.is_listening())
        .filter(|c| {
            f.as_ref().is_none_or(|f| {
                c.process_name.to_lowercase().contains(f.as_str()) || c.local_port.to_string() == *f
            })
        })
        .collect();
    let total = v.len();
    let limite = limite.clamp(1, TETO_ITENS);
    let itens: Vec<Value> = v
        .iter()
        .skip(offset)
        .take(limite)
        .map(|c| {
            json!({
                "proto": c.proto_label(),
                "local": c.local_endpoint(),
                "remoto": c.remote_endpoint(),
                "estado": c.state_label(),
                "pid": c.pid,
                "processo": sanitizar_campo(&c.process_name, cfg.max_campo),
                "protegido": c.is_protected,
            })
        })
        .collect();
    let proxima = if offset + itens.len() < total { json!(offset + itens.len()) } else { Value::Null };
    json!({ "total": total, "offset": offset, "limite": limite, "proxima": proxima, "itens": itens })
}

/// Lista nomes, tipos e tamanhos. **Não lê o conteúdo de nenhum arquivo.**
pub fn fs_listar(caminho: &str, limite: usize, cfg: &McpConfig) -> Result<Value, String> {
    let raizes: Vec<PathBuf> = cfg.raizes_permitidas.iter().map(PathBuf::from).collect();
    let dir = caminho_permitido(&raizes, &cfg.caminhos_negados, Path::new(caminho))?;
    if !dir.is_dir() {
        return Err("o caminho não é uma pasta".into());
    }
    let limite = limite.clamp(1, TETO_ITENS);
    let mut itens = Vec::new();
    let mut omitidos_por_regra = 0usize;
    let entradas = std::fs::read_dir(&dir).map_err(|_| "pasta ilegível".to_string())?;
    for ent in entradas.filter_map(|e| e.ok()) {
        // cada filho passa pela mesma política: junção para fora e itens negados ficam de fora
        if caminho_permitido(&raizes, &cfg.caminhos_negados, &ent.path()).is_err() {
            omitidos_por_regra += 1;
            continue;
        }
        let Ok(md) = ent.metadata() else { continue };
        if itens.len() < limite {
            itens.push(json!({
                "nome": sanitizar_campo(&ent.file_name().to_string_lossy(), cfg.max_campo),
                "pasta": md.is_dir(),
                "bytes": if md.is_file() { json!(md.len()) } else { Value::Null },
            }));
        }
    }
    Ok(json!({ "itens": itens, "omitidos_por_regra": omitidos_por_regra }))
}

/// Alvo de um plano de encerramento: o processo **e** o momento em que ele começou, para que
/// um PID reaproveitado por outro processo não seja encerrado por engano.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct AlvoPlano {
    pub pid: u32,
    pub nome: String,
    pub start_time: u64,
}

/// Monta o plano: só alvos elegíveis. Devolve (alvos, pids_protegidos_excluidos).
/// PID que não existe mais também fica de fora (tratado como protegido por segurança).
pub fn planejar_encerramento(procs: &[ProcessItem], pids: &[u32]) -> (Vec<AlvoPlano>, Vec<u32>) {
    let pares: Vec<(u32, bool)> = pids
        .iter()
        .map(|p| (*p, procs.iter().find(|x| x.pid == *p).is_none_or(|x| x.is_protected)))
        .collect();
    let (liberados, bloqueados) = separar_protegidos(&pares);
    let alvos = liberados
        .into_iter()
        .filter_map(|pid| procs.iter().find(|x| x.pid == pid))
        .map(|x| AlvoPlano { pid: x.pid, nome: x.name.clone(), start_time: x.start_time })
        .collect();
    (alvos, bloqueados)
}

/// Na hora de aplicar: só segue quem continua sendo o **mesmo** processo e não é protegido.
/// Devolve (pids_ok, motivos_de_recusa).
pub fn revalidar_alvos(planejados: &[AlvoPlano], agora: &[ProcessItem]) -> (Vec<u32>, Vec<String>) {
    let (mut ok, mut recusados) = (Vec::new(), Vec::new());
    for a in planejados {
        match agora.iter().find(|p| p.pid == a.pid) {
            None => recusados.push(format!("PID {} não existe mais", a.pid)),
            Some(p) if p.is_protected => recusados.push(format!("PID {} agora é protegido", a.pid)),
            Some(p) if p.name != a.nome || p.start_time != a.start_time => {
                recusados.push(format!("PID {} é outro processo (PID reaproveitado)", a.pid))
            }
            Some(_) => ok.push(a.pid),
        }
    }
    (ok, recusados)
}

#[cfg(test)]
mod testes_plano {
    use super::*;

    fn proc_com(pid: u32, nome: &str, start_time: u64, protegido: bool) -> ProcessItem {
        ProcessItem {
            pid,
            name: nome.into(),
            cpu_pct: 0.0,
            memory_bytes: 0,
            is_protected: protegido,
            selected: false,
            exe_path: String::new(),
            user: String::new(),
            start_time,
            threads: 0,
            parent_pid: None,
            gpu_pct: 0.0,
            vram_bytes: 0,
            disk_read_bytes_sec: 0,
            disk_write_bytes_sec: 0,
            cmd_line: String::new(),
        }
    }

    fn alvo(pid: u32, nome: &str, inicio: u64) -> AlvoPlano {
        AlvoPlano { pid, nome: nome.into(), start_time: inicio }
    }

    #[test]
    fn plano_exclui_protegidos_e_registra_inicio() {
        let procs = vec![proc_com(10, "x.exe", 100, false), proc_com(4, "system", 1, true)];
        let (alvos, bloqueados) = planejar_encerramento(&procs, &[10, 4]);
        assert_eq!(alvos, vec![alvo(10, "x.exe", 100)]);
        assert_eq!(bloqueados, vec![4]);
    }

    #[test]
    fn plano_recusa_pid_inexistente() {
        let (alvos, bloqueados) = planejar_encerramento(&[], &[999]);
        assert!(alvos.is_empty());
        assert_eq!(bloqueados, vec![999]);
    }

    #[test]
    fn revalidar_aceita_o_mesmo_processo() {
        let planejados = vec![alvo(10, "x.exe", 100)];
        let agora = vec![proc_com(10, "x.exe", 100, false)];
        let (ok, recusados) = revalidar_alvos(&planejados, &agora);
        assert_eq!(ok, vec![10]);
        assert!(recusados.is_empty());
    }

    #[test]
    fn revalidar_recusa_pid_reaproveitado_por_outro_processo() {
        let planejados = vec![alvo(10, "x.exe", 100)];
        let agora = vec![proc_com(10, "x.exe", 999, false)]; // mesmo PID e nome, outro início
        let (ok, recusados) = revalidar_alvos(&planejados, &agora);
        assert!(ok.is_empty());
        assert_eq!(recusados.len(), 1);
    }

    #[test]
    fn revalidar_recusa_processo_que_virou_protegido_ou_sumiu() {
        let planejados = vec![alvo(10, "x.exe", 100), alvo(11, "y.exe", 100)];
        let agora = vec![proc_com(10, "x.exe", 100, true)]; // virou protegido; o 11 sumiu
        let (ok, recusados) = revalidar_alvos(&planejados, &agora);
        assert!(ok.is_empty());
        assert_eq!(recusados.len(), 2);
    }
}
