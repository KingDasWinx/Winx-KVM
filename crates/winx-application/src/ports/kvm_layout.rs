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
}
