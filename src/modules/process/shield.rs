use super::models::ProcessItem;
use std::collections::HashSet;
use tracing::warn;

#[derive(Debug, Clone)]
pub struct ProcessShield {
    protected_names: HashSet<String>,
}

impl ProcessShield {
    pub fn new(protected_list: &[String]) -> Self {
        let mut set = HashSet::new();
        let mut insert_name = |name: &str| {
            let lower = name.to_lowercase();
            set.insert(lower.clone());
            if lower.ends_with(".exe") {
                set.insert(lower.trim_end_matches(".exe").to_string());
            } else {
                set.insert(format!("{}.exe", lower));
            }
        };

        for name in protected_list {
            insert_name(name);
        }

        // Sempre proteger processos fundamentais do NT Kernel
        for sys in [
            "system",
            "registry",
            "smss.exe",
            "csrss.exe",
            "wininit.exe",
            "services.exe",
            "lsass.exe",
            "winlogon.exe",
        ] {
            insert_name(sys);
        }

        Self {
            protected_names: set,
        }
    }

    /// Verifica se um processo é protegido contra encerramento
    pub fn is_protected(&self, name: &str, pid: u32) -> bool {
        // PIDs de sistema no Windows
        if pid == 0 || pid == 4 {
            return true;
        }

        let lower = name.to_lowercase();
        self.protected_names.contains(&lower)
    }

    /// Separa uma lista de processos candidatos a kill entre seguros e protegidos
    #[allow(dead_code)]
    pub fn classify(&self, items: &[ProcessItem]) -> (Vec<ProcessItem>, Vec<ProcessItem>) {
        let mut safe = Vec::new();
        let mut critical = Vec::new();

        for item in items {
            if self.is_protected(&item.name, item.pid) {
                warn!(
                    "Processo crítico selecionado para finalização: {} (PID: {})",
                    item.name, item.pid
                );
                critical.push(item.clone());
            } else {
                safe.push(item.clone());
            }
        }

        (safe, critical)
    }
}
