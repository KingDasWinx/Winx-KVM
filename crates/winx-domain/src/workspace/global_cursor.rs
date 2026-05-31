use crate::shared::ids::DeviceId;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Update de cursor remoto (DTO local, sem wire yet).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlobalCursorUpdate {
    pub x: i32,
    pub y: i32,
    pub active_device_id: DeviceId,
    pub monotonic_seq: u64,
}

/// Estado global compartilhado do mouse dentro de um workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlobalCursorState {
    /// Posição X do cursor global.
    pub x: i32,
    /// Posição Y do cursor global.
    pub y: i32,
    /// Device que controla atualmente.
    pub active_device_id: Option<DeviceId>,
    /// Sequência monotônica para evitar aplicar updates fora de ordem.
    pub monotonic_seq: u64,
    /// Último update recebido.
    #[serde(with = "time::serde::rfc3339")]
    pub last_seen_at: OffsetDateTime,
}

impl GlobalCursorState {
    /// Cria um novo estado de cursor zerado.
    #[must_use]
    pub fn new() -> Self {
        Self {
            x: 0,
            y: 0,
            active_device_id: None,
            monotonic_seq: 0,
            last_seen_at: OffsetDateTime::now_utc(),
        }
    }

    /// Atualiza localmente (quando este device move o mouse).
    pub fn update_local(&mut self, x: i32, y: i32, device_id: DeviceId) {
        self.x = x;
        self.y = y;
        self.active_device_id = Some(device_id);
        self.monotonic_seq = self.monotonic_seq.saturating_add(1);
        self.last_seen_at = OffsetDateTime::now_utc();
    }

    /// Aplica update remoto, validando seq monotônico.
    pub fn apply_remote(&mut self, update: GlobalCursorUpdate) -> Result<(), String> {
        if update.monotonic_seq <= self.monotonic_seq {
            return Err(format!(
                "cursor out of order: local seq={}, remote seq={}",
                self.monotonic_seq, update.monotonic_seq
            ));
        }
        self.x = update.x;
        self.y = update.y;
        self.active_device_id = Some(update.active_device_id);
        self.monotonic_seq = update.monotonic_seq;
        self.last_seen_at = OffsetDateTime::now_utc();
        Ok(())
    }
}

impl Default for GlobalCursorState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_is_zeroed() {
        let state = GlobalCursorState::new();
        assert_eq!(state.x, 0);
        assert_eq!(state.y, 0);
        assert!(state.active_device_id.is_none());
        assert_eq!(state.monotonic_seq, 0);
    }

    #[test]
    fn update_local() {
        let mut state = GlobalCursorState::new();
        let device = DeviceId::new();
        state.update_local(100, 200, device);

        assert_eq!(state.x, 100);
        assert_eq!(state.y, 200);
        assert_eq!(state.active_device_id, Some(device));
        assert_eq!(state.monotonic_seq, 1);
    }

    #[test]
    fn update_local_increments_seq() {
        let mut state = GlobalCursorState::new();
        let device = DeviceId::new();

        for expected_seq in 1..=5 {
            state.update_local(expected_seq as i32, 0, device);
            assert_eq!(state.monotonic_seq, expected_seq);
        }
    }

    #[test]
    fn apply_remote_accepts_higher_seq() {
        let mut state = GlobalCursorState::new();
        state.monotonic_seq = 10;

        let update = GlobalCursorUpdate {
            x: 500,
            y: 600,
            active_device_id: DeviceId::new(),
            monotonic_seq: 11,
        };

        assert!(state.apply_remote(update.clone()).is_ok());
        assert_eq!(state.x, 500);
        assert_eq!(state.y, 600);
        assert_eq!(state.monotonic_seq, 11);
    }

    #[test]
    fn apply_remote_rejects_equal_seq() {
        let mut state = GlobalCursorState::new();
        state.monotonic_seq = 10;

        let update = GlobalCursorUpdate {
            x: 500,
            y: 600,
            active_device_id: DeviceId::new(),
            monotonic_seq: 10,
        };

        assert!(state.apply_remote(update).is_err());
        // Estado não deve mudar
        assert_eq!(state.x, 0);
    }

    #[test]
    fn apply_remote_rejects_lower_seq() {
        let mut state = GlobalCursorState::new();
        state.monotonic_seq = 10;

        let update = GlobalCursorUpdate {
            x: 500,
            y: 600,
            active_device_id: DeviceId::new(),
            monotonic_seq: 9,
        };

        assert!(state.apply_remote(update).is_err());
        assert_eq!(state.x, 0);
    }

    #[test]
    fn global_cursor_apply_latency_under_10ms_p99() {
        let mut state = GlobalCursorState::new();
        let device = DeviceId::new();
        let mut durations = Vec::with_capacity(1000);

        for seq in 1..=1000_u64 {
            let start = std::time::Instant::now();
            let update = GlobalCursorUpdate {
                x: seq as i32,
                y: seq as i32,
                active_device_id: device,
                monotonic_seq: seq,
            };
            state
                .apply_remote(update)
                .expect("apply_remote must succeed");
            durations.push(start.elapsed());
        }

        durations.sort();
        let p99 = durations[990];
        assert!(
            p99 < std::time::Duration::from_millis(10),
            "p99 apply_remote latency {p99:?} >= 10ms"
        );
    }

    #[test]
    fn serde_roundtrip() {
        let mut state = GlobalCursorState::new();
        let device = DeviceId::new();
        state.update_local(250, 350, device);

        let json = serde_json::to_string(&state).unwrap();
        let deserialized: GlobalCursorState = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.x, state.x);
        assert_eq!(deserialized.y, state.y);
        assert_eq!(deserialized.monotonic_seq, state.monotonic_seq);
    }
}
