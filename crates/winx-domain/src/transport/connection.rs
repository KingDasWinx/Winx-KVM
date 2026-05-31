use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::shared::ids::{PeerId, SessionId};

/// Tipo de stream multiplexado na conexão QUIC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum StreamKind {
    Control = 0,
    Input = 1,
    Data = 2,
    Audio = 3,
}

impl StreamKind {
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn from_u8(b: u8) -> Option<Self> {
        match b {
            0 => Some(Self::Control),
            1 => Some(Self::Input),
            2 => Some(Self::Data),
            3 => Some(Self::Audio),
            _ => None,
        }
    }
}

/// Estado de uma conexão com um peer remoto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionState {
    Connecting,
    Connected { since: OffsetDateTime },
    Reconnecting { attempts: u32 },
    Disconnected,
}

/// Métricas de transporte expostas à UI (throttle ~1 Hz).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionStats {
    pub rtt_ms: u32,
    pub tx_bytes: u64,
    pub rx_bytes: u64,
    pub lost_packets: u64,
}

/// Agregado de conexão ativa com um peer pareado.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub id: SessionId,
    pub peer_id: PeerId,
    pub state: ConnectionState,
    pub stats: ConnectionStats,
    /// `true` se este device iniciou o handshake QUIC (`connect_peer`).
    pub is_outbound: bool,
    /// Preenchido quando a conexão foi aberta pelo fluxo de workspace (não KVM single).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via_workspace_id: Option<Uuid>,
}

impl Connection {
    #[must_use]
    pub fn new(peer_id: PeerId) -> Self {
        Self {
            id: SessionId::new(),
            peer_id,
            state: ConnectionState::Connecting,
            stats: ConnectionStats::default(),
            is_outbound: false,
            via_workspace_id: None,
        }
    }

    pub fn mark_connected(&mut self) {
        self.state = ConnectionState::Connected {
            since: OffsetDateTime::now_utc(),
        };
    }

    pub fn mark_reconnecting(&mut self) {
        self.state = match self.state {
            ConnectionState::Reconnecting { attempts } => ConnectionState::Reconnecting {
                attempts: attempts.saturating_add(1),
            },
            _ => ConnectionState::Reconnecting { attempts: 1 },
        };
    }

    pub fn mark_disconnected(&mut self) {
        self.state = ConnectionState::Disconnected;
    }

    pub fn update_stats(&mut self, stats: ConnectionStats) {
        self.stats = stats;
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            ConnectionState::Connecting
                | ConnectionState::Connected { .. }
                | ConnectionState::Reconnecting { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn connection_starts_connecting() {
        let peer_id = PeerId::from_uuid(Uuid::new_v4());
        let conn = Connection::new(peer_id);
        assert!(matches!(conn.state, ConnectionState::Connecting));
        assert!(conn.is_active());
    }

    #[test]
    fn mark_connected_records_time() {
        let peer_id = PeerId::from_uuid(Uuid::new_v4());
        let mut conn = Connection::new(peer_id);
        conn.mark_connected();
        assert!(matches!(conn.state, ConnectionState::Connected { .. }));
    }

    #[test]
    fn mark_reconnecting_increments_attempts() {
        let peer_id = PeerId::from_uuid(Uuid::new_v4());
        let mut conn = Connection::new(peer_id);
        conn.mark_reconnecting();
        conn.mark_reconnecting();
        assert_eq!(conn.state, ConnectionState::Reconnecting { attempts: 2 });
    }

    #[test]
    fn mark_disconnected_clears_state() {
        let peer_id = PeerId::from_uuid(Uuid::new_v4());
        let mut conn = Connection::new(peer_id);
        conn.mark_connected();
        conn.mark_disconnected();
        assert_eq!(conn.state, ConnectionState::Disconnected);
        assert!(!conn.is_active());
    }

    #[test]
    fn stats_updated_correctly() {
        let peer_id = PeerId::from_uuid(Uuid::new_v4());
        let mut conn = Connection::new(peer_id);
        let stats = ConnectionStats {
            rtt_ms: 12,
            tx_bytes: 100,
            rx_bytes: 200,
            lost_packets: 0,
        };
        conn.update_stats(stats);
        assert_eq!(conn.stats.rtt_ms, 12);
        assert_eq!(conn.stats.tx_bytes, 100);
    }

    #[test]
    fn stream_kind_roundtrip() {
        assert_eq!(StreamKind::Control.as_u8(), 0);
        assert_eq!(StreamKind::Input.as_u8(), 1);
        assert_eq!(StreamKind::Data.as_u8(), 2);
        assert_eq!(StreamKind::Audio.as_u8(), 3);

        assert_eq!(StreamKind::from_u8(0), Some(StreamKind::Control));
        assert_eq!(StreamKind::from_u8(1), Some(StreamKind::Input));
        assert_eq!(StreamKind::from_u8(2), Some(StreamKind::Data));
        assert_eq!(StreamKind::from_u8(3), Some(StreamKind::Audio));
        assert_eq!(StreamKind::from_u8(99), None);
    }
}
