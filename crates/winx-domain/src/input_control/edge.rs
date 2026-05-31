//! Detecção de cruzamento de borda no layout virtual (sem I/O).

use super::layout::{BorderSide, MonitorLayout};

/// Tolerância em pixels antes da borda de saída (`coord >= edge - tol`).
pub const EDGE_TOLERANCE_PX: i32 = 2;

/// Inset ao entrar no monitor remoto (evita cursor colado na borda).
pub const REMOTE_ENTRY_INSET_PX: i32 = 20;

/// Distância mínima (px) para dentro do remoto antes de permitir retorno pela borda oposta.
pub const REMOTE_MIN_INLAND_PX: i32 = 48;

/// Margem antes da borda de saída onde handoff de sessão não deve puxar o cursor para trás.
pub const HANDOFF_EDGE_MARGIN_PX: i32 = 96;

/// Verdadeiro quando o cursor está na borda de saída ou se aproximando dela (crossing KVM).
#[must_use]
pub fn approaching_local_exit_edge(
    layout: &MonitorLayout,
    screen_x: i32,
    screen_y: i32,
) -> bool {
    if should_switch_to_remote(
        EdgeDetectInput {
            screen_x,
            screen_y,
            lock_mode: false,
        },
        layout,
    ) {
        return true;
    }
    let m = layout.exit_local_monitor();
    let edge = layout.local_exit_edge_coord();
    match layout.edge.local_exit {
        BorderSide::Right => {
            screen_x >= edge.saturating_sub(HANDOFF_EDGE_MARGIN_PX)
                && screen_y >= m.y
                && screen_y < m.bottom_edge()
        }
        BorderSide::Left => {
            screen_x <= edge.saturating_add(HANDOFF_EDGE_MARGIN_PX)
                && screen_y >= m.y
                && screen_y < m.bottom_edge()
        }
        BorderSide::Bottom => {
            screen_y >= edge.saturating_sub(HANDOFF_EDGE_MARGIN_PX)
                && screen_x >= m.x
                && screen_x < m.right_edge()
        }
        BorderSide::Top => {
            screen_y <= edge.saturating_add(HANDOFF_EDGE_MARGIN_PX)
                && screen_x >= m.x
                && screen_x < m.right_edge()
        }
    }
}

/// Distância do cursor para "dentro" do monitor remoto, a partir da borda de entrada.
#[must_use]
pub fn remote_inland_px(est: RemoteCursorEst, layout: &MonitorLayout) -> i32 {
    let remote = layout.placed_remote_bounds();
    let w = remote.width as i32;
    let h = remote.height as i32;
    match layout.edge.remote_entry {
        BorderSide::Top => est.y.saturating_sub(REMOTE_ENTRY_INSET_PX),
        BorderSide::Bottom => (h - REMOTE_ENTRY_INSET_PX).saturating_sub(est.y),
        BorderSide::Left => est.x.saturating_sub(REMOTE_ENTRY_INSET_PX),
        BorderSide::Right => (w - REMOTE_ENTRY_INSET_PX).saturating_sub(est.x),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdgeDetectInput {
    pub screen_x: i32,
    pub screen_y: i32,
    pub lock_mode: bool,
}

#[must_use]
pub fn should_switch_to_remote(input: EdgeDetectInput, layout: &MonitorLayout) -> bool {
    if input.lock_mode {
        return false;
    }
    let exit = layout.exit_local_monitor();
    if input.screen_x < exit.x
        || input.screen_x >= exit.right_edge()
        || input.screen_y < exit.y
        || input.screen_y >= exit.bottom_edge()
    {
        return false;
    }
    let edge = layout.local_exit_edge_coord();
    match layout.edge.local_exit {
        BorderSide::Right => input.screen_x >= edge.saturating_sub(EDGE_TOLERANCE_PX),
        BorderSide::Left => input.screen_x <= edge.saturating_add(EDGE_TOLERANCE_PX),
        BorderSide::Bottom => input.screen_y >= edge.saturating_sub(EDGE_TOLERANCE_PX),
        BorderSide::Top => input.screen_y <= edge.saturating_add(EDGE_TOLERANCE_PX),
    }
}

/// Posição estimada do cursor no monitor remoto (coordenadas 0..width/height).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteCursorEst {
    pub x: i32,
    pub y: i32,
}

/// Retorna `true` quando a posição estimada do cursor remoto atingiu a borda de retorno
/// (mesma borda por onde entrou no remoto).
#[must_use]
pub fn should_return_to_local(est: RemoteCursorEst, layout: &MonitorLayout) -> bool {
    let remote = layout.placed_remote_bounds();
    let w = remote.width as i32;
    let h = remote.height as i32;
    match layout.edge.remote_entry {
        BorderSide::Left => est.x <= EDGE_TOLERANCE_PX,
        BorderSide::Right => est.x >= w.saturating_sub(EDGE_TOLERANCE_PX),
        BorderSide::Top => est.y <= EDGE_TOLERANCE_PX,
        BorderSide::Bottom => est.y >= h.saturating_sub(EDGE_TOLERANCE_PX),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input_control::{MonitorId, MonitorRect};
    use crate::shared::ids::PeerId;
    use uuid::Uuid;

    fn layout_1920() -> MonitorLayout {
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
    fn no_switch_when_lock_mode() {
        let layout = layout_1920();
        assert!(!should_switch_to_remote(
            EdgeDetectInput {
                screen_x: 2000,
                screen_y: 540,
                lock_mode: true,
            },
            &layout,
        ));
    }

    #[test]
    fn switches_at_right_edge() {
        let layout = layout_1920();
        assert!(should_switch_to_remote(
            EdgeDetectInput {
                screen_x: 1919,
                screen_y: 540,
                lock_mode: false,
            },
            &layout,
        ));
    }

    #[test]
    fn no_switch_before_edge() {
        let layout = layout_1920();
        assert!(!should_switch_to_remote(
            EdgeDetectInput {
                screen_x: 1000,
                screen_y: 540,
                lock_mode: false,
            },
            &layout,
        ));
    }

    #[test]
    fn return_triggers_at_left_edge() {
        let layout = layout_1920();
        let est = |x| RemoteCursorEst { x, y: 540 };
        assert!(should_return_to_local(est(2), &layout));
        assert!(should_return_to_local(est(0), &layout));
        assert!(should_return_to_local(est(1), &layout));
        // Posição típica após cruzamento (inset) — ainda dentro, sem retorno.
        assert!(!should_return_to_local(
            est(super::REMOTE_ENTRY_INSET_PX),
            &layout
        ));
    }

    #[test]
    fn return_does_not_trigger_far_from_edge() {
        let layout = layout_1920();
        let est = |x| RemoteCursorEst { x, y: 540 };
        assert!(!should_return_to_local(est(3), &layout));
        assert!(!should_return_to_local(est(100), &layout));
        assert!(!should_return_to_local(est(960), &layout));
    }

    #[test]
    fn return_triggers_at_top_edge_when_crossed_from_bottom() {
        let peer = PeerId::from_uuid(Uuid::new_v4());
        let mut layout = MonitorLayout::default_side_by_side(
            vec![MonitorRect {
                id: MonitorId(1),
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            }],
            peer,
        );
        layout.remote_virtual.x = 0;
        layout.remote_virtual.y = 1080;
        layout.infer_edges_from_geometry();
        assert_eq!(layout.edge.local_exit, BorderSide::Bottom);
        assert_eq!(layout.edge.remote_entry, BorderSide::Top);

        let (_, entry_y) = layout.map_crossing_point(960, 1079);
        assert_eq!(entry_y, REMOTE_ENTRY_INSET_PX);

        let est = |y| RemoteCursorEst { x: 960, y };
        assert!(!should_return_to_local(est(entry_y), &layout));
        assert!(should_return_to_local(est(1), &layout));
    }

    #[test]
    fn return_does_not_trigger_at_bottom_entry_inset() {
        let peer = PeerId::from_uuid(Uuid::new_v4());
        let mut layout = MonitorLayout::default_side_by_side(
            vec![MonitorRect {
                id: MonitorId(1),
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            }],
            peer,
        );
        layout.remote_virtual.x = 0;
        layout.remote_virtual.y = -1080;
        layout.infer_edges_from_geometry();
        assert_eq!(layout.edge.local_exit, BorderSide::Top);
        assert_eq!(layout.edge.remote_entry, BorderSide::Bottom);

        let (_, entry_y) = layout.map_crossing_point(960, 0);
        assert_eq!(entry_y, 1080 - REMOTE_ENTRY_INSET_PX);

        let est = |y| RemoteCursorEst { x: 960, y };
        assert!(!should_return_to_local(est(entry_y), &layout));
        assert!(!should_return_to_local(est(entry_y + 17), &layout));
        assert!(should_return_to_local(est(1078), &layout));
    }
}
