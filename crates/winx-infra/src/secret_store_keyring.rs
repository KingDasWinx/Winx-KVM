use async_trait::async_trait;
use winx_application::ports::SecretStore;

const SERVICE: &str = "Winx-KVM";
const ACCOUNT: &str = "signing_key";

/// Adapter do Credential Manager (Windows DPAPI) para `SecretStore`.
///
/// A signing key Ed25519 (32 bytes) é hex-encoded e gravada como senha
/// no Windows Credential Manager sob o serviço `"Winx-KVM"`.
/// O DPAPI garante que apenas o usuário atual neste PC pode lê-la.
pub struct KeyringSecretStore;

impl KeyringSecretStore {
    pub fn new() -> Self {
        Self
    }
}

impl Default for KeyringSecretStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SecretStore for KeyringSecretStore {
    async fn store_signing_key(&self, key_bytes: &[u8; 32]) -> anyhow::Result<()> {
        let hex = hex::encode(key_bytes);
        let entry = keyring::Entry::new(SERVICE, ACCOUNT)
            .map_err(|e| anyhow::anyhow!("keyring entry: {e}"))?;
        entry
            .set_password(&hex)
            .map_err(|e| anyhow::anyhow!("keyring set: {e}"))?;
        Ok(())
    }

    async fn load_signing_key(&self) -> anyhow::Result<Option<[u8; 32]>> {
        let entry = keyring::Entry::new(SERVICE, ACCOUNT)
            .map_err(|e| anyhow::anyhow!("keyring entry: {e}"))?;

        match entry.get_password() {
            Ok(hex) => {
                let bytes = hex::decode(&hex)
                    .map_err(|e| anyhow::anyhow!("hex decode signing key: {e}"))?;
                let arr: [u8; 32] = bytes
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("signing key corrompida: tamanho incorreto"))?;
                Ok(Some(arr))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(anyhow::anyhow!("keyring get: {e}")),
        }
    }

    async fn delete_signing_key(&self) -> anyhow::Result<()> {
        let entry = keyring::Entry::new(SERVICE, ACCOUNT)
            .map_err(|e| anyhow::anyhow!("keyring entry: {e}"))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(anyhow::anyhow!("keyring delete: {e}")),
        }
    }
}
