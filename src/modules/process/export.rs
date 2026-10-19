use super::models::ProcessItem;

/// BOM UTF-8: faz o Excel (PT-BR) reconhecer acentos como Configurações sem corromper o texto
pub const CSV_BOM: &str = "\u{FEFF}";

const CSV_HEADER: &str =
    "PID,PPID,Nome,CPU%,Memoria_MB,Path,Linha_de_Comando,Usuario,Uptime,Protegido";

/// Escapa um campo CSV (RFC 4180): entre aspas quando contém vírgula, aspas ou quebra de linha
pub fn csv_escape(field: &str) -> String {
    if field.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

/// Gera o relatório CSV (com BOM UTF-8) dos processos informados
pub fn processes_to_csv(items: &[ProcessItem]) -> String {
    let mut csv = format!("{}{}\n", CSV_BOM, CSV_HEADER);
    for proc in items {
        let parent = proc.parent_pid.map(|p| p.to_string()).unwrap_or_default();
        let row = [
            proc.pid.to_string(),
            parent,
            csv_escape(&proc.name),
            format!("{:.1}", proc.cpu_pct),
            format!("{:.1}", proc.memory_mb()),
            csv_escape(&proc.exe_path),
            csv_escape(&proc.cmd_line),
            csv_escape(&proc.user),
            csv_escape(&proc.formatted_uptime()),
            if proc.is_protected { "SIM" } else { "NAO" }.to_string(),
        ];
        csv.push_str(&row.join(","));
        csv.push('\n');
    }
    csv
}

/// Gera o relatório JSON dos processos informados
pub fn processes_to_json(items: &[ProcessItem]) -> Result<String, String> {
    serde_json::to_string_pretty(items).map_err(|e| format!("Erro ao serializar JSON: {}", e))
}

/// Gera o conteúdo do relatório no formato pedido ("csv" ou "json")
pub fn render(items: &[ProcessItem], format: &str) -> Result<String, String> {
    match format {
        "csv" => Ok(processes_to_csv(items)),
        "json" => processes_to_json(items),
        _ => Err(format!("Formato '{}' não suportado. Use 'json' ou 'csv'.", format)),
    }
}

/// Nome padrão do arquivo de exportação, com timestamp
pub fn default_filename(format: &str) -> String {
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    format!("processes_{}.{}", timestamp, format)
}

/// Grava o conteúdo no arquivo indicado
pub fn write_file(path: &str, content: &str) -> Result<(), String> {
    std::fs::write(path, content).map_err(|e| format!("Erro ao escrever arquivo: {}", e))
}
