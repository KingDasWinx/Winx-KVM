//! Commands genéricos de info do app (smoke test do canal IPC).

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub protocol_version: u16,
}

/// Retorna metadados do app. Útil como smoke test do canal Tauri → React.
#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        name: env!("CARGO_PKG_NAME"),
        version: env!("CARGO_PKG_VERSION"),
        protocol_version: winx_protocol::PROTOCOL_VERSION,
    }
}

/// Eco simples para verificar invocação bidirecional.
#[tauri::command]
pub fn ping(message: String) -> String {
    tracing::debug!(%message, "ping recebido");
    format!("pong: {message}")
}
