//! Mensagens de pareamento pré-confiança (UDP porta 7879).
//!
//! Formato do datagrama:
//! `[MAGIC 4][version u16 LE][body_len u32 LE][body bincode][signature 64 opcional]`

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

pub const PAIRING_PROTOCOL_VERSION: u16 = 1;
pub const PAIRING_MAGIC: [u8; 4] = *b"WINX";
pub const PAIRING_SIGNATURE_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PairingMessage {
    Request(PairingRequest),
    Response(PairingResponse),
    Confirm(PairingConfirm),
    Cancel(PairingCancel),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingRequest {
    pub session_id: Uuid,
    pub initiator_peer_id: Uuid,
    pub initiator_username: String,
    pub initiator_ephemeral: [u8; 32],
    pub initiator_pubkey: [u8; 32],
    pub pin_commitment: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingResponse {
    pub session_id: Uuid,
    pub responder_peer_id: Uuid,
    pub responder_ephemeral: [u8; 32],
    pub responder_pubkey: [u8; 32],
    pub pin: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingConfirm {
    pub session_id: Uuid,
    pub confirmer_peer_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingCancel {
    pub session_id: Uuid,
    pub reason: String,
}

#[derive(Debug, Error)]
pub enum PairingProtocolError {
    #[error("datagrama muito curto")]
    TooShort,

    #[error("magic inválido")]
    BadMagic,

    #[error("versão de pairing incompatível: esperada {expected}, recebida {got}")]
    VersionMismatch { expected: u16, got: u16 },

    #[error("falha ao codificar bincode: {0}")]
    Encode(bincode::Error),

    #[error("falha ao decodificar bincode: {0}")]
    Decode(bincode::Error),

    #[error("corpo com tamanho inválido")]
    InvalidBodyLength,

    #[error("assinatura ausente onde obrigatória")]
    MissingSignature,

    #[error("assinatura com tamanho inválido")]
    InvalidSignature,
}

/// Compromisso do PIN: `SHA-256(session_id || pin_utf8)`.
#[must_use]
pub fn pin_commitment(session_id: &Uuid, pin: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(session_id.as_bytes());
    h.update(pin.as_bytes());
    h.finalize().into()
}

/// Corpo bincode da mensagem (sem envelope).
pub fn encode_pairing_body(msg: &PairingMessage) -> Result<Vec<u8>, PairingProtocolError> {
    bincode::serialize(msg).map_err(PairingProtocolError::Encode)
}

/// Monta datagrama completo. `signature` deve ter 64 bytes quando presente.
pub fn encode_pairing_datagram(
    msg: &PairingMessage,
    signature: Option<&[u8; 64]>,
) -> Result<Vec<u8>, PairingProtocolError> {
    let body = encode_pairing_body(msg)?;
    let sig_len = signature.map_or(0, |_| PAIRING_SIGNATURE_LEN);
    let mut out = Vec::with_capacity(10 + body.len() + sig_len);
    out.extend_from_slice(&PAIRING_MAGIC);
    out.extend_from_slice(&PAIRING_PROTOCOL_VERSION.to_le_bytes());
    out.extend_from_slice(
        &(u32::try_from(body.len()).map_err(|_| PairingProtocolError::InvalidBodyLength)?)
            .to_le_bytes(),
    );
    out.extend_from_slice(&body);
    if let Some(sig) = signature {
        out.extend_from_slice(sig);
    }
    Ok(out)
}

/// Retorna `(mensagem, assinatura opcional)`.
pub fn decode_pairing_datagram(
    bytes: &[u8],
) -> Result<(PairingMessage, Option<[u8; 64]>), PairingProtocolError> {
    if bytes.len() < 10 {
        return Err(PairingProtocolError::TooShort);
    }
    if bytes[0..4] != PAIRING_MAGIC {
        return Err(PairingProtocolError::BadMagic);
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != PAIRING_PROTOCOL_VERSION {
        return Err(PairingProtocolError::VersionMismatch {
            expected: PAIRING_PROTOCOL_VERSION,
            got: version,
        });
    }
    let body_len = u32::from_le_bytes([bytes[6], bytes[7], bytes[8], bytes[9]]) as usize;
    let body_start: usize = 10;
    let body_end = body_start
        .checked_add(body_len)
        .ok_or(PairingProtocolError::InvalidBodyLength)?;
    if body_end > bytes.len() {
        return Err(PairingProtocolError::InvalidBodyLength);
    }
    let body = &bytes[body_start..body_end];
    let msg: PairingMessage = bincode::deserialize(body).map_err(PairingProtocolError::Decode)?;

    let signature = if body_end == bytes.len() {
        None
    } else if body_end + PAIRING_SIGNATURE_LEN == bytes.len() {
        let mut sig = [0u8; 64];
        sig.copy_from_slice(&bytes[body_end..]);
        Some(sig)
    } else {
        return Err(PairingProtocolError::InvalidSignature);
    };

    Ok((msg, signature))
}

/// Mensagens assinadas devem carregar assinatura Ed25519.
pub fn message_requires_signature(msg: &PairingMessage) -> bool {
    !matches!(msg, PairingMessage::Request(_))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_request_roundtrips() {
        let msg = PairingMessage::Request(PairingRequest {
            session_id: Uuid::new_v4(),
            initiator_peer_id: Uuid::new_v4(),
            initiator_username: "pc-a".into(),
            initiator_ephemeral: [1u8; 32],
            initiator_pubkey: [2u8; 32],
            pin_commitment: [3u8; 32],
        });
        let bytes = encode_pairing_datagram(&msg, None).unwrap();
        let (decoded, sig) = decode_pairing_datagram(&bytes).unwrap();
        assert_eq!(decoded, msg);
        assert!(sig.is_none());
    }

    #[test]
    fn pairing_response_with_signature_roundtrips() {
        let msg = PairingMessage::Response(PairingResponse {
            session_id: Uuid::new_v4(),
            responder_peer_id: Uuid::new_v4(),
            responder_ephemeral: [4u8; 32],
            responder_pubkey: [5u8; 32],
            pin: "123456".into(),
        });
        let sig = [7u8; 64];
        let bytes = encode_pairing_datagram(&msg, Some(&sig)).unwrap();
        let (decoded, got_sig) = decode_pairing_datagram(&bytes).unwrap();
        assert_eq!(decoded, msg);
        assert_eq!(got_sig, Some(sig));
    }

    #[test]
    fn pin_commitment_is_deterministic() {
        let sid = Uuid::new_v4();
        assert_eq!(
            pin_commitment(&sid, "042819"),
            pin_commitment(&sid, "042819")
        );
        assert_ne!(
            pin_commitment(&sid, "042819"),
            pin_commitment(&sid, "042820")
        );
    }

    #[test]
    fn bad_magic_rejected() {
        let mut bytes = encode_pairing_datagram(
            &PairingMessage::Cancel(PairingCancel {
                session_id: Uuid::new_v4(),
                reason: "x".into(),
            }),
            None,
        )
        .unwrap();
        bytes[0] = b'X';
        assert!(matches!(
            decode_pairing_datagram(&bytes),
            Err(PairingProtocolError::BadMagic)
        ));
    }
}
