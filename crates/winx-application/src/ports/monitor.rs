use async_trait::async_trait;
use winx_domain::input_control::MonitorRect;

#[async_trait]
pub trait MonitorBackend: Send + Sync + 'static {
    async fn enumerate_local_monitors(&self) -> anyhow::Result<Vec<MonitorRect>>;
}
