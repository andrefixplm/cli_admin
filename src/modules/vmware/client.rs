use super::models::{SnapshotInfo, StartMode, StopMode};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, error, info};

#[derive(Debug, Clone)]
pub struct VmwareClient {
    pub vmrun_path: PathBuf,
}

impl VmwareClient {
    pub fn new(configured_path: &str) -> Self {
        let path = PathBuf::from(configured_path);
        let resolved = if path.exists() {
            path
        } else {
            // Tentar caminhos padrão do VMware Workstation no Windows
            let candidates = [
                PathBuf::from(r"C:\Program Files (x86)\VMware\VMware Workstation\vmrun.exe"),
                PathBuf::from(r"C:\Program Files\VMware\VMware Workstation\vmrun.exe"),
            ];
            candidates
                .into_iter()
                .find(|p| p.exists())
                .unwrap_or_else(|| PathBuf::from("vmrun.exe"))
        };

        info!("VmwareClient inicializado com vmrun_path: {:?}", resolved);
        Self {
            vmrun_path: resolved,
        }
    }

    /// Executa um comando vmrun com os argumentos especificados e captura a saída
    async fn run_vmrun(&self, args: &[&str]) -> Result<String, String> {
        debug!(
            "Executando comando vmrun: {:?} com argumentos: {:?}",
            self.vmrun_path, args
        );

        let mut cmd = Command::new(&self.vmrun_path);
        cmd.args(args);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = match cmd.output().await {
            Ok(out) => out,
            Err(e) => {
                let err_msg = format!(
                    "Falha ao executar vmrun em '{:?}': {}. Verifique se o VMware Workstation está instalado.",
                    self.vmrun_path, e
                );
                error!("{}", err_msg);
                return Err(err_msg);
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        debug!(
            "vmrun exit_code: {:?}, stdout: {}, stderr: {}",
            output.status.code(),
            stdout.trim(),
            stderr.trim()
        );

        if output.status.success() {
            Ok(stdout)
        } else {
            let combined = if !stderr.is_empty() {
                stderr.trim().to_string()
            } else if !stdout.is_empty() {
                stdout.trim().to_string()
            } else {
                format!("Código de saída do vmrun: {:?}", output.status.code())
            };
            error!("Erro retornado pelo vmrun: {}", combined);
            Err(combined)
        }
    }

    /// Retorna a lista de caminhos de arquivos .vmx atualmente em execução
    pub async fn list_running(&self) -> Result<Vec<PathBuf>, String> {
        let output = self.run_vmrun(&["-T", "ws", "list"]).await?;
        let mut running_vms = Vec::new();

        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.to_lowercase().starts_with("total running") {
                continue;
            }
            let path = PathBuf::from(trimmed);
            running_vms.push(path);
        }

        debug!("VMs em execução encontradas: {:?}", running_vms);
        Ok(running_vms)
    }

    /// Inicia uma máquina virtual
    pub async fn start(&self, vmx_path: &Path, mode: StartMode) -> Result<String, String> {
        let path_str = vmx_path.to_str().ok_or("Caminho de VM inválido")?;
        info!("Iniciando VM: {} no modo: {}", path_str, mode.as_str());
        self.run_vmrun(&["-T", "ws", "start", path_str, mode.as_str()])
            .await
    }

    /// Desliga uma máquina virtual
    pub async fn stop(&self, vmx_path: &Path, mode: StopMode) -> Result<String, String> {
        let path_str = vmx_path.to_str().ok_or("Caminho de VM inválido")?;
        info!("Parando VM: {} no modo: {}", path_str, mode.as_str());
        self.run_vmrun(&["-T", "ws", "stop", path_str, mode.as_str()])
            .await
    }

    /// Pausa a execução de uma máquina virtual
    pub async fn pause(&self, vmx_path: &Path) -> Result<String, String> {
        let path_str = vmx_path.to_str().ok_or("Caminho de VM inválido")?;
        info!("Pausando VM: {}", path_str);
        self.run_vmrun(&["-T", "ws", "pause", path_str]).await
    }

    /// Retoma a execução de uma máquina virtual pausada
    pub async fn unpause(&self, vmx_path: &Path) -> Result<String, String> {
        let path_str = vmx_path.to_str().ok_or("Caminho de VM inválido")?;
        info!("Retomando (unpause) VM: {}", path_str);
        self.run_vmrun(&["-T", "ws", "unpause", path_str]).await
    }

    /// Suspende uma máquina virtual
    pub async fn suspend(&self, vmx_path: &Path, mode: StopMode) -> Result<String, String> {
        let path_str = vmx_path.to_str().ok_or("Caminho de VM inválido")?;
        info!("Suspendendo VM: {} no modo: {}", path_str, mode.as_str());
        self.run_vmrun(&["-T", "ws", "suspend", path_str, mode.as_str()])
            .await
    }

    /// Reinicia uma máquina virtual
    pub async fn reset(&self, vmx_path: &Path, mode: StopMode) -> Result<String, String> {
        let path_str = vmx_path.to_str().ok_or("Caminho de VM inválido")?;
        info!("Reiniciando VM: {} no modo: {}", path_str, mode.as_str());
        self.run_vmrun(&["-T", "ws", "reset", path_str, mode.as_str()])
            .await
    }

    /// Lista os snapshots de uma máquina virtual
    pub async fn list_snapshots(&self, vmx_path: &Path) -> Result<Vec<SnapshotInfo>, String> {
        let path_str = vmx_path.to_str().ok_or("Caminho de VM inválido")?;
        debug!("Consultando snapshots da VM: {}", path_str);
        let output = self
            .run_vmrun(&["-T", "ws", "listSnapshots", path_str])
            .await?;

        let mut snapshots = Vec::new();
        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.to_lowercase().starts_with("total snapshots") {
                continue;
            }
            snapshots.push(SnapshotInfo {
                name: trimmed.to_string(),
            });
        }
        Ok(snapshots)
    }

    /// Cria um novo snapshot
    pub async fn create_snapshot(
        &self,
        vmx_path: &Path,
        snapshot_name: &str,
    ) -> Result<String, String> {
        let path_str = vmx_path.to_str().ok_or("Caminho de VM inválido")?;
        info!(
            "Criando snapshot '{}' para VM: {}",
            snapshot_name, path_str
        );
        self.run_vmrun(&["-T", "ws", "snapshot", path_str, snapshot_name])
            .await
    }

    /// Reverte para um snapshot existente
    pub async fn revert_snapshot(
        &self,
        vmx_path: &Path,
        snapshot_name: &str,
    ) -> Result<String, String> {
        let path_str = vmx_path.to_str().ok_or("Caminho de VM inválido")?;
        info!(
            "Revertendo para snapshot '{}' na VM: {}",
            snapshot_name, path_str
        );
        self.run_vmrun(&["-T", "ws", "revertToSnapshot", path_str, snapshot_name])
            .await
    }
}
