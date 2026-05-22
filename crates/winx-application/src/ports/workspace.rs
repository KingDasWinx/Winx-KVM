use async_trait::async_trait;
use winx_domain::workspace::{Workspace, WorkspaceId};

/// Port para persistência de workspaces.
#[async_trait]
pub trait WorkspaceStore: Send + Sync + 'static {
    /// Carrega todos os workspaces.
    async fn load_all(&self) -> anyhow::Result<Vec<Workspace>>;

    /// Salva um workspace (upsert).
    async fn save(&self, workspace: &Workspace) -> anyhow::Result<()>;

    /// Deleta um workspace.
    async fn delete(&self, id: WorkspaceId) -> anyhow::Result<()>;

    /// Procura um workspace por ID.
    async fn find_by_id(&self, id: WorkspaceId) -> anyhow::Result<Option<Workspace>>;
}
