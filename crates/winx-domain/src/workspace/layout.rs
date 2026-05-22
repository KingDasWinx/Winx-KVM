use crate::input_control::layout::MonitorLayout;
use crate::shared::ids::DeviceId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Layout de monitores por device dentro de um workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceLayout {
    /// Mapa de device → seu layout local de monitores.
    /// BTreeMap é usado para serialização TOML determinística.
    pub per_device: BTreeMap<DeviceId, MonitorLayout>,
}

impl WorkspaceLayout {
    /// Cria um layout vazio.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            per_device: BTreeMap::new(),
        }
    }

    /// Define ou atualiza o layout de um device.
    pub fn set(&mut self, device_id: DeviceId, layout: MonitorLayout) {
        self.per_device.insert(device_id, layout);
    }

    /// Obtém o layout de um device.
    #[must_use]
    pub fn get(&self, device_id: DeviceId) -> Option<&MonitorLayout> {
        self.per_device.get(&device_id)
    }

    /// Remove o layout de um device.
    pub fn remove(&mut self, device_id: DeviceId) -> Option<MonitorLayout> {
        self.per_device.remove(&device_id)
    }

    /// Retorna true se não há layouts definidos.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.per_device.is_empty()
    }

    /// Retorna o número de devices com layout definido.
    #[must_use]
    pub fn len(&self) -> usize {
        self.per_device.len()
    }
}

impl Default for WorkspaceLayout {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::ids::PeerId;

    #[test]
    fn empty_layout() {
        let layout = WorkspaceLayout::empty();
        assert!(layout.is_empty());
        assert_eq!(layout.len(), 0);
    }

    #[test]
    fn set_and_get() {
        let mut layout = WorkspaceLayout::empty();
        let device_id = DeviceId::new();
        let peer_id = PeerId::from_uuid(uuid::Uuid::new_v4());
        let monitor_layout = MonitorLayout::default_side_by_side(vec![], peer_id);

        layout.set(device_id, monitor_layout.clone());
        assert_eq!(layout.len(), 1);
        assert_eq!(layout.get(device_id), Some(&monitor_layout));
    }

    #[test]
    fn remove() {
        let mut layout = WorkspaceLayout::empty();
        let device_id = DeviceId::new();
        let peer_id = PeerId::from_uuid(uuid::Uuid::new_v4());
        let monitor_layout = MonitorLayout::default_side_by_side(vec![], peer_id);

        layout.set(device_id, monitor_layout.clone());
        let removed = layout.remove(device_id);
        assert_eq!(removed, Some(monitor_layout));
        assert!(layout.is_empty());
    }

    #[test]
    fn serde_roundtrip() {
        let mut layout = WorkspaceLayout::empty();
        let device_id = DeviceId::new();
        let peer_id = PeerId::from_uuid(uuid::Uuid::new_v4());
        let monitor_layout = MonitorLayout::default_side_by_side(vec![], peer_id);
        layout.set(device_id, monitor_layout);

        let json = serde_json::to_string(&layout).unwrap();
        let deserialized: WorkspaceLayout = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, layout);
    }

    #[test]
    fn btreemap_preserves_order() {
        let mut layout = WorkspaceLayout::empty();
        let id1 = DeviceId::from_uuid(uuid::Uuid::nil());
        let id2 = DeviceId::from_uuid(uuid::Uuid::max());
        let peer_id = PeerId::from_uuid(uuid::Uuid::new_v4());
        let monitor_layout = MonitorLayout::default_side_by_side(vec![], peer_id);

        layout.set(id2, monitor_layout.clone());
        layout.set(id1, monitor_layout.clone());

        let keys: Vec<_> = layout.per_device.keys().copied().collect();
        // BTreeMap mantém ordem — id1 deve vir antes de id2
        assert_eq!(keys[0], id1);
        assert_eq!(keys[1], id2);
    }
}
