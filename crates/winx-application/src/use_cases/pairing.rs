use std::{collections::HashMap, sync::Arc};

use rand::rngs::OsRng;
use tokio::sync::Mutex;
use tracing::{info, warn};
use winx_domain::{
    identity::{PublicKey, TrustedPeer},
    pairing::{
        events::{PairingCancelled, PairingCompleted, PairingFailed, PairingRequested},
        PairingSession,
    },
    shared::{
        ids::{PeerId, SessionId},
        DomainErrorCode,
    },
    DomainError, DomainEvent,
};
use x25519_dalek::StaticSecret;

use crate::{bus::EventBus, ports::IdentityStore};

pub struct PairingService {
    sessions: Arc<Mutex<HashMap<SessionId, PairingSession>>>,
    ephemeral_secrets: Arc<Mutex<HashMap<SessionId, StaticSecret>>>,
    identity_store: Arc<dyn IdentityStore>,
    bus: EventBus,
}

impl PairingService {
    pub fn new(identity_store: Arc<dyn IdentityStore>, bus: EventBus) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            ephemeral_secrets: Arc::new(Mutex::new(HashMap::new())),
            identity_store,
            bus,
        }
    }

    /// Inicia o pareamento como **initiator** (o lado que propõe).
    ///
    /// Retorna `(session_id, pin)`. O PIN deve ser exibido na UI do initiator
    /// para que o responder possa confirmar.
    pub async fn initiate_pairing(
        &self,
        peer_id: PeerId,
        peer_username: &str,
    ) -> Result<(SessionId, String), DomainError> {
        let mut sessions = self.sessions.lock().await;

        if sessions.values().any(|s| s.peer_id == peer_id) {
            return Err(DomainError::new(
                DomainErrorCode::PairingSessionExists,
                "já existe sessão ativa para este peer",
            ));
        }

        let secret = StaticSecret::random_from_rng(OsRng);
        let public: x25519_dalek::PublicKey = (&secret).into();
        let session = PairingSession::new(peer_id, public.to_bytes());

        let session_id = session.id;
        let pin = session
            .pin()
            .map(|p| p.as_str().to_string())
            .unwrap_or_default();

        self.ephemeral_secrets
            .lock()
            .await
            .insert(session_id, secret);
        sessions.insert(session_id, session);

        self.bus
            .publish(DomainEvent::PairingRequested(PairingRequested {
                session_id,
                peer_id,
                peer_username: peer_username.to_string(),
                pin: pin.clone(),
            }));

        info!(%session_id, %peer_id, "pareamento iniciado");
        Ok((session_id, pin))
    }

    /// Inicia a sessão no lado **responder** (o lado que recebe o pedido).
    ///
    /// `peer_ephemeral_public_bytes` são os 32 bytes da chave X25519 efêmera
    /// do initiator (recebidos via protocolo — no MVP, passados pelo frontend).
    pub async fn accept_pairing(
        &self,
        peer_id: PeerId,
        peer_ephemeral_public_bytes: [u8; 32],
    ) -> Result<(SessionId, String), DomainError> {
        let mut sessions = self.sessions.lock().await;

        if sessions.values().any(|s| s.peer_id == peer_id) {
            return Err(DomainError::new(
                DomainErrorCode::PairingSessionExists,
                "já existe sessão ativa para este peer",
            ));
        }

        let secret = StaticSecret::random_from_rng(OsRng);
        let public: x25519_dalek::PublicKey = (&secret).into();
        let mut session = PairingSession::new(peer_id, public.to_bytes());
        session.peer_ephemeral_public = Some(peer_ephemeral_public_bytes);

        let session_id = session.id;
        let pin = session
            .pin()
            .map(|p| p.as_str().to_string())
            .unwrap_or_default();

        self.ephemeral_secrets
            .lock()
            .await
            .insert(session_id, secret);
        sessions.insert(session_id, session);

        info!(%session_id, %peer_id, "pareamento aceito (responder)");
        Ok((session_id, pin))
    }

    /// Submete o PIN confirmado pelo usuário.
    ///
    /// Valida o PIN, deriva o shared secret X25519, cria o `TrustedPeer`
    /// e persiste em `peers.toml`. Publica `PairingCompleted` no bus.
    pub async fn submit_pin(
        &self,
        session_id: SessionId,
        pin_input: &str,
        peer_public_key_bytes: [u8; 32],
        peer_username: &str,
    ) -> Result<(), DomainError> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions.get_mut(&session_id).ok_or_else(|| {
            DomainError::new(
                DomainErrorCode::PairingSessionNotFound,
                "sessão não encontrada",
            )
        })?;

        let peer_id = session.peer_id;

        session.verify_pin(pin_input)?;

        let peer_ephemeral_pub_bytes = session.peer_ephemeral_public.ok_or_else(|| {
            DomainError::new(
                DomainErrorCode::InternalError,
                "peer_ephemeral_public não disponível",
            )
        })?;

        // Deriva shared secret X25519
        let secret = {
            let mut secrets = self.ephemeral_secrets.lock().await;
            secrets.remove(&session_id).ok_or_else(|| {
                DomainError::new(
                    DomainErrorCode::InternalError,
                    "ephemeral secret não encontrado",
                )
            })?
        };
        let peer_pub = x25519_dalek::PublicKey::from(peer_ephemeral_pub_bytes);
        let _shared_secret = secret.diffie_hellman(&peer_pub);
        // _shared_secret será usado no Sprint 5 (Transport) para derivar chaves de sessão QUIC.

        let peer_public_key = PublicKey::new(peer_public_key_bytes);
        let trusted_peer = TrustedPeer::new(peer_id, peer_username, peer_public_key);

        self.identity_store
            .save_peer(&trusted_peer)
            .await
            .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?;

        session.complete(peer_ephemeral_pub_bytes);

        self.bus
            .publish(DomainEvent::PairingCompleted(PairingCompleted {
                session_id,
                peer_id,
                peer_username: peer_username.to_string(),
            }));

        sessions.remove(&session_id);
        info!(%session_id, %peer_id, "pareamento concluído");
        Ok(())
    }

    /// Cancela uma sessão ativa.
    pub async fn cancel_pairing(&self, session_id: SessionId) -> Result<(), DomainError> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions.get_mut(&session_id).ok_or_else(|| {
            DomainError::new(
                DomainErrorCode::PairingSessionNotFound,
                "sessão não encontrada",
            )
        })?;

        let peer_id = session.peer_id;
        session.cancel();

        self.bus
            .publish(DomainEvent::PairingCancelled(PairingCancelled {
                session_id,
                peer_id,
            }));

        sessions.remove(&session_id);
        info!(%session_id, %peer_id, "pareamento cancelado");
        Ok(())
    }

    /// Inicia a tarefa de limpeza periódica (deve ser chamada de dentro do runtime Tokio).
    ///
    /// Use `tauri::async_runtime::spawn` no shell Tauri — não chame no `setup()` síncrono.
    /// A cada 30 segundos verifica sessões expiradas e as remove publicando
    /// `PairingFailed` no bus.
    pub fn spawn_cleanup_task(&self) {
        let sessions = Arc::clone(&self.sessions);
        let secrets = Arc::clone(&self.ephemeral_secrets);
        let bus = self.bus.clone();

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;

                let mut sessions = sessions.lock().await;
                let mut secrets = secrets.lock().await;

                let expired: Vec<SessionId> = sessions
                    .values()
                    .filter(|s| s.is_expired())
                    .map(|s| s.id)
                    .collect();

                for sid in expired {
                    if let Some(s) = sessions.remove(&sid) {
                        secrets.remove(&sid);
                        warn!(%sid, peer_id = %s.peer_id, "sessão de pareamento expirada — removida");
                        bus.publish(DomainEvent::PairingFailed(PairingFailed {
                            session_id: sid,
                            peer_id: s.peer_id,
                            reason: DomainErrorCode::PairingPinExpired,
                        }));
                    }
                }
            }
        });
    }
}
