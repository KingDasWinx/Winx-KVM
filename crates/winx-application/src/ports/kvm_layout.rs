use anyhow::Result;
use winx_domain::{
    input_control::{MonitorLayout, SessionDesktopLayout},
    input_control::MonitorRect,
    shared::ids::PeerId,
};

/// Persistência de layout KVM por peer (conexão single).
#[async_trait::async_trait]
pub trait KvmLayoutStore: Send + Sync {
    async fn get(&self, peer_id: PeerId) -> Result<Option<MonitorLayout>>;
    async fn save(&self, peer_id: PeerId, layout: &MonitorLayout) -> Result<()>;
    async fn delete(&self, peer_id: PeerId) -> Result<()>;
    /// Monitores locais reportados pelo peer (cache de sync).
    async fn get_peer_monitors(&self, peer_id: PeerId) -> Result<Option<Vec<MonitorRect>>>;
    async fn save_peer_monitors(
        &self,
        peer_id: PeerId,
        monitors: &[MonitorRect],
    ) -> Result<()>;

    /// Layout canônico compartilhado da sessão (mesmo em todos os PCs).
    async fn get_session(&self, peer_id: PeerId) -> Result<Option<SessionDesktopLayout>>;
    async fn save_session(&self, peer_id: PeerId, layout: &SessionDesktopLayout) -> Result<()>;
}
