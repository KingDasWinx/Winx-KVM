use async_trait::async_trait;
use winx_domain::{
    identity::{Device, TrustedPeer},
    shared::ids::PeerId,
};

/// Port de persistência da identidade e lista de peers.
///
/// Implementação concreta em `winx-infra::identity_store_toml`.
/// Grava em `%APPDATA%\Winx-KVM\identity.toml` e `peers.toml`.
#[async_trait]
pub trait IdentityStore: Send + Sync + 'static {
    /// Carrega o device local. Retorna `None` na primeira execução.
    async fn load_device(&self) -> anyhow::Result<Option<Device>>;

    /// Persiste o device local.
    async fn save_device(&self, device: &Device) -> anyhow::Result<()>;

    /// Carrega todos os peers confiáveis.
    async fn load_peers(&self) -> anyhow::Result<Vec<TrustedPeer>>;

    /// Persiste (ou atualiza) um peer confiável.
    async fn save_peer(&self, peer: &TrustedPeer) -> anyhow::Result<()>;

    /// Remove um peer da lista de confiança ("Esquecer dispositivo").
    async fn remove_peer(&self, peer_id: PeerId) -> anyhow::Result<()>;
}

/// Port de armazenamento de segredos criptográficos.
///
/// Implementação concreta em `winx-infra::secret_store_keyring`,
/// usando o Windows Credential Manager via DPAPI.
/// **Nunca** grave a signing key em TOML ou qualquer arquivo de texto.
#[async_trait]
pub trait SecretStore: Send + Sync + 'static {
    /// Grava os 32 bytes da signing key Ed25519 privada no vault do SO.
    async fn store_signing_key(&self, key_bytes: &[u8; 32]) -> anyhow::Result<()>;

    /// Carrega a signing key. Retorna `None` se ainda não foi gerada.
    async fn load_signing_key(&self) -> anyhow::Result<Option<[u8; 32]>>;

    /// Remove a signing key do vault (usado ao redefinir identidade).
    async fn delete_signing_key(&self) -> anyhow::Result<()>;
}
