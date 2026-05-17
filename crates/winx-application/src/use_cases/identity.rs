use std::sync::Arc;

use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use tracing::{info, warn};
use winx_domain::shared::DomainErrorCode;
use winx_domain::{
    identity::{events::DeviceCreated, Device, PublicKey},
    shared::ids::DeviceId,
    DomainError, DomainEvent,
};

use crate::{
    bus::EventBus,
    ports::{IdentityStore, SecretStore},
};

/// Garante que este device tem uma identidade persistida.
///
/// - Se já existir (`identity.toml` + chave no Credential Manager): retorna o device carregado.
/// - Se não existir (primeira execução): gera keypair Ed25519, salva e publica `DeviceCreated`.
///
/// O `username` é usado apenas na criação; subsequentes chamadas retornam o nome
/// que já está em `identity.toml` (o usuário edita via Settings).
pub struct EnsureDevice {
    identity_store: Arc<dyn IdentityStore>,
    secret_store: Arc<dyn SecretStore>,
    bus: EventBus,
}

impl EnsureDevice {
    pub fn new(
        identity_store: Arc<dyn IdentityStore>,
        secret_store: Arc<dyn SecretStore>,
        bus: EventBus,
    ) -> Self {
        Self {
            identity_store,
            secret_store,
            bus,
        }
    }

    pub async fn execute(&self, default_username: &str) -> Result<Device, DomainError> {
        // Tenta carregar device existente
        if let Some(device) = self
            .identity_store
            .load_device()
            .await
            .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?
        {
            // Verifica que a signing key ainda está no vault
            match self.secret_store.load_signing_key().await {
                Ok(Some(_)) => {
                    info!(device_id = %device.id, "identidade existente carregada");
                    return Ok(device);
                }
                Ok(None) => {
                    warn!(device_id = %device.id, "identity.toml existe mas signing key sumiu do vault — recriando keypair");
                }
                Err(e) => {
                    return Err(DomainError::new(
                        DomainErrorCode::InternalError,
                        e.to_string(),
                    ));
                }
            }
        }

        // Gera novo keypair Ed25519
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let public_key = PublicKey::new(verifying_key.to_bytes());
        let fingerprint = public_key.fingerprint().to_string();

        let device = Device::new(DeviceId::new(), default_username, public_key);

        // Persiste signing key PRIMEIRO — se falhar aqui não gravamos identity.toml
        self.secret_store
            .store_signing_key(&signing_key.to_bytes())
            .await
            .map_err(|e| {
                DomainError::new(DomainErrorCode::IdentityKeyGenerationFailed, e.to_string())
            })?;

        self.identity_store
            .save_device(&device)
            .await
            .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?;

        info!(device_id = %device.id, %fingerprint, "nova identidade criada");

        self.bus.publish(DomainEvent::DeviceCreated(DeviceCreated {
            device_id: device.id,
            fingerprint,
        }));

        Ok(device)
    }
}
