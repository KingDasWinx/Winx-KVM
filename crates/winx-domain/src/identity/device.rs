use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::shared::ids::{DeviceId, PeerId};

use super::key::PublicKey;

/// Agregado raiz do contexto de identidade: representa **este** device.
///
/// A chave de assinatura privada (Ed25519) **nunca** fica aqui —
/// vive exclusivamente no Windows Credential Manager via `SecretStore`.
/// `Device` carrega apenas a parte pública.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: DeviceId,
    /// Nome amigável exibido para outros peers (editável nas configurações).
    pub username: String,
    /// Chave pública Ed25519 derivada da signing key.
    pub public_key: PublicKey,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

impl Device {
    pub fn new(id: DeviceId, username: impl Into<String>, public_key: PublicKey) -> Self {
        Self {
            id,
            username: username.into(),
            public_key,
            created_at: OffsetDateTime::now_utc(),
        }
    }
}

/// Peer confiável: um device remoto que já completou o pareamento com este.
///
/// Persiste em `%APPDATA%\Winx-KVM\peers.toml`.
/// A relação de confiança é verificada pela `public_key` (Ed25519 pública),
/// não por username (que o usuário pode mudar).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedPeer {
    pub id: PeerId,
    pub username: String,
    pub public_key: PublicKey,
    #[serde(with = "time::serde::rfc3339")]
    pub paired_at: OffsetDateTime,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub last_seen: Option<OffsetDateTime>,
}

impl TrustedPeer {
    pub fn new(id: PeerId, username: impl Into<String>, public_key: PublicKey) -> Self {
        Self {
            id,
            username: username.into(),
            public_key,
            paired_at: OffsetDateTime::now_utc(),
            last_seen: None,
        }
    }

    /// Atualiza `last_seen` para agora.
    pub fn touch(&mut self) {
        self.last_seen = Some(OffsetDateTime::now_utc());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_created_at_is_set() {
        let dev = Device::new(DeviceId::new(), "My PC", PublicKey::new([0x01u8; 32]));
        assert!(!dev.username.is_empty());
        // created_at deve ser muito próximo de agora (< 1s)
        let now = OffsetDateTime::now_utc();
        let diff = (now - dev.created_at).abs();
        assert!(diff.whole_seconds() < 2);
    }

    #[test]
    fn trusted_peer_touch_updates_last_seen() {
        let mut peer = TrustedPeer::new(
            PeerId::from_uuid(uuid::Uuid::new_v4()),
            "Notebook",
            PublicKey::new([0x02u8; 32]),
        );
        assert!(peer.last_seen.is_none());
        peer.touch();
        assert!(peer.last_seen.is_some());
    }
}
