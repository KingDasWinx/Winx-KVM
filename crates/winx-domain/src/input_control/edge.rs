//! Detecção de cruzamento de borda no layout virtual (sem I/O).

use super::layout::{BorderSide, MonitorLayout};

/// Tolerância em pixels antes da borda de saída (`coord >= edge - tol`).
pub const EDGE_TOLERANCE_PX: i32 = 2;

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

/// Retorna `true` quando a posição estimada do cursor remoto atingiu a borda de retorno.
#[must_use]
pub fn should_return_to_local(est: RemoteCursorEst, layout: &MonitorLayout) -> bool {
    let w = layout.remote_virtual.width as i32;
    let h = layout.remote_virtual.height as i32;
    match layout.edge.local_exit {
        BorderSide::Right => est.x <= EDGE_TOLERANCE_PX,
        BorderSide::Left => est.x >= w.saturating_sub(EDGE_TOLERANCE_PX),
        BorderSide::Bottom => est.y <= EDGE_TOLERANCE_PX,
        BorderSide::Top => est.y >= h.saturating_sub(EDGE_TOLERANCE_PX),
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
    fn return_triggers_at_right_edge_when_local_exit_is_left() {
        let mut layout = layout_1920();
        layout.remote_virtual.x = -1920;
        layout.infer_edges_from_geometry();
        assert_eq!(layout.edge.local_exit, BorderSide::Left);
        let est = |x| RemoteCursorEst { x, y: 540 };
        assert!(should_return_to_local(est(1918), &layout));
        assert!(should_return_to_local(est(1920), &layout));
        assert!(!should_return_to_local(est(100), &layout));
    }
}
