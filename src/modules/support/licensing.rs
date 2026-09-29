use super::winsvc::{find_services, read_hklm_string};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

/// Um checkout ativo de licença FlexNet (linha de `lmstat`).
#[derive(Debug, Clone)]
pub struct LicUser {
    pub user: String,
    pub host: String,
    pub display: String,
    pub version: String,
    pub server_host: String,
    pub server_port: u32,
    pub handle: u32,
    pub checkout: String,
}

/// Uso de uma feature FlexNet.
#[derive(Debug, Clone)]
pub struct LicFeature {
    pub name: String,
    pub total_issued: u32,
    pub total_used: u32,
    pub users: Vec<LicUser>,
}

impl LicFeature {
    pub fn free(&self) -> u32 {
        self.total_issued.saturating_sub(self.total_used)
    }

    pub fn use_pct(&self) -> f32 {
        if self.total_issued == 0 {
            0.0
        } else {
            self.total_used as f32 / self.total_issued as f32 * 100.0
        }
    }
}

/// Extrai `Total of <emitidas>; Total of <em uso>` do cabeçalho
/// `Users of <feat>: (Total of 10 licenses issued; Total of 3 licenses in use)`.
pub fn parse_totals(header: &str) -> Option<(u32, u32)> {
    let lower = header.to_lowercase();
    let issued_marker = "total of ";
    let mut nums = Vec::new();
    let mut rest = lower.as_str();
    while let Some(pos) = rest.find(issued_marker) {
        let after = &rest[pos + issued_marker.len()..];
        let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(n) = digits.parse::<u32>() {
            nums.push(n);
        }
        rest = after;
    }
    if nums.len() >= 2 {
        Some((nums[0], nums[1]))
    } else {
        None
    }
}

/// Parseia uma linha de usuário `lmstat`:
/// `joao WS012 192.168.1.20 (v2025.0) (srvlic/25734 101), start Wed 9/17 9:00`
pub fn parse_user_line(line: &str) -> Option<LicUser> {
    let t = line.trim();
    if !t.contains("(v") || !t.contains("start ") {
        return None;
    }
    let mut parts = t.split_whitespace();
    let user = parts.next()?.to_string();
    let host = parts.next()?.to_string();
    let display = parts.next()?.to_string();
    let version = t
        .split("(v")
        .nth(1)?
        .split(')')
        .next()
        .map(|v| format!("v{}", v))?;
    let srv = t.split('(').nth(2)?.split(')').next()?;
    let (server_host, server_port, handle) = {
        let mut it = srv.split_whitespace();
        let hostport = it.next()?;
        let handle: u32 = it.next()?.parse().ok()?;
        let (h, p) = hostport.rsplit_once('/')?;
        (h.to_string(), p.parse().ok()?, handle)
    };
    let checkout = t.split("start ").nth(1)?.trim().trim_end_matches(',').to_string();
    Some(LicUser {
        user,
        host,
        display,
        version,
        server_host,
        server_port,
        handle,
        checkout,
    })
}

/// Parseia a saída completa de `lmutil lmstat -a` (ou `-f`) em features.
/// Saída do FlexNet é em inglês independente do idioma do SO.
pub fn parse_lmstat(output: &str) -> Vec<LicFeature> {
    let mut features = Vec::new();
    let mut current: Option<LicFeature> = None;

    for line in output.lines() {
        let t = line.trim();
        if t.to_lowercase().starts_with("users of ") {
            if let Some(f) = current.take() {
                features.push(f);
            }
            let name = t["users of ".len()..]
                .split(':')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            let (issued, used) = parse_totals(t).unwrap_or((0, 0));
            current = Some(LicFeature {
                name,
                total_issued: issued,
                total_used: used,
                users: Vec::new(),
            });
        } else if let Some(ref mut f) = current {
            if let Some(u) = parse_user_line(line) {
                f.users.push(u);
            }
        }
    }
    if let Some(f) = current {
        features.push(f);
    }
    features
}

/// Localiza o `lmutil.exe`: config explícita → dir padrão do SNL → PATH.
pub fn discover_lmutil(configured: Option<&str>) -> Option<PathBuf> {
    if let Some(p) = configured {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return Some(pb);
        }
    }
    let candidates = [
        r"C:\Program Files\SOLIDWORKS SolidNetWork License Manager\lmutil.exe",
        r"C:\Program Files (x86)\SOLIDWORKS SolidNetWork License Manager\lmutil.exe",
    ];
    for c in candidates {
        let pb = PathBuf::from(c);
        if pb.exists() {
            return Some(pb);
        }
    }
    // Fallback: diretórios do SNL instalados em versão (…\20XX\)
    if let Ok(entries) = std::fs::read_dir(r"C:\Program Files") {
        for e in entries.flatten() {
            let p = e.path().join("lmutil.exe");
            if p.exists() {
                return Some(p);
            }
        }
    }
    find_on_path("lmutil.exe")
}

fn find_on_path(exe: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let p = dir.join(exe);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

async fn run_lmutil(
    lmutil: &Path,
    args: &[&str],
    timeout_secs: u64,
) -> Result<String, String> {
    let mut cmd = Command::new(lmutil);
    cmd.args(args);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        cmd.output(),
    )
    .await
    .map_err(|_| format!("lmutil excedeu o timeout de {}s", timeout_secs))?
    .map_err(|e| format!("Falha ao invocar lmutil: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if output.status.success() {
        Ok(stdout)
    } else {
        Err(if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        })
    }
}

/// Consulta `lmstat -a` (opcionalmente contra `port@host`) e parseia.
pub async fn query_all(
    lmutil: &Path,
    server: Option<&str>,
) -> Result<Vec<LicFeature>, String> {
    let server_owned;
    let args: Vec<&str> = match server {
        Some(s) => {
            server_owned = s.to_string();
            vec!["lmstat", "-a", "-c", &server_owned]
        }
        None => vec!["lmstat", "-a"],
    };
    let out = run_lmutil(lmutil, &args, 60).await?;
    Ok(parse_lmstat(&out))
}

/// Libera um checkout (`lmremove feature user host display`).
pub async fn free_checkout(
    lmutil: &Path,
    server: Option<&str>,
    feature: &str,
    user: &LicUser,
) -> Result<String, String> {
    let display = if user.display.is_empty() {
        user.host.clone()
    } else {
        user.display.clone()
    };
    let server_owned;
    let mut args = vec!["lmremove", feature, &user.user, &user.host, &display];
    if let Some(s) = server {
        server_owned = s.to_string();
        args.push("-c");
        args.push(&server_owned);
    }
    run_lmutil(lmutil, &args, 60).await
}

/// Um item do checklist de diagnóstico (estilo Technia).
#[derive(Debug, Clone)]
pub struct DiagCheck {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

impl DiagCheck {
    pub fn pass(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self { name: name.into(), ok: true, detail: detail.into() }
    }
    pub fn fail(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self { name: name.into(), ok: false, detail: detail.into() }
    }
}

async fn tcp_check(name: &str, host: &str, port: u16) -> DiagCheck {
    let addr = format!("{}:{}", host, port);
    match tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::net::TcpStream::connect(&addr),
    )
    .await
    {
        Ok(Ok(_)) => DiagCheck::pass(name, format!("{} acessível", addr)),
        _ => DiagCheck::fail(name, format!("sem resposta em {} (firewall/porta?)", addr)),
    }
}

/// Diagnóstico de conexão SNL: serviço, portas 25734/25735 (+daemon),
/// chave de registro e ping básico ao servidor.
pub async fn diagnose_snl(server_host: &str, lm_port: u16, vendor_port: Option<u16>) -> Vec<DiagCheck> {
    let mut checks = Vec::new();

    // 1. Serviço SNL local (quando o servidor é a própria máquina)
    match find_services("solidnetwork") {
        Ok(svcs) => {
            if svcs.is_empty() {
                match find_services("sw_d") {
                    Ok(alt) if !alt.is_empty() => checks.push(DiagCheck::pass(
                        "Serviço de licença",
                        format!("{}: {}", alt[0].display_name, alt[0].state.label()),
                    )),
                    _ => checks.push(DiagCheck::fail(
                        "Serviço de licença",
                        "Nenhum serviço SolidNetWork encontrado localmente (ok se o servidor for remoto)",
                    )),
                }
            } else {
                let s = &svcs[0];
                if s.state.is_running() {
                    checks.push(DiagCheck::pass("Serviço de licença", format!("{} RUNNING", s.display_name)));
                } else {
                    checks.push(DiagCheck::fail("Serviço de licença", format!("{} {}", s.display_name, s.state.label())));
                }
            }
        }
        Err(e) => checks.push(DiagCheck::fail("Serviço de licença", e)),
    }

    // 2. Portas lmgrd + vendor daemon
    checks.push(tcp_check("Porta lmgrd (25734)", server_host, lm_port).await);
    if let Some(vp) = vendor_port {
        checks.push(tcp_check("Porta vendor daemon", server_host, vp).await);
    }

    // 3. Chave de registro do cliente
    match read_hklm_string(r"SOFTWARE\FLEXlm License Manager", "SW_D_LICENSE_FILE") {
        Some(v) if !v.is_empty() => checks.push(DiagCheck::pass("Registro SW_D_LICENSE_FILE", v)),
        _ => checks.push(DiagCheck::fail(
            "Registro SW_D_LICENSE_FILE",
            "chave ausente — cliente sem servidor configurado",
        )),
    }

    // 4. Ping básico
    checks.push(ping_check(server_host).await);

    checks
}

async fn ping_check(host: &str) -> DiagCheck {
    let mut cmd = Command::new("ping");
    cmd.args(["-n", "2", "-w", "1500", host]);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    match tokio::time::timeout(std::time::Duration::from_secs(15), cmd.output()).await {
        Ok(Ok(out)) if out.status.success() => {
            let txt = String::from_utf8_lossy(&out.stdout).to_string();
            let avg = extract_ping_avg(&txt).unwrap_or_else(|| "ok".to_string());
            DiagCheck::pass("Ping", format!("{} responde ({})", host, avg))
        }
        _ => DiagCheck::fail("Ping", format!("{} não responde", host)),
    }
}

/// Extrai a média de forma tolerante a idioma **e** a mojibake de
/// encoding (`M�dia` quando o console CP850 passa por `from_utf8_lossy`).
pub fn extract_ping_avg(output: &str) -> Option<String> {
    for line in output.lines() {
        let l = line.to_lowercase();
        // Windows: "... Average = 3ms" ou "... Média = 3ms" (ou "M�dia = 3ms")
        if (l.contains("average") || l.contains("dia")) && l.contains("ms") {
            if let Some(pos) = l.rfind('=') {
                return Some(line[pos + 1..].trim().to_string());
            }
        }
    }
    None
}

/// Verifica se há regras de firewall mencionando as portas (só leitura).
pub async fn firewall_mentions_ports(ports: &[u16]) -> Result<Vec<String>, String> {
    let mut cmd = Command::new("netsh");
    cmd.args(["advfirewall", "firewall", "show", "rule", "name=all"]);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let out = tokio::time::timeout(std::time::Duration::from_secs(30), cmd.output())
        .await
        .map_err(|_| "netsh excedeu o timeout".to_string())?
        .map_err(|e| format!("Falha ao invocar netsh: {}", e))?;
    let txt = String::from_utf8_lossy(&out.stdout).to_string();
    let mut hits = Vec::new();
    for line in txt.lines() {
        for p in ports {
            if line.contains(&p.to_string())
                && !hits.iter().any(|h: &String| h == line.trim())
            {
                hits.push(line.trim().to_string());
            }
        }
    }
    Ok(hits)
}

/// Status do Siemens PLM License Server (SPLM) via serviços.
pub fn splm_status() -> Vec<(String, String, bool)> {
    let mut out = Vec::new();
    let queries = ["siemens plm license", "splm", "candle"];
    let mut seen = std::collections::HashSet::new();
    for q in queries {
        if let Ok(svcs) = find_services(q) {
            for s in svcs {
                if seen.insert(s.name.clone()) {
                    out.push((s.display_name, s.state.label().to_string(), s.state.is_running()));
                }
            }
        }
    }
    out
}
