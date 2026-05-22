use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Identificador único de um workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkspaceId(Uuid);

impl WorkspaceId {
    /// Gera um novo WorkspaceId com UUID v4.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Cria um WorkspaceId a partir de um UUID existente.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Retorna o UUID subjacente.
    #[must_use]
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for WorkspaceId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for WorkspaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_generates_different_ids() {
        let id1 = WorkspaceId::new();
        let id2 = WorkspaceId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn from_uuid_round_trip() {
        let uuid = Uuid::new_v4();
        let id = WorkspaceId::from_uuid(uuid);
        assert_eq!(id.as_uuid(), uuid);
    }

    #[test]
    fn serde_transparent() {
        let id = WorkspaceId::new();
        let json = serde_json::to_string(&id).unwrap();
        // Deve serializar como JSON string (com aspas)
        assert!(json.starts_with('"') && json.ends_with('"'));
        let deserialized: WorkspaceId = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, id);
    }

    #[test]
    fn display() {
        let id = WorkspaceId::new();
        let s = id.to_string();
        assert!(!s.is_empty());
    }
}
