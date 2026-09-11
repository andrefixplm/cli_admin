use super::client::VmwareClient;
use super::models::{VmInfo, VmStatus};
use crate::config::VmwareConfig;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};
use walkdir::WalkDir;

fn normalize_clean_path(p: impl AsRef<Path>) -> PathBuf {
    let path = p.as_ref();
    let canonical = match path.canonicalize() {
        Ok(c) => c,
        Err(_) => path.to_path_buf(),
    };
    let clean = canonical
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_string();
    PathBuf::from(clean)
}

pub struct VmDiscovery {
    config: VmwareConfig,
    client: VmwareClient,
}

impl VmDiscovery {
    pub fn new(config: VmwareConfig, client: VmwareClient) -> Self {
        Self { config, client }
    }

    /// Atualiza a configuração interna (ex: após uma VM ser cadastrada em tempo de execução)
    pub fn set_config(&mut self, config: VmwareConfig) {
        self.config = config;
    }

    /// Descobre todas as máquinas virtuais nos diretórios configurados e mapeia seus estados atuais
    pub async fn discover_all(&self) -> Vec<VmInfo> {
        let mut discovered_paths = HashSet::new();
        let mut alias_map = HashMap::new();
        let mut name_map = HashMap::new();

        // 1. Carregar aliases e caminhos configurados explicitamente
        for entry in &self.config.vms {
            let clean = normalize_clean_path(&entry.path);
            alias_map.insert(clean.clone(), entry.alias.clone());
            if let Some(name) = &entry.name {
                name_map.insert(clean.clone(), name.clone());
            }
            // Sempre listar VMs cadastradas, mesmo que o arquivo esteja temporariamente
            // inacessível (ex: disco externo/rede offline) — status vira Offline mais abaixo.
            discovered_paths.insert(clean);
        }

        // 2. Realizar varredura recursiva nos diretórios configurados
        for scan_dir in &self.config.scan_directories {
            if !scan_dir.exists() {
                debug!("Diretório de busca de VMs não existe, pulando: {:?}", scan_dir);
                continue;
            }
            info!("Varrendo diretório em busca de VMs (.vmx): {:?}", scan_dir);

            for entry in WalkDir::new(scan_dir)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if path.is_file() {
                    if let Some(ext) = path.extension() {
                        if ext.to_string_lossy().to_lowercase() == "vmx" {
                            let clean = normalize_clean_path(path);
                            discovered_paths.insert(clean);
                        }
                    }
                }
            }
        }

        // 3. Consultar quais VMs estão em execução pelo vmrun
        let running_paths: HashSet<PathBuf> = match self.client.list_running().await {
            Ok(list) => list
                .into_iter()
                .map(|p| normalize_clean_path(&p))
                .collect(),
            Err(e) => {
                warn!("Não foi possível listar VMs em execução: {}", e);
                HashSet::new()
            }
        };

        // Adicionar automaticamente qualquer VM em execução, mesmo que seu diretório não esteja em scan_directories
        for running in &running_paths {
            discovered_paths.insert(running.clone());
        }

        // 4. Montar a lista final consolidada de VmInfo
        let mut result = Vec::new();
        for path in discovered_paths {
            let is_running = running_paths.contains(&path);
            let status = if is_running {
                VmStatus::Running
            } else if path.exists() {
                VmStatus::Stopped
            } else {
                VmStatus::Offline
            };

            let default_name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "VM".to_string());

            let display_name = name_map
                .get(&path)
                .cloned()
                .unwrap_or(default_name);

            let alias = alias_map.get(&path).cloned();

            result.push(VmInfo {
                alias,
                name: display_name,
                path,
                status,
            });
        }

        // Ordenar alfabeticamente por alias ou nome
        result.sort_by(|a, b| {
            let key_a = a.alias.as_deref().unwrap_or(&a.name);
            let key_b = b.alias.as_deref().unwrap_or(&b.name);
            key_a.to_lowercase().cmp(&key_b.to_lowercase())
        });

        info!("Total de máquinas virtuais mapeadas: {}", result.len());
        result
    }

    /// Resolve um identificador de VM (que pode ser um alias, um caminho direto ou parte do nome) para o caminho .vmx
    pub async fn resolve_vm_path(&self, identifier: &str) -> Result<PathBuf, String> {
        let direct_path = Path::new(identifier);
        if direct_path.exists() && direct_path.extension().map_or(false, |ext| ext == "vmx") {
            return Ok(direct_path.to_path_buf());
        }

        // Buscar nas VMs cadastradas
        for entry in &self.config.vms {
            if entry.alias.eq_ignore_ascii_case(identifier) {
                return Ok(entry.path.clone());
            }
        }

        // Fazer descoberta completa e comparar por alias, nome do arquivo ou nome amigável
        let all_vms = self.discover_all().await;
        for vm in &all_vms {
            if let Some(alias) = &vm.alias {
                if alias.eq_ignore_ascii_case(identifier) {
                    return Ok(vm.path.clone());
                }
            }
            if vm.name.eq_ignore_ascii_case(identifier) {
                return Ok(vm.path.clone());
            }
            if let Some(stem) = vm.path.file_stem() {
                if stem.to_string_lossy().eq_ignore_ascii_case(identifier) {
                    return Ok(vm.path.clone());
                }
            }
        }

        Err(format!(
            "Máquina virtual '{}' não encontrada por alias, nome ou caminho .vmx",
            identifier
        ))
    }
}
