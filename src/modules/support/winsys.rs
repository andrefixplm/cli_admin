use super::winsvc::{enum_hkcu_values, enum_hklm_values, list_services};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Entrada do ranking de disco: filho direto + tamanho recursivo.
#[derive(Debug, Clone)]
pub struct DiskEntry {
    pub path: PathBuf,
    pub bytes: u64,
}

pub fn bytes_to_human(bytes: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const KB: f64 = 1024.0;
    let v = bytes as f64;
    if v >= GB {
        format!("{:.2} GB", v / GB)
    } else if v >= MB {
        format!("{:.1} MB", v / MB)
    } else if v >= KB {
        format!("{:.1} KB", v / KB)
    } else {
        format!("{} B", bytes)
    }
}

/// Capacidade e espaço livre da unidade (volume) que contém um caminho.
#[derive(Debug, Clone)]
pub struct VolumeInfo {
    pub mount: String,
    pub total: u64,
    pub free: u64,
}

/// Volume que contém `path`. `None` se o caminho não existir ou estiver em unidade de rede (UNC).
///
/// Consulta só a unidade do caminho (`GetDiskFreeSpaceExW`). Não usa `sysinfo::Disks`, que
/// enumera todas as unidades e chegou a levar mais de 3 minutos com unidade de rede lenta.
#[cfg(windows)]
pub fn volume_info(path: &Path) -> Option<VolumeInfo> {
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows::core::PCWSTR;

    let abs = std::fs::canonicalize(path).ok()?;
    let texto = abs.to_string_lossy().trim_start_matches(r"\\?\").to_string();
    let mut chars = texto.chars();
    let letra = chars.next()?;
    if !letra.is_ascii_alphabetic() || chars.next()? != ':' {
        return None;
    }
    let raiz = format!("{}:\\", letra.to_ascii_uppercase());
    let wide: Vec<u16> = raiz.encode_utf16().chain(std::iter::once(0)).collect();

    let (mut livre_usuario, mut total, mut livre_total) = (0u64, 0u64, 0u64);
    // SAFETY: `wide` termina em NUL e vive até o fim da chamada; os três ponteiros apontam para
    // variáveis locais válidas.
    unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR(wide.as_ptr()),
            Some(&mut livre_usuario as *mut u64),
            Some(&mut total as *mut u64),
            Some(&mut livre_total as *mut u64),
        )
        .ok()?;
    }
    Some(VolumeInfo { mount: raiz, total, free: livre_usuario })
}

#[cfg(not(windows))]
pub fn volume_info(_path: &Path) -> Option<VolumeInfo> {
    None
}

/// Top filhos diretos de `root` por tamanho recursivo (sob demanda — pode demorar).
/// Versão que não cancela: usada pelo comando `winsys disk-top`.
pub fn disk_top(root: &Path, n: usize) -> Vec<DiskEntry> {
    disk_top_cancelavel(root, n, &AtomicBool::new(false)).unwrap_or_default()
}

/// Igual a [`disk_top`], mas consulta `cancel` a cada entrada visitada.
/// Devolve `None` se foi cancelado. Feita para rodar em thread própria (TUI).
pub fn disk_top_cancelavel(root: &Path, n: usize, cancel: &AtomicBool) -> Option<Vec<DiskEntry>> {
    let mut acc: HashMap<PathBuf, u64> = HashMap::new();
    for entry in walkdir::WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let rel = match entry.path().strip_prefix(root) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let top = match rel.components().next() {
            Some(c) => root.join(c),
            None => continue,
        };
        if entry.file_type().is_file() {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            *acc.entry(top).or_insert(0) += size;
        } else {
            acc.entry(top).or_insert(0);
        }
    }
    let mut entries: Vec<DiskEntry> = acc
        .into_iter()
        .map(|(path, bytes)| DiskEntry { path, bytes })
        .collect();
    entries.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    entries.truncate(n.max(1));
    Some(entries)
}

/// Snapshot de serviços para diff antes/depois.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SvcSnapshot {
    pub name: String,
    pub display: String,
    pub state: String,
}

pub fn snapshot_services() -> Result<Vec<SvcSnapshot>, String> {
    Ok(list_services()?
        .into_iter()
        .map(|s| SvcSnapshot {
            name: s.name,
            display: s.display_name,
            state: s.state.label().to_string(),
        })
        .collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvcDiffKind {
    Added,
    Removed,
    Changed,
}

#[derive(Debug, Clone)]
pub struct SvcDiff {
    pub kind: SvcDiffKind,
    pub name: String,
    pub detail: String,
}

/// Compara dois snapshots: adicionados, removidos e com estado alterado.
pub fn diff_snapshots(before: &[SvcSnapshot], after: &[SvcSnapshot]) -> Vec<SvcDiff> {
    let map_b: HashMap<&str, &SvcSnapshot> = before.iter().map(|s| (s.name.as_str(), s)).collect();
    let map_a: HashMap<&str, &SvcSnapshot> = after.iter().map(|s| (s.name.as_str(), s)).collect();
    let mut diffs = Vec::new();
    for (name, a) in &map_a {
        match map_b.get(name) {
            None => diffs.push(SvcDiff {
                kind: SvcDiffKind::Added,
                name: name.to_string(),
                detail: format!("novo: {} ({})", a.display, a.state),
            }),
            Some(b) if b.state != a.state => diffs.push(SvcDiff {
                kind: SvcDiffKind::Changed,
                name: name.to_string(),
                detail: format!("{} → {}", b.state, a.state),
            }),
            _ => {}
        }
    }
    for (name, b) in &map_b {
        if !map_a.contains_key(name) {
            diffs.push(SvcDiff {
                kind: SvcDiffKind::Removed,
                name: name.to_string(),
                detail: format!("removido (era: {} {})", b.display, b.state),
            });
        }
    }
    diffs.sort_by(|a, b| a.name.cmp(&b.name));
    diffs
}

/// Item de inicialização (Run keys, RunOnce, tarefas agendadas).
#[derive(Debug, Clone)]
pub struct BootItem {
    pub source: String,
    pub name: String,
    pub command: String,
}

/// Chaves Run/RunOnce (HKLM + HKCU) — startups clássicas que atrasam boot/login.
pub fn boot_run_keys() -> Vec<BootItem> {
    let mut items = Vec::new();
    let keys = [
        (false, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run"),
        (false, r"SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce"),
        (true, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run"),
        (true, r"SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce"),
    ];
    for (hkcu, key) in keys {
        let values = if hkcu { enum_hkcu_values(key) } else { enum_hklm_values(key) };
        let hive = if hkcu { "HKCU" } else { "HKLM" };
        for (name, cmd) in values {
            items.push(BootItem {
                source: format!("{}\\{}", hive, key.split('\\').last().unwrap_or("Run")),
                name,
                command: cmd,
            });
        }
    }
    items
}

fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    for c in line.chars() {
        match c {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                fields.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    fields.push(cur.trim().to_string());
    fields
}

/// Parseia `schtasks /Query /FO CSV /NH` por posição (TaskName, Next Run, Status).
pub fn parse_schtasks_csv(output: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for line in output.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let f = split_csv_line(t);
        if f.len() < 3 {
            continue;
        }
        // Cabeçalho em qualquer idioma começa com "TaskName" ou o nome da tarefa começa com "\"
        let name = f[0].trim_matches('"').to_string();
        out.push((
            name,
            f[1].trim_matches('"').to_string(),
            f[2].trim_matches('"').to_string(),
        ));
    }
    out
}

/// Tarefas agendadas ativas (para auditoria de boot/login).
pub async fn boot_scheduled_tasks() -> Vec<BootItem> {
    use std::process::Stdio;
    use tokio::process::Command;

    let mut cmd = Command::new("schtasks");
    cmd.args(["/Query", "/FO", "CSV", "/NH"]);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let out = match tokio::time::timeout(std::time::Duration::from_secs(30), cmd.output()).await {
        Ok(Ok(o)) if o.status.success() => o,
        _ => return Vec::new(),
    };
    let txt = String::from_utf8_lossy(&out.stdout).to_string();
    parse_schtasks_csv(&txt)
        .into_iter()
        .filter(|(_, _, status)| {
            let s = status.to_lowercase();
            s.contains("ready") || s.contains("pronta") || s.contains("running") || s.contains("execu")
        })
        .map(|(name, next, status)| BootItem {
            source: "TaskScheduler".to_string(),
            name,
            command: format!("next: {} | {}", next, status),
        })
        .collect()
}
