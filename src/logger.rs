use std::path::Path;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

pub struct LogGuard {
    _worker_guard: Option<WorkerGuard>,
}

/// Inicializa o sistema de logging com arquivo rotativo e opcionalmente stdout
pub fn init_logger(
    log_file_path: &str,
    log_level: &str,
    log_to_stdout: bool,
    debug_flag: bool,
) -> LogGuard {
    let effective_level = if debug_flag { "debug" } else { log_level };

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(effective_level));

    let path = Path::new(log_file_path);
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let filename = path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "admin.log".to_string());

    let file_appender = tracing_appender::rolling::never(dir, filename);
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(true)
        .with_thread_names(false)
        .with_filter(env_filter.clone());

    if log_to_stdout {
        let stdout_layer = tracing_subscriber::fmt::layer()
            .with_ansi(true)
            .with_target(false)
            .with_filter(env_filter);

        tracing_subscriber::registry()
            .with(file_layer)
            .with(stdout_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(file_layer)
            .init();
    }

    LogGuard {
        _worker_guard: Some(guard),
    }
}
