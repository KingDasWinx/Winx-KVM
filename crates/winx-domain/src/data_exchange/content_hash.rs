use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Hash SHA-256 do conteúdo de clipboard (deduplicação e anti-loop).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    #[must_use]
    pub fn of_text(text: &str) -> Self {
        let digest = Sha256::digest(text.as_bytes());
        Self(digest.into())
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_text_same_hash() {
        let a = ContentHash::of_text("hello");
        let b = ContentHash::of_text("hello");
        assert_eq!(a, b);
    }

    #[test]
    fn different_text_different_hash() {
        assert_ne!(ContentHash::of_text("a"), ContentHash::of_text("b"));
    }
}
