//! Commands de configurações do device.

use std::sync::Arc;

use tauri::State;

use winx_application::UpdateDeviceProfile;

use super::identity::DeviceInfoDto;
use crate::app_state::{DiscoveryState, IdentityState};

/// Atualiza o nome de exibição deste device e re-anuncia via mDNS.
#[tauri::command]
pub async fn update_device_username(
    identity: State<'_, IdentityState>,
    discovery: State<'_, DiscoveryState>,
    username: String,
) -> Result<DeviceInfoDto, String> {
    let device = UpdateDeviceProfile::update_username(
        Arc::clone(&identity.identity_store),
        &discovery.discovery,
        username,
    )
    .await
    .map_err(|e| e.code_str().to_string())?;

    Ok(DeviceInfoDto {
        id: device.id.to_string(),
        username: device.username.clone(),
        fingerprint: device.public_key.fingerprint().to_string(),
    })
}
