use serde::{Deserialize, Serialize};

use crate::shared::ids::PeerId;

use super::monitor::{MonitorId, MonitorRect};
use super::edge::REMOTE_ENTRY_INSET_PX;

/// Qual borda de um monitor está sendo referenciada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BorderSide {
    Right,
    Left,
    Top,
    Bottom,
}

/// Descreve por qual borda local o cursor sai e por qual borda remota entra.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeConfig {
    pub local_exit: BorderSide,
    pub remote_entry: BorderSide,
    /// Monitor local cuja borda encosta no bloco remoto no layout virtual.
    #[serde(default)]
    pub exit_local_monitor_id: Option<MonitorId>,
}

impl Default for EdgeConfig {
    fn default() -> Self {
        Self {
            local_exit: BorderSide::Right,
            remote_entry: BorderSide::Left,
            exit_local_monitor_id: None,
        }
    }
}

/// Layout virtual: monitores locais + bloco remoto posicionável no desktop virtual.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorLayout {
    pub local_monitors: Vec<MonitorRect>,
    pub remote_peer: PeerId,
    /// Retângulo envolvente do bloco remoto no espaço virtual compartilhado.
    pub remote_virtual: MonitorRect,
    /// Monitores reais do peer remoto (coordenadas Windows dele). Vazio = um monitor virtual.
    #[serde(default)]
    pub remote_monitors: Vec<MonitorRect>,
    pub edge: EdgeConfig,
}

/// Tolerância de encostar (px) ao inferir adjacência no editor.
const ADJACENCY_GAP_PX: i32 = 80;

impl MonitorLayout {
    /// Coloca o peer remoto à direita do monitor local mais à direita (v0.1).
    #[must_use]
    pub fn default_side_by_side(local: Vec<MonitorRect>, remote_peer: PeerId) -> Self {
        Self::default_side_by_side_with_remote_size(local, remote_peer, None)
    }

    /// Layout lado a lado; `remote_size` sobrescreve largura/altura do monitor virtual remoto.
    #[must_use]
    pub fn default_side_by_side_with_remote_size(
        local: Vec<MonitorRect>,
        remote_peer: PeerId,
        remote_size: Option<(u32, u32)>,
    ) -> Self {
        let right_edge = local.iter().map(MonitorRect::right_edge).max().unwrap_or(0);
        let height = local.iter().map(|m| m.height).max().unwrap_or(1080);
        let width = local.iter().map(|m| m.width).max().unwrap_or(1920);
        let (rw, rh) = remote_size.unwrap_or((width, height));

        let remote_virtual = MonitorRect {
            id: MonitorId(0xFFFF),
            x: right_edge,
            y: 0,
            width: rw,
            height: rh,
        };

        Self {
            local_monitors: local,
            remote_peer,
            remote_virtual,
            remote_monitors: Vec::new(),
            edge: EdgeConfig::default(),
        }
    }

    /// Monitor local de saída configurado (fallback: envolvente).
    #[must_use]
    pub fn exit_local_monitor(&self) -> MonitorRect {
        if let Some(id) = self.edge.exit_local_monitor_id {
            if let Some(m) = self.local_monitors.iter().find(|m| m.id == id) {
                return *m;
            }
        }
        self.local_union_bounds()
    }

    /// Monitores remotos posicionados no espaço virtual (origem = `remote_virtual`).
    #[must_use]
    pub fn placed_remote_monitors(&self) -> Vec<MonitorRect> {
        if self.remote_monitors.is_empty() {
            return vec![self.remote_virtual];
        }
        let min_x = self
            .remote_monitors
            .iter()
            .map(|m| m.x)
            .min()
            .unwrap_or(0);
        let min_y = self
            .remote_monitors
            .iter()
            .map(|m| m.y)
            .min()
            .unwrap_or(0);
        let ox = self.remote_virtual.x;
        let oy = self.remote_virtual.y;
        self.remote_monitors
            .iter()
            .map(|m| MonitorRect {
                id: m.id,
                x: m.x - min_x + ox,
                y: m.y - min_y + oy,
                width: m.width,
                height: m.height,
            })
            .collect()
    }

    /// Envelope do bloco remoto posicionado.
    #[must_use]
    pub fn placed_remote_bounds(&self) -> MonitorRect {
        let placed = self.placed_remote_monitors();
        let Some(first) = placed.first() else {
            return self.remote_virtual;
        };
        let mut min_x = first.x;
        let mut min_y = first.y;
        let mut max_r = first.right_edge();
        let mut max_b = first.bottom_edge();
        for m in placed.iter().skip(1) {
            min_x = min_x.min(m.x);
            min_y = min_y.min(m.y);
            max_r = max_r.max(m.right_edge());
            max_b = max_b.max(m.bottom_edge());
        }
        MonitorRect {
            id: MonitorId(0xFFFF),
            x: min_x,
            y: min_y,
            width: (max_r - min_x).max(1) as u32,
            height: (max_b - min_y).max(1) as u32,
        }
    }

    fn overlap_1d(a0: i32, a1: i32, b0: i32, b1: i32) -> i32 {
        (a1.min(b1) - a0.max(b0)).max(0)
    }

    /// Encontra o par monitor-local ↔ borda com melhor adjacência ao bloco remoto.
    fn find_adjacent_edge(
        locals: &[MonitorRect],
        remote: MonitorRect,
    ) -> (MonitorId, BorderSide, BorderSide) {
        let mut best: Option<(MonitorId, BorderSide, BorderSide, i32)> = None;

        let consider = |best: &mut Option<(MonitorId, BorderSide, BorderSide, i32)>,
                        id: MonitorId,
                        local_exit: BorderSide,
                        remote_entry: BorderSide,
                        gap: i32,
                        overlap: i32| {
            if overlap <= 0 {
                return;
            }
            if gap.abs() > ADJACENCY_GAP_PX {
                return;
            }
            // Menor gap e maior overlap ganham.
            let score = gap.abs() * 10_000 - overlap;
            if best.map_or(true, |(_, _, _, s)| score < s) {
                *best = Some((id, local_exit, remote_entry, score));
            }
        };

        for local in locals {
            let overlap_y =
                Self::overlap_1d(local.y, local.bottom_edge(), remote.y, remote.bottom_edge());
            let gap_right = remote.x - local.right_edge();
            consider(
                &mut best,
                local.id,
                BorderSide::Right,
                BorderSide::Left,
                gap_right,
                overlap_y,
            );

            let gap_left = local.x - remote.right_edge();
            consider(
                &mut best,
                local.id,
                BorderSide::Left,
                BorderSide::Right,
                gap_left,
                overlap_y,
            );

            let overlap_x = Self::overlap_1d(
                local.x,
                local.right_edge(),
                remote.x,
                remote.right_edge(),
            );
            let gap_bottom = remote.y - local.bottom_edge();
            consider(
                &mut best,
                local.id,
                BorderSide::Bottom,
                BorderSide::Top,
                gap_bottom,
                overlap_x,
            );

            let gap_top = local.y - remote.bottom_edge();
            consider(
                &mut best,
                local.id,
                BorderSide::Top,
                BorderSide::Bottom,
                gap_top,
                overlap_x,
            );
        }

        best.map(|(id, le, re, _)| (id, le, re)).unwrap_or((
            locals
                .first()
                .map(|m| m.id)
                .unwrap_or(MonitorId(1)),
            BorderSide::Right,
            BorderSide::Left,
        ))
    }

    /// Fator de escala sugerido para mouse remoto (clamp 0.5–2.0).
    #[must_use]
    pub fn remote_mouse_scale(&self) -> f32 {
        let local_w = self
            .local_monitors
            .iter()
            .map(|m| m.width)
            .max()
            .unwrap_or(1920);
        if local_w == 0 {
            return 1.0;
        }
        let scale = self.remote_virtual.width as f32 / local_w as f32;
        scale.clamp(0.5, 2.0)
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
    #[allow(dead_code)]
    pub fn remote_left_edge_x(&self) -> i32 {
        self.remote_virtual.x
    }

    /// Coordenada da borda local de saída no monitor de crossing.
    #[must_use]
    pub fn local_exit_edge_coord(&self) -> i32 {
        let m = self.exit_local_monitor();
        match self.edge.local_exit {
            BorderSide::Right => m.right_edge(),
            BorderSide::Left => m.x,
            BorderSide::Bottom => m.bottom_edge(),
            BorderSide::Top => m.y,
        }
    }

    /// Coordenada da borda local de retorno (oposta à saída, no monitor de crossing).
    #[must_use]
    pub fn local_return_edge_coord(&self) -> i32 {
        let m = self.exit_local_monitor();
        match self.edge.local_exit {
            BorderSide::Right => m.x,
            BorderSide::Left => m.right_edge(),
            BorderSide::Bottom => m.y,
            BorderSide::Top => m.bottom_edge(),
        }
    }

    /// Mapeia o ponto de cruzamento local para coordenadas de entrada no monitor remoto,
    /// preservando a posição proporcional no eixo perpendicular à borda cruzada.
    #[must_use]
    pub fn map_crossing_point(&self, screen_x: i32, screen_y: i32) -> (i32, i32) {
        let local = self.exit_local_monitor();
        let remote = self.placed_remote_bounds();

        match self.edge.local_exit {
            BorderSide::Right | BorderSide::Left => {
                let local_rel_y = (screen_y - local.y).max(0) as u64;
                let local_h = local.height.max(1) as u64;
                let remote_h = remote.height as u64;
                let prop_y = ((local_rel_y * remote_h) / local_h) as i32;

                let entry_x = match self.edge.remote_entry {
                    BorderSide::Left => REMOTE_ENTRY_INSET_PX,
                    BorderSide::Right => remote.width as i32 - REMOTE_ENTRY_INSET_PX,
                    BorderSide::Top | BorderSide::Bottom => remote.width as i32 / 2,
                };
                let entry_y = prop_y.clamp(0, remote.height as i32 - 1);
                (entry_x, entry_y)
            }
            BorderSide::Top | BorderSide::Bottom => {
                let local_rel_x = (screen_x - local.x).max(0) as u64;
                let local_w = local.width.max(1) as u64;
                let remote_w = remote.width as u64;
                let prop_x = ((local_rel_x * remote_w) / local_w) as i32;

                let entry_y = match self.edge.remote_entry {
                    BorderSide::Top => REMOTE_ENTRY_INSET_PX,
                    BorderSide::Bottom => remote.height as i32 - REMOTE_ENTRY_INSET_PX,
                    BorderSide::Left | BorderSide::Right => remote.height as i32 / 2,
                };
                let entry_x = prop_x.clamp(0, remote.width as i32 - 1);
                (entry_x, entry_y)
            }
        }
    }

    /// Retângulo envolvente de todos os monitores locais.
    #[must_use]
    pub fn local_union_bounds(&self) -> MonitorRect {
        let Some(first) = self.local_monitors.first() else {
            return MonitorRect {
                id: MonitorId(0),
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            };
        };
        let mut min_x = first.x;
        let mut min_y = first.y;
        let mut max_r = first.right_edge();
        let mut max_b = first.bottom_edge();
        for m in self.local_monitors.iter().skip(1) {
            min_x = min_x.min(m.x);
            min_y = min_y.min(m.y);
            max_r = max_r.max(m.right_edge());
            max_b = max_b.max(m.bottom_edge());
        }
        MonitorRect {
            id: MonitorId(0),
            x: min_x,
            y: min_y,
            width: (max_r - min_x).max(1) as u32,
            height: (max_b - min_y).max(1) as u32,
        }
    }

    /// Monitor local sob o ponto do cursor (fallback: envolvente).
    #[must_use]
    pub fn local_monitor_at(&self, screen_x: i32, screen_y: i32) -> MonitorRect {
        self.local_monitors
            .iter()
            .copied()
            .find(|m| {
                screen_x >= m.x
                    && screen_x < m.right_edge()
                    && screen_y >= m.y
                    && screen_y < m.bottom_edge()
            })
            .unwrap_or_else(|| self.local_union_bounds())
    }

    /// Infere bordas a partir da adjacência real entre monitores locais e o bloco remoto.
    pub fn infer_edges_from_geometry(&mut self) {
        let remote = self.placed_remote_bounds();
        let (id, local_exit, remote_entry) =
            Self::find_adjacent_edge(&self.local_monitors, remote);
        self.edge = EdgeConfig {
            local_exit,
            remote_entry,
            exit_local_monitor_id: Some(id),
        };
    }

    /// Atualiza monitores locais reais e recalcula bordas de cruzamento.
    pub fn finalize_for_runtime(&mut self, local_monitors: Vec<MonitorRect>, remote_peer: PeerId) {
        self.local_monitors = local_monitors;
        self.remote_peer = remote_peer;
        self.infer_edges_from_geometry();
    }

    /// Converte o layout compartilhado pelo peer (perspectiva dele) para o nosso desktop virtual.
    #[must_use]
    pub fn from_peer_share(
        peer_share: &MonitorLayout,
        local_monitors: Vec<MonitorRect>,
        remote_peer: PeerId,
    ) -> Self {
        fn opposite(side: BorderSide) -> BorderSide {
            match side {
                BorderSide::Right => BorderSide::Left,
                BorderSide::Left => BorderSide::Right,
                BorderSide::Top => BorderSide::Bottom,
                BorderSide::Bottom => BorderSide::Top,
            }
        }

        let peer_local = peer_share.local_union_bounds();
        let peer_remote = peer_share.placed_remote_bounds();
        let our_local = Self::union_of_monitors(&local_monitors);
        let peer_block = Self::union_of_monitors(&peer_share.local_monitors);
        let pw = (peer_block.right_edge() - peer_block.x).max(1) as u32;
        let ph = (peer_block.bottom_edge() - peer_block.y).max(1) as u32;

        let (rv_x, rv_y) = match peer_share.edge.local_exit {
            BorderSide::Right => (
                our_local.x - pw as i32,
                our_local.y + (peer_remote.y - peer_local.y),
            ),
            BorderSide::Left => (
                our_local.right_edge(),
                our_local.y + (peer_remote.y - peer_local.y),
            ),
            BorderSide::Bottom => (
                our_local.x + (peer_remote.x - peer_local.x),
                our_local.y - ph as i32,
            ),
            BorderSide::Top => (
                our_local.x + (peer_remote.x - peer_local.x),
                our_local.bottom_edge(),
            ),
        };

        let mut layout = Self {
            local_monitors,
            remote_peer,
            remote_virtual: MonitorRect {
                id: MonitorId(0xFFFF),
                x: rv_x,
                y: rv_y,
                width: pw,
                height: ph,
            },
            remote_monitors: peer_share.local_monitors.clone(),
            edge: EdgeConfig {
                local_exit: opposite(peer_share.edge.remote_entry),
                remote_entry: opposite(peer_share.edge.local_exit),
                exit_local_monitor_id: None,
            },
        };
        layout.infer_edges_from_geometry();
        layout
    }

    fn union_of_monitors(monitors: &[MonitorRect]) -> MonitorRect {
        let Some(first) = monitors.first() else {
            return MonitorRect {
                id: MonitorId(0),
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            };
        };
        let mut min_x = first.x;
        let mut min_y = first.y;
        let mut max_r = first.right_edge();
        let mut max_b = first.bottom_edge();
        for m in monitors.iter().skip(1) {
            min_x = min_x.min(m.x);
            min_y = min_y.min(m.y);
            max_r = max_r.max(m.right_edge());
            max_b = max_b.max(m.bottom_edge());
        }
        MonitorRect {
            id: MonitorId(0),
            x: min_x,
            y: min_y,
            width: (max_r - min_x).max(1) as u32,
            height: (max_b - min_y).max(1) as u32,
        }
    }

    /// Ponto de warp ao retornar do controle remoto para o desktop local.
    #[must_use]
    pub fn local_return_warp_point(&self) -> (i32, i32) {
        let m = self.exit_local_monitor();
        match self.edge.local_exit {
            BorderSide::Right => (
                self.local_return_edge_coord().saturating_sub(4),
                m.y + i32::try_from(m.height).unwrap_or(1080) / 2,
            ),
            BorderSide::Left => (
                self.local_return_edge_coord().saturating_add(4),
                m.y + i32::try_from(m.height).unwrap_or(1080) / 2,
            ),
            BorderSide::Bottom => (
                m.x + i32::try_from(m.width).unwrap_or(1920) / 2,
                self.local_return_edge_coord().saturating_sub(4),
            ),
            BorderSide::Top => (
                m.x + i32::try_from(m.width).unwrap_or(1920) / 2,
                self.local_return_edge_coord().saturating_add(4),
            ),
        }
    }

    /// Retângulo de clip do monitor local enquanto controla o remoto.
    #[must_use]
    pub fn local_clip_rect_while_remote(&self) -> (i32, i32, u32, u32) {
        let m = self.exit_local_monitor();
        (m.x, m.y, m.width, m.height)
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn layout_1920x1080_remote_same() -> MonitorLayout {
        let peer = PeerId::from_uuid(Uuid::new_v4());
        MonitorLayout::default_side_by_side(
            vec![MonitorRect {
                id: MonitorId(1),
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            }],
            peer,
        )
    }

    #[test]
    fn default_layout_places_remote_to_the_right() {
        let layout = layout_1920x1080_remote_same();
        assert_eq!(layout.remote_virtual.x, 1920);
        assert_eq!(layout.edge.local_exit, BorderSide::Right);
    }

    #[test]
    fn remote_size_overrides_virtual_monitor() {
        let local = vec![MonitorRect {
            id: MonitorId(1),
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        }];
        let peer = PeerId::from_uuid(Uuid::new_v4());
        let layout =
            MonitorLayout::default_side_by_side_with_remote_size(local, peer, Some((3840, 2160)));
        assert_eq!(layout.remote_virtual.width, 3840);
        assert_eq!(layout.remote_virtual.height, 2160);
    }

    #[test]
    fn map_crossing_point_same_height_preserves_y() {
        let layout = layout_1920x1080_remote_same();
        // Cruza pela borda direita em Y=300
        let (x, y) = layout.map_crossing_point(1919, 300);
        assert_eq!(x, REMOTE_ENTRY_INSET_PX); // entra REMOTE_ENTRY_INSET da borda esquerda do receiver
        assert_eq!(y, 300); // Y preservado (mesma altura)
    }

    #[test]
    fn map_crossing_point_different_height_scales_y() {
        let peer = PeerId::from_uuid(Uuid::new_v4());
        // Local 1080p, remoto 4K
        let layout = MonitorLayout::default_side_by_side_with_remote_size(
            vec![MonitorRect {
                id: MonitorId(1),
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            }],
            peer,
            Some((3840, 2160)),
        );
        // Cruza em Y=540 (meio da tela local) → deve mapear para Y=1080 (meio da remota)
        let (_, y) = layout.map_crossing_point(1919, 540);
        assert_eq!(y, 1080);
    }

    #[test]
    fn from_peer_share_mirrors_right_adjacency_to_left() {
        let peer = PeerId::from_uuid(Uuid::new_v4());
        let peer_layout = MonitorLayout::default_side_by_side(
            vec![MonitorRect {
                id: MonitorId(1),
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            }],
            peer,
        );
        let local = vec![MonitorRect {
            id: MonitorId(2),
            x: 0,
            y: 0,
            width: 2560,
            height: 1080,
        }];
        let mirrored = MonitorLayout::from_peer_share(&peer_layout, local, peer);
        assert_eq!(mirrored.edge.local_exit, BorderSide::Left);
        assert_eq!(mirrored.edge.remote_entry, BorderSide::Right);
        assert!(mirrored.remote_virtual.x + mirrored.remote_virtual.width as i32 <= 0);
    }

    #[test]
    fn map_crossing_point_clamps_to_remote_bounds() {
        let layout = layout_1920x1080_remote_same();
        // Y negativo → clampado para y=0
        let (_, y) = layout.map_crossing_point(1919, -100);
        assert_eq!(y, 0);
        // Y além do limite → clampado para height-1
        let (_, y2) = layout.map_crossing_point(1919, 2000);
        assert_eq!(y2, 1079);
    }

    #[test]
    fn local_exit_edge_coord_right() {
        let layout = layout_1920x1080_remote_same();
        assert_eq!(layout.local_exit_edge_coord(), 1920);
    }

    #[test]
    fn infer_edges_places_remote_left_of_local() {
        let peer = PeerId::from_uuid(Uuid::new_v4());
        let mut layout = MonitorLayout {
            local_monitors: vec![MonitorRect {
                id: MonitorId(1),
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            }],
            remote_peer: peer,
            remote_virtual: MonitorRect {
                id: MonitorId(0xFFFF),
                x: -1920,
                y: 0,
                width: 1920,
                height: 1080,
            },
            remote_monitors: Vec::new(),
            edge: EdgeConfig::default(),
        };
        layout.infer_edges_from_geometry();
        assert_eq!(layout.edge.local_exit, BorderSide::Left);
        assert_eq!(layout.edge.remote_entry, BorderSide::Right);
        assert_eq!(layout.edge.exit_local_monitor_id, Some(MonitorId(1)));
    }

    #[test]
    fn dual_local_only_rightmost_monitor_is_exit() {
        let peer = PeerId::from_uuid(Uuid::new_v4());
        let mut layout = MonitorLayout {
            local_monitors: vec![
                MonitorRect {
                    id: MonitorId(1),
                    x: 0,
                    y: 0,
                    width: 1920,
                    height: 1080,
                },
                MonitorRect {
                    id: MonitorId(2),
                    x: 1920,
                    y: 0,
                    width: 1920,
                    height: 1080,
                },
            ],
            remote_peer: peer,
            remote_virtual: MonitorRect {
                id: MonitorId(0xFFFF),
                x: 3840,
                y: 0,
                width: 1920,
                height: 1080,
            },
            remote_monitors: Vec::new(),
            edge: EdgeConfig::default(),
        };
        layout.infer_edges_from_geometry();
        assert_eq!(layout.edge.exit_local_monitor_id, Some(MonitorId(2)));

        use crate::input_control::edge::{should_switch_to_remote, EdgeDetectInput};
        assert!(!should_switch_to_remote(
            EdgeDetectInput {
                screen_x: 1919,
                screen_y: 540,
                lock_mode: false,
            },
            &layout,
        ));
        assert!(should_switch_to_remote(
            EdgeDetectInput {
                screen_x: 3839,
                screen_y: 540,
                lock_mode: false,
            },
            &layout,
        ));
    }
}
