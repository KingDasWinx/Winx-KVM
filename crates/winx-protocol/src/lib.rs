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

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Versão atual do wire format. Incremente quando mudar `Payload` de forma
/// incompatível.
pub const PROTOCOL_VERSION: u16 = 1;

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
    fn wrong_version_rejected() {
        let mut frame = Frame::new(Payload::Heartbeat);
        frame.version = 999;
        let bytes = bincode::serialize(&frame).unwrap();
        let err = decode(&bytes).unwrap_err();
        assert!(matches!(err, ProtocolError::VersionMismatch { .. }));
    }
}
