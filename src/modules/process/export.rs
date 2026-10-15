use super::models::ProcessItem;
use std::path::{Path, PathBuf};

/// Exporta a lista de processos para um arquivo JSON ou CSV.
/// Sem `output`, gera `processes_<timestamp>.<formato>` no diretório atual.
pub fn export_processes(
    items: &[ProcessItem],
    format: &str,
    output: Option<&Path>,
) -> Result<PathBuf, String> {
    let content = match format {
        "json" => serde_json::to_string_pretty(items)
            .map_err(|e| format!("Erro ao serializar JSON: {}", e))?,
        "csv" => to_csv(items),
        _ => return Err(format!("Formato '{}' não suportado. Use 'json' ou 'csv'.", format)),
    };

    let path = match output {
        Some(p) => p.to_path_buf(),
        None => {
            let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
            PathBuf::from(format!("processes_{}.{}", timestamp, format))
        }
    };

    std::fs::write(&path, &content).map_err(|e| format!("Erro ao escrever arquivo: {}", e))?;

    Ok(path)
}

/// Gera o relatório CSV (RFC 4180: campos com vírgula, aspas ou quebra de linha são citados)
pub fn to_csv(items: &[ProcessItem]) -> String {
    let mut csv = String::from(
        "PID,PPID,Nome,CPU%,Memoria_MB,Path,Linha_de_Comando,Usuario,Uptime,Protegido\n",
    );
    for proc in items {
        csv.push_str(&format!(
            "{},{},{},{:.1},{:.1},{},{},{},{},{}\n",
            proc.pid,
            proc.parent_pid.map(|p| p.to_string()).unwrap_or_default(),
            csv_field(&proc.name),
            proc.cpu_pct,
            proc.memory_mb(),
            csv_field(&proc.exe_path),
            csv_field(&proc.cmd_line),
            csv_field(&proc.user),
            proc.formatted_uptime(),
            if proc.is_protected { "SIM" } else { "NAO" }
        ));
    }
    csv
}

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}
