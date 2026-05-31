use anyhow::Result;
use winx_domain::{
    input_control::MonitorLayout,
    shared::ids::PeerId,
};

/// Persistência de layout KVM por peer (conexão single).
#[async_trait::async_trait]
pub trait KvmLayoutStore: Send + Sync {
    async fn get(&self, peer_id: PeerId) -> Result<Option<MonitorLayout>>;
    async fn save(&self, peer_id: PeerId, layout: &MonitorLayout) -> Result<()>;
    async fn delete(&self, peer_id: PeerId) -> Result<()>;
    /// Monitores locais reportados pelo peer (cache de sync).
    async fn get_peer_monitors(&self, peer_id: PeerId) -> Result<Option<Vec<winx_domain::input_control::MonitorRect>>>;
    async fn save_peer_monitors(
        &self,
        peer_id: PeerId,
        monitors: &[winx_domain::input_control::MonitorRect],
    ) -> Result<()>;
}
