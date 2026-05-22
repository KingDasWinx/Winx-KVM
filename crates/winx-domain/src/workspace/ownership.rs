use crate::shared::ids::DeviceId;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Modo de propriedade de um workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum OwnershipMode {
    /// Workspace criado neste device — pode ser editado, deletado, convites enviados.
    Original,
    /// Cópia recebida de um convite — mutável via sync remoto apenas.
    Mirror {
        /// DeviceId do proprietário original.
        owner_device_id: DeviceId,
        /// Username do proprietário (snapshot no momento do convite).
        owner_username_snapshot: String,
        /// Último momento em que recebemos um sync/heartbeat do owner.
        #[serde(with = "time::serde::rfc3339")]
        owner_last_seen: OffsetDateTime,
        /// True se o owner deletou este workspace.
        #[serde(default)]
        is_orphan: bool,
    },
}

impl OwnershipMode {
    /// Retorna true se este é um mirror.
    #[must_use]
    pub const fn is_mirror(&self) -> bool {
        matches!(self, Self::Mirror { .. })
    }

    /// Retorna true se é original.
    #[must_use]
    pub const fn is_original(&self) -> bool {
        matches!(self, Self::Original)
    }

    /// Marca este mirror como órfão (owner deletou).
    pub fn mark_orphan(&mut self) -> Result<(), String> {
        if let Self::Mirror { is_orphan, .. } = self {
            *is_orphan = true;
            Ok(())
        } else {
            Err("Apenas mirrors podem ser órfãos".to_string())
        }
    }

    /// Atualiza o timestamp de last_seen do owner.
    pub fn touch_owner_seen(&mut self) -> Result<(), String> {
        if let Self::Mirror {
            owner_last_seen, ..
        } = self
        {
            *owner_last_seen = OffsetDateTime::now_utc();
            Ok(())
        } else {
            Err("Apenas mirrors rastreiam owner_last_seen".to_string())
        }
    }

    /// Retorna o DeviceId do owner (se mirror), ou None.
    #[must_use]
    pub fn owner_device_id(&self) -> Option<DeviceId> {
        if let Self::Mirror {
            owner_device_id, ..
        } = self
        {
            Some(*owner_device_id)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_mirror_and_is_original() {
        let original = OwnershipMode::Original;
        assert!(original.is_original());
        assert!(!original.is_mirror());

        let mirror = OwnershipMode::Mirror {
            owner_device_id: DeviceId::new(),
            owner_username_snapshot: "João".to_string(),
            owner_last_seen: OffsetDateTime::now_utc(),
            is_orphan: false,
        };
        assert!(mirror.is_mirror());
        assert!(!mirror.is_original());
    }

    #[test]
    fn mark_orphan_succeeds_on_mirror() {
        let mut mirror = OwnershipMode::Mirror {
            owner_device_id: DeviceId::new(),
            owner_username_snapshot: "João".to_string(),
            owner_last_seen: OffsetDateTime::now_utc(),
            is_orphan: false,
        };
        assert!(mirror.mark_orphan().is_ok());
        assert!(matches!(
            mirror,
            OwnershipMode::Mirror {
                is_orphan: true,
                ..
            }
        ));
    }

    #[test]
    fn mark_orphan_fails_on_original() {
        let mut original = OwnershipMode::Original;
        assert!(original.mark_orphan().is_err());
    }

    #[test]
    fn touch_owner_seen_updates_timestamp() {
        let mut mirror = OwnershipMode::Mirror {
            owner_device_id: DeviceId::new(),
            owner_username_snapshot: "João".to_string(),
            owner_last_seen: OffsetDateTime::now_utc() - time::Duration::seconds(100),
            is_orphan: false,
        };
        let old_time = match &mirror {
            OwnershipMode::Mirror {
                owner_last_seen, ..
            } => *owner_last_seen,
            _ => panic!(),
        };
        assert!(mirror.touch_owner_seen().is_ok());
        if let OwnershipMode::Mirror {
            owner_last_seen, ..
        } = &mirror
        {
            assert!(*owner_last_seen > old_time);
        }
    }

    #[test]
    fn serde_original() {
        let original = OwnershipMode::Original;
        let json = serde_json::to_string(&original).unwrap();
        assert!(json.contains("\"mode\":\"original\""));
        let deserialized: OwnershipMode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, original);
    }

    #[test]
    fn serde_mirror() {
        let mirror = OwnershipMode::Mirror {
            owner_device_id: DeviceId::new(),
            owner_username_snapshot: "Test".to_string(),
            owner_last_seen: OffsetDateTime::now_utc(),
            is_orphan: false,
        };
        let json = serde_json::to_string(&mirror).unwrap();
        assert!(json.contains("\"mode\":\"mirror\""));
        let deserialized: OwnershipMode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.is_mirror(), true);
    }
}
