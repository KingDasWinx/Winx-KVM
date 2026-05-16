//! Entry-point da aplicação Tauri do Winx-KVM.
//!
//! Aqui montamos:
//! - O subscriber de tracing (logs estruturados, filtro por `WINX_LOG`)
//! - O event bus (`winx-application`)
//! - O AppState compartilhado entre Tauri commands
//! - Os Tauri commands e o forwarder de eventos para o frontend
//!
//! O fluxo concreto de cada bounded context será plugado sprint a sprint.

mod app_state;
mod commands;
mod events;
mod telemetry;

use tracing::info;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    telemetry::init();
    info!(version = env!("CARGO_PKG_VERSION"), "iniciando Winx-KVM");

    let app_state = app_state::AppState::new();

    tauri::Builder::default()
        .manage(app_state)
        .setup(|app| {
            events::install_forwarder(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![commands::app_info, commands::ping,])
        .run(tauri::generate_context!())
        .expect("erro ao executar a aplicação Tauri");
}
