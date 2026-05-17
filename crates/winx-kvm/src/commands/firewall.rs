use tauri::{command, State};

use crate::app_state::FirewallState;

#[command]
pub async fn get_firewall_status(state: State<'_, FirewallState>) -> Result<bool, String> {
    let is_configured = *state.is_configured.lock().await;
    Ok(is_configured)
}

#[command]
pub async fn reconfigure_firewall(state: State<'_, FirewallState>) -> Result<(), String> {
    use winx_infra::network_config;

    let result = tokio::task::spawn_blocking(move || {
        network_config::request_firewall_setup_via_uac()
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;

    if result.is_ok() {
        *state.is_configured.lock().await = true;
    }

    result
}
