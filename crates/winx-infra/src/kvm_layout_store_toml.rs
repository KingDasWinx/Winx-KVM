use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use winx_application::ports::KvmLayoutStore;
use winx_domain::{
    input_control::MonitorLayout,
    input_control::{MonitorRect, SessionDesktopLayout},
    shared::ids::PeerId,
};

/// Adapter TOML para layouts KVM por peer (`%APPDATA%\Winx-KVM\kvm_layouts.toml`).
pub struct TomlKvmLayoutStore {
    config_dir: PathBuf,
}

impl TomlKvmLayoutStore {
    pub fn new(config_dir: impl Into<PathBuf>) -> Self {
        Self {
            config_dir: config_dir.into(),
        }
    }

    fn layouts_path(&self) -> PathBuf {
        self.config_dir.join("kvm_layouts.toml")
    }

    async fn read_all(&self) -> Result<KvmLayoutsFile> {
        let path = self.layouts_path();
        if !path.exists() {
            return Ok(KvmLayoutsFile::default());
        }
        let raw = tokio::fs::read_to_string(&path).await?;
        let file = toml::from_str(&raw)?;
        Ok(file)
    }

    async fn write_all(&self, file: &KvmLayoutsFile) -> Result<()> {
        tokio::fs::create_dir_all(&self.config_dir).await?;
        let raw = toml::to_string_pretty(file)?;
        tokio::fs::write(self.layouts_path(), raw).await?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct KvmLayoutsFile {
    schema_version: u32,
    #[serde(default)]
    layouts: BTreeMap<String, MonitorLayout>,
    #[serde(default)]
    session_layouts: BTreeMap<String, SessionDesktopLayout>,
    /// Monitores locais de cada peer (reportados via sync).
    #[serde(default)]
    peer_monitors: BTreeMap<String, Vec<MonitorRect>>,
}

impl Default for KvmLayoutsFile {
    fn default() -> Self {
        Self {
            schema_version: 2,
            layouts: BTreeMap::new(),
            session_layouts: BTreeMap::new(),
            peer_monitors: BTreeMap::new(),
        }
    }
}

#[async_trait::async_trait]
impl KvmLayoutStore for TomlKvmLayoutStore {
    async fn get(&self, peer_id: PeerId) -> Result<Option<MonitorLayout>> {
        let file = self.read_all().await?;
        if file.schema_version != 1 && file.schema_version != 2 {
            return Err(anyhow!(
                "kvm_layouts.toml schema version {} não é suportado",
                file.schema_version
            ));
        }
        Ok(file.layouts.get(&peer_id.to_string()).cloned())
    }

    async fn save(&self, peer_id: PeerId, layout: &MonitorLayout) -> Result<()> {
        let mut file = self.read_all().await?;
        file.layouts.insert(peer_id.to_string(), layout.clone());
        self.write_all(&file).await
    }

    async fn delete(&self, peer_id: PeerId) -> Result<()> {
        let mut file = self.read_all().await?;
        file.layouts.remove(&peer_id.to_string());
        file.peer_monitors.remove(&peer_id.to_string());
        self.write_all(&file).await
    }

    async fn get_peer_monitors(&self, peer_id: PeerId) -> Result<Option<Vec<MonitorRect>>> {
        let file = self.read_all().await?;
        Ok(file.peer_monitors.get(&peer_id.to_string()).cloned())
    }

    async fn save_peer_monitors(
        &self,
        peer_id: PeerId,
        monitors: &[MonitorRect],
    ) -> Result<()> {
        let mut file = self.read_all().await?;
        file.peer_monitors
            .insert(peer_id.to_string(), monitors.to_vec());
        self.write_all(&file).await
    }

    async fn get_session(&self, peer_id: PeerId) -> Result<Option<SessionDesktopLayout>> {
        let file = self.read_all().await?;
        Ok(file.session_layouts.get(&peer_id.to_string()).cloned())
    }

    async fn save_session(&self, peer_id: PeerId, layout: &SessionDesktopLayout) -> Result<()> {
        let mut file = self.read_all().await?;
        file.schema_version = 2;
        file.session_layouts
            .insert(peer_id.to_string(), layout.clone());
        self.write_all(&file).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use uuid::Uuid;
    use winx_domain::input_control::{MonitorId, MonitorRect};

    #[tokio::test]
    async fn save_and_load_roundtrip() {
        let dir = tempdir().unwrap();
        let store = TomlKvmLayoutStore::new(dir.path());
        let peer = PeerId::from_uuid(Uuid::new_v4());
        let layout = MonitorLayout::default_side_by_side(
            vec![MonitorRect {
                id: MonitorId(1),
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            }],
            peer,
        );

        store.save(peer, &layout).await.unwrap();
        let loaded = store.get(peer).await.unwrap().expect("layout salvo");
        assert_eq!(loaded.remote_virtual.x, layout.remote_virtual.x);
    }
}
