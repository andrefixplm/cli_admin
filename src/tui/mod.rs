pub mod app;
pub mod event;
pub mod ui;

use app::App;
use event::{AppEvent, EventHandler};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, Stdout};
use std::path::PathBuf;
use std::time::Duration;
use tracing::info;

pub async fn run_tui(
    config: crate::config::AppConfig,
    config_path: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Inicializando Terminal User Interface (TUI)...");

    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::cursor::Hide
    )?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_loop(&mut terminal, config, config_path).await;

    // Restaurar o terminal
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::cursor::Show
    )?;

    if let Err(e) = res {
        eprintln!("Erro na execução da TUI: {}", e);
    }

    Ok(())
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    config: crate::config::AppConfig,
    config_path: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let tick_rate = Duration::from_millis(150);
    let event_handler = EventHandler::new(tick_rate);
    let mut app = App::new(config, config_path).await;

    let mut last_metric_tick = std::time::Instant::now();
    let metric_interval = Duration::from_millis(app.config.general.refresh_rate_ms);

    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;

        match event_handler.next()? {
            AppEvent::Key(key) => {
                let should_quit = app.handle_key(key).await;
                if should_quit {
                    break;
                }
            }
            AppEvent::Tick => {
                if last_metric_tick.elapsed() >= metric_interval {
                    app.tick().await;
                    last_metric_tick = std::time::Instant::now();
                }
            }
        }
    }

    Ok(())
}
