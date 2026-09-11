use crossterm::event::{self, Event as CEvent, KeyEvent, KeyEventKind};
use std::time::Duration;

pub enum AppEvent {
    Key(KeyEvent),
    Tick,
}

pub struct EventHandler {
    tick_rate: Duration,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        Self { tick_rate }
    }

    pub fn next(&self) -> Result<AppEvent, Box<dyn std::error::Error>> {
        if event::poll(self.tick_rate)? {
            if let CEvent::Key(key) = event::read()? {
                // Filtrar apenas eventos de Press para evitar repetições no Windows
                if key.kind == KeyEventKind::Press {
                    return Ok(AppEvent::Key(key));
                }
            }
        }
        Ok(AppEvent::Tick)
    }
}
