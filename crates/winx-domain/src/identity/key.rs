use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Chave pública Ed25519 de um device (32 bytes).
///
/// Usada tanto para verificar assinaturas quanto para identificar o peer
/// de forma criptograficamente segura no handshake QUIC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PublicKey([u8; 32]);

impl PublicKey {
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Fingerprint para exibição: primeiros 8 bytes do SHA-256 em hex separado por `:`.
    ///
    /// Exemplo: `"A3:4F:B2:1C:D9:E5:07:AB"`. Comprimento fixo de 23 chars.
    pub fn fingerprint(&self) -> Fingerprint {
        let hash = Sha256::digest(self.0);
        let pairs: Vec<String> = hash[..8].iter().map(|b| format!("{b:02X}")).collect();
        Fingerprint(pairs.join(":"))
    }
}

impl TryFrom<&[u8]> for PublicKey {
    type Error = &'static str;

    fn try_from(slice: &[u8]) -> Result<Self, Self::Error> {
        let arr: [u8; 32] = slice
            .try_into()
            .map_err(|_| "public key deve ter 32 bytes")?;
        Ok(Self(arr))
    }
}

/// Fingerprint de exibição derivado da chave pública.
///
/// Imutável após criação. Formato: `"XX:XX:XX:XX:XX:XX:XX:XX"` (8 bytes em hex).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Fingerprint(String);

impl Fingerprint {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_has_correct_format() {
        let key = PublicKey::new([0xABu8; 32]);
        let fp = key.fingerprint();
        let s = fp.as_str();
        // 8 grupos de 2 hex chars separados por ":"
        assert_eq!(s.len(), 23, "fingerprint deve ter 23 chars: {s}");
        assert_eq!(s.chars().filter(|&c| c == ':').count(), 7);
    }

    #[test]
    fn same_key_same_fingerprint() {
        let key = PublicKey::new([0x42u8; 32]);
        assert_eq!(key.fingerprint(), key.fingerprint());
    }

    #[test]
    fn different_keys_different_fingerprints() {
        let a = PublicKey::new([0x01u8; 32]);
        let b = PublicKey::new([0x02u8; 32]);
        assert_ne!(a.fingerprint(), b.fingerprint());
    }
}
