//! Estado global compartilhado entre Tauri commands.
//!
//! Mantido enxuto: apenas o event bus por enquanto. Conforme bounded contexts
//! são plugados, suas instâncias de use cases entram aqui (atrás de Arcs).

use winx_application::EventBus;

#[derive(Debug, Clone)]
pub struct AppState {
    pub bus: EventBus,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            bus: EventBus::new(),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
