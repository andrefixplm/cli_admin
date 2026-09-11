use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, error, info};

pub struct ProcessKiller;

impl ProcessKiller {
    /// Finaliza um processo pelo PID utilizando taskkill no Windows
    pub async fn kill(pid: u32, force: bool, tree: bool) -> Result<String, String> {
        let pid_str = pid.to_string();
        let mut args = vec!["/PID", &pid_str];

        if force {
            args.push("/F");
        }
        if tree {
            args.push("/T");
        }

        info!(
            "Tentando finalizar processo PID: {} (force={}, tree={})",
            pid, force, tree
        );
        debug!("Executando taskkill com argumentos: {:?}", args);

        let mut cmd = Command::new("taskkill");
        cmd.args(&args);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = match cmd.output().await {
            Ok(out) => out,
            Err(e) => {
                let err = format!("Falha ao invocar taskkill: {}", e);
                error!("{}", err);
                return Err(err);
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

        debug!(
            "taskkill exit code: {:?}, stdout: '{}', stderr: '{}'",
            output.status.code(),
            stdout,
            stderr
        );

        if output.status.success() {
            info!("Processo {} finalizado com sucesso", pid);
            Ok(stdout)
        } else {
            let msg = if !stderr.is_empty() {
                stderr
            } else if !stdout.is_empty() {
                stdout
            } else {
                format!("Código de saída do taskkill: {:?}", output.status.code())
            };
            error!("Erro ao finalizar processo {}: {}", pid, msg);
            Err(msg)
        }
    }

    /// Finaliza múltiplos processos em lote
    pub async fn kill_batch(
        pids: &[u32],
        force: bool,
        tree: bool,
    ) -> Vec<(u32, Result<String, String>)> {
        let mut results = Vec::new();
        for &pid in pids {
            let res = Self::kill(pid, force, tree).await;
            results.push((pid, res));
        }
        results
    }
}
