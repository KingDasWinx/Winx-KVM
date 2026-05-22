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
use crate::ports::{DiscoveryQuery, IdentityStore, WorkspaceInviteTransport, WorkspaceStore};
use tracing::{debug, info, warn};

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
        discovery_query: Arc<dyn DiscoveryQuery>,
        local_device_id: Uuid,
        local_username: String,
        bus: EventBus,
    ) -> Self {
        Self {
            store,
            transport,
            identity_store,
            discovery_query,
            pending_invites: Arc::new(Mutex::new(HashMap::new())),
            active_workspace: Arc::new(RwLock::new(None)),
            local_device_id,
            local_username,
            bus,
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
        // Create owner member
        let owner_device_id = DeviceId::from_uuid(self.local_device_id);
        let owner_pubkey = PublicKey::new([0u8; 32]); // TODO: get from identity_store
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
            .map(|m| {
                MemberSnapshotPayload {
                    device_id: m.device_id.as_uuid(),
                    public_key: *m.public_key.as_bytes(),
                    username: m.username_cache.clone(),
                    // TODO(W3): format OffsetDateTime to RFC3339 (time crate not available in application layer)
                    // For MVP, use timestamp-based approximation
                    joined_at_rfc3339: format!("{:?}", m.joined_at),
                }
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
        let local_device = self.identity_store.load_device().await
            .map_err(|e| DomainError::new(DomainErrorCode::InternalError, format!("failed to load device: {}", e)))?
            .ok_or_else(|| DomainError::new(DomainErrorCode::InternalError, "local device not found"))?;
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

        // Resolve target address and send invite (async, don't block on network failure)
        let svc = Arc::new(self.clone());
        let msg = WorkspaceInviteMessage::Invite(payload);
        tokio::spawn(async move {
            let target_device_id = DeviceId::from_uuid(target_device_uuid);
            match svc.discovery_query.resolve_address(target_device_id).await {
                Ok(Some(addr)) => match svc.transport.send_to(addr, msg).await {
                    Ok(()) => info!(%invite_id, %addr, "invite sent"),
                    Err(e) => warn!(%invite_id, ?e, "failed to send invite"),
                },
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
        use winx_domain::identity::TrustedPeer;
        use winx_domain::shared::ids::PeerId;

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

        info!(%invite_id, %workspace_id, "invite rejected");

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
    #[tokio::test]
    async fn test_placeholder() {
        // TODO: add comprehensive tests for WorkspaceService
        // - create_workspace with various edge cases
        // - invite_to_workspace validation
        // - accept_invite TOFU + mirror creation flow
        // - reject_invite with event publishing
        // - connect/disconnect/force_disconnect_and_connect
        // - conflict detection on simultaneous connection attempts
        // - expiration loop with timeout events
    }
}
