use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub vmware: VmwareConfig,
    #[serde(default)]
    pub metrics: MetricsConfig,
    #[serde(default)]
    pub security: SecurityConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            vmware: VmwareConfig::default(),
            metrics: MetricsConfig::default(),
            security: SecurityConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_refresh_rate")]
    pub refresh_rate_ms: u64,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_log_file")]
    pub log_file: String,
    #[serde(default)]
    pub log_to_stdout: bool,
}

fn default_refresh_rate() -> u64 {
    1000
}
fn default_log_level() -> String {
    "info".to_string()
}
fn default_log_file() -> String {
    "admin.log".to_string()
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            refresh_rate_ms: default_refresh_rate(),
            log_level: default_log_level(),
            log_file: default_log_file(),
            log_to_stdout: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmwareConfig {
    #[serde(default = "default_vmrun_path")]
    pub vmrun_path: String,
    #[serde(default = "default_start_mode")]
    pub default_start_mode: String,
    #[serde(default = "default_stop_mode")]
    pub default_stop_mode: String,
    #[serde(default = "default_scan_directories")]
    pub scan_directories: Vec<PathBuf>,
    #[serde(default)]
    pub vms: Vec<VmConfigEntry>,
}

fn default_vmrun_path() -> String {
    "C:\\Program Files (x86)\\VMware\\VMware Workstation\\vmrun.exe".to_string()
}
fn default_start_mode() -> String {
    "nogui".to_string()
}
fn default_stop_mode() -> String {
    "soft".to_string()
}
fn default_scan_directories() -> Vec<PathBuf> {
    vec![
        PathBuf::from("D:\\VMs"),
        dirs::document_dir()
            .unwrap_or_else(|| PathBuf::from("C:\\"))
            .join("Virtual Machines"),
    ]
}

impl Default for VmwareConfig {
    fn default() -> Self {
        Self {
            vmrun_path: default_vmrun_path(),
            default_start_mode: default_start_mode(),
            default_stop_mode: default_stop_mode(),
            scan_directories: default_scan_directories(),
            vms: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmConfigEntry {
    pub alias: String,
    pub name: Option<String>,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    #[serde(default = "default_sort_field")]
    pub default_sort: String,
    #[serde(default = "default_top_count")]
    pub top_process_count: usize,
    #[serde(default = "default_true")]
    pub enable_gpu_metrics: bool,
    #[serde(default = "default_true")]
    pub enable_nvml: bool,
}

fn default_sort_field() -> String {
    "vram".to_string()
}
fn default_top_count() -> usize {
    30
}
fn default_true() -> bool {
    true
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            default_sort: default_sort_field(),
            top_process_count: default_top_count(),
            enable_gpu_metrics: true,
            enable_nvml: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default = "default_protected_processes")]
    pub protected_processes: Vec<String>,
    #[serde(default = "default_true")]
    pub require_kill_confirmation: bool,
}

fn default_protected_processes() -> Vec<String> {
    vec![
        "csrss.exe".to_string(),
        "svchost.exe".to_string(),
        "explorer.exe".to_string(),
        "winlogon.exe".to_string(),
        "services.exe".to_string(),
        "lsass.exe".to_string(),
        "smss.exe".to_string(),
        "system".to_string(),
        "registry".to_string(),
        "dwm.exe".to_string(),
        "fontdrvhost.exe".to_string(),
    ]
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            protected_processes: default_protected_processes(),
            require_kill_confirmation: true,
        }
    }
}

impl AppConfig {
    /// Carrega as configurações seguindo a ordem de precedência:
    /// 1. Caminho explícito fornecido pelo usuário (--config)
    /// 2. `./config.toml` no diretório atual
    /// 3. `<diretório_do_executável>/config.toml`
    /// 4. `%APPDATA%/cli_admin/config.toml`
    /// Se nenhum arquivo for encontrado, cria `./config.toml` com valores padrão.
    pub fn load(custom_path: Option<&Path>) -> Result<(Self, PathBuf), Box<dyn std::error::Error>> {
        if let Some(path) = custom_path {
            if path.exists() {
                let content = fs::read_to_string(path)?;
                let cfg: AppConfig = toml::from_str(&content)?;
                return Ok((cfg, path.to_path_buf()));
            } else {
                warn!("Arquivo de configuração especificado não encontrado: {:?}", path);
            }
        }

        let candidates = [
            PathBuf::from("config.toml"),
            PathBuf::from("admin.toml"),
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("config.toml")))
                .unwrap_or_else(|| PathBuf::from("config.toml")),
            dirs::config_dir()
                .map(|p| p.join("cli_admin").join("config.toml"))
                .unwrap_or_else(|| PathBuf::from("config.toml")),
        ];

        for path in &candidates {
            if path.exists() {
                info!("Carregando configuração de: {:?}", path);
                let content = fs::read_to_string(path)?;
                let cfg: AppConfig = toml::from_str(&content)?;
                return Ok((cfg, path.clone()));
            }
        }

        // Criar template padrão se não encontrado
        let default_path = PathBuf::from("config.toml");
        info!("Nenhum arquivo de configuração encontrado. Criando template padrão em: {:?}", default_path);
        let default_cfg = AppConfig::default();
        let serialized = toml::to_string_pretty(&default_cfg)?;
        let _ = fs::write(&default_path, serialized);

        Ok((default_cfg, default_path))
    }

    /// Verifica se já existe uma VM cadastrada com o alias informado (case-insensitive)
    pub fn has_alias(&self, alias: &str) -> bool {
        self.vmware
            .vms
            .iter()
            .any(|v| v.alias.eq_ignore_ascii_case(alias))
    }

    /// Anexa uma nova entrada `[[vmware.vms]]` ao final do arquivo de configuração,
    /// sem reescrever o restante do arquivo — preserva comentários e formatação existentes.
    pub fn append_vm_entry(config_path: &Path, entry: &VmConfigEntry) -> std::io::Result<()> {
        use std::io::Write;

        let mut block = String::new();
        block.push_str("\n[[vmware.vms]]\n");
        block.push_str(&format!("alias = {}\n", toml_escape(&entry.alias)));
        if let Some(name) = &entry.name {
            block.push_str(&format!("name = {}\n", toml_escape(name)));
        }
        block.push_str(&format!(
            "path = {}\n",
            toml_escape(&entry.path.to_string_lossy())
        ));

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(config_path)?;
        file.write_all(block.as_bytes())
    }
}

/// Escapa uma string para uso como TOML basic string (barra invertida e aspas duplas)
fn toml_escape(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toml_escape_windows_path() {
        assert_eq!(
            toml_escape(r"D:\VMs\TC14\tc14.vmx"),
            r#""D:\\VMs\\TC14\\tc14.vmx""#
        );
    }

    #[test]
    fn test_toml_escape_quotes() {
        assert_eq!(toml_escape(r#"foo "bar""#), r#""foo \"bar\"""#);
    }

    #[test]
    fn test_has_alias_case_insensitive() {
        let mut cfg = AppConfig::default();
        cfg.vmware.vms.push(VmConfigEntry {
            alias: "TC-DCTA".to_string(),
            name: None,
            path: PathBuf::from(r"D:\VM\TC_DCTA\Windows_Server_2019.vmx"),
        });
        assert!(cfg.has_alias("tc-dcta"));
        assert!(cfg.has_alias("TC-DCTA"));
        assert!(!cfg.has_alias("outra-vm"));
    }
}
