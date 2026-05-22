use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use tokio::sync::Mutex;
use tracing::{info, warn};
use winx_domain::{
    discovery::DiscoveryRegistry,
    identity::{PublicKey, TrustedPeer},
    pairing::{
        events::{PairingCancelled, PairingCompleted, PairingFailed, PairingIncoming},
        PairingRole, PairingSession,
    },
    shared::{
        ids::{PeerId, SessionId},
        DomainErrorCode,
    },
    DomainError, DomainEvent,
};
use winx_protocol::{
    pin_commitment, PairingCancel, PairingConfirm, PairingMessage, PairingRequest, PairingResponse,
};
use x25519_dalek::StaticSecret;

use crate::{
    bus::EventBus,
    ports::{
        pairing::{pairing_socket_addr, DecodedPairingMessage, PairingTransport},
        IdentityStore, SecretStore,
    },
};

pub struct PairingService {
    sessions: Arc<Mutex<HashMap<SessionId, PairingSession>>>,
    ephemeral_secrets: Arc<Mutex<HashMap<SessionId, StaticSecret>>>,
    peer_addrs: Arc<Mutex<HashMap<SessionId, SocketAddr>>>,
    remote_usernames: Arc<Mutex<HashMap<SessionId, String>>>,
    identity_store: Arc<dyn IdentityStore>,
    secret_store: Arc<dyn SecretStore>,
    transport: Arc<dyn PairingTransport>,
    discovery_registry: Arc<Mutex<DiscoveryRegistry>>,
    local_peer_id: PeerId,
    local_username: String,
    local_pubkey: [u8; 32],
    bus: EventBus,
}

impl PairingService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        identity_store: Arc<dyn IdentityStore>,
        secret_store: Arc<dyn SecretStore>,
        transport: Arc<dyn PairingTransport>,
        discovery_registry: Arc<Mutex<DiscoveryRegistry>>,
        local_peer_id: PeerId,
        local_username: String,
        local_pubkey: [u8; 32],
        bus: EventBus,
    ) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            ephemeral_secrets: Arc::new(Mutex::new(HashMap::new())),
            peer_addrs: Arc::new(Mutex::new(HashMap::new())),
            remote_usernames: Arc::new(Mutex::new(HashMap::new())),
            identity_store,
            secret_store,
            transport,
            discovery_registry,
            local_peer_id,
            local_username,
            local_pubkey,
            bus,
        }
    }

    /// Loop do listener UDP de pairing. Deve rodar dentro do runtime Tokio do app
    /// (`rt.spawn` no shell Tauri), não na thread principal.
    pub async fn run_network_listener(self: Arc<Self>) {
        match self.transport.listen().await {
            Ok(mut rx) => {
                while let Some(msg) = rx.recv().await {
                    if let Err(err) = self.handle_datagram(msg).await {
                        warn!(?err, "falha ao processar datagrama de pairing");
                    }
                }
            }
            Err(err) => warn!(?err, "falha ao iniciar listener UDP de pairing"),
        }
    }

    /// Inicia pareamento como **initiator**.
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
        let session = PairingSession::initiator(peer_id, public.to_bytes());

        let session_id = session.id;
        let pin = session
            .pin()
            .map(|p| p.as_str().to_string())
            .unwrap_or_default();
        let commitment = pin_commitment(&session_id.as_uuid(), &pin);

        self.ephemeral_secrets
            .lock()
            .await
            .insert(session_id, secret);
        sessions.insert(session_id, session);
        drop(sessions);

        let peer_addr = self.resolve_peer_addr(peer_id).await.ok_or_else(|| {
            DomainError::new(
                DomainErrorCode::InternalError,
                "endereço do peer não encontrado no discovery",
            )
        })?;

        let request = PairingMessage::Request(PairingRequest {
            session_id: session_id.as_uuid(),
            initiator_peer_id: self.local_peer_id.as_uuid(),
            initiator_username: self.local_username.clone(),
            initiator_ephemeral: public.to_bytes(),
            initiator_pubkey: self.local_pubkey,
            pin_commitment: commitment,
        });

        self.transport
            .send_to(peer_addr, &request, None)
            .await
            .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?;

        self.peer_addrs.lock().await.insert(session_id, peer_addr);

        info!(%session_id, %peer_id, %peer_username, "pareamento iniciado (rede)");
        Ok((session_id, pin))
    }

    /// Submete PIN: no **responder** envia `PairingResponse` ao initiator.
    pub async fn submit_pin(
        &self,
        session_id: SessionId,
        pin_input: &str,
    ) -> Result<(), DomainError> {
        let sessions = self.sessions.lock().await;
        let session = sessions.get(&session_id).ok_or_else(|| {
            DomainError::new(
                DomainErrorCode::PairingSessionNotFound,
                "sessão não encontrada",
            )
        })?;

        if session.role != PairingRole::Responder {
            return Err(DomainError::new(
                DomainErrorCode::InternalError,
                "submit_pin só é válido no lado responder",
            ));
        }

        if pin_input.len() != 6 || !pin_input.chars().all(|c| c.is_ascii_digit()) {
            return Err(DomainError::new(
                DomainErrorCode::PairingPinMismatch,
                "PIN deve ter 6 dígitos",
            ));
        }

        let peer_id = session.peer_id;
        let responder_ephemeral = session.ephemeral_public;
        let initiator_addr = self
            .peer_addrs
            .lock()
            .await
            .get(&session_id)
            .copied()
            .ok_or_else(|| {
                DomainError::new(
                    DomainErrorCode::InternalError,
                    "endereço do initiator não registrado",
                )
            })?;

        let signing_key = self.load_signing_key().await?;
        drop(sessions);

        let response = PairingMessage::Response(PairingResponse {
            session_id: session_id.as_uuid(),
            responder_peer_id: self.local_peer_id.as_uuid(),
            responder_ephemeral,
            responder_pubkey: self.local_pubkey,
            pin: pin_input.to_string(),
        });

        self.transport
            .send_to(initiator_addr, &response, Some(&signing_key))
            .await
            .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?;

        info!(%session_id, %peer_id, "PIN enviado ao initiator");
        Ok(())
    }

    pub async fn cancel_pairing(&self, session_id: SessionId) -> Result<(), DomainError> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions.get_mut(&session_id).ok_or_else(|| {
            DomainError::new(
                DomainErrorCode::PairingSessionNotFound,
                "sessão não encontrada",
            )
        })?;

        let peer_id = session.peer_id;
        let is_initiator = session.role == PairingRole::Initiator;
        session.cancel();
        sessions.remove(&session_id);
        drop(sessions);

        self.ephemeral_secrets.lock().await.remove(&session_id);

        if is_initiator {
            if let Some(addr) = self.peer_addrs.lock().await.remove(&session_id) {
                let cancel = PairingMessage::Cancel(PairingCancel {
                    session_id: session_id.as_uuid(),
                    reason: "cancelled by user".into(),
                });
                let _ = self.transport.send_to(addr, &cancel, None).await;
            }
        }

        self.peer_addrs.lock().await.remove(&session_id);
        self.remote_usernames.lock().await.remove(&session_id);

        self.bus
            .publish(DomainEvent::PairingCancelled(PairingCancelled {
                session_id,
                peer_id,
            }));

        info!(%session_id, %peer_id, "pareamento cancelado");
        Ok(())
    }

    pub async fn handle_datagram(&self, msg: DecodedPairingMessage) -> Result<(), DomainError> {
        match msg.message {
            PairingMessage::Request(req) => self.handle_incoming_request(req, msg.from_addr).await,
            PairingMessage::Response(resp) => {
                self.handle_incoming_response(resp, msg.from_addr, msg.signature)
                    .await
            }
            PairingMessage::Confirm(conf) => {
                self.handle_incoming_confirm(conf, msg.signature).await
            }
            PairingMessage::Cancel(cancel) => self.handle_incoming_cancel(cancel).await,
        }
    }

    async fn handle_incoming_request(
        &self,
        req: PairingRequest,
        from_addr: SocketAddr,
    ) -> Result<(), DomainError> {
        let initiator_peer_id = PeerId::from_uuid(req.initiator_peer_id);
        if initiator_peer_id == self.local_peer_id {
            return Ok(());
        }

        let session_id = SessionId::from_uuid(req.session_id);

        {
            let sessions = self.sessions.lock().await;
            if sessions.contains_key(&session_id)
                || sessions.values().any(|s| s.peer_id == initiator_peer_id)
            {
                return Ok(());
            }
        }

        let initiator_fingerprint = PublicKey::new(req.initiator_pubkey)
            .fingerprint()
            .to_string();
        if let Some(discovered) = self.discovery_registry.lock().await.get(initiator_peer_id) {
            if discovered.fingerprint != initiator_fingerprint {
                warn!(
                    %initiator_peer_id,
                    expected = %discovered.fingerprint,
                    got = %initiator_fingerprint,
                    "fingerprint do pairing não confere com mDNS"
                );
            }
        }

        let secret = StaticSecret::random_from_rng(OsRng);
        let public: x25519_dalek::PublicKey = (&secret).into();
        let session = PairingSession::responder(
            session_id,
            initiator_peer_id,
            req.initiator_ephemeral,
            public.to_bytes(),
            req.initiator_pubkey,
        );

        self.ephemeral_secrets
            .lock()
            .await
            .insert(session_id, secret);
        self.sessions.lock().await.insert(session_id, session);
        self.peer_addrs.lock().await.insert(session_id, from_addr);
        self.remote_usernames
            .lock()
            .await
            .insert(session_id, req.initiator_username.clone());

        self.bus
            .publish(DomainEvent::PairingIncoming(PairingIncoming {
                session_id,
                peer_id: initiator_peer_id,
                peer_username: req.initiator_username.clone(),
                initiator_fingerprint: Some(initiator_fingerprint),
            }));

        info!(%session_id, %initiator_peer_id, "pedido de pareamento recebido");
        Ok(())
    }

    async fn handle_incoming_response(
        &self,
        resp: PairingResponse,
        from_addr: SocketAddr,
        signature: Option<[u8; 64]>,
    ) -> Result<(), DomainError> {
        let session_id = SessionId::from_uuid(resp.session_id);
        let sig = signature.ok_or_else(|| {
            DomainError::new(
                DomainErrorCode::InternalError,
                "PairingResponse sem assinatura",
            )
        })?;

        verify_message_signature(
            &PairingMessage::Response(resp.clone()),
            &resp.responder_pubkey,
            &sig,
        )?;

        let mut sessions = self.sessions.lock().await;
        let session = sessions.get_mut(&session_id).ok_or_else(|| {
            DomainError::new(
                DomainErrorCode::PairingSessionNotFound,
                "sessão initiator não encontrada",
            )
        })?;

        if session.role != PairingRole::Initiator {
            return Err(DomainError::new(
                DomainErrorCode::InternalError,
                "response recebida em sessão não-initiator",
            ));
        }

        session.verify_pin(&resp.pin)?;

        let peer_id = session.peer_id;
        let peer_username = self.remote_username(session_id, peer_id).await;

        session.complete(resp.responder_ephemeral);

        let secret = self
            .ephemeral_secrets
            .lock()
            .await
            .remove(&session_id)
            .ok_or_else(|| {
                DomainError::new(
                    DomainErrorCode::InternalError,
                    "ephemeral secret não encontrado",
                )
            })?;

        let peer_pub = x25519_dalek::PublicKey::from(resp.responder_ephemeral);
        let _shared = secret.diffie_hellman(&peer_pub);

        sessions.remove(&session_id);
        drop(sessions);

        let peer_public_key = PublicKey::new(resp.responder_pubkey);
        let trusted_peer = TrustedPeer::new(peer_id, &peer_username, peer_public_key);
        self.identity_store
            .save_peer(&trusted_peer)
            .await
            .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?;

        let signing_key = self.load_signing_key().await?;
        let confirm = PairingMessage::Confirm(PairingConfirm {
            session_id: session_id.as_uuid(),
            confirmer_peer_id: self.local_peer_id.as_uuid(),
        });
        self.transport
            .send_to(pairing_socket_addr(from_addr), &confirm, Some(&signing_key))
            .await
            .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?;

        self.bus
            .publish(DomainEvent::PairingCompleted(PairingCompleted {
                session_id,
                peer_id,
                peer_username: peer_username.clone(),
            }));

        self.peer_addrs.lock().await.remove(&session_id);
        self.remote_usernames.lock().await.remove(&session_id);

        info!(%session_id, %peer_id, "pareamento concluído (initiator)");
        Ok(())
    }

    async fn handle_incoming_confirm(
        &self,
        conf: PairingConfirm,
        signature: Option<[u8; 64]>,
    ) -> Result<(), DomainError> {
        let session_id = SessionId::from_uuid(conf.session_id);
        let sig = signature.ok_or_else(|| {
            DomainError::new(
                DomainErrorCode::InternalError,
                "PairingConfirm sem assinatura",
            )
        })?;

        let sessions = self.sessions.lock().await;
        let session = sessions.get(&session_id).ok_or_else(|| {
            DomainError::new(
                DomainErrorCode::PairingSessionNotFound,
                "sessão responder não encontrada",
            )
        })?;

        if session.role != PairingRole::Responder {
            return Err(DomainError::new(
                DomainErrorCode::InternalError,
                "confirm recebido em sessão não-responder",
            ));
        }

        let initiator_pubkey = session.initiator_pubkey.ok_or_else(|| {
            DomainError::new(
                DomainErrorCode::InternalError,
                "initiator_pubkey ausente na sessão",
            )
        })?;

        let peer_id = session.peer_id;
        let peer_username = self.remote_username(session_id, peer_id).await;

        drop(sessions);

        verify_message_signature(
            &PairingMessage::Confirm(conf.clone()),
            &initiator_pubkey,
            &sig,
        )?;

        let mut sessions = self.sessions.lock().await;
        let session = sessions.remove(&session_id).ok_or_else(|| {
            DomainError::new(
                DomainErrorCode::PairingSessionNotFound,
                "sessão já removida",
            )
        })?;

        let peer_ephemeral = session.peer_ephemeral_public.ok_or_else(|| {
            DomainError::new(
                DomainErrorCode::InternalError,
                "ephemeral do initiator ausente",
            )
        })?;

        let secret = self.ephemeral_secrets.lock().await.remove(&session_id);

        if let Some(secret) = secret {
            let peer_pub = x25519_dalek::PublicKey::from(peer_ephemeral);
            let _shared = secret.diffie_hellman(&peer_pub);
        }

        let peer_public_key = PublicKey::new(initiator_pubkey);
        let trusted_peer = TrustedPeer::new(peer_id, &peer_username, peer_public_key);
        self.identity_store
            .save_peer(&trusted_peer)
            .await
            .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?;

        self.bus
            .publish(DomainEvent::PairingCompleted(PairingCompleted {
                session_id,
                peer_id,
                peer_username: peer_username.clone(),
            }));

        self.peer_addrs.lock().await.remove(&session_id);
        self.remote_usernames.lock().await.remove(&session_id);

        info!(%session_id, %peer_id, "pareamento concluído (responder)");
        Ok(())
    }

    async fn handle_incoming_cancel(&self, cancel: PairingCancel) -> Result<(), DomainError> {
        let session_id = SessionId::from_uuid(cancel.session_id);
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.remove(&session_id) {
            let peer_id = session.peer_id;
            self.ephemeral_secrets.lock().await.remove(&session_id);
            self.peer_addrs.lock().await.remove(&session_id);
            self.remote_usernames.lock().await.remove(&session_id);
            self.bus.publish(DomainEvent::PairingFailed(PairingFailed {
                session_id,
                peer_id,
                reason: DomainErrorCode::PairingPinExpired,
            }));
        }
        Ok(())
    }

    async fn resolve_peer_addr(&self, peer_id: PeerId) -> Option<SocketAddr> {
        self.discovery_registry
            .lock()
            .await
            .get(peer_id)
            .and_then(|p| p.addresses.first().copied())
            .map(pairing_socket_addr)
    }

    async fn remote_username(&self, session_id: SessionId, peer_id: PeerId) -> String {
        if let Some(name) = self.remote_usernames.lock().await.get(&session_id) {
            return name.clone();
        }
        self.discovery_registry
            .lock()
            .await
            .get(peer_id)
            .map_or_else(|| peer_id.to_string(), |p| p.username.clone())
    }

    async fn load_signing_key(&self) -> Result<SigningKey, DomainError> {
        let bytes = self
            .secret_store
            .load_signing_key()
            .await
            .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?
            .ok_or_else(|| {
                DomainError::new(DomainErrorCode::InternalError, "signing key não encontrada")
            })?;
        Ok(SigningKey::from_bytes(&bytes))
    }

    pub fn spawn_cleanup_task(&self) {
        let sessions = Arc::clone(&self.sessions);
        let secrets = Arc::clone(&self.ephemeral_secrets);
        let addrs = Arc::clone(&self.peer_addrs);
        let usernames = Arc::clone(&self.remote_usernames);
        let bus = self.bus.clone();

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;

                let mut sessions = sessions.lock().await;
                let mut secrets = secrets.lock().await;
                let mut addrs = addrs.lock().await;
                let mut names = usernames.lock().await;

                let expired: Vec<SessionId> = sessions
                    .values()
                    .filter(|s| s.is_expired())
                    .map(|s| s.id)
                    .collect();

                for sid in expired {
                    if let Some(s) = sessions.remove(&sid) {
                        secrets.remove(&sid);
                        addrs.remove(&sid);
                        names.remove(&sid);
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

fn verify_message_signature(
    msg: &PairingMessage,
    pubkey_bytes: &[u8; 32],
    signature_bytes: &[u8; 64],
) -> Result<(), DomainError> {
    let body = winx_protocol::encode_pairing_body(msg).map_err(
        |e: winx_protocol::PairingProtocolError| {
            DomainError::new(DomainErrorCode::InternalError, e.to_string())
        },
    )?;
    let vk = VerifyingKey::from_bytes(pubkey_bytes).map_err(|e| {
        DomainError::new(
            DomainErrorCode::InternalError,
            format!("chave pública inválida: {e}"),
        )
    })?;
    let signature = Signature::from_bytes(signature_bytes);
    vk.verify_strict(&body, &signature)
        .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))
}
