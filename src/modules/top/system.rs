use super::gpu_nvml::NvidiaGpuInfo;
use super::gpu_pdh::ProcessGpuMetrics;
use super::models::{ProcessResourceInfo, SortField, SystemResourceOverview};
use std::collections::HashMap;
use std::ffi::OsString;
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

/// Campos de processo que a aplicação usa. `everything()` também lia `environ`, `cwd` e `root`,
/// que nenhuma tela usa, a cada tick. Campos estáticos (`exe`, `user`, `cmd`) só são lidos
/// enquanto ainda estão vazios: o caminho, o dono e a linha de comando de um processo não mudam.
fn proc_refresh_kind() -> ProcessRefreshKind {
    ProcessRefreshKind::nothing()
        .with_cpu()
        .with_memory()
        .with_disk_usage()
        .with_exe(UpdateKind::OnlyIfNotSet)
        .with_user(UpdateKind::OnlyIfNotSet)
        .with_cmd(UpdateKind::OnlyIfNotSet)
}

pub struct SystemMetricsManager {
    system: System,
}

impl SystemMetricsManager {
    pub fn new() -> Self {
        let mut system = System::new();
        // Carga inicial do sistema
        system.refresh_cpu_all();
        system.refresh_memory();
        system.refresh_processes_specifics(ProcessesToUpdate::All, true, proc_refresh_kind());
        Self { system }
    }

    /// Atualiza os contadores de CPU, memória e processos do sistema
    pub fn refresh(&mut self) {
        self.system.refresh_cpu_specifics(CpuRefreshKind::everything());
        self.system.refresh_memory_specifics(MemoryRefreshKind::everything());
        self.system.refresh_processes_specifics(ProcessesToUpdate::All, true, proc_refresh_kind());
    }

    /// Coleta a visão geral do sistema (CPU global, RAM e GPU)
    pub fn get_overview(
        &self,
        nvidia_info: Option<&NvidiaGpuInfo>,
        cached_gpu_metrics: &HashMap<u32, ProcessGpuMetrics>,
    ) -> SystemResourceOverview {
        let cpu_pct = self.system.global_cpu_usage();
        let total_ram_bytes = self.system.total_memory();
        let used_ram_bytes = self.system.used_memory();

        let sum_vram_from_processes: u64 =
            cached_gpu_metrics.values().map(|m| m.vram_bytes).sum();

        let (total_vram, used_vram, gpu_name, temp, power) = match nvidia_info {
            Some(info) if info.memory_total_bytes > 0 => (
                info.memory_total_bytes,
                if info.memory_used_bytes > 0 {
                    info.memory_used_bytes
                } else {
                    sum_vram_from_processes
                },
                info.name.clone(),
                info.temperature_c,
                info.power_watts,
            ),
            _ => (
                // Estimar ou usar soma dos processos se não houver driver NVIDIA
                sum_vram_from_processes.max(4 * 1024 * 1024 * 1024),
                sum_vram_from_processes,
                Some("Windows Graphics Adapter".to_string()),
                None,
                None,
            ),
        };

        SystemResourceOverview {
            cpu_pct,
            total_ram_bytes,
            used_ram_bytes,
            total_vram_bytes: total_vram,
            used_vram_bytes: used_vram,
            gpu_name,
            gpu_temp_c: temp,
            gpu_power_watts: power,
        }
    }

    /// Retorna a lista de processos ordenados pelo campo escolhido (CPU, RAM, IO, GPU, VRAM)
    pub fn get_top_processes(
        &self,
        sort_by: SortField,
        limit: usize,
        gpu_metrics: &HashMap<u32, ProcessGpuMetrics>,
    ) -> Vec<ProcessResourceInfo> {
        let mut list = self.get_all_processes(gpu_metrics);

        match sort_by {
            SortField::Cpu => {
                list.sort_by(|a, b| b.cpu_pct.partial_cmp(&a.cpu_pct).unwrap_or(std::cmp::Ordering::Equal));
            }
            SortField::Ram => {
                list.sort_by(|a, b| b.memory_bytes.cmp(&a.memory_bytes));
            }
            SortField::DiskIo => {
                list.sort_by(|a, b| {
                    (b.disk_read_bytes_sec + b.disk_write_bytes_sec)
                        .cmp(&(a.disk_read_bytes_sec + a.disk_write_bytes_sec))
                });
            }
            SortField::Gpu => {
                list.sort_by(|a, b| b.gpu_pct.partial_cmp(&a.gpu_pct).unwrap_or(std::cmp::Ordering::Equal));
            }
            SortField::Vram => {
                list.sort_by(|a, b| b.vram_bytes.cmp(&a.vram_bytes));
            }
        }

        if limit > 0 && list.len() > limit {
            list.truncate(limit);
        }

        list
    }

    /// Retorna TODOS os processos do sistema (para o gerenciador de processos completo)
    pub fn get_all_processes(
        &self,
        gpu_metrics: &HashMap<u32, ProcessGpuMetrics>,
    ) -> Vec<ProcessResourceInfo> {
        self.system
            .processes()
            .iter()
            .map(|(pid, proc)| {
                let pid_u32 = pid.as_u32();
                let (gpu_pct, vram_bytes) = match gpu_metrics.get(&pid_u32) {
                    Some(m) => (m.gpu_pct, m.vram_bytes),
                    None => (0.0, 0),
                };

                let disk = proc.disk_usage();
                let exe_path = proc.exe()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                let user = proc.user_id().map(|u| u.to_string()).unwrap_or_default();
                let start_time = proc.start_time();
                let parent_pid = proc.parent().map(|p| p.as_u32());
                let cmd_line = format_cmd_line(proc.cmd());

                ProcessResourceInfo {
                    pid: pid_u32,
                    name: proc.name().to_string_lossy().to_string(),
                    cpu_pct: proc.cpu_usage(),
                    memory_bytes: proc.memory(),
                    disk_read_bytes_sec: disk.read_bytes,
                    disk_write_bytes_sec: disk.written_bytes,
                    gpu_pct,
                    vram_bytes,
                    exe_path,
                    user,
                    start_time,
                    threads: 0,
                    parent_pid,
                    cmd_line,
                }
            })
            .collect()
    }
}

/// Monta a linha de comando a partir dos argumentos do processo, colocando entre aspas
/// os argumentos que têm espaço ou aspas (ex.: `"C:\Program Filespp.exe" --flag`)
pub fn format_cmd_line(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| {
            let text = arg.to_string_lossy();
            if text.is_empty() || text.contains(char::is_whitespace) || text.contains('"') {
                format!("\"{}\"", text.replace('"', "\\\""))
            } else {
                text.into_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
