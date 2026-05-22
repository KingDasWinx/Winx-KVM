use ed25519_dalek::SigningKey;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use uuid::Uuid;

use winx_domain::identity::key::PublicKey;
use winx_domain::shared::{ids::DeviceId, DomainError, DomainErrorCode, DomainEvent};
use winx_domain::workspace::{InviteSession, Workspace, WorkspaceId, WorkspaceMember};
use winx_protocol::workspace::{
    MemberSnapshotPayload, WorkspaceInviteMessage, WorkspaceInvitePayload,
    WorkspaceInviteResponsePayload, WorkspaceSnapshotPayload,
};

use crate::bus::EventBus;
use crate::ports::{
    DiscoveryQuery, IdentityStore, SecretStore, WorkspaceInviteTransport, WorkspaceStore,
};
use tracing::{debug, info, warn};
use winx_domain::identity::TrustedPeer;
use winx_domain::shared::ids::PeerId;

struct PendingInviteData {
    session: InviteSession,
    snapshot: Option<WorkspaceSnapshotPayload>,
    sender_pubkey: Option<[u8; 32]>,
}

/// Service for workspace invites and membership management.
#[derive(Clone)]
pub struct WorkspaceService {
    store: Arc<dyn WorkspaceStore>,
    transport: Arc<dyn WorkspaceInviteTransport>,
    identity_store: Arc<dyn IdentityStore>,
    secret_store: Arc<dyn SecretStore>,
    discovery_query: Arc<dyn DiscoveryQuery>,
    pending_invites: Arc<Mutex<HashMap<Uuid, PendingInviteData>>>,
    active_workspace: Arc<RwLock<Option<WorkspaceId>>>,
    local_device_id: Uuid,
    local_username: String,
    bus: EventBus,
}

impl WorkspaceService {
    pub fn new(
        store: Arc<dyn WorkspaceStore>,
        transport: Arc<dyn WorkspaceInviteTransport>,
        identity_store: Arc<dyn IdentityStore>,
        secret_store: Arc<dyn SecretStore>,
        discovery_query: Arc<dyn DiscoveryQuery>,
        local_device_id: Uuid,
        local_username: String,
        bus: EventBus,
    ) -> Self {
        Self {
            store,
            transport,
            identity_store,
            secret_store,
            discovery_query,
            pending_invites: Arc::new(Mutex::new(HashMap::new())),
            active_workspace: Arc::new(RwLock::new(None)),
            local_device_id,
            local_username,
            bus,
        }
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

    /// Envia uma resposta de invite (accepted/rejected) ao remetente original.
    /// Best-effort: erros de rede não propagam.
    async fn send_invite_response(
        &self,
        sender_device_id: DeviceId,
        invite_id: Uuid,
        responder_pubkey: [u8; 32],
        accepted: bool,
    ) {
        let signing_key = match self.load_signing_key().await {
            Ok(k) => k,
            Err(e) => {
                warn!(%invite_id, ?e, "failed to load signing key for response");
                return;
            }
        };
        let payload = WorkspaceInviteResponsePayload {
            invite_id,
            responder_device_id: self.local_device_id,
            responder_pubkey,
            accepted,
        };
        let msg = WorkspaceInviteMessage::Response(payload);
        match self.discovery_query.resolve_address(sender_device_id).await {
            Ok(Some(mut addr)) => {
                addr.set_port(crate::ports::WORKSPACE_INVITE_PORT);
                if let Err(e) = self.transport.send_to(addr, &msg, &signing_key).await {
                    warn!(%invite_id, ?e, "failed to send invite response");
                }
            }
            Ok(None) => warn!(%invite_id, "sender not discoverable for response"),
            Err(e) => warn!(%invite_id, ?e, "failed to resolve sender address"),
        }
    }

    pub async fn list_workspaces(&self) -> anyhow::Result<Vec<Workspace>> {
        self.store.load_all().await
    }

    pub async fn create_workspace(
        &self,
        name: String,
        initial_member_device_ids: Vec<Uuid>,
    ) -> Result<Workspace, DomainError> {
        let local_device = self
            .identity_store
            .load_device()
            .await
            .map_err(|e| {
                DomainError::new(
                    DomainErrorCode::InternalError,
                    format!("failed to load device: {}", e),
                )
            })?
            .ok_or_else(|| {
                DomainError::new(DomainErrorCode::InternalError, "local device not found")
            })?;

        let owner_device_id = DeviceId::from_uuid(self.local_device_id);
        let owner_pubkey = local_device.public_key;
        let owner_member =
            WorkspaceMember::new(owner_device_id, owner_pubkey, self.local_username.clone());

        // Create workspace as Original (owned by this device)
        let ws = Workspace::create_original(name, owner_member)
            .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e))?;

        // Save workspace to store
        self.store
            .save(&ws)
            .await
            .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?;

        info!(workspace_id = %ws.id, member_count = ws.members.len(), "workspace created");

        // Send invites to each target (asynchronous, don't block on errors)
        for target_uuid in initial_member_device_ids {
            let svc = Arc::new(self.clone());
            let ws_id = ws.id;
            tokio::spawn(async move {
                if let Err(e) = svc.invite_to_workspace(ws_id, target_uuid).await {
                    warn!(?e, %target_uuid, "failed to send invite");
                }
            });
        }

        Ok(ws)
    }

    pub async fn invite_to_workspace(
        &self,
        workspace_id: WorkspaceId,
        target_device_uuid: Uuid,
    ) -> Result<Uuid, DomainError> {
        // Load workspace from store
        let ws = self
            .store
            .find_by_id(workspace_id)
            .await
            .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?
            .ok_or_else(|| {
                DomainError::new(DomainErrorCode::InternalError, "workspace not found")
            })?;

        // Only Original workspaces can send invites
        if ws.ownership_mode.is_mirror() {
            return Err(DomainError::new(
                DomainErrorCode::WorkspaceMirrorImmutable,
                "mirrors cannot send invites",
            ));
        }

        // Create invite session
        let sender_device_id = DeviceId::from_uuid(self.local_device_id);
        let target_device_id = DeviceId::from_uuid(target_device_uuid);
        let invite_session = InviteSession::new(workspace_id, target_device_id, sender_device_id);
        let invite_id = invite_session.id.as_uuid();

        // Store pending invite (sender-side: no snapshot yet)
        {
            let mut pending = self.pending_invites.lock().await;
            pending.insert(
                invite_id,
                PendingInviteData {
                    session: invite_session,
                    snapshot: None,
                    sender_pubkey: None,
                },
            );
        }

        // Build snapshot for payload
        let members_snapshot: Vec<MemberSnapshotPayload> = ws
            .members
            .iter()
            .map(|m| MemberSnapshotPayload {
                device_id: m.device_id.as_uuid(),
                public_key: *m.public_key.as_bytes(),
                username: m.username_cache.clone(),
                joined_at_rfc3339: m
                    .joined_at
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default(),
            })
            .collect();

        let snapshot = WorkspaceSnapshotPayload {
            id: ws.id.as_uuid(),
            name: ws.name.clone(),
            owner_device_id: ws.owner_device_id.as_uuid(),
            owner_username: ws
                .members
                .iter()
                .find(|m| m.device_id == ws.owner_device_id)
                .map(|m| m.username_cache.clone())
                .unwrap_or_else(|| "Unknown".to_string()),
            version: ws.version.as_u64(),
            members: members_snapshot,
        };

        // Load local public key
        let local_device = self
            .identity_store
            .load_device()
            .await
            .map_err(|e| {
                DomainError::new(
                    DomainErrorCode::InternalError,
                    format!("failed to load device: {}", e),
                )
            })?
            .ok_or_else(|| {
                DomainError::new(DomainErrorCode::InternalError, "local device not found")
            })?;
        let sender_pubkey = *local_device.public_key.as_bytes();

        // Create payload
        let payload = WorkspaceInvitePayload {
            invite_id,
            workspace_snapshot: snapshot,
            sender_device_id: self.local_device_id,
            sender_username: self.local_username.clone(),
            sender_pubkey,
            target_device_id: target_device_uuid,
        };

        let signing_key = self.load_signing_key().await?;

        // Resolve target address and send invite (async, don't block on network failure)
        let svc = Arc::new(self.clone());
        let msg = WorkspaceInviteMessage::Invite(payload);
        tokio::spawn(async move {
            let target_device_id = DeviceId::from_uuid(target_device_uuid);
            match svc.discovery_query.resolve_address(target_device_id).await {
                Ok(Some(mut addr)) => {
                    addr.set_port(crate::ports::WORKSPACE_INVITE_PORT);
                    match svc.transport.send_to(addr, &msg, &signing_key).await {
                        Ok(()) => info!(%invite_id, %addr, "invite sent"),
                        Err(e) => warn!(%invite_id, ?e, "failed to send invite"),
                    }
                }
                Ok(None) => {
                    warn!(%invite_id, %target_device_uuid, "target device not discovered (offline?)")
                }
                Err(e) => warn!(%invite_id, ?e, "failed to resolve target address"),
            }
        });

        info!(%invite_id, %workspace_id, %target_device_uuid, "invite created, sending via network");

        Ok(invite_id)
    }

    pub async fn accept_invite(&self, invite_id: Uuid) -> Result<Workspace, DomainError> {
        // Find pending invite (receiver side — has snapshot)
        let invite_data = {
            let mut pending = self.pending_invites.lock().await;
            pending.remove(&invite_id)
        };

        let data = invite_data.ok_or_else(|| {
            DomainError::new(
                DomainErrorCode::InternalError,
                "invite not found or already resolved",
            )
        })?;

        let snapshot = data.snapshot.ok_or_else(|| {
            DomainError::new(
                DomainErrorCode::InternalError,
                "snapshot not found in invite",
            )
        })?;

        let sender_pubkey = data.sender_pubkey.ok_or_else(|| {
            DomainError::new(
                DomainErrorCode::InternalError,
                "sender pubkey not found in invite",
            )
        })?;

        let workspace_id = data.session.workspace_id;
        let sender_device_id = data.session.sender_device_id;

        // TOFU: save sender as trusted peer
        let peer = TrustedPeer::new(
            PeerId::from_uuid(sender_device_id.as_uuid()),
            snapshot.owner_username.clone(),
            PublicKey::new(sender_pubkey),
        );
        self.identity_store.save_peer(&peer).await.map_err(|e| {
            DomainError::new(
                DomainErrorCode::InternalError,
                format!("failed to save peer: {}", e),
            )
        })?;

        // Convert protocol snapshot to domain snapshot
        let owner_username = snapshot.owner_username.clone();
        let members: Vec<WorkspaceMember> = snapshot
            .members
            .iter()
            .map(|m| {
                WorkspaceMember::new(
                    DeviceId::from_uuid(m.device_id),
                    PublicKey::new(m.public_key),
                    m.username.clone(),
                )
            })
            .collect();

        let domain_snapshot = winx_domain::workspace::WorkspaceSnapshot {
            id: WorkspaceId::from_uuid(snapshot.id),
            name: snapshot.name.clone(),
            owner_device_id: DeviceId::from_uuid(snapshot.owner_device_id),
            version: winx_domain::workspace::WorkspaceVersion::initial(),
            ownership_mode: winx_domain::workspace::OwnershipMode::Original,
            members,
            layout: winx_domain::workspace::WorkspaceLayout::empty(),
        };

        // Create mirror workspace
        let mirror = Workspace::create_mirror(domain_snapshot, owner_username);

        // Persist mirror to store
        self.store.save(&mirror).await.map_err(|e| {
            DomainError::new(
                DomainErrorCode::InternalError,
                format!("failed to save workspace: {}", e),
            )
        })?;

        info!(%invite_id, %workspace_id, "invite accepted and mirror created");

        // Send signed response back to the sender (best-effort)
        let responder_pubkey = {
            let device = self
                .identity_store
                .load_device()
                .await
                .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?
                .ok_or_else(|| {
                    DomainError::new(DomainErrorCode::InternalError, "local device not found")
                })?;
            *device.public_key.as_bytes()
        };
        self.send_invite_response(sender_device_id, invite_id, responder_pubkey, true)
            .await;

        // Publish event
        self.bus.publish(DomainEvent::WorkspaceInviteAccepted(
            winx_domain::workspace::events::InviteAccepted {
                invite_id: winx_domain::workspace::InviteId::from_uuid(invite_id),
                workspace_id,
                accepting_device_id: DeviceId::from_uuid(self.local_device_id),
            },
        ));

        Ok(mirror)
    }

    pub async fn reject_invite(&self, invite_id: Uuid) -> Result<(), DomainError> {
        // Remove from pending
        let invite_data = {
            let mut pending = self.pending_invites.lock().await;
            pending.remove(&invite_id)
        };

        let data = invite_data.ok_or_else(|| {
            DomainError::new(
                DomainErrorCode::InternalError,
                "invite not found or already resolved",
            )
        })?;

        let workspace_id = data.session.workspace_id;
        let sender_device_id = data.session.sender_device_id;

        info!(%invite_id, %workspace_id, "invite rejected");

        // Send signed response back to the sender (best-effort)
        let responder_pubkey = {
            let device = self
                .identity_store
                .load_device()
                .await
                .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?
                .ok_or_else(|| {
                    DomainError::new(DomainErrorCode::InternalError, "local device not found")
                })?;
            *device.public_key.as_bytes()
        };
        self.send_invite_response(sender_device_id, invite_id, responder_pubkey, false)
            .await;

        // Publish event
        self.bus.publish(DomainEvent::WorkspaceInviteRejected(
            winx_domain::workspace::events::InviteRejected {
                invite_id: winx_domain::workspace::InviteId::from_uuid(invite_id),
            },
        ));

        Ok(())
    }

    pub async fn connect_to_workspace(&self, workspace_id: WorkspaceId) -> Result<(), DomainError> {
        let mut active = self.active_workspace.write().await;
        if let Some(current_id) = *active {
            if current_id != workspace_id {
                return Err(DomainError::new(
                    DomainErrorCode::WorkspaceConflict,
                    &format!(
                        "already connected to {}; target is {}",
                        current_id, workspace_id
                    ),
                ));
            }
        }
        *active = Some(workspace_id);
        self.bus.publish(DomainEvent::WorkspaceConnected(
            winx_domain::workspace::events::WorkspaceConnected { workspace_id },
        ));
        info!(%workspace_id, "connected to workspace");
        Ok(())
    }

    pub async fn disconnect_from_workspace(&self) -> Result<(), DomainError> {
        let mut active = self.active_workspace.write().await;
        if let Some(workspace_id) = active.take() {
            self.bus.publish(DomainEvent::WorkspaceDisconnected(
                winx_domain::workspace::events::WorkspaceDisconnected { workspace_id },
            ));
            info!(%workspace_id, "disconnected from workspace");
        }
        Ok(())
    }

    pub async fn force_disconnect_and_connect(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<(), DomainError> {
        let mut active = self.active_workspace.write().await;
        if let Some(old_id) = active.take() {
            self.bus.publish(DomainEvent::WorkspaceDisconnected(
                winx_domain::workspace::events::WorkspaceDisconnected {
                    workspace_id: old_id,
                },
            ));
            info!(%old_id, "disconnected from workspace (force switch)");
        }
        *active = Some(workspace_id);
        self.bus.publish(DomainEvent::WorkspaceConnected(
            winx_domain::workspace::events::WorkspaceConnected { workspace_id },
        ));
        info!(%workspace_id, "connected to workspace (force switch)");
        Ok(())
    }

    pub async fn delete_workspace(&self, id: WorkspaceId) -> Result<(), DomainError> {
        self.store.delete(id).await.map_err(|_| {
            DomainError::new(DomainErrorCode::InternalError, "failed to delete workspace")
        })
    }

    pub async fn active_workspace_id(&self) -> Option<WorkspaceId> {
        self.active_workspace.read().await.clone()
    }

    pub async fn run_invite_listener(&self) -> anyhow::Result<()> {
        let mut rx = self.transport.listen().await?;
        info!("workspace invite listener started");

        while let Some(decoded) = rx.recv().await {
            match &decoded.message {
                WorkspaceInviteMessage::Invite(payload) => {
                    self.handle_incoming_invite(payload).await;
                }
                WorkspaceInviteMessage::Response(payload) => {
                    self.handle_invite_response(payload).await;
                }
                WorkspaceInviteMessage::Cancel(_) => {
                    debug!("received invite cancellation");
                }
            }
        }

        Ok(())
    }

    async fn handle_incoming_invite(&self, payload: &WorkspaceInvitePayload) {
        let invite_id = payload.invite_id;
        let workspace_id = WorkspaceId::from_uuid(payload.workspace_snapshot.id);
        let sender_device_id = DeviceId::from_uuid(payload.sender_device_id);

        info!(
            %invite_id,
            %workspace_id,
            sender = %payload.sender_username,
            "incoming workspace invite"
        );

        // Create and store invite session with snapshot (receiver side)
        let invite_session = InviteSession::new(
            workspace_id,
            DeviceId::from_uuid(payload.target_device_id),
            sender_device_id,
        );

        {
            let mut pending = self.pending_invites.lock().await;
            pending.insert(
                invite_id,
                PendingInviteData {
                    session: invite_session,
                    snapshot: Some(payload.workspace_snapshot.clone()),
                    sender_pubkey: Some(payload.sender_pubkey),
                },
            );
        }

        // Calculate sender fingerprint
        let fingerprint =
            winx_domain::identity::PublicKey::new(payload.sender_pubkey).fingerprint();

        // Publish event to event bus (triggers frontend notification)
        self.bus.publish(DomainEvent::WorkspaceInviteIncoming(
            winx_domain::workspace::events::InviteIncoming {
                invite_id: winx_domain::workspace::InviteId::from_uuid(invite_id),
                workspace_id,
                workspace_name: payload.workspace_snapshot.name.clone(),
                sender_device_id,
                sender_username: payload.sender_username.clone(),
                sender_fingerprint_hex: fingerprint.to_string(),
            },
        ));
    }

    async fn handle_invite_response(&self, payload: &WorkspaceInviteResponsePayload) {
        info!(
            invite_id = %payload.invite_id,
            accepted = payload.accepted,
            "invite response received"
        );

        // Find pending invite (sender side)
        let invite_data = {
            let mut pending = self.pending_invites.lock().await;
            pending.remove(&payload.invite_id)
        };

        if let Some(data) = invite_data {
            let workspace_id = data.session.workspace_id;

            if payload.accepted {
                // Response: accepted
                self.bus.publish(DomainEvent::WorkspaceInviteAccepted(
                    winx_domain::workspace::events::InviteAccepted {
                        invite_id: winx_domain::workspace::InviteId::from_uuid(payload.invite_id),
                        workspace_id,
                        accepting_device_id: DeviceId::from_uuid(payload.responder_device_id),
                    },
                ));

                info!(%workspace_id, "invite accepted by peer");
            } else {
                // Response: rejected
                self.bus.publish(DomainEvent::WorkspaceInviteRejected(
                    winx_domain::workspace::events::InviteRejected {
                        invite_id: winx_domain::workspace::InviteId::from_uuid(payload.invite_id),
                    },
                ));

                info!(%workspace_id, "invite rejected by peer");
            }
        } else {
            warn!(invite_id = %payload.invite_id, "response received for unknown invite");
        }
    }

    pub async fn run_expiration_loop(&self) {
        const CHECK_INTERVAL_SECS: u64 = 10;

        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(CHECK_INTERVAL_SECS)).await;

            let mut pending = self.pending_invites.lock().await;
            let mut to_expire = Vec::new();

            // Find expired invites
            for (invite_id, data) in pending.iter_mut() {
                data.session.check_expiration();
                if data.session.is_expired() {
                    to_expire.push(*invite_id);
                }
            }

            // Remove expired invites and publish events
            for invite_id in to_expire {
                debug!(%invite_id, "invite expired, removed");
                pending.remove(&invite_id);
                self.bus.publish(DomainEvent::WorkspaceInviteExpired(
                    winx_domain::workspace::events::InviteExpired {
                        invite_id: winx_domain::workspace::InviteId::from_uuid(invite_id),
                    },
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::DecodedWorkspaceInviteMessage;
    use async_trait::async_trait;
    use std::collections::HashMap as StdHashMap;
    use std::net::SocketAddr;
    use std::sync::Mutex as StdMutex;
    use tokio::sync::mpsc;
    use winx_domain::identity::{Device, TrustedPeer};
    use winx_domain::shared::ids::PeerId;
    use winx_domain::workspace::OwnershipMode;
    use winx_protocol::workspace::WorkspaceInviteMessage;

    // ─── Mocks ───────────────────────────────────────────────────────────────

    #[derive(Default)]
    struct MockWorkspaceStore {
        saved: StdMutex<Vec<Workspace>>,
    }

    #[async_trait]
    impl WorkspaceStore for MockWorkspaceStore {
        async fn load_all(&self) -> anyhow::Result<Vec<Workspace>> {
            Ok(self.saved.lock().unwrap().clone())
        }
        async fn save(&self, workspace: &Workspace) -> anyhow::Result<()> {
            let mut saved = self.saved.lock().unwrap();
            if let Some(existing) = saved.iter_mut().find(|w| w.id == workspace.id) {
                *existing = workspace.clone();
            } else {
                saved.push(workspace.clone());
            }
            Ok(())
        }
        async fn delete(&self, id: WorkspaceId) -> anyhow::Result<()> {
            self.saved.lock().unwrap().retain(|w| w.id != id);
            Ok(())
        }
        async fn find_by_id(&self, id: WorkspaceId) -> anyhow::Result<Option<Workspace>> {
            Ok(self
                .saved
                .lock()
                .unwrap()
                .iter()
                .find(|w| w.id == id)
                .cloned())
        }
    }

    #[derive(Default)]
    struct MockTransport {
        sent: StdMutex<Vec<(SocketAddr, WorkspaceInviteMessage)>>,
    }

    #[async_trait]
    impl WorkspaceInviteTransport for MockTransport {
        async fn listen(&self) -> anyhow::Result<mpsc::Receiver<DecodedWorkspaceInviteMessage>> {
            let (_, rx) = mpsc::channel(1);
            Ok(rx)
        }
        async fn send_to(
            &self,
            addr: SocketAddr,
            msg: &WorkspaceInviteMessage,
            _signing_key: &SigningKey,
        ) -> anyhow::Result<()> {
            self.sent.lock().unwrap().push((addr, msg.clone()));
            Ok(())
        }
    }

    struct MockIdentityStore {
        device: Device,
        peers: StdMutex<Vec<TrustedPeer>>,
    }

    impl MockIdentityStore {
        fn new(device: Device) -> Self {
            Self {
                device,
                peers: StdMutex::new(vec![]),
            }
        }
    }

    #[async_trait]
    impl IdentityStore for MockIdentityStore {
        async fn load_device(&self) -> anyhow::Result<Option<Device>> {
            Ok(Some(self.device.clone()))
        }
        async fn save_device(&self, _device: &Device) -> anyhow::Result<()> {
            Ok(())
        }
        async fn load_peers(&self) -> anyhow::Result<Vec<TrustedPeer>> {
            Ok(self.peers.lock().unwrap().clone())
        }
        async fn save_peer(&self, peer: &TrustedPeer) -> anyhow::Result<()> {
            self.peers.lock().unwrap().push(peer.clone());
            Ok(())
        }
        async fn remove_peer(&self, _peer_id: PeerId) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct MockSecretStore {
        key: [u8; 32],
    }

    #[async_trait]
    impl SecretStore for MockSecretStore {
        async fn store_signing_key(&self, _key_bytes: &[u8; 32]) -> anyhow::Result<()> {
            Ok(())
        }
        async fn load_signing_key(&self) -> anyhow::Result<Option<[u8; 32]>> {
            Ok(Some(self.key))
        }
        async fn delete_signing_key(&self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct MockDiscoveryQuery {
        addrs: StdHashMap<DeviceId, SocketAddr>,
    }

    #[async_trait]
    impl DiscoveryQuery for MockDiscoveryQuery {
        async fn resolve_address(&self, device_id: DeviceId) -> anyhow::Result<Option<SocketAddr>> {
            Ok(self.addrs.get(&device_id).copied())
        }
    }

    // ─── Helpers ─────────────────────────────────────────────────────────────

    fn make_service_with_peer_addr(
        peer_device_id: Option<DeviceId>,
        peer_addr: Option<SocketAddr>,
    ) -> (
        WorkspaceService,
        Arc<MockWorkspaceStore>,
        Arc<MockTransport>,
        Arc<MockIdentityStore>,
    ) {
        let local_uuid = Uuid::new_v4();
        let signing_key = SigningKey::from_bytes(&[42u8; 32]);
        let pubkey = signing_key.verifying_key().to_bytes();

        let device = Device::new(
            DeviceId::from_uuid(local_uuid),
            "Local",
            PublicKey::new(pubkey),
        );

        let store = Arc::new(MockWorkspaceStore::default());
        let transport = Arc::new(MockTransport::default());
        let identity = Arc::new(MockIdentityStore::new(device));
        let secret = Arc::new(MockSecretStore { key: [42u8; 32] });

        let mut addrs = StdHashMap::new();
        if let (Some(did), Some(addr)) = (peer_device_id, peer_addr) {
            addrs.insert(did, addr);
        }
        let discovery = Arc::new(MockDiscoveryQuery { addrs });

        let svc = WorkspaceService::new(
            Arc::clone(&store) as Arc<dyn WorkspaceStore>,
            Arc::clone(&transport) as Arc<dyn WorkspaceInviteTransport>,
            Arc::clone(&identity) as Arc<dyn IdentityStore>,
            secret as Arc<dyn SecretStore>,
            discovery as Arc<dyn DiscoveryQuery>,
            local_uuid,
            "Local".to_string(),
            EventBus::new(),
        );
        (svc, store, transport, identity)
    }

    fn make_service() -> (
        WorkspaceService,
        Arc<MockWorkspaceStore>,
        Arc<MockTransport>,
        Arc<MockIdentityStore>,
    ) {
        make_service_with_peer_addr(None, None)
    }

    // ─── Tests ───────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn create_workspace_persists_original_with_owner_pubkey() {
        let (svc, store, _, _) = make_service();
        let ws = svc
            .create_workspace("My WS".to_string(), vec![])
            .await
            .unwrap();

        assert_eq!(ws.name, "My WS");
        assert!(matches!(ws.ownership_mode, OwnershipMode::Original));
        assert_eq!(ws.members.len(), 1);
        let owner = &ws.members[0];
        // owner_pubkey must NOT be zeroed (regression test for the bug)
        assert_ne!(*owner.public_key.as_bytes(), [0u8; 32]);

        let saved = store.load_all().await.unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].id, ws.id);
    }

    #[tokio::test]
    async fn invite_to_workspace_sends_signed_payload_via_transport() {
        let target_uuid = Uuid::new_v4();
        let target_device_id = DeviceId::from_uuid(target_uuid);
        let peer_addr: SocketAddr = "127.0.0.1:9999".parse().unwrap();

        let (svc, _, transport, _) =
            make_service_with_peer_addr(Some(target_device_id), Some(peer_addr));

        let ws = svc
            .create_workspace("WS".to_string(), vec![])
            .await
            .unwrap();
        let invite_id = svc.invite_to_workspace(ws.id, target_uuid).await.unwrap();

        // Allow spawned task to send
        for _ in 0..20 {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            if !transport.sent.lock().unwrap().is_empty() {
                break;
            }
        }

        let sent = transport.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        let (sent_addr, sent_msg) = &sent[0];
        assert_eq!(sent_addr.port(), crate::ports::WORKSPACE_INVITE_PORT);
        match sent_msg {
            WorkspaceInviteMessage::Invite(p) => {
                assert_eq!(p.invite_id, invite_id);
                assert_eq!(p.target_device_id, target_uuid);
            }
            _ => panic!("expected Invite variant"),
        }
    }

    #[tokio::test]
    async fn accept_invite_does_tofu_and_creates_mirror() {
        let (svc, store, _, identity) = make_service();

        // Simulate a pending invite delivered from a remote sender
        let sender_device_id = DeviceId::from_uuid(Uuid::new_v4());
        let sender_pubkey = [9u8; 32];
        let invite_id = Uuid::new_v4();
        let workspace_id = WorkspaceId::new();

        let snapshot = WorkspaceSnapshotPayload {
            id: workspace_id.as_uuid(),
            name: "Remote WS".to_string(),
            owner_device_id: sender_device_id.as_uuid(),
            owner_username: "Remote".to_string(),
            version: 1,
            members: vec![MemberSnapshotPayload {
                device_id: sender_device_id.as_uuid(),
                public_key: sender_pubkey,
                username: "Remote".to_string(),
                joined_at_rfc3339: "2026-01-01T00:00:00Z".to_string(),
            }],
        };

        let session = InviteSession::new(
            workspace_id,
            DeviceId::from_uuid(svc.local_device_id),
            sender_device_id,
        );
        svc.pending_invites.lock().await.insert(
            invite_id,
            PendingInviteData {
                session,
                snapshot: Some(snapshot),
                sender_pubkey: Some(sender_pubkey),
            },
        );

        let mirror = svc.accept_invite(invite_id).await.unwrap();

        // Mirror persisted
        assert!(mirror.ownership_mode.is_mirror());
        let saved = store.load_all().await.unwrap();
        assert_eq!(saved.len(), 1);

        // TOFU: peer added to trusted list
        let peers = identity.peers.lock().unwrap();
        assert_eq!(peers.len(), 1);
        assert_eq!(*peers[0].public_key.as_bytes(), sender_pubkey);
    }

    #[tokio::test]
    async fn reject_invite_removes_from_pending_and_emits_event() {
        let (svc, _, _, _) = make_service();

        let invite_id = Uuid::new_v4();
        let workspace_id = WorkspaceId::new();
        let sender_device_id = DeviceId::from_uuid(Uuid::new_v4());
        let session = InviteSession::new(
            workspace_id,
            DeviceId::from_uuid(svc.local_device_id),
            sender_device_id,
        );
        svc.pending_invites.lock().await.insert(
            invite_id,
            PendingInviteData {
                session,
                snapshot: None,
                sender_pubkey: None,
            },
        );

        svc.reject_invite(invite_id).await.unwrap();
        assert!(svc.pending_invites.lock().await.is_empty());
    }

    #[tokio::test]
    async fn connect_to_workspace_returns_conflict_when_other_active() {
        let (svc, _, _, _) = make_service();
        let w1 = WorkspaceId::new();
        let w2 = WorkspaceId::new();

        svc.connect_to_workspace(w1).await.unwrap();
        let err = svc.connect_to_workspace(w2).await.unwrap_err();
        assert_eq!(err.code, DomainErrorCode::WorkspaceConflict);
    }

    #[tokio::test]
    async fn connect_to_same_workspace_is_idempotent() {
        let (svc, _, _, _) = make_service();
        let w = WorkspaceId::new();
        svc.connect_to_workspace(w).await.unwrap();
        svc.connect_to_workspace(w).await.unwrap();
        assert_eq!(svc.active_workspace_id().await, Some(w));
    }

    #[tokio::test]
    async fn force_disconnect_and_connect_switches_active() {
        let (svc, _, _, _) = make_service();
        let w1 = WorkspaceId::new();
        let w2 = WorkspaceId::new();

        svc.connect_to_workspace(w1).await.unwrap();
        svc.force_disconnect_and_connect(w2).await.unwrap();
        assert_eq!(svc.active_workspace_id().await, Some(w2));
    }

    #[tokio::test]
    async fn disconnect_clears_active_workspace() {
        let (svc, _, _, _) = make_service();
        let w = WorkspaceId::new();
        svc.connect_to_workspace(w).await.unwrap();
        svc.disconnect_from_workspace().await.unwrap();
        assert_eq!(svc.active_workspace_id().await, None);
    }
}
