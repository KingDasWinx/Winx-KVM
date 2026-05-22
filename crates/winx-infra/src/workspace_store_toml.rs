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
        file.workspaces
            .iter()
            .map(WorkspaceEntry::to_workspace)
            .collect()
    }

    async fn save(&self, workspace: &Workspace) -> Result<()> {
        let mut file = self.read_all().await?;
        let entry = WorkspaceEntry::from_workspace(workspace);
        if let Some(existing) = file.workspaces.iter_mut().find(|w| w.id == entry.id) {
            *existing = entry;
        } else {
            file.workspaces.push(entry);
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
        file.workspaces
            .iter()
            .find(|w| w.id == id)
            .map(WorkspaceEntry::to_workspace)
            .transpose()
    }
}

/// Envelope TOML para serialização.
#[derive(Serialize, Deserialize)]
struct WorkspacesFile {
    #[serde(default = "default_schema_version")]
    schema_version: u32,
    #[serde(default)]
    workspaces: Vec<WorkspaceEntry>,
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

/// Entry individual no arquivo TOML (espelha Workspace).
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
struct WorkspaceEntry {
    id: WorkspaceId,
    #[serde(flatten)]
    workspace: Workspace,
}

impl WorkspaceEntry {
    fn from_workspace(workspace: &Workspace) -> Self {
        Self {
            id: workspace.id,
            workspace: workspace.clone(),
        }
    }

    fn to_workspace(&self) -> Result<Workspace> {
        Ok(self.workspace.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_version_defaults_to_one() {
        let file = WorkspacesFile::default();
        assert_eq!(file.schema_version, 1);
        assert!(file.workspaces.is_empty());
    }
}
