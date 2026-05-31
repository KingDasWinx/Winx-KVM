use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;

/// Versão monotônica de um workspace para resolução LWW (Last-Write-Wins).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkspaceVersion(u64);

impl WorkspaceVersion {
    /// Retorna a versão inicial (1, não 0 — 0 é reservado para "não definido").
    #[must_use]
    pub const fn initial() -> Self {
        Self(1)
    }

    /// Retorna a próxima versão (incrementa em 1).
    #[must_use]
    pub const fn next(&self) -> Self {
        Self(self.0 + 1)
    }

    /// Retorna o valor inteiro.
    #[must_use]
    pub const fn as_u64(&self) -> u64 {
        self.0
    }

    /// Constrói a partir de um valor inteiro.
    #[must_use]
    pub const fn from_u64(v: u64) -> Self {
        Self(v)
    }

    /// Verifica se esta versão é mais nova que outra.
    #[must_use]
    pub const fn is_newer_than(&self, other: WorkspaceVersion) -> bool {
        self.0 > other.0
    }
}

impl Default for WorkspaceVersion {
    fn default() -> Self {
        Self::initial()
    }
}

impl PartialOrd for WorkspaceVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for WorkspaceVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl fmt::Display for WorkspaceVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_is_one() {
        assert_eq!(WorkspaceVersion::initial().as_u64(), 1);
    }

    #[test]
    fn next_increments() {
        let v1 = WorkspaceVersion::initial();
        let v2 = v1.next();
        assert_eq!(v2.as_u64(), 2);
    }

    #[test]
    fn is_newer_than() {
        let v1 = WorkspaceVersion::initial();
        let v2 = v1.next();
        assert!(v2.is_newer_than(v1));
        assert!(!v1.is_newer_than(v2));
        assert!(!v1.is_newer_than(v1));
    }

    #[test]
    fn ordering() {
        let v1 = WorkspaceVersion::initial();
        let v2 = v1.next();
        assert!(v1 < v2);
        assert!(v2 > v1);
        assert_eq!(v1, WorkspaceVersion::initial());
    }

    #[test]
    fn serde() {
        let v = WorkspaceVersion::initial().next().next();
        let json = serde_json::to_string(&v).unwrap();
        let deserialized: WorkspaceVersion = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, v);
    }
}
