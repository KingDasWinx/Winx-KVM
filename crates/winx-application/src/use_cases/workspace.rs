use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use uuid::Uuid;
use std::collections::HashMap;

use winx_domain::shared::{DomainError, DomainErrorCode};
use winx_domain::workspace::{InviteSession, Workspace, WorkspaceId};

use crate::bus::EventBus;
use crate::ports::{IdentityStore, WorkspaceInviteTransport, WorkspaceStore};

/// Service for workspace invites and membership management.
pub struct WorkspaceService {
    store: Arc<dyn WorkspaceStore>,
    transport: Arc<dyn WorkspaceInviteTransport>,
    identity_store: Arc<dyn IdentityStore>,
    pending_invites: Arc<Mutex<HashMap<Uuid, InviteSession>>>,
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
        local_device_id: Uuid,
        local_username: String,
        bus: EventBus,
    ) -> Self {
        Self {
            store,
            transport,
            identity_store,
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
        _initial_member_device_ids: Vec<Uuid>,
    ) -> Result<Workspace, DomainError> {
        // TODO: implement workspace creation and invites
        Err(DomainError::new(
            DomainErrorCode::InternalError,
            "not yet implemented",
        ))
    }

    pub async fn invite_to_workspace(
        &self,
        _workspace_id: WorkspaceId,
        _target_device_id: Uuid,
    ) -> Result<Uuid, DomainError> {
        // TODO: implement sending invite
        Err(DomainError::new(
            DomainErrorCode::InternalError,
            "not yet implemented",
        ))
    }

    pub async fn accept_invite(&self, _invite_id: Uuid) -> Result<Workspace, DomainError> {
        // TODO: implement accept invite (TOFU + save mirror)
        Err(DomainError::new(
            DomainErrorCode::InternalError,
            "not yet implemented",
        ))
    }

    pub async fn reject_invite(&self, _invite_id: Uuid) -> Result<(), DomainError> {
        // TODO: implement reject invite
        Err(DomainError::new(
            DomainErrorCode::InternalError,
            "not yet implemented",
        ))
    }

    pub async fn connect_to_workspace(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<(), DomainError> {
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
        // TODO: publish WorkspaceConnected event
        Ok(())
    }

    pub async fn disconnect_from_workspace(&self) -> Result<(), DomainError> {
        let mut active = self.active_workspace.write().await;
        *active = None;
        // TODO: publish WorkspaceDisconnected event
        Ok(())
    }

    pub async fn force_disconnect_and_connect(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<(), DomainError> {
        let mut active = self.active_workspace.write().await;
        *active = Some(workspace_id);
        // TODO: publish WorkspaceDisconnected + WorkspaceConnected events
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
        while let Some(_decoded) = rx.recv().await {
            // TODO: handle incoming invite
        }
        Ok(())
    }

    pub async fn run_expiration_loop(&self) {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            // TODO: check for expired invites and mark as Expired
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_placeholder() {
        // TODO: add comprehensive tests
    }
}
