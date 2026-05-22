use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use winx_application::ports::WorkspaceStore;
use winx_domain::workspace::Workspace;
use winx_domain::workspace::WorkspaceId;

/// Adapter TOML para persistência de workspaces.
pub struct TomlWorkspaceStore {
    config_dir: PathBuf,
}

impl TomlWorkspaceStore {
    /// Cria uma nova instância.
    pub fn new(config_dir: impl Into<PathBuf>) -> Self {
        Self {
            config_dir: config_dir.into(),
        }
    }

    fn workspaces_path(&self) -> PathBuf {
        self.config_dir.join("workspaces.toml")
    }

    async fn read_all(&self) -> Result<WorkspacesFile> {
        let path = self.workspaces_path();
        if !path.exists() {
            return Ok(WorkspacesFile::default());
        }
        let raw = tokio::fs::read_to_string(&path).await?;
        let file = toml::from_str(&raw)?;
        Ok(file)
    }

    async fn write_all(&self, file: &WorkspacesFile) -> Result<()> {
        tokio::fs::create_dir_all(&self.config_dir).await?;
        let raw = toml::to_string_pretty(file)?;
        tokio::fs::write(self.workspaces_path(), raw).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl WorkspaceStore for TomlWorkspaceStore {
    async fn load_all(&self) -> Result<Vec<Workspace>> {
        let file = self.read_all().await?;
        if file.schema_version != 1 {
            return Err(anyhow!(
                "workspaces.toml schema version {} não é suportado",
                file.schema_version
            ));
        }
        Ok(file.workspaces)
    }

    async fn save(&self, workspace: &Workspace) -> Result<()> {
        let mut file = self.read_all().await?;
        if let Some(existing) = file.workspaces.iter_mut().find(|w| w.id == workspace.id) {
            *existing = workspace.clone();
        } else {
            file.workspaces.push(workspace.clone());
        }
        self.write_all(&file).await
    }

    async fn delete(&self, id: WorkspaceId) -> Result<()> {
        let mut file = self.read_all().await?;
        file.workspaces.retain(|w| w.id != id);
        self.write_all(&file).await
    }

    async fn find_by_id(&self, id: WorkspaceId) -> Result<Option<Workspace>> {
        let file = self.read_all().await?;
        Ok(file.workspaces.into_iter().find(|w| w.id == id))
    }
}

/// Envelope TOML para serialização.
#[derive(Serialize, Deserialize)]
struct WorkspacesFile {
    #[serde(default = "default_schema_version")]
    schema_version: u32,
    #[serde(default, rename = "workspace")]
    workspaces: Vec<Workspace>,
}

fn default_schema_version() -> u32 {
    1
}

impl Default for WorkspacesFile {
    fn default() -> Self {
        Self {
            schema_version: 1,
            workspaces: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use winx_domain::identity::key::PublicKey;
    use winx_domain::shared::ids::DeviceId;
    use winx_domain::workspace::{Workspace, WorkspaceMember, WorkspaceSnapshot};

    fn fake_member(name: &str) -> WorkspaceMember {
        WorkspaceMember::new(
            DeviceId::from_uuid(uuid::Uuid::new_v4()),
            PublicKey::new([7u8; 32]),
            name.to_string(),
        )
    }

    fn make_original(name: &str) -> Workspace {
        Workspace::create_original(name, fake_member("Owner")).unwrap()
    }

    fn make_mirror(name: &str) -> Workspace {
        let owner_member = fake_member("Owner");
        let snapshot = WorkspaceSnapshot {
            id: winx_domain::workspace::WorkspaceId::new(),
            name: name.to_string(),
            owner_device_id: owner_member.device_id,
            version: winx_domain::workspace::WorkspaceVersion::initial(),
            ownership_mode: winx_domain::workspace::OwnershipMode::Original,
            members: vec![owner_member],
            layout: winx_domain::workspace::WorkspaceLayout::empty(),
        };
        Workspace::create_mirror(snapshot, "Owner")
    }

    #[test]
    fn schema_version_defaults_to_one() {
        let file = WorkspacesFile::default();
        assert_eq!(file.schema_version, 1);
        assert!(file.workspaces.is_empty());
    }

    #[tokio::test]
    async fn save_then_load_original_roundtrip() {
        let dir = TempDir::new().unwrap();
        let store = TomlWorkspaceStore::new(dir.path());
        let ws = make_original("Sala");

        store.save(&ws).await.unwrap();
        let loaded = store.load_all().await.unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0], ws);
    }

    #[tokio::test]
    async fn save_then_load_mirror_roundtrip() {
        let dir = TempDir::new().unwrap();
        let store = TomlWorkspaceStore::new(dir.path());
        let ws = make_mirror("Quarto");

        store.save(&ws).await.unwrap();
        let loaded = store.find_by_id(ws.id).await.unwrap().unwrap();

        assert!(loaded.ownership_mode.is_mirror());
        if let winx_domain::workspace::OwnershipMode::Mirror { is_orphan, .. } =
            &loaded.ownership_mode
        {
            assert!(!*is_orphan);
        } else {
            panic!("expected mirror");
        }
        assert_eq!(loaded, ws);
    }

    #[tokio::test]
    async fn multiple_workspaces_persist_independently() {
        let dir = TempDir::new().unwrap();
        let store = TomlWorkspaceStore::new(dir.path());
        let w1 = make_original("A");
        let w2 = make_original("B");
        let w3 = make_mirror("C");

        store.save(&w1).await.unwrap();
        store.save(&w2).await.unwrap();
        store.save(&w3).await.unwrap();

        let loaded = store.load_all().await.unwrap();
        assert_eq!(loaded.len(), 3);
        let names: Vec<&str> = loaded.iter().map(|w| w.name.as_str()).collect();
        assert!(names.contains(&"A"));
        assert!(names.contains(&"B"));
        assert!(names.contains(&"C"));
    }

    #[tokio::test]
    async fn delete_removes_only_target() {
        let dir = TempDir::new().unwrap();
        let store = TomlWorkspaceStore::new(dir.path());
        let w1 = make_original("Keep");
        let w2 = make_original("Delete");

        store.save(&w1).await.unwrap();
        store.save(&w2).await.unwrap();
        store.delete(w2.id).await.unwrap();

        let loaded = store.load_all().await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, w1.id);
    }

    #[tokio::test]
    async fn save_overwrites_existing_workspace() {
        let dir = TempDir::new().unwrap();
        let store = TomlWorkspaceStore::new(dir.path());
        let mut ws = make_original("Original Name");

        store.save(&ws).await.unwrap();
        ws.rename("Updated Name").unwrap();
        store.save(&ws).await.unwrap();

        let loaded = store.load_all().await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "Updated Name");
    }

    #[tokio::test]
    async fn schema_version_mismatch_returns_error() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("workspaces.toml");
        tokio::fs::write(&path, "schema_version = 99\n")
            .await
            .unwrap();

        let store = TomlWorkspaceStore::new(dir.path());
        let result = store.load_all().await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("schema version"));
    }
}
