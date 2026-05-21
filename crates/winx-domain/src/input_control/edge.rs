//! Detecção de cruzamento de borda no layout virtual (sem I/O).

use super::layout::{BorderSide, MonitorLayout};

/// Tolerância em pixels antes da borda de saída (`coord >= edge - tol`).
pub const EDGE_TOLERANCE_PX: i32 = 2;

/// Movimento acumulado no eixo de retorno para voltar ao local.
pub const RETURN_LEFT_THRESHOLD_PX: i32 = 100;

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
        BorderSide::Right  => input.screen_x >= edge.saturating_sub(EDGE_TOLERANCE_PX),
        BorderSide::Left   => input.screen_x <= edge.saturating_add(EDGE_TOLERANCE_PX),
        BorderSide::Bottom => input.screen_y >= edge.saturating_sub(EDGE_TOLERANCE_PX),
        BorderSide::Top    => input.screen_y <= edge.saturating_add(EDGE_TOLERANCE_PX),
    }
}

#[must_use]
pub fn accumulate_return_left(dx: i32, current_accum: i32) -> (i32, bool) {
    if dx >= 0 {
        return (0, false);
    }
    let next = current_accum.saturating_add(dx);
    if next < -RETURN_LEFT_THRESHOLD_PX {
        (0, true)
    } else {
        (next, false)
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
    fn return_left_triggers_after_threshold() {
        let (acc, go) = accumulate_return_left(-40, -60);
        assert!(!go);
        assert_eq!(acc, -100);
        let (_, go) = accumulate_return_left(-5, -100);
        assert!(go);
    }

    #[test]
    fn return_left_resets_on_right_movement() {
        let (acc, go) = accumulate_return_left(1, -50);
        assert!(!go);
        assert_eq!(acc, 0);
    }
}
