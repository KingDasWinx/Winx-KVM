use crate::identity::key::PublicKey;
use crate::shared::ids::DeviceId;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Membro de um workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceMember {
    /// Identificador do device.
    pub device_id: DeviceId,
    /// Chave pública Ed25519 do device (usado para trust).
    pub public_key: PublicKey,
    /// Nome amigável do device (cache — pode ficar desatualizado).
    pub username_cache: String,
    /// Momento em que este member se juntou ao workspace.
    #[serde(with = "time::serde::rfc3339")]
    pub joined_at: OffsetDateTime,
}

impl WorkspaceMember {
    /// Cria um novo membro.
    pub fn new(device_id: DeviceId, public_key: PublicKey, username: impl Into<String>) -> Self {
        Self {
            device_id,
            public_key,
            username_cache: username.into(),
            joined_at: OffsetDateTime::now_utc(),
        }
    }

    /// Atualiza o cache de username.
    pub fn rename(&mut self, new_username: impl Into<String>) {
        self.username_cache = new_username.into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_sets_joined_at() {
        let member = WorkspaceMember::new(DeviceId::new(), PublicKey::new([0u8; 32]), "Test");
        let now = OffsetDateTime::now_utc();
        // Deve estar recente (últimos 5 segundos)
        assert!((now - member.joined_at).as_seconds_f64().abs() < 5.0);
    }

    #[test]
    fn rename_updates_username() {
        let mut member =
            WorkspaceMember::new(DeviceId::new(), PublicKey::new([0u8; 32]), "OldName");
        member.rename("NewName");
        assert_eq!(member.username_cache, "NewName");
    }

    #[test]
    fn serde_roundtrip() {
        let member = WorkspaceMember::new(DeviceId::new(), PublicKey::new([42u8; 32]), "João");
        let json = serde_json::to_string(&member).unwrap();
        let deserialized: WorkspaceMember = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.device_id, member.device_id);
        assert_eq!(deserialized.username_cache, member.username_cache);
    }
}
