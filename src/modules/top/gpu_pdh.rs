use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::RwLock;
use tracing::{debug, warn};

#[derive(Debug, Clone, Default)]
pub struct ProcessGpuMetrics {
    pub gpu_pct: f32,
    pub vram_bytes: u64,
}

#[derive(Clone)]
pub struct GpuPdhCollector {
    cache: Arc<RwLock<HashMap<u32, ProcessGpuMetrics>>>,
    #[allow(dead_code)]
    enabled: bool,
}

impl GpuPdhCollector {
    pub fn new(enabled: bool) -> Self {
        let collector = Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            enabled,
        };

        if enabled {
            let cache_clone = Arc::clone(&collector.cache);
            tokio::spawn(async move {
                Self::background_worker(cache_clone).await;
            });
        }

        collector
    }

    /// Retorna as métricas cacheadas de GPU por PID
    pub async fn get_metrics(&self) -> HashMap<u32, ProcessGpuMetrics> {
        self.cache.read().await.clone()
    }

    /// Retorna as métricas cacheadas de GPU por PID (versão síncrona, pode retornar dados stale)
    pub fn get_metrics_sync(&self) -> HashMap<u32, ProcessGpuMetrics> {
        self.cache.try_read().map(|r| r.clone()).unwrap_or_default()
    }

    /// Loop em background que coleta métricas sem bloquear a interface TUI
    async fn background_worker(cache: Arc<RwLock<HashMap<u32, ProcessGpuMetrics>>>) {
        debug!("Iniciando background worker para métricas PDH de GPU");
        let mut interval = tokio::time::interval(Duration::from_millis(2000));

        loop {
            interval.tick().await;

            // 1. Coletar VRAM (Local Usage)
            let vram_map = Self::query_vram_sample().await;

            // 2. Coletar GPU Engine %
            let gpu_map = Self::query_gpu_engine_sample().await;

            // 3. Consolidar no mapa de métricas
            let mut combined = HashMap::new();
            for (pid, vram) in vram_map {
                combined.insert(
                    pid,
                    ProcessGpuMetrics {
                        gpu_pct: 0.0,
                        vram_bytes: vram,
                    },
                );
            }

            for (pid, gpu_pct) in gpu_map {
                combined
                    .entry(pid)
                    .and_modify(|m| m.gpu_pct = gpu_pct)
                    .or_insert(ProcessGpuMetrics {
                        gpu_pct,
                        vram_bytes: 0,
                    });
            }

            if !combined.is_empty() {
                let mut lock = cache.write().await;
                *lock = combined;
                debug!("Cache de GPU/VRAM atualizado com sucesso");
            }
        }
    }

    /// Consulta VRAM por processo via typeperf
    async fn query_vram_sample() -> HashMap<u32, u64> {
        let mut cmd = Command::new("typeperf");
        cmd.args([r"\GPU Process Memory(*)\Local Usage", "-sc", "1"]);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = match cmd.output().await {
            Ok(out) if out.status.success() => out,
            Ok(_) => return HashMap::new(),
            Err(e) => {
                debug!("Não foi possível executar typeperf para VRAM: {}", e);
                return HashMap::new();
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        Self::parse_typeperf_csv(&stdout)
            .into_iter()
            .map(|(pid, val)| (pid, val as u64))
            .collect()
    }

    /// Consulta uso de GPU Engine % por processo via typeperf
    async fn query_gpu_engine_sample() -> HashMap<u32, f32> {
        let mut cmd = Command::new("typeperf");
        cmd.args([r"\GPU Engine(*)\Utilization Percentage", "-sc", "1"]);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = match cmd.output().await {
            Ok(out) if out.status.success() => out,
            Ok(_) => return HashMap::new(),
            Err(e) => {
                debug!("Não foi possível executar typeperf para GPU: {}", e);
                return HashMap::new();
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        Self::parse_typeperf_csv(&stdout)
            .into_iter()
            .map(|(pid, val)| (pid, val as f32))
            .collect()
    }

    /// Parser genérico de saída CSV do typeperf mapeando pid_... para valores numéricos acumulados
    fn parse_typeperf_csv(csv_content: &str) -> HashMap<u32, f64> {
        let mut lines = csv_content.lines();
        let mut header_line = None;
        let mut value_line = None;

        for line in lines.by_ref() {
            let trimmed = line.trim();
            if trimmed.starts_with("\"(PDH-CSV") {
                header_line = Some(trimmed);
                break;
            }
        }

        for line in lines {
            let trimmed = line.trim();
            if trimmed.starts_with('"') && !trimmed.starts_with("\"(PDH-CSV") {
                value_line = Some(trimmed);
                break;
            }
        }

        let (header, values) = match (header_line, value_line) {
            (Some(h), Some(v)) => (h, v),
            _ => return HashMap::new(),
        };

        let headers: Vec<&str> = header.split("\",\"").collect();
        let val_tokens: Vec<&str> = values.split("\",\"").collect();

        if headers.len() != val_tokens.len() {
            warn!("Cabeçalhos e valores do typeperf com contagens divergentes");
            return HashMap::new();
        }

        let mut map: HashMap<u32, f64> = HashMap::new();

        for i in 1..headers.len() {
            let col = headers[i];
            let raw_val = val_tokens[i].trim_matches('"');
            let val = match raw_val.parse::<f64>() {
                Ok(v) => v,
                Err(_) => continue,
            };

            if let Some(idx) = col.find("pid_") {
                let rest = &col[idx + 4..];
                if let Some(end_idx) = rest.find('_') {
                    if let Ok(pid) = rest[..end_idx].parse::<u32>() {
                        *map.entry(pid).or_insert(0.0) += val;
                    }
                }
            }
        }

        map
    }
}
