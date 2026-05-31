use ed25519_dalek::SigningKey;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use uuid::Uuid;

use winx_domain::identity::key::PublicKey;
use winx_domain::shared::{ids::DeviceId, DomainError, DomainErrorCode, DomainEvent};
use winx_domain::workspace::{
    GlobalCursorUpdate, InviteSession, Workspace, WorkspaceId, WorkspaceMember,
};
use winx_protocol::workspace::{
    GlobalCursorPayload, MemberSnapshotPayload, WorkspaceDeletePayload, WorkspaceInviteMessage,
    WorkspaceInvitePayload, WorkspaceInviteResponsePayload, WorkspaceSnapshotPayload,
    WorkspaceSyncPayload,
};

use crate::bus::EventBus;
use crate::ports::{
    DiscoveryQuery, IdentityStore, SecretStore, WorkspaceGlobalCursor, WorkspaceInviteTransport,
    WorkspaceStore,
};
use tracing::{debug, info, warn};
use winx_domain::identity::TrustedPeer;
use winx_domain::shared::ids::PeerId;

struct PendingInviteData {
    session: InviteSession,
    snapshot: Option<WorkspaceSnapshotPayload>,
    sender_pubkey: Option<[u8; 32]>,
}

const CURSOR_BROADCAST_MIN_INTERVAL: Duration = Duration::from_millis(16);
const CURSOR_PERSIST_DEBOUNCE: Duration = Duration::from_secs(1);

/// Mutação aplicável a um Workspace via use case `update_workspace`.
///
/// Cada variante mapeia 1-para-1 com uma operação no aggregate `Workspace`.
#[derive(Debug, Clone)]
pub enum WorkspacePatch {
    Rename {
        new_name: String,
    },
    AddMember {
        device_id: DeviceId,
        public_key: PublicKey,
        username: String,
    },
    RemoveMember {
        device_id: DeviceId,
    },
    UpdateLayout {
        device_id: DeviceId,
        layout: winx_domain::input_control::layout::MonitorLayout,
    },
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
    member_online_state: Arc<Mutex<HashMap<(WorkspaceId, DeviceId), bool>>>,
    cursor_last_broadcast: Arc<Mutex<Option<Instant>>>,
    cursor_persist_generation: Arc<AtomicU64>,
    cursor_pending: Arc<Mutex<HashMap<WorkspaceId, Workspace>>>,
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
            member_online_state: Arc::new(Mutex::new(HashMap::new())),
            cursor_last_broadcast: Arc::new(Mutex::new(None)),
            cursor_persist_generation: Arc::new(AtomicU64::new(0)),
            cursor_pending: Arc::new(Mutex::new(HashMap::new())),
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
        responder_username: String,
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
            responder_username,
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

        self.refresh_member_presence(std::slice::from_ref(&ws))
            .await;

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
        let snapshot = build_snapshot_payload(&ws);

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
        let local_device_id = DeviceId::from_uuid(self.local_device_id);
        let local_device = self
            .identity_store
            .load_device()
            .await
            .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?
            .ok_or_else(|| {
                DomainError::new(DomainErrorCode::InternalError, "local device not found")
            })?;

        let mut members: Vec<WorkspaceMember> = snapshot
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

        // Mirror local inclui o device que aceitou (snapshot do invite só traz membros atuais do owner).
        let local_member = WorkspaceMember::new(
            local_device_id,
            local_device.public_key,
            self.local_username.clone(),
        );
        if !members.iter().any(|m| m.device_id == local_device_id) {
            members.push(local_member);
        }

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
        let responder_pubkey = *local_device.public_key.as_bytes();
        self.send_invite_response(
            sender_device_id,
            invite_id,
            responder_pubkey,
            true,
            self.local_username.clone(),
        )
        .await;

        // Publish event
        self.bus.publish(DomainEvent::WorkspaceInviteAccepted(
            winx_domain::workspace::events::InviteAccepted {
                invite_id: winx_domain::workspace::InviteId::from_uuid(invite_id),
                workspace_id,
                accepting_device_id: DeviceId::from_uuid(self.local_device_id),
            },
        ));

        self.refresh_member_presence(std::slice::from_ref(&mirror))
            .await;

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
        self.send_invite_response(
            sender_device_id,
            invite_id,
            responder_pubkey,
            false,
            String::new(),
        )
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
        if let Ok(workspaces) = self.store.load_all().await {
            self.refresh_member_presence(&workspaces).await;
        }
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
        if let Ok(workspaces) = self.store.load_all().await {
            self.refresh_member_presence(&workspaces).await;
        }
        info!(%workspace_id, "connected to workspace (force switch)");
        Ok(())
    }

    pub async fn delete_workspace(&self, id: WorkspaceId) -> Result<(), DomainError> {
        let ws = self
            .store
            .find_by_id(id)
            .await
            .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?
            .ok_or_else(|| {
                DomainError::new(DomainErrorCode::InternalError, "workspace not found")
            })?;

        if ws.ownership_mode.is_mirror() {
            return Err(DomainError::new(
                DomainErrorCode::WorkspaceNotOwner,
                "use forget_workspace to remove a mirror locally",
            ));
        }

        // Notify all members BEFORE removing locally (best-effort)
        self.broadcast_delete(&ws).await;

        self.store
            .delete(id)
            .await
            .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?;

        self.bus.publish(DomainEvent::WorkspaceDeleted(
            winx_domain::workspace::events::WorkspaceDeleted { workspace_id: id },
        ));

        info!(%id, "workspace deleted");
        Ok(())
    }

    /// Aplica uma mutação local e propaga `Sync` para todos os membros.
    pub async fn update_workspace(
        &self,
        workspace_id: WorkspaceId,
        patch: WorkspacePatch,
    ) -> Result<Workspace, DomainError> {
        let mut ws = self
            .store
            .find_by_id(workspace_id)
            .await
            .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?
            .ok_or_else(|| {
                DomainError::new(DomainErrorCode::InternalError, "workspace not found")
            })?;

        if ws.ownership_mode.is_mirror() {
            return Err(DomainError::new(
                DomainErrorCode::WorkspaceMirrorImmutable,
                "mirrors cannot be edited locally",
            ));
        }

        apply_patch_local(&mut ws, patch)?;

        self.store
            .save(&ws)
            .await
            .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?;

        self.bus.publish(DomainEvent::WorkspaceSyncApplied(
            winx_domain::workspace::events::WorkspaceSyncApplied {
                workspace_id,
                new_version: ws.version.as_u64(),
                workspace_name: ws.name.clone(),
                from_remote: false,
            },
        ));

        self.broadcast_sync(&ws).await;

        Ok(ws)
    }

    /// Remove um mirror localmente sem notificar o owner. Usado para órfãos
    /// ou para "sair" voluntariamente.
    pub async fn forget_workspace(&self, id: WorkspaceId) -> Result<(), DomainError> {
        let ws = self
            .store
            .find_by_id(id)
            .await
            .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?
            .ok_or_else(|| {
                DomainError::new(DomainErrorCode::InternalError, "workspace not found")
            })?;

        if !ws.ownership_mode.is_mirror() {
            return Err(DomainError::new(
                DomainErrorCode::WorkspaceMirrorImmutable,
                "use delete_workspace on originals",
            ));
        }

        self.store
            .delete(id)
            .await
            .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?;

        self.bus.publish(DomainEvent::WorkspaceDeleted(
            winx_domain::workspace::events::WorkspaceDeleted { workspace_id: id },
        ));

        info!(%id, "mirror forgotten locally");
        Ok(())
    }

    pub async fn active_workspace_id(&self) -> Option<WorkspaceId> {
        self.active_workspace.read().await.clone()
    }

    pub async fn get_workspace(&self, id: WorkspaceId) -> Result<Workspace, DomainError> {
        self.store
            .find_by_id(id)
            .await
            .map_err(|e| DomainError::new(DomainErrorCode::InternalError, e.to_string()))?
            .ok_or_else(|| DomainError::new(DomainErrorCode::InternalError, "workspace not found"))
    }

    /// Peer remoto principal para KVM (primeiro membro que não é este device).
    #[must_use]
    pub fn primary_remote_peer(&self, ws: &Workspace) -> Option<PeerId> {
        let local = DeviceId::from_uuid(self.local_device_id);
        ws.members
            .iter()
            .find(|m| m.device_id != local)
            .map(|m| PeerId::from_uuid(m.device_id.as_uuid()))
    }

    fn resolve_input_layout(
        &self,
        ws: &Workspace,
        remote_peer: PeerId,
        local_monitors: Vec<winx_domain::input_control::MonitorRect>,
    ) -> Option<winx_domain::input_control::MonitorLayout> {
        use winx_domain::input_control::MonitorLayout;

        let local_device_id = DeviceId::from_uuid(self.local_device_id);
        let remote_device = DeviceId::from_uuid(remote_peer.as_uuid());
        if !ws
            .members
            .iter()
            .any(|m| m.device_id == remote_device)
        {
            return None;
        }

        if let Some(saved) = ws.layout.get(local_device_id) {
            let mut layout = saved.clone();
            layout.local_monitors = local_monitors;
            layout.remote_peer = remote_peer;
            return Some(layout);
        }

        Some(MonitorLayout::default_side_by_side(
            local_monitors,
            remote_peer,
        ))
    }

    async fn load_workspace_for_cursor(&self, workspace_id: WorkspaceId) -> Option<Workspace> {
        if let Some(ws) = self.cursor_pending.lock().await.get(&workspace_id).cloned() {
            return Some(ws);
        }
        self.store.find_by_id(workspace_id).await.ok().flatten()
    }

    async fn schedule_cursor_persist(&self, workspace_id: WorkspaceId, ws: Workspace) {
        {
            let mut pending = self.cursor_pending.lock().await;
            pending.insert(workspace_id, ws);
        }
        let gen = self
            .cursor_persist_generation
            .fetch_add(1, Ordering::SeqCst)
            + 1;
        let svc = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(CURSOR_PERSIST_DEBOUNCE).await;
            if svc.cursor_persist_generation.load(Ordering::SeqCst) != gen {
                return;
            }
            let ws = {
                let mut pending = svc.cursor_pending.lock().await;
                pending.remove(&workspace_id)
            };
            if let Some(ws) = ws {
                if let Err(e) = svc.store.save(&ws).await {
                    warn!(?e, %workspace_id, "failed to persist global cursor");
                }
            }
        });
    }

    /// Publica a posição local do cursor global (throttle 60Hz) e faz broadcast UDP.
    pub async fn publish_global_cursor(&self, x: i32, y: i32) {
        let workspace_id = match self.active_workspace.read().await.clone() {
            Some(id) => id,
            None => return,
        };

        {
            let mut last = self.cursor_last_broadcast.lock().await;
            let now = Instant::now();
            if let Some(prev) = *last {
                if now.duration_since(prev) < CURSOR_BROADCAST_MIN_INTERVAL {
                    return;
                }
            }
            *last = Some(now);
        }

        let local_device = match self.identity_store.load_device().await {
            Ok(Some(d)) => d,
            _ => return,
        };
        let local_device_id = local_device.id;

        let mut ws = match self.load_workspace_for_cursor(workspace_id).await {
            Some(ws) => ws,
            None => {
                warn!(%workspace_id, "publish_global_cursor: workspace not found");
                return;
            }
        };

        ws.global_cursor.update_local(x, y, local_device_id);

        let monotonic_seq = ws.global_cursor.monotonic_seq;
        let active_device_id = ws.global_cursor.active_device_id.unwrap_or(local_device_id);
        let sender_pubkey = *local_device.public_key.as_bytes();

        self.bus.publish(DomainEvent::WorkspaceGlobalCursorMoved(
            winx_domain::workspace::events::GlobalCursorMoved {
                workspace_id,
                x,
                y,
                seq: monotonic_seq,
            },
        ));

        let payload = GlobalCursorPayload {
            workspace_id: workspace_id.as_uuid(),
            x,
            y,
            active_device_id: active_device_id.as_uuid(),
            monotonic_seq,
            sender_device_id: self.local_device_id,
            sender_pubkey,
        };

        self.broadcast_global_cursor(&ws, payload).await;
        self.schedule_cursor_persist(workspace_id, ws).await;
    }

    pub async fn handle_global_cursor(&self, payload: &GlobalCursorPayload) {
        let workspace_id = WorkspaceId::from_uuid(payload.workspace_id);

        let mut ws = match self.load_workspace_for_cursor(workspace_id).await {
            Some(ws) => ws,
            None => {
                debug!(%workspace_id, "global cursor for unknown workspace, ignoring");
                return;
            }
        };

        let sender_device = DeviceId::from_uuid(payload.sender_device_id);
        let is_member = ws.members.iter().any(|m| {
            m.device_id == sender_device && m.public_key.as_bytes() == &payload.sender_pubkey
        });
        if !is_member {
            warn!(
                %workspace_id,
                %sender_device,
                "global cursor rejected: sender not a workspace member"
            );
            return;
        }

        let update = GlobalCursorUpdate {
            x: payload.x,
            y: payload.y,
            active_device_id: DeviceId::from_uuid(payload.active_device_id),
            monotonic_seq: payload.monotonic_seq,
        };

        if ws.apply_cursor(update).is_err() {
            debug!(
                %workspace_id,
                local_seq = ws.global_cursor.monotonic_seq,
                incoming_seq = payload.monotonic_seq,
                "global cursor update rejected (stale seq)"
            );
            return;
        }

        self.bus.publish(DomainEvent::WorkspaceGlobalCursorMoved(
            winx_domain::workspace::events::GlobalCursorMoved {
                workspace_id,
                x: payload.x,
                y: payload.y,
                seq: payload.monotonic_seq,
            },
        ));

        self.schedule_cursor_persist(workspace_id, ws).await;
    }

    async fn broadcast_global_cursor(&self, ws: &Workspace, payload: GlobalCursorPayload) {
        let signing_key = match self.load_signing_key().await {
            Ok(k) => k,
            Err(e) => {
                warn!(workspace_id = %ws.id, ?e, "failed to load signing key for cursor broadcast");
                return;
            }
        };
        let local_device = match self.identity_store.load_device().await {
            Ok(Some(d)) => d,
            _ => return,
        };

        let msg = WorkspaceInviteMessage::GlobalCursor(payload);

        for member in &ws.members {
            if member.device_id == local_device.id {
                continue;
            }
            let target_device_id = member.device_id;
            let svc = self.clone();
            let msg_clone = msg.clone();
            let signing_key_clone = signing_key.clone();
            tokio::spawn(async move {
                match svc.discovery_query.resolve_address(target_device_id).await {
                    Ok(Some(mut addr)) => {
                        addr.set_port(crate::ports::WORKSPACE_INVITE_PORT);
                        if let Err(e) = svc
                            .transport
                            .send_to(addr, &msg_clone, &signing_key_clone)
                            .await
                        {
                            warn!(?e, "failed to send global cursor");
                        }
                    }
                    Ok(None) => {
                        debug!(%target_device_id, "member offline, cursor broadcast skipped")
                    }
                    Err(e) => warn!(?e, "failed to resolve member addr for cursor"),
                }
            });
        }
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
                WorkspaceInviteMessage::Sync(payload) => {
                    self.handle_workspace_sync(payload).await;
                }
                WorkspaceInviteMessage::Delete(payload) => {
                    self.handle_workspace_delete(payload).await;
                }
                WorkspaceInviteMessage::GlobalCursor(payload) => {
                    self.handle_global_cursor(payload).await;
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

    /// Owner: adiciona membro ao workspace Original quando o invite é aceito.
    async fn register_accepted_invite_member(
        &self,
        workspace_id: WorkspaceId,
        device_id: DeviceId,
        public_key: PublicKey,
        username: String,
    ) {
        let mut ws = match self.store.find_by_id(workspace_id).await {
            Ok(Some(w)) => w,
            Ok(None) => {
                warn!(%workspace_id, "invite accept for unknown workspace");
                return;
            }
            Err(e) => {
                warn!(?e, %workspace_id, "failed to load workspace for invite accept");
                return;
            }
        };

        if ws.ownership_mode.is_mirror() {
            return;
        }

        if ws.members.iter().any(|m| m.device_id == device_id) {
            debug!(%workspace_id, %device_id, "member already in workspace");
            return;
        }

        let member = WorkspaceMember::new(device_id, public_key, username.clone());
        if let Err(e) = ws.add_member(member) {
            warn!(?e, %workspace_id, "add_member failed on invite accept");
            return;
        }

        let peer = TrustedPeer::new(
            PeerId::from_uuid(device_id.as_uuid()),
            username.clone(),
            public_key,
        );
        if let Err(e) = self.identity_store.save_peer(&peer).await {
            warn!(?e, %device_id, "failed to TOFU peer on invite accept");
        }

        if let Err(e) = self.store.save(&ws).await {
            warn!(?e, %workspace_id, "failed to persist workspace after invite accept");
            return;
        }

        self.bus.publish(DomainEvent::WorkspaceMemberJoined(
            winx_domain::workspace::events::MemberJoined {
                workspace_id,
                device_id,
                username,
            },
        ));

        self.bus.publish(DomainEvent::WorkspaceSyncApplied(
            winx_domain::workspace::events::WorkspaceSyncApplied {
                workspace_id,
                new_version: ws.version.as_u64(),
                workspace_name: ws.name.clone(),
                from_remote: false,
            },
        ));

        info!(%workspace_id, %device_id, member_count = ws.members.len(), "member added after invite accept");
        self.broadcast_sync(&ws).await;
        self.refresh_member_presence(std::slice::from_ref(&ws)).await;
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
                let accepting_device_id = DeviceId::from_uuid(payload.responder_device_id);
                let username = if payload.responder_username.is_empty() {
                    accepting_device_id.to_string()
                } else {
                    payload.responder_username.clone()
                };
                self.register_accepted_invite_member(
                    workspace_id,
                    accepting_device_id,
                    PublicKey::new(payload.responder_pubkey),
                    username,
                )
                .await;

                self.bus.publish(DomainEvent::WorkspaceInviteAccepted(
                    winx_domain::workspace::events::InviteAccepted {
                        invite_id: winx_domain::workspace::InviteId::from_uuid(payload.invite_id),
                        workspace_id,
                        accepting_device_id,
                    },
                ));

                info!(%workspace_id, %accepting_device_id, "invite accepted by peer");
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

    async fn handle_workspace_sync(&self, payload: &WorkspaceSyncPayload) {
        let workspace_id = WorkspaceId::from_uuid(payload.workspace_id);

        let mut ws = match self.store.find_by_id(workspace_id).await {
            Ok(Some(w)) => w,
            Ok(None) => {
                debug!(%workspace_id, "received sync for unknown workspace, ignoring");
                return;
            }
            Err(e) => {
                warn!(?e, %workspace_id, "failed to load workspace for sync");
                return;
            }
        };

        // Convert protocol snapshot → domain snapshot
        let domain_members: Vec<WorkspaceMember> = payload
            .snapshot
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
            id: workspace_id,
            name: payload.snapshot.name.clone(),
            owner_device_id: DeviceId::from_uuid(payload.snapshot.owner_device_id),
            version: winx_domain::workspace::WorkspaceVersion::from_u64(payload.snapshot.version),
            ownership_mode: ws.ownership_mode.clone(),
            members: domain_members,
            layout: ws.layout.clone(), // layout não vem no MVP de sync; preservar local
        };

        let local_version = ws.version.as_u64();
        let outcome = ws.apply_sync(domain_snapshot);

        match outcome {
            winx_domain::workspace::SyncOutcome::Applied => {
                // Mirror: refresh owner_last_seen
                if let winx_domain::workspace::OwnershipMode::Mirror { .. } = &mut ws.ownership_mode
                {
                    let _ = ws.ownership_mode.touch_owner_seen();
                }
                if let Err(e) = self.store.save(&ws).await {
                    warn!(?e, %workspace_id, "failed to persist synced workspace");
                    return;
                }
                self.bus.publish(DomainEvent::WorkspaceSyncApplied(
                    winx_domain::workspace::events::WorkspaceSyncApplied {
                        workspace_id,
                        new_version: ws.version.as_u64(),
                        workspace_name: ws.name.clone(),
                        from_remote: true,
                    },
                ));
                info!(%workspace_id, new_version = ws.version.as_u64(), "sync applied (LWW)");
            }
            winx_domain::workspace::SyncOutcome::Discarded {
                incoming_version, ..
            } => {
                self.bus.publish(DomainEvent::WorkspaceSyncDiscarded(
                    winx_domain::workspace::events::WorkspaceSyncDiscarded {
                        workspace_id,
                        local_version,
                        incoming_version,
                    },
                ));
                debug!(%workspace_id, local_version, incoming_version, "sync discarded (LWW)");
            }
        }
    }

    async fn handle_workspace_delete(&self, payload: &WorkspaceDeletePayload) {
        let workspace_id = WorkspaceId::from_uuid(payload.workspace_id);

        let mut ws = match self.store.find_by_id(workspace_id).await {
            Ok(Some(w)) => w,
            Ok(None) => return,
            Err(e) => {
                warn!(?e, %workspace_id, "failed to load workspace for delete notice");
                return;
            }
        };

        // Only mirrors should be marked orphan; an Original receiving delete is suspicious — ignore.
        if !ws.ownership_mode.is_mirror() {
            warn!(%workspace_id, "received Delete for an Original workspace, ignoring");
            return;
        }

        if let Err(e) = ws.mark_orphan() {
            warn!(?e, %workspace_id, "failed to mark orphan");
            return;
        }

        if let Err(e) = self.store.save(&ws).await {
            warn!(?e, %workspace_id, "failed to persist orphan flag");
            return;
        }

        self.bus.publish(DomainEvent::WorkspaceMarkedOrphan(
            winx_domain::workspace::events::WorkspaceMarkedOrphan { workspace_id },
        ));

        info!(%workspace_id, "mirror marked as orphan after owner delete");
    }

    /// Envia `WorkspaceSyncPayload` assinado para todos os membros (exceto self).
    async fn broadcast_sync(&self, ws: &Workspace) {
        let signing_key = match self.load_signing_key().await {
            Ok(k) => k,
            Err(e) => {
                warn!(workspace_id = %ws.id, ?e, "failed to load signing key for sync broadcast");
                return;
            }
        };
        let local_device = match self.identity_store.load_device().await {
            Ok(Some(d)) => d,
            _ => return,
        };
        let sender_pubkey = *local_device.public_key.as_bytes();

        let snapshot = build_snapshot_payload(ws);

        for member in &ws.members {
            if member.device_id == local_device.id {
                continue;
            }
            let payload = WorkspaceSyncPayload {
                workspace_id: ws.id.as_uuid(),
                snapshot: snapshot.clone(),
                sender_device_id: self.local_device_id,
                sender_pubkey,
            };
            let msg = WorkspaceInviteMessage::Sync(payload);
            let target_device_id = member.device_id;
            let svc = self.clone();
            let signing_key_clone = signing_key.clone();
            tokio::spawn(async move {
                match svc.discovery_query.resolve_address(target_device_id).await {
                    Ok(Some(mut addr)) => {
                        addr.set_port(crate::ports::WORKSPACE_INVITE_PORT);
                        if let Err(e) = svc.transport.send_to(addr, &msg, &signing_key_clone).await
                        {
                            warn!(?e, "failed to send workspace sync");
                        }
                    }
                    Ok(None) => debug!(%target_device_id, "member offline, sync skipped"),
                    Err(e) => warn!(?e, "failed to resolve member addr"),
                }
            });
        }
    }

    async fn broadcast_delete(&self, ws: &Workspace) {
        let signing_key = match self.load_signing_key().await {
            Ok(k) => k,
            Err(e) => {
                warn!(?e, "failed to load signing key for delete broadcast");
                return;
            }
        };
        let local_device = match self.identity_store.load_device().await {
            Ok(Some(d)) => d,
            _ => return,
        };
        let sender_pubkey = *local_device.public_key.as_bytes();
        let workspace_id = ws.id.as_uuid();

        for member in &ws.members {
            if member.device_id == local_device.id {
                continue;
            }
            let payload = WorkspaceDeletePayload {
                workspace_id,
                sender_device_id: self.local_device_id,
                sender_pubkey,
            };
            let msg = WorkspaceInviteMessage::Delete(payload);
            let target_device_id = member.device_id;
            let svc = self.clone();
            let signing_key_clone = signing_key.clone();
            tokio::spawn(async move {
                match svc.discovery_query.resolve_address(target_device_id).await {
                    Ok(Some(mut addr)) => {
                        addr.set_port(crate::ports::WORKSPACE_INVITE_PORT);
                        if let Err(e) = svc.transport.send_to(addr, &msg, &signing_key_clone).await
                        {
                            warn!(?e, "failed to send workspace delete");
                        }
                    }
                    Ok(None) => debug!(%target_device_id, "member offline, delete notice skipped"),
                    Err(e) => warn!(?e, "failed to resolve member addr"),
                }
            });
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

    /// Atualiza presença de todos os membros via mDNS (registry compartilhado).
    ///
    /// Emite `MemberPresenceChanged` apenas quando o estado muda. O device local
    /// é sempre considerado online.
    async fn refresh_member_presence(&self, workspaces: &[Workspace]) {
        let local_device_id = DeviceId::from_uuid(self.local_device_id);
        let mut state = self.member_online_state.lock().await;

        for ws in workspaces {
            for member in &ws.members {
                let is_online = if member.device_id == local_device_id {
                    true
                } else {
                    matches!(
                        self.discovery_query.resolve_address(member.device_id).await,
                        Ok(Some(_))
                    )
                };

                let key = (ws.id, member.device_id);
                if state.get(&key).copied() != Some(is_online) {
                    state.insert(key, is_online);
                    self.bus
                        .publish(DomainEvent::WorkspaceMemberPresenceChanged(
                            winx_domain::workspace::events::MemberPresenceChanged {
                                workspace_id: ws.id,
                                device_id: member.device_id,
                                is_online,
                            },
                        ));
                }
            }
        }
    }

    /// Loop periódico que republica presença de membros com base no mDNS.
    pub async fn run_presence_watcher(&self) {
        const CHECK_INTERVAL_SECS: u64 = 5;

        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(CHECK_INTERVAL_SECS)).await;

            let workspaces = match self.store.load_all().await {
                Ok(ws) => ws,
                Err(e) => {
                    warn!(?e, "presence_watcher: failed to load workspaces");
                    continue;
                }
            };

            self.refresh_member_presence(&workspaces).await;
        }
    }

    /// Reavalia presença imediatamente quando peers aparecem/somem no mDNS.
    pub async fn run_presence_on_discovery(&self) {
        let mut rx = self.bus.subscribe();
        loop {
            match rx.recv().await {
                Ok(
                    DomainEvent::PeerAppeared(_)
                    | DomainEvent::PeerDisappeared(_)
                    | DomainEvent::PeerUpdated(_),
                ) => {
                    if let Ok(workspaces) = self.store.load_all().await {
                        self.refresh_member_presence(&workspaces).await;
                    }
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    }
}

fn apply_patch_local(ws: &mut Workspace, patch: WorkspacePatch) -> Result<(), DomainError> {
    let map_err = |e: String| {
        if e == "workspace.mirror_immutable" {
            DomainError::new(DomainErrorCode::WorkspaceMirrorImmutable, e)
        } else {
            DomainError::new(DomainErrorCode::InternalError, e)
        }
    };
    match patch {
        WorkspacePatch::Rename { new_name } => ws.rename(new_name).map_err(map_err),
        WorkspacePatch::AddMember {
            device_id,
            public_key,
            username,
        } => {
            let member = WorkspaceMember::new(device_id, public_key, username);
            ws.add_member(member).map_err(map_err)
        }
        WorkspacePatch::RemoveMember { device_id } => ws.remove_member(device_id).map_err(map_err),
        WorkspacePatch::UpdateLayout { device_id, layout } => {
            ws.update_layout(device_id, layout).map_err(map_err)
        }
    }
}

#[async_trait::async_trait]
impl WorkspaceGlobalCursor for WorkspaceService {
    async fn publish_local_cursor(&self, x: i32, y: i32) {
        self.publish_global_cursor(x, y).await;
    }

    async fn restore_cursor_on_focus(&self) -> Option<(i32, i32)> {
        let workspace_id = self.active_workspace.read().await.clone()?;
        let ws = self.load_workspace_for_cursor(workspace_id).await?;
        Some((ws.global_cursor.x, ws.global_cursor.y))
    }

    async fn input_layout_for_peer(
        &self,
        remote_peer: PeerId,
        local_monitors: Vec<winx_domain::input_control::MonitorRect>,
    ) -> Option<winx_domain::input_control::MonitorLayout> {
        let workspace_id = self.active_workspace.read().await.clone()?;
        let ws = self.load_workspace_for_cursor(workspace_id).await?;
        self.resolve_input_layout(&ws, remote_peer, local_monitors)
    }
}

fn build_snapshot_payload(ws: &Workspace) -> WorkspaceSnapshotPayload {
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

    let owner_username = ws
        .members
        .iter()
        .find(|m| m.device_id == ws.owner_device_id)
        .map(|m| m.username_cache.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    WorkspaceSnapshotPayload {
        id: ws.id.as_uuid(),
        name: ws.name.clone(),
        owner_device_id: ws.owner_device_id.as_uuid(),
        owner_username,
        version: ws.version.as_u64(),
        members: members_snapshot,
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
    async fn update_workspace_rename_increments_version_and_persists() {
        let (svc, store, transport, _) = make_service();
        let ws = svc
            .create_workspace("Old Name".to_string(), vec![])
            .await
            .unwrap();
        let v0 = ws.version.as_u64();

        let updated = svc
            .update_workspace(
                ws.id,
                WorkspacePatch::Rename {
                    new_name: "New Name".to_string(),
                },
            )
            .await
            .unwrap();

        assert_eq!(updated.name, "New Name");
        assert_eq!(updated.version.as_u64(), v0 + 1);

        let loaded = store.load_all().await.unwrap();
        assert_eq!(loaded[0].name, "New Name");

        // Solo workspace (apenas owner) — não há membros remotos pra enviar Sync
        assert!(transport.sent.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn update_mirror_returns_mirror_immutable_error() {
        let (svc, store, _, _) = make_service();
        let owner_member = WorkspaceMember::new(
            DeviceId::from_uuid(Uuid::new_v4()),
            PublicKey::new([1u8; 32]),
            "Other".to_string(),
        );
        let snapshot = winx_domain::workspace::WorkspaceSnapshot {
            id: WorkspaceId::new(),
            name: "Mirror".to_string(),
            owner_device_id: owner_member.device_id,
            version: winx_domain::workspace::WorkspaceVersion::initial(),
            ownership_mode: winx_domain::workspace::OwnershipMode::Original,
            members: vec![owner_member],
            layout: winx_domain::workspace::WorkspaceLayout::empty(),
        };
        let mirror = Workspace::create_mirror(snapshot, "Other");
        let mirror_id = mirror.id;
        store.save(&mirror).await.unwrap();

        let err = svc
            .update_workspace(
                mirror_id,
                WorkspacePatch::Rename {
                    new_name: "x".into(),
                },
            )
            .await
            .unwrap_err();

        assert_eq!(err.code, DomainErrorCode::WorkspaceMirrorImmutable);
    }

    #[tokio::test]
    async fn update_workspace_broadcasts_sync_to_remote_members() {
        let remote_uuid = Uuid::new_v4();
        let remote_device = DeviceId::from_uuid(remote_uuid);
        let peer_addr: SocketAddr = "127.0.0.1:8001".parse().unwrap();

        let (svc, store, transport, _) =
            make_service_with_peer_addr(Some(remote_device), Some(peer_addr));

        let mut ws = svc
            .create_workspace("WS".to_string(), vec![])
            .await
            .unwrap();
        let remote_member = WorkspaceMember::new(
            remote_device,
            PublicKey::new([8u8; 32]),
            "Remote".to_string(),
        );
        ws.add_member(remote_member).unwrap();
        store.save(&ws).await.unwrap();

        svc.update_workspace(
            ws.id,
            WorkspacePatch::Rename {
                new_name: "Renamed".into(),
            },
        )
        .await
        .unwrap();

        for _ in 0..20 {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            if !transport.sent.lock().unwrap().is_empty() {
                break;
            }
        }

        let sent = transport.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        let (_, msg) = &sent[0];
        assert!(matches!(msg, WorkspaceInviteMessage::Sync(_)));
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

    #[tokio::test]
    async fn delete_mirror_returns_not_owner_error() {
        let (svc, store, _, _) = make_service();
        let owner_member = WorkspaceMember::new(
            DeviceId::from_uuid(Uuid::new_v4()),
            PublicKey::new([1u8; 32]),
            "Other".to_string(),
        );
        let snapshot = winx_domain::workspace::WorkspaceSnapshot {
            id: WorkspaceId::new(),
            name: "Mirror".to_string(),
            owner_device_id: owner_member.device_id,
            version: winx_domain::workspace::WorkspaceVersion::initial(),
            ownership_mode: winx_domain::workspace::OwnershipMode::Original,
            members: vec![owner_member],
            layout: winx_domain::workspace::WorkspaceLayout::empty(),
        };
        let mirror = Workspace::create_mirror(snapshot, "Other");
        let mirror_id = mirror.id;
        store.save(&mirror).await.unwrap();

        let err = svc.delete_workspace(mirror_id).await.unwrap_err();
        assert_eq!(err.code, DomainErrorCode::WorkspaceNotOwner);
    }

    #[tokio::test]
    async fn forget_workspace_removes_mirror_locally() {
        let (svc, store, _, _) = make_service();
        let owner_member = WorkspaceMember::new(
            DeviceId::from_uuid(Uuid::new_v4()),
            PublicKey::new([1u8; 32]),
            "Other".to_string(),
        );
        let snapshot = winx_domain::workspace::WorkspaceSnapshot {
            id: WorkspaceId::new(),
            name: "Mirror".to_string(),
            owner_device_id: owner_member.device_id,
            version: winx_domain::workspace::WorkspaceVersion::initial(),
            ownership_mode: winx_domain::workspace::OwnershipMode::Original,
            members: vec![owner_member],
            layout: winx_domain::workspace::WorkspaceLayout::empty(),
        };
        let mirror = Workspace::create_mirror(snapshot, "Other");
        let mirror_id = mirror.id;
        store.save(&mirror).await.unwrap();

        svc.forget_workspace(mirror_id).await.unwrap();
        assert!(store.load_all().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn forget_workspace_on_original_returns_error() {
        let (svc, _, _, _) = make_service();
        let ws = svc
            .create_workspace("Mine".to_string(), vec![])
            .await
            .unwrap();
        let err = svc.forget_workspace(ws.id).await.unwrap_err();
        assert_eq!(err.code, DomainErrorCode::WorkspaceMirrorImmutable);
    }

    #[tokio::test]
    async fn publish_throttles_broadcast() {
        let remote_uuid = Uuid::new_v4();
        let remote_device = DeviceId::from_uuid(remote_uuid);
        let remote_addr: SocketAddr = "127.0.0.1:9001".parse().unwrap();
        let (svc, store, transport, _) =
            make_service_with_peer_addr(Some(remote_device), Some(remote_addr));

        let mut ws = svc
            .create_workspace("WS".to_string(), vec![])
            .await
            .unwrap();
        ws.add_member(WorkspaceMember::new(
            remote_device,
            PublicKey::new([7u8; 32]),
            "Remote".to_string(),
        ))
        .unwrap();
        store.save(&ws).await.unwrap();
        svc.connect_to_workspace(ws.id).await.unwrap();

        svc.publish_global_cursor(100, 200).await;
        svc.publish_global_cursor(101, 201).await;

        for _ in 0..20 {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            if !transport.sent.lock().unwrap().is_empty() {
                break;
            }
        }

        let sent = transport.sent.lock().unwrap();
        assert_eq!(sent.len(), 1, "second publish should be throttled");
        assert!(matches!(sent[0].1, WorkspaceInviteMessage::GlobalCursor(_)));
    }

    #[tokio::test]
    async fn handle_global_cursor_applies_higher_seq() {
        let (svc, store, _, _) = make_service();
        let mut ws = svc
            .create_workspace("Cursor".to_string(), vec![])
            .await
            .unwrap();
        ws.global_cursor.monotonic_seq = 10;
        let remote_id = DeviceId::from_uuid(Uuid::new_v4());
        ws.add_member(WorkspaceMember::new(
            remote_id,
            PublicKey::new([3u8; 32]),
            "Remote".to_string(),
        ))
        .unwrap();
        store.save(&ws).await.unwrap();

        let payload = winx_protocol::workspace::GlobalCursorPayload {
            workspace_id: ws.id.as_uuid(),
            x: 640,
            y: 480,
            active_device_id: remote_id.as_uuid(),
            monotonic_seq: 11,
            sender_device_id: remote_id.as_uuid(),
            sender_pubkey: [3u8; 32],
        };

        svc.handle_global_cursor(&payload).await;

        let pending = svc.cursor_pending.lock().await;
        let applied = pending
            .get(&ws.id)
            .expect("cursor should be staged for persist");
        assert_eq!(applied.global_cursor.x, 640);
        assert_eq!(applied.global_cursor.y, 480);
        assert_eq!(applied.global_cursor.monotonic_seq, 11);
    }

    #[tokio::test]
    async fn handle_global_cursor_rejects_stale_seq() {
        let (svc, store, _, _) = make_service();
        let mut ws = svc
            .create_workspace("Cursor".to_string(), vec![])
            .await
            .unwrap();
        ws.global_cursor.monotonic_seq = 10;
        ws.global_cursor.x = 50;
        ws.global_cursor.y = 60;
        let remote_id = DeviceId::from_uuid(Uuid::new_v4());
        ws.add_member(WorkspaceMember::new(
            remote_id,
            PublicKey::new([4u8; 32]),
            "Remote".to_string(),
        ))
        .unwrap();
        store.save(&ws).await.unwrap();

        let payload = winx_protocol::workspace::GlobalCursorPayload {
            workspace_id: ws.id.as_uuid(),
            x: 999,
            y: 888,
            active_device_id: remote_id.as_uuid(),
            monotonic_seq: 10,
            sender_device_id: remote_id.as_uuid(),
            sender_pubkey: [4u8; 32],
        };

        svc.handle_global_cursor(&payload).await;

        let pending = svc.cursor_pending.lock().await;
        assert!(
            pending.get(&ws.id).is_none(),
            "stale cursor update must not be staged"
        );
    }

    #[tokio::test]
    async fn cursor_position_restored_after_remote_apply() {
        let (svc, store, _, _) = make_service();
        let mut ws = svc
            .create_workspace("CursorRestore".to_string(), vec![])
            .await
            .unwrap();
        ws.global_cursor.monotonic_seq = 5;
        ws.global_cursor.x = 10;
        ws.global_cursor.y = 20;
        let remote_id = DeviceId::from_uuid(Uuid::new_v4());
        ws.add_member(WorkspaceMember::new(
            remote_id,
            PublicKey::new([5u8; 32]),
            "Remote".to_string(),
        ))
        .unwrap();
        store.save(&ws).await.unwrap();
        svc.connect_to_workspace(ws.id).await.unwrap();

        let payload = winx_protocol::workspace::GlobalCursorPayload {
            workspace_id: ws.id.as_uuid(),
            x: 640,
            y: 480,
            active_device_id: remote_id.as_uuid(),
            monotonic_seq: 6,
            sender_device_id: remote_id.as_uuid(),
            sender_pubkey: [5u8; 32],
        };

        svc.handle_global_cursor(&payload).await;

        let restored = svc.restore_cursor_on_focus().await;
        assert_eq!(restored, Some((640, 480)));
    }

    #[tokio::test]
    async fn split_brain_lww_resolves_to_higher_version() {
        let (svc, store, _, _) = make_service();
        let owner_device_id = DeviceId::from_uuid(Uuid::new_v4());
        let owner_member = WorkspaceMember::new(
            owner_device_id,
            PublicKey::new([1u8; 32]),
            "Owner".to_string(),
        );
        let snapshot = winx_domain::workspace::WorkspaceSnapshot {
            id: WorkspaceId::new(),
            name: "SplitBrain".to_string(),
            owner_device_id,
            version: winx_domain::workspace::WorkspaceVersion::from_u64(5),
            ownership_mode: winx_domain::workspace::OwnershipMode::Original,
            members: vec![owner_member.clone()],
            layout: winx_domain::workspace::WorkspaceLayout::empty(),
        };
        let mirror = Workspace::create_mirror(snapshot, "Owner");
        let workspace_id = mirror.id;
        store.save(&mirror).await.unwrap();

        let make_sync = |name: &str, version: u64| WorkspaceSyncPayload {
            workspace_id: workspace_id.as_uuid(),
            snapshot: WorkspaceSnapshotPayload {
                id: workspace_id.as_uuid(),
                name: name.to_string(),
                owner_device_id: owner_device_id.as_uuid(),
                owner_username: "Owner".to_string(),
                version,
                members: vec![MemberSnapshotPayload {
                    device_id: owner_device_id.as_uuid(),
                    public_key: [1u8; 32],
                    username: "Owner".to_string(),
                    joined_at_rfc3339: "2026-01-01T00:00:00Z".to_string(),
                }],
            },
            sender_device_id: owner_device_id.as_uuid(),
            sender_pubkey: [1u8; 32],
        };

        svc.handle_workspace_sync(&make_sync("Winner", 8)).await;
        svc.handle_workspace_sync(&make_sync("Stale", 6)).await;

        let loaded = store.find_by_id(workspace_id).await.unwrap().unwrap();
        assert_eq!(loaded.name, "Winner");
        assert_eq!(loaded.version.as_u64(), 8);
    }

    #[tokio::test]
    async fn handle_workspace_sync_applies_when_incoming_version_higher() {
        let (svc, store, _, _) = make_service();
        let owner_device_id = DeviceId::from_uuid(Uuid::new_v4());
        let owner_member = WorkspaceMember::new(
            owner_device_id,
            PublicKey::new([1u8; 32]),
            "Owner".to_string(),
        );
        let initial_snapshot = winx_domain::workspace::WorkspaceSnapshot {
            id: WorkspaceId::new(),
            name: "Original".to_string(),
            owner_device_id,
            version: winx_domain::workspace::WorkspaceVersion::initial(),
            ownership_mode: winx_domain::workspace::OwnershipMode::Original,
            members: vec![owner_member.clone()],
            layout: winx_domain::workspace::WorkspaceLayout::empty(),
        };
        let mirror = Workspace::create_mirror(initial_snapshot.clone(), "Owner");
        let workspace_id = mirror.id;
        store.save(&mirror).await.unwrap();

        let sync_payload = winx_protocol::workspace::WorkspaceSyncPayload {
            workspace_id: workspace_id.as_uuid(),
            snapshot: winx_protocol::workspace::WorkspaceSnapshotPayload {
                id: workspace_id.as_uuid(),
                name: "Renamed".to_string(),
                owner_device_id: owner_device_id.as_uuid(),
                owner_username: "Owner".to_string(),
                version: 5,
                members: vec![MemberSnapshotPayload {
                    device_id: owner_device_id.as_uuid(),
                    public_key: [1u8; 32],
                    username: "Owner".to_string(),
                    joined_at_rfc3339: "2026-01-01T00:00:00Z".to_string(),
                }],
            },
            sender_device_id: owner_device_id.as_uuid(),
            sender_pubkey: [1u8; 32],
        };

        svc.handle_workspace_sync(&sync_payload).await;

        let loaded = store.find_by_id(workspace_id).await.unwrap().unwrap();
        assert_eq!(loaded.name, "Renamed");
        assert_eq!(loaded.version.as_u64(), 5);
    }

    #[tokio::test]
    async fn handle_workspace_sync_discards_when_incoming_version_lower() {
        let (svc, store, _, _) = make_service();
        let owner_device_id = DeviceId::from_uuid(Uuid::new_v4());
        let owner_member = WorkspaceMember::new(
            owner_device_id,
            PublicKey::new([1u8; 32]),
            "Owner".to_string(),
        );
        let snapshot = winx_domain::workspace::WorkspaceSnapshot {
            id: WorkspaceId::new(),
            name: "Local".to_string(),
            owner_device_id,
            version: winx_domain::workspace::WorkspaceVersion::from_u64(10),
            ownership_mode: winx_domain::workspace::OwnershipMode::Original,
            members: vec![owner_member],
            layout: winx_domain::workspace::WorkspaceLayout::empty(),
        };
        let mirror = Workspace::create_mirror(snapshot, "Owner");
        let workspace_id = mirror.id;
        store.save(&mirror).await.unwrap();

        let sync_payload = winx_protocol::workspace::WorkspaceSyncPayload {
            workspace_id: workspace_id.as_uuid(),
            snapshot: winx_protocol::workspace::WorkspaceSnapshotPayload {
                id: workspace_id.as_uuid(),
                name: "Stale".to_string(),
                owner_device_id: owner_device_id.as_uuid(),
                owner_username: "Owner".to_string(),
                version: 3,
                members: vec![],
            },
            sender_device_id: owner_device_id.as_uuid(),
            sender_pubkey: [1u8; 32],
        };

        svc.handle_workspace_sync(&sync_payload).await;
        let loaded = store.find_by_id(workspace_id).await.unwrap().unwrap();
        assert_eq!(loaded.name, "Local");
        assert_eq!(loaded.version.as_u64(), 10);
    }

    #[tokio::test]
    async fn handle_workspace_delete_marks_mirror_orphan() {
        let (svc, store, _, _) = make_service();
        let owner_device_id = DeviceId::from_uuid(Uuid::new_v4());
        let owner_member = WorkspaceMember::new(
            owner_device_id,
            PublicKey::new([1u8; 32]),
            "Owner".to_string(),
        );
        let snapshot = winx_domain::workspace::WorkspaceSnapshot {
            id: WorkspaceId::new(),
            name: "Mirror".to_string(),
            owner_device_id,
            version: winx_domain::workspace::WorkspaceVersion::initial(),
            ownership_mode: winx_domain::workspace::OwnershipMode::Original,
            members: vec![owner_member],
            layout: winx_domain::workspace::WorkspaceLayout::empty(),
        };
        let mirror = Workspace::create_mirror(snapshot, "Owner");
        let workspace_id = mirror.id;
        store.save(&mirror).await.unwrap();

        let delete_payload = winx_protocol::workspace::WorkspaceDeletePayload {
            workspace_id: workspace_id.as_uuid(),
            sender_device_id: owner_device_id.as_uuid(),
            sender_pubkey: [1u8; 32],
        };

        svc.handle_workspace_delete(&delete_payload).await;

        let loaded = store.find_by_id(workspace_id).await.unwrap().unwrap();
        match loaded.ownership_mode {
            winx_domain::workspace::OwnershipMode::Mirror { is_orphan, .. } => {
                assert!(is_orphan);
            }
            _ => panic!("expected mirror"),
        }
    }
}
