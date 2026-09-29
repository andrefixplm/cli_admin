use std::net::{IpAddr, Ipv4Addr};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::process::Command;

/// Estatística de uma série de ping.
#[derive(Debug, Clone, Default)]
pub struct PingStats {
    pub host: String,
    pub sent: u32,
    pub received: u32,
    pub min_ms: Option<u32>,
    pub max_ms: Option<u32>,
    pub avg_ms: Option<u32>,
}

impl PingStats {
    pub fn loss_pct(&self) -> f32 {
        if self.sent == 0 {
            0.0
        } else {
            (self.sent - self.received) as f32 / self.sent as f32 * 100.0
        }
    }

    pub fn summary(&self) -> String {
        format!(
            "{}/{} respostas, perda {:.0}%, min/avg/max {}/{}/{} ms",
            self.received,
            self.sent,
            self.loss_pct(),
            self.min_ms.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string()),
            self.avg_ms.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string()),
            self.max_ms.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string()),
        )
    }
}

fn first_u32(s: &str) -> Option<u32> {
    let mut digits = String::new();
    let mut started = false;
    for c in s.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
            started = true;
        } else if started {
            break;
        }
    }
    digits.parse().ok()
}

/// Parseia o resumo do `ping` tolerando PT-BR, EN **e** mojibake de
/// encoding (console CP850 vira `M�nimo` após `from_utf8_lossy`).
/// Por isso casa por fragmentos sem acento (`nimo`, `ximo`, `dia`).
pub fn parse_ping_summary(output: &str) -> (Option<u32>, Option<u32>, Option<u32>) {
    let mut min = None;
    let mut max = None;
    let mut avg = None;
    for line in output.lines() {
        let l = line.to_lowercase();
        let is_summary = (l.contains("minimum") || l.contains("nimo"))
            && (l.contains("average") || l.contains("dia"));
        if !is_summary {
            continue;
        }
        for chunk in l.split(',') {
            let c = chunk.trim();
            if c.starts_with("minimum") || (c.contains("nimo") && !c.contains("ximo")) {
                min = first_u32(c);
            } else if c.starts_with("maximum") || c.contains("ximo") {
                max = first_u32(c);
            } else if c.starts_with("average") || c.contains("dia") {
                avg = first_u32(c);
            }
        }
    }
    (min, max, avg)
}

/// Conta pacotes: linhas `Resposta de`/`Reply from` e `Esgotado`/`Request timed out`.
pub fn parse_ping_counts(output: &str) -> (u32, u32) {
    let mut received = 0u32;
    let mut sent = 0u32;
    for line in output.lines() {
        let l = line.to_lowercase();
        if l.contains("resposta de") || l.contains("reply from") {
            received += 1;
        }
        if l.contains("esgotado o tempo") || l.contains("request timed out")
            || l.contains("falha geral") || l.contains("general failure")
            || l.contains("host de destino inacess") || l.contains("destination host unreachable")
        {
            sent += 1;
        }
    }
    // Total enviado = recebidos + perdidos (aproximação robusta sem parse de cabeçalho)
    sent += received;
    (sent, received)
}

/// Executa `ping -n <count>` e agrega estatísticas.
pub async fn ping_series(host: &str, count: u32) -> PingStats {
    let mut cmd = Command::new("ping");
    cmd.args(["-n", &count.to_string(), "-w", "1500", host]);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let mut stats = PingStats { host: host.to_string(), ..Default::default() };
    let out = match tokio::time::timeout(Duration::from_secs(10 + count as u64 * 2), cmd.output()).await {
        Ok(Ok(o)) => o,
        _ => return stats,
    };
    let txt = String::from_utf8_lossy(&out.stdout).to_string();
    let (sent, received) = parse_ping_counts(&txt);
    let (min, max, avg) = parse_ping_summary(&txt);
    stats.sent = sent;
    stats.received = received;
    stats.min_ms = min;
    stats.max_ms = max;
    stats.avg_ms = avg;
    stats
}

/// Resultado de um port probe (tempo de connect TCP).
#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub host: String,
    pub port: u16,
    pub open: bool,
    pub elapsed_ms: u128,
}

/// Testa host:porta com timeout (substitui telnet manual).
pub async fn probe_port(host: &str, port: u16, timeout_ms: u64) -> ProbeResult {
    let addr = format!("{}:{}", host, port);
    let start = Instant::now();
    let open = tokio::time::timeout(
        Duration::from_millis(timeout_ms),
        tokio::net::TcpStream::connect(&addr),
    )
    .await
    .map(|r| r.is_ok())
    .unwrap_or(false);
    ProbeResult { host: host.to_string(), port, open, elapsed_ms: start.elapsed().as_millis() }
}

/// Entrada da tabela ARP (mapeamento IP ↔ MAC da LAN).
#[derive(Debug, Clone)]
pub struct ArpEntry {
    pub ip: String,
    pub mac: String,
    pub entry_type: u32,
}

pub fn arp_type_label(t: u32) -> &'static str {
    match t {
        2 => "inválida",
        3 => "dinâmica",
        4 => "estática",
        _ => "outra",
    }
}

/// Parseia o buffer de `GetIpNetTable`: DWORD count + linhas de 24 bytes
/// (ifIndex, physLen, mac[8], addr, type).
pub fn parse_ipnet_table(buf: &[u8]) -> Vec<ArpEntry> {
    if buf.len() < 4 {
        return Vec::new();
    }
    let n = u32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    let mut out = Vec::new();
    for i in 0..n {
        let off = 4 + i * 24;
        if off + 24 > buf.len() {
            break;
        }
        let mac_len = u32::from_ne_bytes([buf[off + 4], buf[off + 5], buf[off + 6], buf[off + 7]]) as usize;
        let mac_len = mac_len.min(8);
        let mac = buf[off + 8..off + 8 + mac_len]
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<Vec<_>>()
            .join("-");
        let addr = u32::from_ne_bytes([buf[off + 16], buf[off + 17], buf[off + 18], buf[off + 19]]);
        let ip = Ipv4Addr::from(addr.to_ne_bytes()).to_string();
        let etype = u32::from_ne_bytes([buf[off + 20], buf[off + 21], buf[off + 22], buf[off + 23]]);
        if mac.is_empty() {
            continue;
        }
        out.push(ArpEntry { ip, mac, entry_type: etype });
    }
    out
}

#[cfg(windows)]
pub fn read_arp_table() -> Vec<ArpEntry> {
    use windows::Win32::NetworkManagement::IpHelper::{GetIpNetTable, MIB_IPNETTABLE};

    unsafe {
        let mut size: u32 = 0;
        let _ = GetIpNetTable(None, &mut size, false);
        if size == 0 {
            return Vec::new();
        }
        let mut buf = vec![0u8; size as usize];
        if GetIpNetTable(
            Some(buf.as_mut_ptr() as *mut MIB_IPNETTABLE),
            &mut size,
            true,
        ) != 0
        {
            return Vec::new();
        }
        if (size as usize) < buf.len() {
            buf.truncate(size as usize);
        }
        parse_ipnet_table(&buf)
    }
}

#[cfg(not(windows))]
pub fn read_arp_table() -> Vec<ArpEntry> {
    Vec::new()
}

/// Expande `192.168.1.0/24` em lista de IPs (limite de segurança em /16).
pub fn expand_subnet(cidr: &str) -> Result<Vec<Ipv4Addr>, String> {
    let (base_s, prefix_s) = cidr
        .split_once('/')
        .ok_or_else(|| "Use o formato A.B.C.D/P (ex: 192.168.1.0/24)".to_string())?;
    let base: Ipv4Addr = base_s.parse().map_err(|_| format!("IP inválido: {}", base_s))?;
    let prefix: u8 = prefix_s.parse().map_err(|_| format!("Prefixo inválido: {}", prefix_s))?;
    if !(16..=30).contains(&prefix) {
        return Err("Prefixo fora do intervalo suportado (/16 a /30)".to_string());
    }
    let base_u = u32::from(base);
    let mask = u32::MAX << (32 - prefix);
    let net = base_u & mask;
    let count = 2u32.pow(32 - prefix as u32);
    Ok((1..count - 1)
        .map(|i| Ipv4Addr::from(net + i))
        .collect())
}

/// Varredura TCP de uma subnet nas portas dadas (concorrência limitada).
pub async fn sweep_subnet(
    cidr: &str,
    ports: &[u16],
    timeout_ms: u64,
    max_concurrency: usize,
) -> Result<Vec<(String, u16)>, String> {
    let ips = expand_subnet(cidr)?;
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(max_concurrency.max(1)));
    let mut tasks = Vec::new();
    for ip in ips {
        for &port in ports {
            let s = sem.clone();
            tasks.push(tokio::spawn(async move {
                let _permit = s.acquire_owned().await.ok()?;
                let r = probe_port(&ip.to_string(), port, timeout_ms).await;
                if r.open {
                    Some((r.host, r.port))
                } else {
                    None
                }
            }));
        }
    }
    let mut open = Vec::new();
    for t in tasks {
        if let Ok(Some(hit)) = t.await {
            open.push(hit);
        }
    }
    open.sort();
    Ok(open)
}

/// Resolução DNS via resolvedor do sistema.
pub async fn resolve_host(host: &str) -> Vec<IpAddr> {
    match tokio::net::lookup_host(format!("{}:80", host)).await {
        Ok(addrs) => {
            let mut ips: Vec<IpAddr> = addrs.map(|a| a.ip()).collect();
            ips.sort();
            ips.dedup();
            ips
        }
        Err(_) => Vec::new(),
    }
}

/// Gera os comandos `ntttcp` (sender/receiver) para teste de throughput.
/// O binário é externo (Microsoft) — nunca bundlado.
pub fn ntttcp_commands(role: &str, peer_ip: &str, secs: u64, threads: u32) -> Vec<String> {
    let recv = format!("ntttcp -r -m {},*,{} -t {}", threads, peer_ip, secs);
    let send = format!("ntttcp -s -m {},*,{} -t {}", threads, peer_ip, secs);
    match role.to_lowercase().as_str() {
        "receiver" | "receptor" | "r" => vec![recv],
        "sender" | "transmissor" | "s" => vec![send],
        _ => vec![recv, send],
    }
}

/// Localiza o ntttcp.exe: config → PATH → pastas comuns de ferramentas.
pub fn discover_ntttcp(configured: Option<&str>) -> Option<std::path::PathBuf> {
    if let Some(p) = configured {
        let pb = std::path::PathBuf::from(p);
        if pb.exists() {
            return Some(pb);
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let p = dir.join("ntttcp.exe");
            if p.exists() {
                return Some(p);
            }
        }
    }
    None
}

/// Extrai throughput ("1234.5 Mbps") da saída do ntttcp de forma tolerante.
pub fn parse_ntttcp_throughput(output: &str) -> Option<f64> {
    let mut best = None;
    for line in output.lines() {
        let l = line.to_lowercase();
        if !l.contains("mbps") && !l.contains("gbps") {
            continue;
        }
        let is_gbps = l.contains("gbps");
        let mut num = String::new();
        let mut chars = l.chars().peekable();
        while let Some(c) = chars.next() {
            if c.is_ascii_digit() || c == '.' || c == ',' {
                num.push(c);
                // captura o número imediatamente anterior a mbps/gbps
                let rest: String = chars.clone().take(12).collect();
                if rest.trim_start_matches([' ', '.', ',']).starts_with("mbps")
                    || rest.trim_start_matches([' ', '.', ',']).starts_with("gbps")
                {
                    break;
                }
            } else {
                num.clear();
            }
        }
        if let Ok(mut v) = num.replace(',', ".").parse::<f64>() {
            if is_gbps {
                v *= 1000.0;
            }
            best = Some(v);
        }
    }
    best
}

/// Linha CSV de evidência para série de ping.
pub fn ping_csv_row(s: &PingStats) -> String {
    format!(
        "{},{},{},{:.1},{},{},{}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        s.host,
        s.sent,
        s.loss_pct(),
        s.min_ms.map(|v| v.to_string()).unwrap_or_default(),
        s.avg_ms.map(|v| v.to_string()).unwrap_or_default(),
        s.max_ms.map(|v| v.to_string()).unwrap_or_default(),
    )
}
