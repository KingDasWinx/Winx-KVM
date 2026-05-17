use serde::{Deserialize, Serialize};

use crate::shared::ids::PeerId;

use super::monitor::{MonitorId, MonitorRect};

/// Layout virtual lado-a-lado: monitores locais + um monitor representando o peer remoto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorLayout {
    pub local_monitors: Vec<MonitorRect>,
    pub remote_peer: PeerId,
    pub remote_virtual: MonitorRect,
}

impl MonitorLayout {
    /// Coloca o peer remoto à direita do monitor local mais à direita (v0.1).
    #[must_use]
    pub fn default_side_by_side(local: Vec<MonitorRect>, remote_peer: PeerId) -> Self {
        let right_edge = local.iter().map(MonitorRect::right_edge).max().unwrap_or(0);
        let height = local.iter().map(|m| m.height).max().unwrap_or(1080);
        let width = local.iter().map(|m| m.width).max().unwrap_or(1920);

        let remote_virtual = MonitorRect {
            id: MonitorId(0xFFFF),
            x: right_edge,
            y: 0,
            width,
            height,
        };

        Self {
            local_monitors: local,
            remote_peer,
            remote_virtual,
        }
    }

    #[must_use]
    pub fn remote_virtual_monitor(&self) -> &MonitorRect {
        &self.remote_virtual
    }

    #[must_use]
    pub fn local_right_edge_x(&self) -> i32 {
        self.local_monitors
            .iter()
            .map(MonitorRect::right_edge)
            .max()
            .unwrap_or(0)
    }

    #[must_use]
    pub fn remote_left_edge_x(&self) -> i32 {
        self.remote_virtual.x
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    #[test]
    fn default_layout_places_remote_to_the_right() {
        let local = vec![MonitorRect {
            id: MonitorId(1),
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        }];
        let peer = PeerId::from_uuid(Uuid::new_v4());
        let layout = MonitorLayout::default_side_by_side(local, peer);
        assert_eq!(layout.remote_virtual.x, 1920);
    }
}
