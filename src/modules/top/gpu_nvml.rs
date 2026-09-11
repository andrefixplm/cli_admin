use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, warn};

#[derive(Debug, Clone, Default)]
pub struct NvidiaGpuInfo {
    pub name: Option<String>,
    pub temperature_c: Option<u32>,
    pub power_watts: Option<f32>,
    pub memory_total_bytes: u64,
    pub memory_used_bytes: u64,
}

pub struct NvidiaCollector;

impl NvidiaCollector {
    /// Consulta telemetria global da GPU NVIDIA via nvidia-smi
    pub async fn query_global_telemetry() -> Option<NvidiaGpuInfo> {
        let mut cmd = Command::new("nvidia-smi");
        cmd.args([
            "--query-gpu=name,temperature.gpu,power.draw,memory.total,memory.used",
            "--format=csv,noheader,nounits",
        ]);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = match cmd.output().await {
            Ok(out) if out.status.success() => out,
            Ok(out) => {
                debug!(
                    "nvidia-smi retornou código não-zero: {:?}",
                    out.status.code()
                );
                return None;
            }
            Err(_) => {
                debug!("nvidia-smi não encontrado no PATH");
                return None;
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let first_line = stdout.lines().next()?;
        let parts: Vec<&str> = first_line.split(',').map(|s| s.trim()).collect();

        if parts.len() < 5 {
            warn!("Formato de saída inesperado do nvidia-smi: {}", first_line);
            return None;
        }

        let name = Some(parts[0].to_string());
        let temperature_c = parts[1].parse::<u32>().ok();
        let power_watts = parts[2].parse::<f32>().ok();
        let memory_total_mib = parts[3].parse::<u64>().unwrap_or(0);
        let memory_used_mib = parts[4].parse::<u64>().unwrap_or(0);

        Some(NvidiaGpuInfo {
            name,
            temperature_c,
            power_watts,
            memory_total_bytes: memory_total_mib * 1024 * 1024,
            memory_used_bytes: memory_used_mib * 1024 * 1024,
        })
    }

    /// Consulta uso de VRAM de processos dedicados via nvidia-smi (compute/graphics apps)
    #[allow(dead_code)]
    pub async fn query_process_vram() -> Vec<(u32, u64)> {
        let mut cmd = Command::new("nvidia-smi");
        cmd.args([
            "--query-compute-apps=pid,used_memory",
            "--format=csv,noheader,nounits",
        ]);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut results = Vec::new();
        if let Ok(output) = cmd.output().await {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
                    if parts.len() >= 2 {
                        if let (Ok(pid), Ok(mib)) =
                            (parts[0].parse::<u32>(), parts[1].parse::<u64>())
                        {
                            results.push((pid, mib * 1024 * 1024));
                        }
                    }
                }
            }
        }
        results
    }
}
