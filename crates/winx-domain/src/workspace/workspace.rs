use crate::input_control::layout::MonitorLayout;
use crate::shared::ids::DeviceId;
use crate::workspace::{
    GlobalCursorState, GlobalCursorUpdate, OwnershipMode, WorkspaceId, WorkspaceLayout,
    WorkspaceMember, WorkspaceVersion,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Snapshot de um workspace (usado na sincronização e storage).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceSnapshot {
    pub id: WorkspaceId,
    pub name: String,
    pub owner_device_id: DeviceId,
    pub version: WorkspaceVersion,
    pub ownership_mode: OwnershipMode,
    pub members: Vec<WorkspaceMember>,
    pub layout: WorkspaceLayout,
}

/// Resultado de uma sincronização.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncOutcome {
    Applied,
    Discarded {
        local_version: u64,
        incoming_version: u64,
    },
}

/// Aggregate root: Workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    pub owner_device_id: DeviceId,
    pub version: WorkspaceVersion,
    pub ownership_mode: OwnershipMode,
    pub members: Vec<WorkspaceMember>,
    pub layout: WorkspaceLayout,
    pub global_cursor: GlobalCursorState,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

impl Workspace {
    /// Cria um novo workspace original.
    pub fn create_original(
        name: impl Into<String>,
        owner: WorkspaceMember,
    ) -> Result<Self, String> {
        let name = name.into();
        if name.is_empty() {
            return Err("workspace.name_empty".to_string());
        }

        let members = vec![owner.clone()];

        Ok(Self {
            id: WorkspaceId::new(),
            name,
            owner_device_id: owner.device_id,
            version: WorkspaceVersion::initial(),
            ownership_mode: OwnershipMode::Original,
            members,
            layout: WorkspaceLayout::empty(),
            global_cursor: GlobalCursorState::new(),
            created_at: OffsetDateTime::now_utc(),
        })
    }

    /// Cria um workspace mirror a partir de um snapshot.
    pub fn create_mirror(snapshot: WorkspaceSnapshot, owner_username: impl Into<String>) -> Self {
        Self {
            id: snapshot.id,
            name: snapshot.name,
            owner_device_id: snapshot.owner_device_id,
            version: snapshot.version,
            ownership_mode: OwnershipMode::Mirror {
                owner_device_id: snapshot.owner_device_id,
                owner_username_snapshot: owner_username.into(),
                owner_last_seen: OffsetDateTime::now_utc(),
                is_orphan: false,
            },
            members: snapshot.members,
            layout: snapshot.layout,
            global_cursor: GlobalCursorState::new(),
            created_at: OffsetDateTime::now_utc(),
        }
    }

    /// Renomeia o workspace. Só funciona se é original.
    pub fn rename(&mut self, new_name: impl Into<String>) -> Result<(), String> {
        if self.ownership_mode.is_mirror() {
            return Err("workspace.mirror_immutable".to_string());
        }
        let new_name = new_name.into();
        if new_name.is_empty() {
            return Err("workspace.name_empty".to_string());
        }
        self.name = new_name;
        self.version = self.version.next();
        Ok(())
    }

    /// Adiciona um membro. Só funciona se é original.
    pub fn add_member(&mut self, member: WorkspaceMember) -> Result<(), String> {
        if self.ownership_mode.is_mirror() {
            return Err("workspace.mirror_immutable".to_string());
        }
        if self.members.iter().any(|m| m.device_id == member.device_id) {
            return Err("workspace.member_already_exists".to_string());
        }
        self.members.push(member);
        self.version = self.version.next();
        Ok(())
    }

    /// Remove um membro. Só funciona se é original.
    pub fn remove_member(&mut self, device_id: DeviceId) -> Result<(), String> {
        if self.ownership_mode.is_mirror() {
            return Err("workspace.mirror_immutable".to_string());
        }
        if device_id == self.owner_device_id {
            return Err("workspace.cannot_remove_owner".to_string());
        }
        if self.members.len() <= 1 {
            return Err("workspace.must_have_one_member".to_string());
        }
        self.members.retain(|m| m.device_id != device_id);
        self.version = self.version.next();
        Ok(())
    }

    /// Atualiza o layout. Só funciona se é original.
    pub fn update_layout(
        &mut self,
        device_id: DeviceId,
        layout: MonitorLayout,
    ) -> Result<(), String> {
        if self.ownership_mode.is_mirror() {
            return Err("workspace.mirror_immutable".to_string());
        }
        self.layout.set(device_id, layout);
        self.version = self.version.next();
        Ok(())
    }

    /// Atualiza o layout do próprio device em um mirror (sync bidirecional de layout).
    pub fn update_own_layout_as_mirror(
        &mut self,
        device_id: DeviceId,
        layout: MonitorLayout,
    ) -> Result<(), String> {
        if !self.ownership_mode.is_mirror() {
            return Err("workspace.not_mirror".to_string());
        }
        if !self.members.iter().any(|m| m.device_id == device_id) {
            return Err("workspace.member_not_found".to_string());
        }
        self.layout.set(device_id, layout);
        self.version = self.version.next();
        Ok(())
    }

    /// Aplica um update de cursor remoto.
    pub fn apply_cursor(&mut self, update: GlobalCursorUpdate) -> Result<(), String> {
        self.global_cursor.apply_remote(update)
    }

    /// Marca este workspace como órfão. Só funciona para mirrors.
    pub fn mark_orphan(&mut self) -> Result<(), String> {
        self.ownership_mode.mark_orphan()
    }

    /// Aplica um sync remoto (implementação stub em W1).
    pub fn apply_sync(&mut self, snapshot: WorkspaceSnapshot) -> SyncOutcome {
        if snapshot.version.is_newer_than(self.version) {
            self.name = snapshot.name;
            self.members = snapshot.members;
            for (device_id, monitor_layout) in snapshot.layout.per_device {
                self.layout.set(device_id, monitor_layout);
            }
            self.version = snapshot.version;
            SyncOutcome::Applied
        } else {
            SyncOutcome::Discarded {
                local_version: self.version.as_u64(),
                incoming_version: snapshot.version.as_u64(),
            }
        }
    }

    /// Retorna um snapshot do workspace.
    pub fn to_snapshot(&self) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            id: self.id,
            name: self.name.clone(),
            owner_device_id: self.owner_device_id,
            version: self.version,
            ownership_mode: self.ownership_mode.clone(),
            members: self.members.clone(),
            layout: self.layout.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::key::PublicKey;

    fn make_member(name: &str) -> WorkspaceMember {
        WorkspaceMember::new(DeviceId::new(), PublicKey::new([0u8; 32]), name)
    }

    #[test]
    fn create_original_succeeds() {
        let owner = make_member("Owner");
        let ws = Workspace::create_original("Test", owner.clone()).unwrap();
        assert_eq!(ws.name, "Test");
        assert!(ws.ownership_mode.is_original());
        assert_eq!(ws.members.len(), 1);
        assert_eq!(ws.version, WorkspaceVersion::initial());
    }

    #[test]
    fn create_original_rejects_empty_name() {
        let owner = make_member("Owner");
        assert!(Workspace::create_original("", owner).is_err());
    }

    #[test]
    fn rename_increments_version() {
        let owner = make_member("Owner");
        let mut ws = Workspace::create_original("Test", owner).unwrap();
        let v1 = ws.version;
        ws.rename("NewName").unwrap();
        assert!(ws.version.is_newer_than(v1));
    }

    #[test]
    fn rename_fails_on_mirror() {
        let owner = make_member("Owner");
        let snapshot = WorkspaceSnapshot {
            id: WorkspaceId::new(),
            name: "Test".to_string(),
            owner_device_id: owner.device_id,
            version: WorkspaceVersion::initial(),
            ownership_mode: OwnershipMode::Original,
            members: vec![owner.clone()],
            layout: WorkspaceLayout::empty(),
        };
        let mut ws = Workspace::create_mirror(snapshot, "Owner");
        assert!(ws.rename("NewName").is_err());
    }

    #[test]
    fn add_member_succeeds() {
        let owner = make_member("Owner");
        let mut ws = Workspace::create_original("Test", owner).unwrap();
        let member2 = make_member("Member2");
        assert!(ws.add_member(member2.clone()).is_ok());
        assert_eq!(ws.members.len(), 2);
    }

    #[test]
    fn add_member_rejects_duplicate() {
        let owner = make_member("Owner");
        let mut ws = Workspace::create_original("Test", owner.clone()).unwrap();
        assert!(ws.add_member(owner).is_err());
    }

    #[test]
    fn remove_member_fails_for_owner() {
        let owner = make_member("Owner");
        let mut ws = Workspace::create_original("Test", owner.clone()).unwrap();
        assert!(ws.remove_member(owner.device_id).is_err());
    }

    #[test]
    fn apply_sync_accepts_newer_version() {
        let owner = make_member("Owner");
        let mut ws = Workspace::create_original("Test", owner.clone()).unwrap();
        let mut snapshot = ws.to_snapshot();
        snapshot.version = ws.version.next().next();
        snapshot.name = "Updated".to_string();
        let outcome = ws.apply_sync(snapshot.clone());
        assert_eq!(outcome, SyncOutcome::Applied);
        assert_eq!(ws.name, "Updated");
        assert_eq!(ws.version, snapshot.version);
    }

    #[test]
    fn apply_sync_discards_older_version() {
        let owner = make_member("Owner");
        let mut ws = Workspace::create_original("Test", owner.clone()).unwrap();
        let old_snapshot = WorkspaceSnapshot {
            id: ws.id,
            name: "Old".to_string(),
            owner_device_id: ws.owner_device_id,
            version: WorkspaceVersion::initial(),
            ownership_mode: ws.ownership_mode.clone(),
            members: ws.members.clone(),
            layout: ws.layout.clone(),
        };
        let outcome = ws.apply_sync(old_snapshot);
        assert!(matches!(outcome, SyncOutcome::Discarded { .. }));
        assert_eq!(ws.name, "Test");
    }
}
