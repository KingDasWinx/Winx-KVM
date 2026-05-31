//! Erros de domínio.
//!
//! Erros carregam um **code estável** (`DomainErrorCode`) que é serializado
//! para a UI e traduzido via i18n no frontend. A mensagem em si pode ser
//! útil para logs/devs, mas a UI **nunca** deve exibi-la diretamente —
//! sempre usar `t(code)`.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Code estável que identifica o tipo de erro de forma traduzível.
///
/// Formato: `<bounded_context>.<error_kind>`. Esses códigos são a fonte
/// da verdade para mensagens de erro localizadas no frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainErrorCode {
    // Identity
    IdentityKeyGenerationFailed,
    IdentityPeerNotFound,
    IdentityUsernameInvalid,

    // Pairing
    PairingPinExpired,
    PairingPinMismatch,
    PairingTooManyAttempts,
    PairingSessionNotFound,
    PairingSessionExists,

    // Transport
    TransportConnectionFailed,
    TransportProtocolMismatch,
    TransportPeerNotTrusted,

    // InputControl
    InputCaptureFailed,
    InputInjectionFailed,
    InputMonitorNotFound,

    // Media
    MediaVbCableMissing,
    MediaDeviceUnavailable,
    MediaCodecFailed,

    // DataExchange
    ClipboardFormatUnsupported,
    ClipboardPayloadTooLarge,
    FileTransferHashMismatch,
    FileTransferCancelled,
    FileTransferTooLarge,

    // Workspace
    WorkspaceNameEmpty,
    WorkspaceOwnerNotMember,
    WorkspaceMemberAlreadyExists,
    WorkspaceMemberNotFound,
    WorkspaceCannotRemoveOwner,
    WorkspaceMustHaveOneMember,
    WorkspaceInviteExpired,
    WorkspaceInviteAlreadyResolved,
    WorkspaceCursorOutOfOrder,
    WorkspaceMirrorImmutable,
    WorkspaceNotOwner,
    WorkspaceVersionStale,
    WorkspaceConflict,
    WorkspaceNotMember,
    WorkspacePeerNotDiscovered,

    // Geral
    InternalError,
}

impl DomainErrorCode {
    /// Retorna o code como string estável (formato `bounded.kind`).
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::IdentityKeyGenerationFailed => "identity.key_generation_failed",
            Self::IdentityPeerNotFound => "identity.peer_not_found",
            Self::IdentityUsernameInvalid => "identity.username_invalid",

            Self::PairingPinExpired => "pairing.pin_expired",
            Self::PairingPinMismatch => "pairing.pin_mismatch",
            Self::PairingTooManyAttempts => "pairing.too_many_attempts",
            Self::PairingSessionNotFound => "pairing.session_not_found",
            Self::PairingSessionExists => "pairing.session_exists",

            Self::TransportConnectionFailed => "transport.connection_failed",
            Self::TransportProtocolMismatch => "transport.protocol_mismatch",
            Self::TransportPeerNotTrusted => "transport.peer_not_trusted",

            Self::InputCaptureFailed => "input.capture_failed",
            Self::InputInjectionFailed => "input.injection_failed",
            Self::InputMonitorNotFound => "input.monitor_not_found",

            Self::MediaVbCableMissing => "media.vb_cable_missing",
            Self::MediaDeviceUnavailable => "media.device_unavailable",
            Self::MediaCodecFailed => "media.codec_failed",

            Self::ClipboardFormatUnsupported => "clipboard.format_unsupported",
            Self::ClipboardPayloadTooLarge => "clipboard.payload_too_large",
            Self::FileTransferHashMismatch => "file_transfer.hash_mismatch",
            Self::FileTransferCancelled => "file_transfer.cancelled",
            Self::FileTransferTooLarge => "file_transfer.too_large",

            Self::WorkspaceNameEmpty => "workspace.name_empty",
            Self::WorkspaceOwnerNotMember => "workspace.owner_not_member",
            Self::WorkspaceMemberAlreadyExists => "workspace.member_already_exists",
            Self::WorkspaceMemberNotFound => "workspace.member_not_found",
            Self::WorkspaceCannotRemoveOwner => "workspace.cannot_remove_owner",
            Self::WorkspaceMustHaveOneMember => "workspace.must_have_one_member",
            Self::WorkspaceInviteExpired => "workspace.invite_expired",
            Self::WorkspaceInviteAlreadyResolved => "workspace.invite_already_resolved",
            Self::WorkspaceCursorOutOfOrder => "workspace.cursor_out_of_order",
            Self::WorkspaceMirrorImmutable => "workspace.mirror_immutable",
            Self::WorkspaceNotOwner => "workspace.not_owner",
            Self::WorkspaceVersionStale => "workspace.version_stale",
            Self::WorkspaceConflict => "workspace.conflict",
            Self::WorkspaceNotMember => "workspace.not_member",
            Self::WorkspacePeerNotDiscovered => "workspace.peer_not_discovered",

            Self::InternalError => "general.internal_error",
        }
    }
}

/// Erro de domínio. Sempre carrega um `DomainErrorCode` traduzível.
#[derive(Debug, Error, Clone, Serialize, Deserialize)]
#[error("{code:?}: {message}")]
pub struct DomainError {
    pub code: DomainErrorCode,
    /// Mensagem técnica para logs/devs. **Não** exiba na UI — use `code`.
    pub message: String,
}

impl DomainError {
    pub fn new(code: DomainErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn code_str(&self) -> &'static str {
        self.code.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_strings_are_stable() {
        assert_eq!(
            DomainErrorCode::PairingPinExpired.as_str(),
            "pairing.pin_expired"
        );
        assert_eq!(
            DomainErrorCode::MediaVbCableMissing.as_str(),
            "media.vb_cable_missing"
        );
    }

    #[test]
    fn error_roundtrips_json() {
        let err = DomainError::new(DomainErrorCode::PairingPinExpired, "PIN expired after 90s");
        let json = serde_json::to_string(&err).unwrap();
        let back: DomainError = serde_json::from_str(&json).unwrap();
        assert_eq!(back.code, DomainErrorCode::PairingPinExpired);
    }
}
