use constant_time_eq::constant_time_eq;
use rand::Rng;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::shared::{
    ids::{PeerId, SessionId},
    DomainError, DomainErrorCode,
};

/// PIN de 6 dígitos gerado criptograficamente para confirmar o pareamento.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pin(String);

impl Pin {
    /// Gera um PIN de 6 dígitos usando `OsRng`.
    #[must_use]
    pub fn generate() -> Self {
        let n: u32 = rand::rngs::OsRng.gen_range(0..1_000_000);
        Self(format!("{n:06}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Comparação em tempo constante para evitar timing attacks.
    pub fn verify(&self, input: &str) -> bool {
        // Padeia para o mesmo comprimento antes de comparar
        let expected = self.0.as_bytes();
        let got = input.as_bytes();
        expected.len() == got.len() && constant_time_eq(expected, got)
    }
}

/// Papel deste device na sessão de pareamento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PairingRole {
    Initiator,
    Responder,
}

/// Estado da máquina de estados de uma `PairingSession`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PairingState {
    /// Initiator: PIN gerado e exibido na UI.
    Initiated {
        pin: Pin,
        attempts: u8,
        initiated_at: OffsetDateTime,
    },
    /// Responder: aguardando o usuário digitar o PIN do initiator.
    AwaitingPin {
        attempts: u8,
        initiated_at: OffsetDateTime,
    },
    Completed,
    Cancelled,
    /// PIN esgotou tentativas ou expirou antes de ser confirmado.
    Failed,
}

/// Sessão de pareamento entre dois devices.
///
/// Criada pelo initiator via `PairingService::initiate_pairing` ou pelo
/// responder via `accept_pairing`. Vive em memória no `PairingService`;
/// é removida quando completa, cancelada ou falhada.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingSession {
    pub id: SessionId,
    pub role: PairingRole,
    pub peer_id: PeerId,
    /// Chave pública X25519 efêmera deste lado (32 bytes).
    pub ephemeral_public: [u8; 32],
    /// Chave pública X25519 efêmera do peer.
    pub peer_ephemeral_public: Option<[u8; 32]>,
    /// Ed25519 do initiator (preenchido no responder ao aceitar pedido).
    pub initiator_pubkey: Option<[u8; 32]>,
    pub state: PairingState,
}

impl PairingSession {
    pub const TIMEOUT_SECS: i64 = 90;
    pub const MAX_ATTEMPTS: u8 = 3;

    pub fn initiator(peer_id: PeerId, ephemeral_public: [u8; 32]) -> Self {
        Self {
            id: SessionId::new(),
            role: PairingRole::Initiator,
            peer_id,
            ephemeral_public,
            peer_ephemeral_public: None,
            initiator_pubkey: None,
            state: PairingState::Initiated {
                pin: Pin::generate(),
                attempts: 0,
                initiated_at: OffsetDateTime::now_utc(),
            },
        }
    }

    /// Cria sessão no responder com o mesmo `session_id` enviado pelo initiator.
    pub fn responder(
        session_id: SessionId,
        peer_id: PeerId,
        initiator_ephemeral: [u8; 32],
        ephemeral_public: [u8; 32],
        initiator_pubkey: [u8; 32],
    ) -> Self {
        Self {
            id: session_id,
            role: PairingRole::Responder,
            peer_id,
            ephemeral_public,
            peer_ephemeral_public: Some(initiator_ephemeral),
            initiator_pubkey: Some(initiator_pubkey),
            state: PairingState::AwaitingPin {
                attempts: 0,
                initiated_at: OffsetDateTime::now_utc(),
            },
        }
    }

    /// Compat: alias para `initiator`.
    pub fn new(peer_id: PeerId, ephemeral_public: [u8; 32]) -> Self {
        Self::initiator(peer_id, ephemeral_public)
    }

    pub fn is_responder(&self) -> bool {
        self.role == PairingRole::Responder
    }

    /// Retorna o PIN se a sessão ainda estiver no estado `Initiated`.
    pub fn pin(&self) -> Option<&Pin> {
        match &self.state {
            PairingState::Initiated { pin, .. } => Some(pin),
            _ => None,
        }
    }

    pub fn is_expired(&self) -> bool {
        let initiated_at = match &self.state {
            PairingState::Initiated { initiated_at, .. }
            | PairingState::AwaitingPin { initiated_at, .. } => *initiated_at,
            _ => return false,
        };
        let elapsed = OffsetDateTime::now_utc() - initiated_at;
        elapsed.whole_seconds() >= Self::TIMEOUT_SECS
    }

    /// Verifica o PIN fornecido pelo usuário.
    ///
    /// Incrementa o contador de tentativas. Transiciona para `Failed` se o
    /// limite for atingido ou se o PIN expirou.
    pub fn verify_pin(&mut self, input: &str) -> Result<(), DomainError> {
        let (pin, attempts, initiated_at) = match &mut self.state {
            PairingState::Initiated {
                pin,
                attempts,
                initiated_at,
            } => (pin.clone(), attempts, *initiated_at),
            _ => {
                return Err(DomainError::new(
                    DomainErrorCode::PairingSessionNotFound,
                    "sessão não está no estado Initiated",
                ))
            }
        };

        let elapsed = OffsetDateTime::now_utc() - initiated_at;
        if elapsed.whole_seconds() >= Self::TIMEOUT_SECS {
            self.state = PairingState::Failed;
            return Err(DomainError::new(
                DomainErrorCode::PairingPinExpired,
                "PIN expirado",
            ));
        }

        *attempts += 1;

        if !pin.verify(input) {
            if *attempts >= Self::MAX_ATTEMPTS {
                self.state = PairingState::Failed;
                return Err(DomainError::new(
                    DomainErrorCode::PairingTooManyAttempts,
                    "tentativas esgotadas",
                ));
            }
            return Err(DomainError::new(
                DomainErrorCode::PairingPinMismatch,
                "PIN incorreto",
            ));
        }

        Ok(())
    }

    /// Marca a sessão como `Completed` e registra a chave pública efêmera do peer.
    pub fn complete(&mut self, peer_ephemeral_public: [u8; 32]) {
        self.peer_ephemeral_public = Some(peer_ephemeral_public);
        self.state = PairingState::Completed;
    }

    /// Cancela a sessão.
    pub fn cancel(&mut self) {
        self.state = PairingState::Cancelled;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_session() -> PairingSession {
        PairingSession::new(PeerId::from_uuid(uuid::Uuid::new_v4()), [0u8; 32])
    }

    #[test]
    fn pin_has_6_digits() {
        for _ in 0..20 {
            let p = Pin::generate();
            assert_eq!(
                p.as_str().len(),
                6,
                "PIN deve ter 6 dígitos: {}",
                p.as_str()
            );
            assert!(p.as_str().chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn pin_verify_wrong_pin_returns_error() {
        let mut s = make_session();
        let result = s.verify_pin("999999");
        // PIN gerado aleatoriamente — probabilidade ínfima de ser "999999"
        // Não podemos garantir que é erro sem conhecer o PIN real; testamos
        // a mecânica via PIN extraído da sessão.
        let pin_str = match &s.state {
            PairingState::Initiated { pin, .. } => pin.as_str().to_string(),
            // verify_pin pode ter transitado para Failed se acertou por acaso
            _ => return,
        };
        if pin_str != "999999" {
            assert!(result.is_err());
        }
    }

    #[test]
    fn pin_verify_correct_pin_succeeds() {
        let mut s = make_session();
        let pin_str = match &s.state {
            PairingState::Initiated { pin, .. } => pin.as_str().to_string(),
            _ => panic!("estado inesperado"),
        };
        assert!(s.verify_pin(&pin_str).is_ok());
    }

    #[test]
    fn pin_fails_after_3_attempts() {
        let mut s = make_session();
        // Garante que o PIN correto não é "000000"
        let pin_str = match &s.state {
            PairingState::Initiated { pin, .. } => pin.as_str().to_string(),
            _ => panic!("estado inesperado"),
        };
        let wrong = if pin_str == "000000" {
            "111111"
        } else {
            "000000"
        };

        for _ in 0..PairingSession::MAX_ATTEMPTS {
            let _ = s.verify_pin(wrong);
        }
        assert!(matches!(s.state, PairingState::Failed));
    }

    #[test]
    fn pin_expires_after_90s() {
        let mut s = make_session();
        // Injeta `initiated_at` no passado
        if let PairingState::Initiated {
            ref mut initiated_at,
            ..
        } = s.state
        {
            *initiated_at = OffsetDateTime::now_utc() - time::Duration::seconds(91);
        }
        let result = s.verify_pin("000000");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, DomainErrorCode::PairingPinExpired);
        assert!(matches!(s.state, PairingState::Failed));
    }

    #[test]
    fn session_cancel_sets_state() {
        let mut s = make_session();
        s.cancel();
        assert!(matches!(s.state, PairingState::Cancelled));
    }

    #[test]
    fn session_complete_records_peer_key() {
        let mut s = make_session();
        let peer_pub = [0xABu8; 32];
        s.complete(peer_pub);
        assert!(matches!(s.state, PairingState::Completed));
        assert_eq!(s.peer_ephemeral_public, Some(peer_pub));
    }

    #[test]
    fn responder_uses_given_session_id() {
        let sid = SessionId::new();
        let peer_id = PeerId::from_uuid(uuid::Uuid::new_v4());
        let s = PairingSession::responder(sid, peer_id, [1u8; 32], [2u8; 32], [3u8; 32]);
        assert_eq!(s.id, sid);
        assert!(s.pin().is_none());
        assert!(s.is_responder());
    }
}
