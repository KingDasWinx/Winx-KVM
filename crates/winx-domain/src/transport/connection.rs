use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::shared::ids::{PeerId, SessionId};

/// Tipo de stream multiplexado na conexão QUIC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamKind {
    Control,
    Input,
    Data,
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
}

impl Connection {
    #[must_use]
    pub fn new(peer_id: PeerId) -> Self {
        Self {
            id: SessionId::new(),
            peer_id,
            state: ConnectionState::Connecting,
            stats: ConnectionStats::default(),
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
}
