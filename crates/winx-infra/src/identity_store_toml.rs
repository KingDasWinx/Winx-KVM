use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;
use winx_application::ports::IdentityStore;
use winx_domain::{
    identity::{Device, PublicKey, TrustedPeer},
    shared::ids::{DeviceId, PeerId},
};

/// Adapter TOML para `IdentityStore`.
///
/// Grava em `<config_dir>/identity.toml` e `<config_dir>/peers.toml`.
/// Formato humano legível — nunca contém segredos.
pub struct TomlIdentityStore {
    config_dir: PathBuf,
}

impl TomlIdentityStore {
    pub fn new(config_dir: PathBuf) -> Self {
        Self { config_dir }
    }

    fn identity_path(&self) -> PathBuf {
        self.config_dir.join("identity.toml")
    }

    fn peers_path(&self) -> PathBuf {
        self.config_dir.join("peers.toml")
    }
}

// ── Structs de serialização TOML ──────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct IdentityFile {
    id: Uuid,
    username: String,
    /// Hex encoding dos 32 bytes da public key Ed25519.
    public_key: String,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
}

#[derive(Serialize, Deserialize, Default)]
struct PeersFile {
    #[serde(default)]
    peers: Vec<PeerEntry>,
}

#[derive(Serialize, Deserialize)]
struct PeerEntry {
    id: Uuid,
    username: String,
    public_key: String,
    #[serde(with = "time::serde::rfc3339")]
    paired_at: OffsetDateTime,
    #[serde(default, with = "time::serde::rfc3339::option")]
    last_seen: Option<OffsetDateTime>,
}

// ── Conversões ────────────────────────────────────────────────────────────────

impl TryFrom<IdentityFile> for Device {
    type Error = anyhow::Error;

    fn try_from(f: IdentityFile) -> Result<Self, Self::Error> {
        let key_bytes = hex::decode(&f.public_key)?;
        let public_key =
            PublicKey::try_from(key_bytes.as_slice()).map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(Self {
            id: DeviceId::from_uuid(f.id),
            username: f.username,
            public_key,
            created_at: f.created_at,
        })
    }
}

impl From<&Device> for IdentityFile {
    fn from(d: &Device) -> Self {
        Self {
            id: d.id.as_uuid(),
            username: d.username.clone(),
            public_key: hex::encode(d.public_key.as_bytes()),
            created_at: d.created_at,
        }
    }
}

impl TryFrom<PeerEntry> for TrustedPeer {
    type Error = anyhow::Error;

    fn try_from(e: PeerEntry) -> Result<Self, Self::Error> {
        let key_bytes = hex::decode(&e.public_key)?;
        let public_key =
            PublicKey::try_from(key_bytes.as_slice()).map_err(|msg| anyhow::anyhow!("{msg}"))?;
        Ok(Self {
            id: PeerId::from_uuid(e.id),
            username: e.username,
            public_key,
            paired_at: e.paired_at,
            last_seen: e.last_seen,
        })
    }
}

impl From<&TrustedPeer> for PeerEntry {
    fn from(p: &TrustedPeer) -> Self {
        Self {
            id: p.id.as_uuid(),
            username: p.username.clone(),
            public_key: hex::encode(p.public_key.as_bytes()),
            paired_at: p.paired_at,
            last_seen: p.last_seen,
        }
    }
}

// ── Implementação do port ─────────────────────────────────────────────────────

#[async_trait]
impl IdentityStore for TomlIdentityStore {
    async fn load_device(&self) -> anyhow::Result<Option<Device>> {
        let path = self.identity_path();
        if !path.exists() {
            return Ok(None);
        }
        let raw = tokio::fs::read_to_string(&path).await?;
        let file: IdentityFile = toml::from_str(&raw)?;
        Ok(Some(Device::try_from(file)?))
    }

    async fn save_device(&self, device: &Device) -> anyhow::Result<()> {
        tokio::fs::create_dir_all(&self.config_dir).await?;
        let file = IdentityFile::from(device);
        let raw = toml::to_string_pretty(&file)?;
        tokio::fs::write(self.identity_path(), raw).await?;
        Ok(())
    }

    async fn load_peers(&self) -> anyhow::Result<Vec<TrustedPeer>> {
        let path = self.peers_path();
        if !path.exists() {
            return Ok(vec![]);
        }
        let raw = tokio::fs::read_to_string(&path).await?;
        let file: PeersFile = toml::from_str(&raw)?;
        file.peers.into_iter().map(TrustedPeer::try_from).collect()
    }

    async fn save_peer(&self, peer: &TrustedPeer) -> anyhow::Result<()> {
        let mut peers = self.load_peers().await?;
        let id = peer.id;
        if let Some(existing) = peers.iter_mut().find(|p| p.id == id) {
            *existing = peer.clone();
        } else {
            peers.push(peer.clone());
        }
        self.write_peers(&peers).await
    }

    async fn remove_peer(&self, peer_id: PeerId) -> anyhow::Result<()> {
        let peers = self.load_peers().await?;
        let peers: Vec<_> = peers.into_iter().filter(|p| p.id != peer_id).collect();
        self.write_peers(&peers).await
    }
}

impl TomlIdentityStore {
    async fn write_peers(&self, peers: &[TrustedPeer]) -> anyhow::Result<()> {
        tokio::fs::create_dir_all(&self.config_dir).await?;
        let file = PeersFile {
            peers: peers.iter().map(PeerEntry::from).collect(),
        };
        let raw = toml::to_string_pretty(&file)?;
        tokio::fs::write(self.peers_path(), raw).await?;
        Ok(())
    }
}
