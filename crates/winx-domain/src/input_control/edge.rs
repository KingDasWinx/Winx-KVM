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
        BorderSide::Right  => input.screen_x >= edge.saturating_sub(EDGE_TOLERANCE_PX),
        BorderSide::Left   => input.screen_x <= edge.saturating_add(EDGE_TOLERANCE_PX),
        BorderSide::Bottom => input.screen_y >= edge.saturating_sub(EDGE_TOLERANCE_PX),
        BorderSide::Top    => input.screen_y <= edge.saturating_add(EDGE_TOLERANCE_PX),
    }
}

/// Retorna `true` quando a posição X estimada do cursor remoto atingiu a borda de retorno.
///
/// `remote_cursor_x_est` é mantido pelo use case em coordenadas locais ao monitor remoto (0..width).
/// O retorno ocorre pela borda oposta à saída: saiu pela Right → volta pela Left (x ≤ tol).
#[must_use]
pub fn should_return_to_local(remote_cursor_x_est: i32, layout: &MonitorLayout) -> bool {
    match layout.edge.local_exit {
        BorderSide::Right | BorderSide::Left => remote_cursor_x_est <= EDGE_TOLERANCE_PX,
        // Bordas verticais: implementar quando layout configurável estiver disponível.
        BorderSide::Top | BorderSide::Bottom => false,
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
        // Na borda esquerda (x=2 = EDGE_TOLERANCE_PX) → deve retornar
        assert!(should_return_to_local(2, &layout));
        assert!(should_return_to_local(0, &layout));
        assert!(should_return_to_local(1, &layout));
    }

    #[test]
    fn return_does_not_trigger_far_from_edge() {
        let layout = layout_1920();
        // No centro da tela ou à direita → não deve retornar
        assert!(!should_return_to_local(3, &layout));
        assert!(!should_return_to_local(100, &layout));
        assert!(!should_return_to_local(960, &layout));
    }
}
