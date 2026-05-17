//! Commands do contexto de identidade.

use serde::Serialize;
use tauri::State;

use crate::app_state::{DiscoveryState, IdentityState};

/// DTO de resposta do device local para o frontend.
#[derive(Debug, Serialize)]
pub struct DeviceInfoDto {
    pub id: String,
    pub username: String,
    /// Fingerprint para exibição: `"A3:4F:B2:1C:D9:E5:07:AB"`.
    pub fingerprint: String,
}

/// Retorna (ou cria na primeira vez) a identidade deste device.
///
/// Também inicia o announce + browsing mDNS (idempotente — ignora se já rodando).
/// Lê o username do env var `COMPUTERNAME`; se ausente usa `"My PC"`.
#[tauri::command]
pub async fn get_device_info(
    identity: State<'_, IdentityState>,
    discovery: State<'_, DiscoveryState>,
) -> Result<DeviceInfoDto, String> {
    let username = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "My PC".to_string());

    let device = identity
        .ensure_device
        .execute(&username)
        .await
        .map_err(|e| e.code_str().to_string())?;

    // Idempotent: discovery já foi iniciado no startup, mas chamamos novamente
    // em caso de um novo username ou como fallback.
    let _ = discovery
        .discovery
        .start_for_device(&device)
        .await;

    Ok(DeviceInfoDto {
        id: device.id.to_string(),
        username: device.username.clone(),
        fingerprint: device.public_key.fingerprint().to_string(),
    })
}
