use crate::modules::top::ProcessResourceInfo;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessItem {
    pub pid: u32,
    pub name: String,
    pub cpu_pct: f32,
    pub memory_bytes: u64,
    pub is_protected: bool,
    #[serde(skip)]
    pub selected: bool,
    #[serde(default)]
    pub exe_path: String,
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub start_time: u64,
    #[serde(default)]
    pub threads: u32,
    #[serde(default)]
    pub parent_pid: Option<u32>,
    #[serde(default)]
    pub gpu_pct: f32,
    #[serde(default)]
    pub vram_bytes: u64,
    #[serde(default)]
    pub disk_read_bytes_sec: u64,
    #[serde(default)]
    pub disk_write_bytes_sec: u64,
    #[serde(default)]
    pub cmd_line: String,
}

impl ProcessItem {
    /// Converte um processo coletado do sistema, já com a classificação de proteção
    pub fn from_resource(info: &ProcessResourceInfo, is_protected: bool, selected: bool) -> Self {
        Self {
            pid: info.pid,
            name: info.name.clone(),
            cpu_pct: info.cpu_pct,
            memory_bytes: info.memory_bytes,
            is_protected,
            selected,
            exe_path: info.exe_path.clone(),
            user: info.user.clone(),
            start_time: info.start_time,
            threads: info.threads,
            parent_pid: info.parent_pid,
            gpu_pct: info.gpu_pct,
            vram_bytes: info.vram_bytes,
            disk_read_bytes_sec: info.disk_read_bytes_sec,
            disk_write_bytes_sec: info.disk_write_bytes_sec,
            cmd_line: info.cmd_line.clone(),
        }
    }

    pub fn memory_mb(&self) -> f64 {
        self.memory_bytes as f64 / (1024.0 * 1024.0)
    }

    pub fn vram_mb(&self) -> f64 {
        self.vram_bytes as f64 / (1024.0 * 1024.0)
    }

    pub fn total_disk_io_kb_sec(&self) -> f64 {
        (self.disk_read_bytes_sec + self.disk_write_bytes_sec) as f64 / 1024.0
    }

    pub fn uptime_seconds(&self) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(self.start_time)
    }

    pub fn formatted_uptime(&self) -> String {
        let secs = self.uptime_seconds();
        let days = secs / 86400;
        let hours = (secs % 86400) / 3600;
        let mins = (secs % 3600) / 60;
        if days > 0 {
            format!("{}d {}h {}m", days, hours, mins)
        } else if hours > 0 {
            format!("{}h {}m", hours, mins)
        } else {
            format!("{}m", mins)
        }
    }
}
