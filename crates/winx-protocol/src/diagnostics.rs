//! Datagramas de diagnóstico na porta de pairing (7879), independentes do fluxo PIN.

pub const DIAG_MAGIC: [u8; 4] = *b"WINP";
pub const DIAG_PING: u8 = 0x01;
pub const DIAG_PONG: u8 = 0x02;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagPing {
    pub nonce: u64,
}

impl DiagPing {
    #[must_use]
    pub fn encode(self) -> [u8; 13] {
        let mut buf = [0u8; 13];
        buf[..4].copy_from_slice(&DIAG_MAGIC);
        buf[4] = DIAG_PING;
        buf[5..13].copy_from_slice(&self.nonce.to_le_bytes());
        buf
    }

    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < 13 || data[..4] != DIAG_MAGIC || data[4] != DIAG_PING {
            return None;
        }
        let nonce = u64::from_le_bytes(data[5..13].try_into().ok()?);
        Some(Self { nonce })
    }

    #[must_use]
    pub fn pong_bytes(self) -> [u8; 13] {
        let mut buf = self.encode();
        buf[4] = DIAG_PONG;
        buf
    }

    pub fn decode_pong(data: &[u8]) -> Option<u64> {
        if data.len() < 13 || data[..4] != DIAG_MAGIC || data[4] != DIAG_PONG {
            return None;
        }
        Some(u64::from_le_bytes(data[5..13].try_into().ok()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_pong_roundtrip_nonce() {
        let ping = DiagPing { nonce: 42 };
        let pong = ping.pong_bytes();
        assert_eq!(DiagPing::decode_pong(&pong), Some(42));
    }

    #[test]
    fn rejects_wrong_magic() {
        assert!(DiagPing::decode(&[0u8; 13]).is_none());
    }
}
