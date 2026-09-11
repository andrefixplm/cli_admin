use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortField {
    Cpu,
    Ram,
    DiskIo,
    Gpu,
    Vram,
}

impl SortField {
    pub fn as_str(&self) -> &'static str {
        match self {
            SortField::Cpu => "cpu",
            SortField::Ram => "ram",
            SortField::DiskIo => "io",
            SortField::Gpu => "gpu",
            SortField::Vram => "vram",
        }
    }
}

impl std::str::FromStr for SortField {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "cpu" => Ok(SortField::Cpu),
            "ram" | "mem" | "memory" => Ok(SortField::Ram),
            "io" | "disk" => Ok(SortField::DiskIo),
            "gpu" => Ok(SortField::Gpu),
            "vram" => Ok(SortField::Vram),
            other => Err(format!(
                "Campo de ordenação desconhecido: {}. Opções: cpu, ram, io, gpu, vram",
                other
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessResourceInfo {
    pub pid: u32,
    pub name: String,
    pub cpu_pct: f32,
    pub memory_bytes: u64,
    pub disk_read_bytes_sec: u64,
    pub disk_write_bytes_sec: u64,
    pub gpu_pct: f32,
    pub vram_bytes: u64,
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
}

impl ProcessResourceInfo {
    pub fn memory_mb(&self) -> f64 {
        self.memory_bytes as f64 / (1024.0 * 1024.0)
    }

    pub fn vram_mb(&self) -> f64 {
        self.vram_bytes as f64 / (1024.0 * 1024.0)
    }

    pub fn total_disk_io_kb_sec(&self) -> f64 {
        (self.disk_read_bytes_sec + self.disk_write_bytes_sec) as f64 / 1024.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemResourceOverview {
    pub cpu_pct: f32,
    pub total_ram_bytes: u64,
    pub used_ram_bytes: u64,
    pub total_vram_bytes: u64,
    pub used_vram_bytes: u64,
    pub gpu_name: Option<String>,
    pub gpu_temp_c: Option<u32>,
    pub gpu_power_watts: Option<f32>,
}

impl SystemResourceOverview {
    pub fn ram_usage_pct(&self) -> f32 {
        if self.total_ram_bytes == 0 {
            0.0
        } else {
            (self.used_ram_bytes as f32 / self.total_ram_bytes as f32) * 100.0
        }
    }

    pub fn vram_usage_pct(&self) -> f32 {
        if self.total_vram_bytes == 0 {
            0.0
        } else {
            (self.used_vram_bytes as f32 / self.total_vram_bytes as f32) * 100.0
        }
    }
}
