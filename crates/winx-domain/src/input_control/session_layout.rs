//! Layout canônico da sessão single-KVM: todos os monitores de todos os PCs
//! compartilham o mesmo espaço de coordenadas absolutas.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::shared::ids::{DeviceId, PeerId};

use super::layout::MonitorLayout;
use super::monitor::{MonitorId, MonitorRect};

/// Desktop virtual compartilhado entre os peers de uma sessão KVM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SessionDesktopLayout {
    /// Monitores por device, em coordenadas absolutas do desktop virtual.
    pub per_device: BTreeMap<DeviceId, Vec<MonitorRect>>,
}

impl SessionDesktopLayout {
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn set_device_monitors(&mut self, device_id: DeviceId, monitors: Vec<MonitorRect>) {
        if monitors.is_empty() {
            self.per_device.remove(&device_id);
        } else {
            self.per_device.insert(device_id, monitors);
        }
    }

    #[must_use]
    pub fn device_monitors(&self, device_id: DeviceId) -> Vec<MonitorRect> {
        self.per_device.get(&device_id).cloned().unwrap_or_default()
    }

    /// Converte o layout canônico para `MonitorLayout` de runtime (cruzamento de borda).
    #[must_use]
    pub fn derive_runtime_layout(
        &self,
        local_device: DeviceId,
        remote_peer: PeerId,
    ) -> MonitorLayout {
        let remote_device = DeviceId::from_uuid(remote_peer.as_uuid());
        let local_monitors = self.device_monitors(local_device);
        let remote_abs = self.device_monitors(remote_device);

        if local_monitors.is_empty() {
            return MonitorLayout::default_side_by_side(Vec::new(), remote_peer);
        }

        if remote_abs.is_empty() {
            return MonitorLayout::default_side_by_side(local_monitors, remote_peer);
        }

        let min_x = remote_abs.iter().map(|m| m.x).min().unwrap_or(0);
        let min_y = remote_abs.iter().map(|m| m.y).min().unwrap_or(0);
        let max_r = remote_abs.iter().map(MonitorRect::right_edge).max().unwrap_or(min_x);
        let max_b = remote_abs.iter().map(MonitorRect::bottom_edge).max().unwrap_or(min_y);

        let remote_relative: Vec<MonitorRect> = remote_abs
            .iter()
            .map(|m| MonitorRect {
                id: m.id,
                x: m.x - min_x,
                y: m.y - min_y,
                width: m.width,
                height: m.height,
            })
            .collect();

        let mut layout = MonitorLayout {
            local_monitors,
            remote_peer,
            remote_virtual: MonitorRect {
                id: MonitorId(0xFFFF),
                x: min_x,
                y: min_y,
                width: (max_r - min_x).max(1) as u32,
                height: (max_b - min_y).max(1) as u32,
            },
            remote_monitors: remote_relative,
            edge: Default::default(),
        };
        layout.infer_edges_from_geometry();
        layout
    }

    /// Mescla monitores anunciados pelo peer (coords OS dele) no desktop canônico,
    /// preservando posições já salvas quando existirem.
    pub fn merge_announced_monitors(
        &mut self,
        device_id: DeviceId,
        announced: &[MonitorRect],
        anchor_next_to: Option<DeviceId>,
    ) {
        if announced.is_empty() {
            return;
        }
        if let Some(existing) = self.per_device.get(&device_id) {
            if !existing.is_empty() {
                return;
            }
        }

        let placed = if let Some(anchor_id) = anchor_next_to {
            if anchor_id == device_id {
                announced.to_vec()
            } else if let Some(anchor_mons) = self.per_device.get(&anchor_id) {
                place_monitors_beside(announced, anchor_mons)
            } else {
                announced.to_vec()
            }
        } else {
            announced.to_vec()
        };

        self.set_device_monitors(device_id, placed);
    }
}

fn place_monitors_beside(announced: &[MonitorRect], anchor: &[MonitorRect]) -> Vec<MonitorRect> {
    let Some(first) = announced.first() else {
        return Vec::new();
    };
    let ann_min_x = announced.iter().map(|m| m.x).min().unwrap_or(first.x);
    let ann_min_y = announced.iter().map(|m| m.y).min().unwrap_or(first.y);
    let ann_max_r = announced.iter().map(MonitorRect::right_edge).max().unwrap_or(first.right_edge());
    let ann_max_b = announced
        .iter()
        .map(MonitorRect::bottom_edge)
        .max()
        .unwrap_or(first.bottom_edge());

    let anchor_max_r = anchor.iter().map(MonitorRect::right_edge).max().unwrap_or(0);
    let anchor_min_y = anchor.iter().map(|m| m.y).min().unwrap_or(0);

    let offset_x = anchor_max_r - ann_min_x;
    let offset_y = anchor_min_y - ann_min_y;

    announced
        .iter()
        .map(|m| MonitorRect {
            id: m.id,
            x: m.x + offset_x,
            y: m.y + offset_y,
            width: m.width,
            height: m.height,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn rect(id: u32, x: i32, y: i32, w: u32, h: u32) -> MonitorRect {
        MonitorRect {
            id: MonitorId(id),
            x,
            y,
            width: w,
            height: h,
        }
    }

    #[test]
    fn derive_runtime_places_remote_below_local() {
        let local_dev = DeviceId::from_uuid(Uuid::new_v4());
        let remote_dev = DeviceId::from_uuid(Uuid::new_v4());
        let remote_peer = PeerId::from_uuid(remote_dev.as_uuid());

        let mut session = SessionDesktopLayout::empty();
        session.set_device_monitors(
            local_dev,
            vec![rect(1, 0, 0, 1920, 1080)],
        );
        session.set_device_monitors(
            remote_dev,
            vec![rect(1, 0, 1080, 1920, 1080)],
        );

        let runtime = session.derive_runtime_layout(local_dev, remote_peer);
        assert_eq!(runtime.edge.local_exit, super::super::layout::BorderSide::Bottom);
        assert_eq!(runtime.remote_virtual.y, 1080);
    }

    #[test]
    fn same_session_yields_consistent_edges_on_both_sides() {
        let dev_a = DeviceId::from_uuid(Uuid::new_v4());
        let dev_b = DeviceId::from_uuid(Uuid::new_v4());

        let mut session = SessionDesktopLayout::empty();
        session.set_device_monitors(dev_a, vec![rect(1, 0, 0, 2560, 1080)]);
        session.set_device_monitors(dev_b, vec![rect(1, 0, 1080, 1920, 1080)]);

        let rt_a = session.derive_runtime_layout(dev_a, PeerId::from_uuid(dev_b.as_uuid()));
        let rt_b = session.derive_runtime_layout(dev_b, PeerId::from_uuid(dev_a.as_uuid()));

        assert_eq!(rt_a.edge.local_exit, super::super::layout::BorderSide::Bottom);
        assert_eq!(rt_b.edge.local_exit, super::super::layout::BorderSide::Top);
    }
}
