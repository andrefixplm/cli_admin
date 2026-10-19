//! Servidor MCP por stdio. **stdout é do protocolo**: nenhuma linha de log vai para lá.
//! Uso: cli_admin_mcp [--config <arquivo.toml>] [--permitir-elevado]

use cli_admin::config::AppConfig;
use cli_admin::mcp::{elevation, server::Servidor};
use rmcp::{ServiceExt, transport::stdio};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let argumentos: Vec<String> = std::env::args().collect();
    let caminho = argumentos
        .iter()
        .position(|a| a == "--config")
        .and_then(|i| argumentos.get(i + 1))
        .map(std::path::PathBuf::from);
    let permitir_elevado = argumentos.iter().any(|a| a == "--permitir-elevado");

    if let Err(m) = elevation::decidir_inicio(elevation::is_admin(), permitir_elevado) {
        eprintln!("{m}"); // stderr, nunca stdout
        std::process::exit(2);
    }

    let (cfg, _) = AppConfig::load(caminho.as_deref())?;
    // log só em arquivo (o arquivo de log da configuração): stdout é do protocolo
    let _log = cli_admin::logger::init_logger(&cfg.general.log_file, &cfg.general.log_level, false, false);

    let servico = Servidor::novo(cfg).serve(stdio()).await?;
    servico.waiting().await?;
    Ok(())
}
