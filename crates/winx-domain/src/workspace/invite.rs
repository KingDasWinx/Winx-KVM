use crate::shared::ids::DeviceId;
use serde::{Deserialize, Serialize};
use std::fmt;
use time::OffsetDateTime;
use uuid::Uuid;

/// Identificador único de um invite de workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InviteId(Uuid);

impl InviteId {
    /// Gera um novo InviteId.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Cria a partir de UUID.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Retorna o UUID subjacente.
    #[must_use]
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for InviteId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for InviteId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Estado da sessão de invite.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum InviteState {
    /// Invite enviado, aguardando resposta.
    Pending {
        #[serde(with = "time::serde::rfc3339")]
        initiated_at: OffsetDateTime,
    },
    /// Invite entregue ao peer.
    Delivered {
        #[serde(with = "time::serde::rfc3339")]
        delivered_at: OffsetDateTime,
    },
    /// Invite aceito.
    Accepted {
        #[serde(with = "time::serde::rfc3339")]
        accepted_at: OffsetDateTime,
    },
    /// Invite rejeitado.
    Rejected,
    /// Invite expirou (90s sem resposta).
    Expired,
}

/// Sessão de invite para um workspace (sem PIN — TOFU).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InviteSession {
    pub id: InviteId,
    pub workspace_id: crate::workspace::WorkspaceId,
    pub target_device_id: DeviceId,
    pub sender_device_id: DeviceId,
    pub state: InviteState,
}

impl InviteSession {
    /// Timeout do invite em segundos.
    pub const TIMEOUT_SECS: i64 = 90;

    /// Cria um novo invite.
    #[must_use]
    pub fn new(
        workspace_id: crate::workspace::WorkspaceId,
        target_device_id: DeviceId,
        sender_device_id: DeviceId,
    ) -> Self {
        Self {
            id: InviteId::new(),
            workspace_id,
            target_device_id,
            sender_device_id,
            state: InviteState::Pending {
                initiated_at: OffsetDateTime::now_utc(),
            },
        }
    }

    /// Marca o invite como entregue.
    pub fn mark_delivered(&mut self) -> Result<(), String> {
        if !matches!(self.state, InviteState::Pending { .. }) {
            return Err("Apenas invites Pending podem ser marcados como Delivered".to_string());
        }
        self.state = InviteState::Delivered {
            delivered_at: OffsetDateTime::now_utc(),
        };
        Ok(())
    }

    /// Aceita o invite.
    pub fn accept(&mut self) -> Result<(), String> {
        self.check_expiration();
        match &self.state {
            InviteState::Pending { .. } | InviteState::Delivered { .. } => {
                self.state = InviteState::Accepted {
                    accepted_at: OffsetDateTime::now_utc(),
                };
                Ok(())
            }
            _ => Err("Apenas invites Pending/Delivered podem ser aceitos".to_string()),
        }
    }

    /// Rejeita o invite.
    pub fn reject(&mut self) -> Result<(), String> {
        self.check_expiration();
        match &self.state {
            InviteState::Expired => Err("Invite já expirou".to_string()),
            InviteState::Rejected | InviteState::Accepted { .. } => {
                Err("Invite já foi resolvido".to_string())
            }
            _ => {
                self.state = InviteState::Rejected;
                Ok(())
            }
        }
    }

    /// Verifica se o invite expirou.
    #[must_use]
    pub fn is_expired(&self) -> bool {
        matches!(self.state, InviteState::Expired)
    }

    /// Atualiza o estado para Expired se passou o timeout.
    pub fn check_expiration(&mut self) {
        if self.is_expired() {
            return;
        }

        let initiated_at = match &self.state {
            InviteState::Pending { initiated_at } => *initiated_at,
            InviteState::Delivered { delivered_at } => *delivered_at,
            _ => return, // Já resolvido, não expira
        };

        let now = OffsetDateTime::now_utc();
        if (now - initiated_at).as_seconds_f64() > Self::TIMEOUT_SECS as f64 {
            self.state = InviteState::Expired;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_invite() -> InviteSession {
        InviteSession::new(
            crate::workspace::WorkspaceId::new(),
            DeviceId::new(),
            DeviceId::new(),
        )
    }

    #[test]
    fn new_invite_is_pending() {
        let invite = make_invite();
        assert!(matches!(invite.state, InviteState::Pending { .. }));
    }

    #[test]
    fn mark_delivered_succeeds() {
        let mut invite = make_invite();
        assert!(invite.mark_delivered().is_ok());
        assert!(matches!(invite.state, InviteState::Delivered { .. }));
    }

    #[test]
    fn mark_delivered_fails_twice() {
        let mut invite = make_invite();
        assert!(invite.mark_delivered().is_ok());
        assert!(invite.mark_delivered().is_err());
    }

    #[test]
    fn accept_from_pending() {
        let mut invite = make_invite();
        assert!(invite.accept().is_ok());
        assert!(matches!(invite.state, InviteState::Accepted { .. }));
    }

    #[test]
    fn accept_from_delivered() {
        let mut invite = make_invite();
        assert!(invite.mark_delivered().is_ok());
        assert!(invite.accept().is_ok());
        assert!(matches!(invite.state, InviteState::Accepted { .. }));
    }

    #[test]
    fn reject_from_pending() {
        let mut invite = make_invite();
        assert!(invite.reject().is_ok());
        assert!(matches!(invite.state, InviteState::Rejected));
    }

    #[test]
    fn reject_fails_after_accept() {
        let mut invite = make_invite();
        assert!(invite.accept().is_ok());
        assert!(invite.reject().is_err());
    }

    #[test]
    fn expires_after_90s() {
        let mut invite = make_invite();
        if let InviteState::Pending {
            ref mut initiated_at,
        } = invite.state
        {
            *initiated_at = OffsetDateTime::now_utc() - time::Duration::seconds(91);
        }
        invite.check_expiration();
        assert!(invite.is_expired());
    }

    #[test]
    fn not_expired_before_90s() {
        let invite = make_invite();
        assert!(!invite.is_expired());
    }

    #[test]
    fn serde_roundtrip() {
        let invite = make_invite();
        let json = serde_json::to_string(&invite).unwrap();
        let deserialized: InviteSession = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, invite.id);
        assert_eq!(deserialized.state, invite.state);
    }
}
