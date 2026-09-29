use super::winsvc::{enum_hklm_subkeys, enum_hklm_values, read_hklm_string};
use std::process::Stdio;
use tokio::process::Command;

/// Máscara serial preservando só o último grupo:
/// `9000 0563 2940 6304 BTDH JXJJ` → `**** **** **** **** **** JXJJ`.
pub fn mask_serial(serial: &str) -> String {
    let groups: Vec<&str> = serial.split_whitespace().collect();
    if groups.len() <= 1 {
        return "****".to_string();
    }
    groups
        .iter()
        .enumerate()
        .map(|(i, g)| {
            if i + 1 == groups.len() {
                g.to_string()
            } else {
                "*".repeat(g.len().max(4))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Instalação do SolidWorks encontrada no registro.
#[derive(Debug, Clone)]
pub struct SwInstall {
    pub version_key: String,
    pub install_dir: String,
    pub install_date: String,
    /// (produto, serial mascarado)
    pub serials: Vec<(String, String)>,
}

/// Descobre versões instaladas (`HKLM\SOFTWARE\SolidWorks\SOLIDWORKS 20XX`)
/// com pasta, data e seriais (sempre mascarados).
pub fn sw_installations() -> Vec<SwInstall> {
    let mut out = Vec::new();
    for key in enum_hklm_subkeys(r"SOFTWARE\SolidWorks") {
        let upper = key.to_uppercase();
        if !upper.starts_with("SOLIDWORKS ") {
            continue;
        }
        // Aceita `SOLIDWORKS 2026` (4 dígitos); pula CAM/Inspection/etc.
        let tail = key["SOLIDWORKS ".len()..].trim();
        if tail.len() != 4 || !tail.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let base = format!(r"SOFTWARE\SolidWorks\{}", key);
        let install_dir = read_hklm_string(&format!(r"{}\Setup", base), "SolidWorks Folder")
            .unwrap_or_default();
        let install_date = read_hklm_string(&format!(r"{}\Setup", base), "Install Date Stamp")
            .unwrap_or_default();
        let serials = enum_hklm_values(r"SOFTWARE\SolidWorks\Licenses\Serial Numbers")
            .into_iter()
            .map(|(prod, sn)| (prod, mask_serial(&sn)))
            .collect();
        out.push(SwInstall { version_key: key, install_dir, install_date, serials });
    }
    out
}

/// Adaptador de vídeo (genérico via CIM — cobre NVIDIA/AMD/Intel).
#[derive(Debug, Clone)]
pub struct GpuAdapter {
    pub name: String,
    pub driver_version: String,
    pub ram_mb: u64,
}

/// Heurística honesta de certificação SolidWorks (linha pro vs. consumo).
/// Confirme sempre o build exato no portal de certificação da SW.
pub fn certified_status(name: &str) -> &'static str {
    let l = name.to_lowercase();
    if l.contains("quadro")
        || l.contains("rtx a")
        || l.contains("ada generation")
        || l.contains("radeon pro")
        || l.contains("firepro")
        || l.contains("arc pro")
        || l.contains("arc(tm) pro")
    {
        "LINHA CERTIFICADA (confirme o build no portal SW)"
    } else if l.contains("geforce")
        || l.contains("radeon rx")
        || l.contains("radeon tm")
        || l.contains("uhd")
        || l.contains("iris")
        || l.contains(" arc ")
        || l.starts_with("intel")
    {
        "NÃO CERTIFICADA (uso geral, sem suporte oficial SW)"
    } else {
        "DESCONHECIDA (verifique no portal SW)"
    }
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

/// Lista adaptadores via `Get-CimInstance Win32_VideoController` (CSV posicional).
pub async fn gpu_adapters() -> Vec<GpuAdapter> {
    let mut cmd = Command::new("powershell");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "Get-CimInstance Win32_VideoController | Select-Object Name,DriverVersion,AdapterRAM | ConvertTo-Csv -NoTypeInformation",
    ]);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let out = match tokio::time::timeout(std::time::Duration::from_secs(30), cmd.output()).await {
        Ok(Ok(o)) if o.status.success() => o,
        _ => return Vec::new(),
    };
    let txt = String::from_utf8_lossy(&out.stdout).to_string();
    let mut adapters = Vec::new();
    for (i, line) in txt.lines().enumerate() {
        if i == 0 || line.trim().is_empty() {
            continue; // cabeçalho (ignorado por posição, não por idioma)
        }
        let f = split_csv_line(line);
        if f.len() < 3 || f[0].trim_matches('"').is_empty() {
            continue;
        }
        adapters.push(GpuAdapter {
            name: f[0].trim_matches('"').to_string(),
            driver_version: f[1].trim_matches('"').to_string(),
            ram_mb: f[2].trim_matches('"').parse::<u64>().unwrap_or(0) / (1024 * 1024),
        });
    }
    adapters
}

/// Versão do driver NVIDIA via nvidia-smi (quando presente).
pub async fn nvidia_driver_version() -> Option<String> {
    let mut cmd = Command::new("nvidia-smi");
    cmd.args(["--query-gpu=driver_version", "--format=csv,noheader"]);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let out = tokio::time::timeout(std::time::Duration::from_secs(15), cmd.output())
        .await
        .ok()?
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v = String::from_utf8_lossy(&out.stdout).lines().next()?.trim().to_string();
    if v.is_empty() { None } else { Some(v) }
}

/// Entrada de empréstimo (borrow) de licença encontrada no log do SNL.
#[derive(Debug, Clone)]
pub struct BorrowEntry {
    pub time: String,
    pub feature: String,
    pub user_host: String,
}

/// Parseia linhas de BORROW do debug log do SNL de forma tolerante:
/// `16:20:11 (sw_d) BORROW: "solidworks" joao@WS012 ...`
pub fn parse_borrow_log(text: &str) -> Vec<BorrowEntry> {
    let mut out = Vec::new();
    for line in text.lines() {
        if !line.to_uppercase().contains("BORROW") {
            continue;
        }
        let time = line.split_whitespace().next().unwrap_or("").to_string();
        let feature = line
            .split('"')
            .nth(1)
            .unwrap_or("")
            .to_string();
        let user_host = line
            .split_whitespace()
            .find(|t| t.contains('@'))
            .unwrap_or("")
            .trim_matches(|c| c == '"' || c == ',' || c == ')')
            .to_string();
        if feature.is_empty() && user_host.is_empty() {
            continue;
        }
        out.push(BorrowEntry { time, feature, user_host });
    }
    out
}

/// Localiza o debug log do SNL: config → candidatos padrão.
pub fn discover_snl_log(configured: Option<&str>) -> Option<std::path::PathBuf> {
    if let Some(p) = configured {
        let pb = std::path::PathBuf::from(p);
        if pb.exists() {
            return Some(pb);
        }
    }
    let candidates = [
        r"C:\Program Files\SOLIDWORKS SolidNetWork License Manager\sw_d.log",
        r"C:\Program Files\SOLIDWORKS SolidNetWork License Manager\lmgrd.log",
        r"C:\Program Files (x86)\SOLIDWORKS SolidNetWork License Manager\sw_d.log",
    ];
    for c in candidates {
        let pb = std::path::PathBuf::from(c);
        if pb.exists() {
            return Some(pb);
        }
    }
    None
}

/// Item de limpeza (temporário do SW ou caminho extra configurado).
#[derive(Debug, Clone)]
pub struct CleanItem {
    pub path: std::path::PathBuf,
    pub bytes: u64,
}

fn expand_win(s: &str) -> String {
    let mut out = s.to_string();
    while let Some(start) = out.find('%') {
        let rest = &out[start + 1..];
        if let Some(end) = rest.find('%') {
            let var = &rest[..end];
            if let Ok(val) = std::env::var(var) {
                out = format!("{}{}{}", &out[..start], val, &rest[end + 1..]);
            } else {
                break;
            }
        } else {
            break;
        }
    }
    out
}

/// Alvos padrão de limpeza: `swx*`/`sld*.tmp` no %TEMP% + extras da config.
pub fn default_clean_roots(extra: &[String]) -> Vec<std::path::PathBuf> {
    let mut roots = Vec::new();
    if let Ok(temp) = std::env::var("TEMP") {
        roots.push(std::path::PathBuf::from(temp));
    }
    for e in extra {
        roots.push(std::path::PathBuf::from(expand_win(e)));
    }
    roots
}

/// Varre alvos de limpeza somando tamanhos (somente leitura).
pub fn scan_clean_targets(roots: &[std::path::PathBuf]) -> Vec<CleanItem> {
    let mut items = Vec::new();
    for root in roots {
        let entries = match std::fs::read_dir(root) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            let is_candidate = name.starts_with("swx")
                || (name.starts_with("sld") && name.ends_with(".tmp"));
            if !is_candidate {
                continue;
            }
            let bytes = dir_size(&entry.path());
            items.push(CleanItem { path: entry.path(), bytes });
        }
    }
    items.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    items
}

fn dir_size(path: &std::path::Path) -> u64 {
    if path.is_file() {
        return path.metadata().map(|m| m.len()).unwrap_or(0);
    }
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

/// Remove os itens (chamado somente após confirmação). Retorna (removidos, erros).
pub fn delete_clean_targets(items: &[CleanItem]) -> (usize, Vec<String>) {
    let mut removed = 0usize;
    let mut errors = Vec::new();
    for item in items {
        let res = if item.path.is_dir() {
            std::fs::remove_dir_all(&item.path)
        } else {
            std::fs::remove_file(&item.path)
        };
        match res {
            Ok(_) => removed += 1,
            Err(e) => errors.push(format!("{}: {}", item.path.display(), e)),
        }
    }
    (removed, errors)
}
