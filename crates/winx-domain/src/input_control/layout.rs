use serde::{Deserialize, Serialize};

use crate::shared::ids::PeerId;

use super::monitor::{MonitorId, MonitorRect};

/// Qual borda de um monitor está sendo referenciada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BorderSide {
    Right,
    Left,
    Top,
    Bottom,
}

/// Descreve por qual borda local o cursor sai e por qual borda remota entra.
/// Permite layouts futuros configuráveis (remoto acima, abaixo, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeConfig {
    pub local_exit: BorderSide,
    pub remote_entry: BorderSide,
}

impl Default for EdgeConfig {
    fn default() -> Self {
        Self {
            local_exit: BorderSide::Right,
            remote_entry: BorderSide::Left,
        }
    }
}

/// Layout virtual lado-a-lado: monitores locais + um monitor representando o peer remoto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorLayout {
    pub local_monitors: Vec<MonitorRect>,
    pub remote_peer: PeerId,
    pub remote_virtual: MonitorRect,
    /// Configuração de qual borda local conecta ao remoto. Default: Right→Left.
    pub edge: EdgeConfig,
}

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
            edge: EdgeConfig::default(),
        }
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

    /// Coordenada da borda local de saída (eixo X para Right/Left, Y para Top/Bottom).
    #[must_use]
    pub fn local_exit_edge_coord(&self) -> i32 {
        match self.edge.local_exit {
            BorderSide::Right => self
                .local_monitors
                .iter()
                .map(MonitorRect::right_edge)
                .max()
                .unwrap_or(0),
            BorderSide::Left => self.local_monitors.iter().map(|m| m.x).min().unwrap_or(0),
            BorderSide::Bottom => self
                .local_monitors
                .iter()
                .map(MonitorRect::bottom_edge)
                .max()
                .unwrap_or(0),
            BorderSide::Top => self.local_monitors.iter().map(|m| m.y).min().unwrap_or(0),
        }
    }

    /// Coordenada da borda local de retorno (oposta à saída).
    #[must_use]
    pub fn local_return_edge_coord(&self) -> i32 {
        match self.edge.local_exit {
            BorderSide::Right => self.local_monitors.iter().map(|m| m.x).min().unwrap_or(0),
            BorderSide::Left => self
                .local_monitors
                .iter()
                .map(MonitorRect::right_edge)
                .max()
                .unwrap_or(0),
            BorderSide::Bottom => self.local_monitors.iter().map(|m| m.y).min().unwrap_or(0),
            BorderSide::Top => self
                .local_monitors
                .iter()
                .map(MonitorRect::bottom_edge)
                .max()
                .unwrap_or(0),
        }
    }

    /// Mapeia o ponto de cruzamento local para coordenadas de entrada no monitor remoto,
    /// preservando a posição proporcional no eixo perpendicular à borda cruzada.
    #[must_use]
    pub fn map_crossing_point(&self, screen_x: i32, screen_y: i32) -> (i32, i32) {
        let local = self.local_monitor_at(screen_x, screen_y);
        let remote = &self.remote_virtual;

        match self.edge.local_exit {
            BorderSide::Right | BorderSide::Left => {
                let local_rel_y = (screen_y - local.y).max(0) as u64;
                let local_h = local.height.max(1) as u64;
                let remote_h = remote.height as u64;
                let prop_y = ((local_rel_y * remote_h) / local_h) as i32;

                let entry_x = match self.edge.remote_entry {
                    BorderSide::Left => 2,
                    BorderSide::Right => remote.width as i32 - 2,
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
                    BorderSide::Top => 2,
                    BorderSide::Bottom => remote.height as i32 - 2,
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

    /// Infere `edge` a partir da posição de `remote_virtual` em relação aos monitores locais.
    pub fn infer_edges_from_geometry(&mut self) {
        let bounds = self.local_union_bounds();
        let local_l = bounds.x;
        let local_t = bounds.y;
        let rv = &self.remote_virtual;
        let remote_cx = rv.x + i32::try_from(rv.width).unwrap_or(0) / 2;
        let remote_cy = rv.y + i32::try_from(rv.height).unwrap_or(0) / 2;
        let local_cx = local_l + i32::try_from(bounds.width).unwrap_or(0) / 2;
        let local_cy = local_t + i32::try_from(bounds.height).unwrap_or(0) / 2;

        let dx = remote_cx - local_cx;
        let dy = remote_cy - local_cy;

        self.edge = if dx.abs() >= dy.abs() {
            if dx >= 0 {
                EdgeConfig {
                    local_exit: BorderSide::Right,
                    remote_entry: BorderSide::Left,
                }
            } else {
                EdgeConfig {
                    local_exit: BorderSide::Left,
                    remote_entry: BorderSide::Right,
                }
            }
        } else if dy >= 0 {
            EdgeConfig {
                local_exit: BorderSide::Bottom,
                remote_entry: BorderSide::Top,
            }
        } else {
            EdgeConfig {
                local_exit: BorderSide::Top,
                remote_entry: BorderSide::Bottom,
            }
        };
    }

    /// Atualiza monitores locais reais e recalcula bordas de cruzamento.
    pub fn finalize_for_runtime(&mut self, local_monitors: Vec<MonitorRect>, remote_peer: PeerId) {
        self.local_monitors = local_monitors;
        self.remote_peer = remote_peer;
        self.infer_edges_from_geometry();
    }

    /// Ponto de warp ao retornar do controle remoto para o desktop local.
    #[must_use]
    pub fn local_return_warp_point(&self) -> (i32, i32) {
        let bounds = self.local_union_bounds();
        match self.edge.local_exit {
            BorderSide::Right => (
                self.local_return_edge_coord().saturating_sub(4),
                bounds.y + i32::try_from(bounds.height).unwrap_or(1080) / 2,
            ),
            BorderSide::Left => (
                self.local_return_edge_coord().saturating_add(4),
                bounds.y + i32::try_from(bounds.height).unwrap_or(1080) / 2,
            ),
            BorderSide::Bottom => (
                bounds.x + i32::try_from(bounds.width).unwrap_or(1920) / 2,
                self.local_return_edge_coord().saturating_sub(4),
            ),
            BorderSide::Top => (
                bounds.x + i32::try_from(bounds.width).unwrap_or(1920) / 2,
                self.local_return_edge_coord().saturating_add(4),
            ),
        }
    }

    /// Retângulo de clip do monitor local enquanto controla o remoto.
    #[must_use]
    pub fn local_clip_rect_while_remote(&self) -> (i32, i32, u32, u32) {
        let m = self.local_monitor_at(
            self.local_return_edge_coord(),
            self.local_union_bounds().y,
        );
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
        assert_eq!(x, 2); // entra 2px da borda esquerda da tela do receiver (coord local = 0)
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
            edge: EdgeConfig::default(),
        };
        layout.infer_edges_from_geometry();
        assert_eq!(layout.edge.local_exit, BorderSide::Left);
        assert_eq!(layout.edge.remote_entry, BorderSide::Right);
    }
}
