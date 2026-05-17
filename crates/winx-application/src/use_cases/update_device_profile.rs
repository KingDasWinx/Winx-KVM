use std::sync::Arc;

use tracing::info;
use winx_domain::{
    identity::Device,
    shared::{ids::PeerId, DomainError, DomainErrorCode},
};

use crate::{
    ports::{discovery::AnnounceInfo, IdentityStore, WINX_KVM_PORT},
    DiscoveryService,
};

fn validate_username(username: &str) -> Result<String, DomainError> {
    let trimmed = username.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return Err(DomainError::new(
            DomainErrorCode::IdentityUsernameInvalid,
            "username must be 1..=64 characters after trim",
        ));
    }
    Ok(trimmed.to_string())
}

/// Atualiza o perfil do device local (username) e re-anuncia na rede.
pub struct UpdateDeviceProfile;

impl UpdateDeviceProfile {
    pub async fn update_username(
        identity_store: Arc<dyn IdentityStore>,
        discovery: &DiscoveryService,
        username: String,
    ) -> Result<Device, DomainError> {
        let validated = validate_username(&username)?;

        let mut device = identity_store
            .load_device()
            .await
            .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?
            .ok_or_else(|| DomainError::new(DomainErrorCode::InternalError, "device not found"))?;

        if device.username == validated {
            return Ok(device);
        }

        device.username = validated;

        identity_store
            .save_device(&device)
            .await
            .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?;

        let info = AnnounceInfo {
            peer_id: PeerId::from_uuid(device.id.as_uuid()),
            username: device.username.clone(),
            fingerprint: device.public_key.fingerprint().to_string(),
            port: WINX_KVM_PORT,
        };

        discovery
            .reannounce(&info)
            .await
            .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?;

        info!(device_id = %device.id, username = %device.username, "username atualizado");

        Ok(device)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_username() {
        assert!(validate_username("   ").is_err());
    }

    #[test]
    fn trims_and_accepts_valid_username() {
        let ok = validate_username("  My PC  ").unwrap();
        assert_eq!(ok, "My PC");
    }

    #[test]
    fn rejects_too_long_username() {
        let long = "a".repeat(65);
        assert!(validate_username(&long).is_err());
    }
}
