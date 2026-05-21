use serde_json::json;
use std::path::PathBuf;
use tauri::{command, State};

use crate::app_state::{DiscoveryState, FirewallState, InputControlState};

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

#[command]
pub async fn export_diagnostics(
    discovery: State<'_, DiscoveryState>,
    input: State<'_, InputControlState>,
) -> Result<String, String> {
    use winx_infra::network_config;

    let config_status = network_config::inspect()
        .map_err(|e| format!("Failed to inspect network config: {}", e))?;

    let peers = discovery.discovery.get_peers().await;
    let peers_json: Vec<serde_json::Value> = peers
        .iter()
        .map(|p| {
            json!({
                "peer_id": p.id.to_string(),
                "username": p.username,
                "fingerprint": p.fingerprint,
                "addresses": p.addresses.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
            })
        })
        .collect();

    let log_path = {
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(appdata)
            .join("br.com.winxkvm.app")
            .join("logs")
            .join("winx-kvm.log")
    };

    let log_tail = std::fs::read_to_string(&log_path)
        .unwrap_or_else(|_| "[logs not available]".to_string())
        .lines()
        .rev()
        .take(50)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");

    let input_stats = input.input_control.get_input_debug_stats().await;

    let diagnostics = json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "app_version": env!("CARGO_PKG_VERSION"),
        "input_debug": input_stats,
        "os": std::env::consts::OS,
        "os_version": winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE)
            .open_subkey("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion")
            .and_then(|k| k.get_value::<String, _>("ProductName"))
            .unwrap_or_else(|_| "Unknown".to_string()),
        "firewall_rules": config_status.firewall_rules.iter().map(|r| {
            json!({
                "name": r.name,
                "protocol": r.protocol,
                "direction": r.direction,
                "profile": r.profile,
                "enabled": r.enabled,
            })
        }).collect::<Vec<_>>(),
        "network_profiles": config_status.network_profiles.iter().map(|p| {
            json!({
                "interface": p.interface_alias,
                "category": format!("{:?}", p.category),
                "ipv4": p.ipv4.map(|ip| ip.to_string()),
            })
        }).collect::<Vec<_>>(),
        "discovered_peers": peers_json,
        "exe_path": config_status.current_exe.to_string_lossy(),
        "log_tail": log_tail,
    });

    Ok(diagnostics.to_string())
}
