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

fn min_corner(monitors: &[MonitorRect]) -> (i32, i32) {
    let Some(first) = monitors.first() else {
        return (0, 0);
    };
    (
        monitors.iter().map(|m| m.x).min().unwrap_or(first.x),
        monitors.iter().map(|m| m.y).min().unwrap_or(first.y),
    )
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
    ///
    /// Monitores locais usam coordenadas reais do OS; o bloco remoto é transladado para
    /// encostar no local conforme a adjacência definida no layout canônico.
    #[must_use]
    pub fn derive_runtime_layout(
        &self,
        local_device: DeviceId,
        remote_peer: PeerId,
        local_os: &[MonitorRect],
    ) -> MonitorLayout {
        let remote_device = DeviceId::from_uuid(remote_peer.as_uuid());
        let canon_local = self.device_monitors(local_device);
        let canon_remote = self.device_monitors(remote_device);

        if local_os.is_empty() {
            return MonitorLayout::default_side_by_side(Vec::new(), remote_peer);
        }

        if canon_remote.is_empty() {
            return MonitorLayout::default_side_by_side(local_os.to_vec(), remote_peer);
        }

        let (dx, dy) = if canon_local.is_empty() {
            (0, 0)
        } else {
            let (clx, cly) = min_corner(&canon_local);
            let (olx, oly) = min_corner(local_os);
            (olx - clx, oly - cly)
        };

        let remote_abs: Vec<MonitorRect> = canon_remote
            .iter()
            .map(|m| MonitorRect {
                id: m.id,
                x: m.x + dx,
                y: m.y + dy,
                width: m.width,
                height: m.height,
            })
            .collect();

        let min_x = remote_abs.iter().map(|m| m.x).min().unwrap_or(0);
        let min_y = remote_abs.iter().map(|m| m.y).min().unwrap_or(0);
        let max_r = remote_abs
            .iter()
            .map(MonitorRect::right_edge)
            .max()
            .unwrap_or(min_x);
        let max_b = remote_abs
            .iter()
            .map(MonitorRect::bottom_edge)
            .max()
            .unwrap_or(min_y);

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
            local_monitors: local_os.to_vec(),
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
                place_monitors_adjacent(announced, anchor_mons)
            } else {
                announced.to_vec()
            }
        } else {
            announced.to_vec()
        };

        self.set_device_monitors(device_id, placed);
    }
}

/// Posiciona monitores anunciados encostados no anchor pela borda com melhor encaixe.
fn place_monitors_adjacent(announced: &[MonitorRect], anchor: &[MonitorRect]) -> Vec<MonitorRect> {
    let Some(first) = announced.first() else {
        return Vec::new();
    };
    let ann_min_x = announced.iter().map(|m| m.x).min().unwrap_or(first.x);
    let ann_min_y = announced.iter().map(|m| m.y).min().unwrap_or(first.y);
    let ann_w = announced
        .iter()
        .map(MonitorRect::right_edge)
        .max()
        .unwrap_or(first.right_edge())
        - ann_min_x;
    let ann_h = announced
        .iter()
        .map(MonitorRect::bottom_edge)
        .max()
        .unwrap_or(first.bottom_edge())
        - ann_min_y;

    let anchor_min_x = anchor.iter().map(|m| m.x).min().unwrap_or(0);
    let anchor_min_y = anchor.iter().map(|m| m.y).min().unwrap_or(0);
    let anchor_max_r = anchor.iter().map(MonitorRect::right_edge).max().unwrap_or(0);
    let anchor_max_b = anchor.iter().map(MonitorRect::bottom_edge).max().unwrap_or(0);
    let anchor_w = anchor_max_r - anchor_min_x;
    let anchor_h = anchor_max_b - anchor_min_y;

    // Candidatos: direita, esquerda, abaixo, acima do anchor.
    let candidates = [
        (anchor_max_r - ann_min_x, anchor_min_y - ann_min_y), // right
        (anchor_min_x - ann_w - ann_min_x, anchor_min_y - ann_min_y), // left
        (anchor_min_x - ann_min_x, anchor_max_b - ann_min_y), // bottom
        (anchor_min_x - ann_min_x, anchor_min_y - ann_h - ann_min_y), // top
    ];

    // Prefere direita; empate pelo menor desalinhamento vertical/horizontal.
    let first = candidates[0];
    let (offset_x, offset_y) = candidates.into_iter().min_by_key(|(ox, oy)| {
        if *ox == first.0 && *oy == first.1 {
            0
        } else {
            ox.abs() + oy.abs()
        }
    }).unwrap_or(first);

    let _ = (anchor_w, anchor_h);

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
        let os_local = vec![rect(1, 0, 0, 1920, 1080)];

        let mut session = SessionDesktopLayout::empty();
        session.set_device_monitors(local_dev, vec![rect(1, 0, 0, 1920, 1080)]);
        session.set_device_monitors(
            remote_dev,
            vec![rect(1, 0, 1080, 1920, 1080)],
        );

        let runtime = session.derive_runtime_layout(local_dev, remote_peer, &os_local);
        assert_eq!(runtime.edge.local_exit, super::super::layout::BorderSide::Bottom);
        assert_eq!(runtime.remote_virtual.y, 1080);
        assert_eq!(runtime.local_monitors[0].x, 0);
    }

    #[test]
    fn same_session_yields_consistent_edges_on_both_sides() {
        let dev_a = DeviceId::from_uuid(Uuid::new_v4());
        let dev_b = DeviceId::from_uuid(Uuid::new_v4());
        let os_a = vec![rect(1, 0, 0, 2560, 1080)];
        let os_b = vec![rect(1, 0, 0, 1920, 1080)];

        let mut session = SessionDesktopLayout::empty();
        session.set_device_monitors(dev_a, vec![rect(1, 0, 0, 2560, 1080)]);
        session.set_device_monitors(dev_b, vec![rect(1, 2560, 0, 1920, 1080)]);

        let rt_a = session.derive_runtime_layout(dev_a, PeerId::from_uuid(dev_b.as_uuid()), &os_a);
        let rt_b = session.derive_runtime_layout(dev_b, PeerId::from_uuid(dev_a.as_uuid()), &os_b);

        assert_eq!(rt_a.edge.local_exit, super::super::layout::BorderSide::Right);
        assert_eq!(rt_b.edge.local_exit, super::super::layout::BorderSide::Left);
        assert_eq!(rt_a.local_monitors[0].x, 0);
        assert_eq!(rt_b.local_monitors[0].x, 0);
    }

    #[test]
    fn remote_side_places_peer_to_the_left_in_os_space() {
        let dev_a = DeviceId::from_uuid(Uuid::new_v4());
        let dev_b = DeviceId::from_uuid(Uuid::new_v4());
        let os_b = vec![rect(1, 0, 0, 1920, 1080)];

        let mut session = SessionDesktopLayout::empty();
        session.set_device_monitors(dev_a, vec![rect(1, 0, 0, 1920, 1080)]);
        session.set_device_monitors(dev_b, vec![rect(1, 1920, 0, 1920, 1080)]);

        let rt_b = session.derive_runtime_layout(dev_b, PeerId::from_uuid(dev_a.as_uuid()), &os_b);
        assert_eq!(rt_b.edge.local_exit, super::super::layout::BorderSide::Left);
        assert!(rt_b.remote_virtual.x < 0);
    }
}
