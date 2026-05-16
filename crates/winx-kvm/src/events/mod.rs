//! Forwarder: consome o event bus interno e emite eventos para o frontend
//! via `app_handle.emit()`.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tracing::error;
use winx_domain::DomainEvent;

use crate::app_state::AppState;

/// Wrapper serializável para eventos enviados ao frontend.
///
/// O JS recebe `{ kind: string, payload: ... }`. O `kind` é estável para
/// que o frontend possa fazer narrow type-safe.
#[derive(Debug, Clone, Serialize)]
struct FrontendEvent {
    kind: &'static str,
}

impl From<&DomainEvent> for FrontendEvent {
    fn from(event: &DomainEvent) -> Self {
        // `DomainEvent` é `#[non_exhaustive]`. Conforme novas variantes
        // forem adicionadas em outros bounded contexts, mapeie aqui.
        match event {
            DomainEvent::Placeholder => FrontendEvent {
                kind: "placeholder",
            },
            _ => FrontendEvent { kind: "unknown" },
        }
    }
}

/// Inicia uma task que reencaminha cada `DomainEvent` do bus para o
/// frontend Tauri (event name: `winx://event`).
pub fn install_forwarder(handle: AppHandle) {
    let state = handle.state::<AppState>();
    let mut rx = state.bus.subscribe();

    tauri::async_runtime::spawn(async move {
        while let Ok(event) = rx.recv().await {
            let payload = FrontendEvent::from(&event);
            if let Err(err) = handle.emit("winx://event", &payload) {
                error!(?err, "falha ao emitir evento para o frontend");
            }
        }
    });
}
