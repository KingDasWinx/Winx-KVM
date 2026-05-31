//! Wire format do Winx-KVM.
//!
//! Define todas as mensagens trocadas via QUIC entre dois peers. Serialização
//! é **bincode**; o frame externo carrega a versão do protocolo para futuras
//! evoluções incompatíveis.
//!
//! Veja [README §"Protocolo de Comunicação"][r] para a visão geral dos
//! streams (`Control`, `Input`, `Audio`, `Data`).
//!
//! [r]: ../../../../README.md

#![forbid(unsafe_code)]

pub mod clipboard;
pub mod diagnostics;
pub mod input;
pub mod layout;
pub mod pairing;
pub mod workspace;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use clipboard::ClipboardPayload;
pub use diagnostics::DiagPing;
pub use layout::{KvmSessionLayoutPayload, MonitorLayoutPayload, MonitorRectPayload, PeerMonitorsPayload};
pub use input::{InputEventDto, InputPayload};
pub use pairing::{
    decode_pairing_datagram, encode_pairing_body, encode_pairing_datagram,
    message_requires_signature, pin_commitment, PairingCancel, PairingConfirm, PairingMessage,
    PairingProtocolError, PairingRequest, PairingResponse, PAIRING_PROTOCOL_VERSION,
};

/// Versão atual do wire format. Incremente quando mudar `Payload` de forma
/// incompatível.
/// W2: bumped from 4 to 5 to add WorkspaceInviteMessage support.
/// W3: bumped to 6 for PeerMonitorsAnnounce / KvmLayoutShare (single connection layout sync).
/// W4: bumped to 7 for KvmSessionLayoutPayload (layout canônico compartilhado).
pub const PROTOCOL_VERSION: u16 = 7;

/// Magic bytes para início de stream (identifica tipo de stream).
/// Formato: [MAGIC 4 bytes (b"WXSK")][kind u8] = 5 bytes
pub const STREAM_HEADER_MAGIC: &[u8; 4] = b"WXSK";

/// Frame externo de todas as mensagens. Carrega a versão do protocolo para
/// que peers detectem incompatibilidades cedo no handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frame {
    pub version: u16,
    pub payload: Payload,
}

impl Frame {
    #[must_use]
    pub fn new(payload: Payload) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            payload,
        }
    }
}

/// Payload de uma mensagem. Variantes serão adicionadas sprint a sprint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Payload {
    /// Heartbeat keep-alive no stream Control.
    Heartbeat,
    /// Resposta ao heartbeat no stream Control.
    HeartbeatAck,
    /// Solicita abertura de um stream nomeado (`kind`: 0=Control, 1=Input, 2=Data).
    OpenStream { kind: u8 },
    /// Encerra um stream nomeado.
    CloseStream { kind: u8 },
    /// Evento de mouse/teclado no stream Input.
    Input(input::InputPayload),
    /// Notifica mudança de foco no stream Control (`None` = foco local).
    FocusSwitch { target_peer: Option<uuid::Uuid> },
    /// Texto de clipboard no stream Data.
    Clipboard(clipboard::ClipboardPayload),
    /// Monitores locais do peer (stream Data, single connection).
    PeerMonitorsAnnounce(layout::PeerMonitorsPayload),
    /// Layout canônico compartilhado da sessão KVM (stream Data).
    KvmLayoutShare(layout::KvmSessionLayoutPayload),
}

/// Erros ao serializar/desserializar frames.
#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("falha ao codificar bincode: {0}")]
    Encode(bincode::Error),

    #[error("falha ao decodificar bincode: {0}")]
    Decode(bincode::Error),

    #[error("versão de protocolo incompatível: esperada {expected}, recebida {got}")]
    VersionMismatch { expected: u16, got: u16 },
}

/// Serializa um frame para bytes (bincode).
pub fn encode(frame: &Frame) -> Result<Vec<u8>, ProtocolError> {
    bincode::serialize(frame).map_err(ProtocolError::Encode)
}

/// Desserializa um frame, validando a versão.
pub fn decode(bytes: &[u8]) -> Result<Frame, ProtocolError> {
    let frame: Frame = bincode::deserialize(bytes).map_err(ProtocolError::Decode)?;
    if frame.version != PROTOCOL_VERSION {
        return Err(ProtocolError::VersionMismatch {
            expected: PROTOCOL_VERSION,
            got: frame.version,
        });
    }
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_roundtrips() {
        let frame = Frame::new(Payload::Heartbeat);
        let bytes = encode(&frame).unwrap();
        let back = decode(&bytes).unwrap();
        assert_eq!(back.version, PROTOCOL_VERSION);
        assert!(matches!(back.payload, Payload::Heartbeat));
    }

    #[test]
    fn input_payload_roundtrips() {
        let p = InputPayload {
            seq: 1,
            event: InputEventDto::MouseMove { dx: 1, dy: 2 },
        };
        let frame = Frame::new(Payload::Input(p));
        let bytes = encode(&frame).unwrap();
        let back = decode(&bytes).unwrap();
        assert!(matches!(back.payload, Payload::Input(_)));
    }

    #[test]
    fn clipboard_text_roundtrips() {
        let p = ClipboardPayload {
            origin_peer_id: uuid::Uuid::new_v4(),
            content_hash: [1u8; 32],
            text: "copiado".into(),
        };
        let frame = Frame::new(Payload::Clipboard(p));
        let bytes = encode(&frame).unwrap();
        let back = decode(&bytes).unwrap();
        assert!(matches!(back.payload, Payload::Clipboard(_)));
    }

    #[test]
    fn wrong_version_rejected() {
        let mut frame = Frame::new(Payload::Heartbeat);
        frame.version = 999;
        let bytes = bincode::serialize(&frame).unwrap();
        let err = decode(&bytes).unwrap_err();
        assert!(matches!(err, ProtocolError::VersionMismatch { .. }));
    }
}
