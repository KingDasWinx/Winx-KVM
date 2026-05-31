//! Conversão entre layout de domínio e wire format (workspace sync).

use winx_domain::input_control::layout::{BorderSide, EdgeConfig, MonitorLayout};
use winx_domain::input_control::{MonitorId, MonitorRect};
use winx_domain::shared::ids::{DeviceId, PeerId};
use winx_domain::workspace::WorkspaceLayout;
use winx_protocol::workspace::{
    EdgeConfigPayload, MonitorLayoutPayload, MonitorRectPayload, WorkspaceLayoutPayload,
};

pub fn layout_to_payload(layout: &WorkspaceLayout) -> WorkspaceLayoutPayload {
    WorkspaceLayoutPayload {
        per_device: layout
            .per_device
            .iter()
            .map(|(device_id, monitor_layout)| {
                (device_id.to_string(), monitor_layout_to_payload(monitor_layout))
            })
            .collect(),
    }
}

pub fn layout_from_payload(payload: &WorkspaceLayoutPayload) -> WorkspaceLayout {
    let mut layout = WorkspaceLayout::empty();
    for (device_id, monitor_layout) in &payload.per_device {
        if let Ok(uuid) = uuid::Uuid::parse_str(device_id) {
            layout.set(
                DeviceId::from_uuid(uuid),
                monitor_layout_from_payload(monitor_layout),
            );
        }
    }
    layout
}

fn monitor_layout_to_payload(layout: &MonitorLayout) -> MonitorLayoutPayload {
    fn border_str(side: BorderSide) -> String {
        match side {
            BorderSide::Right => "Right".to_string(),
            BorderSide::Left => "Left".to_string(),
            BorderSide::Top => "Top".to_string(),
            BorderSide::Bottom => "Bottom".to_string(),
        }
    }

    fn rect_to_payload(r: &MonitorRect) -> MonitorRectPayload {
        MonitorRectPayload {
            id: r.id.0,
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
        }
    }

    MonitorLayoutPayload {
        local_monitors: layout.local_monitors.iter().map(rect_to_payload).collect(),
        remote_peer: layout.remote_peer.as_uuid(),
        remote_virtual: rect_to_payload(&layout.remote_virtual),
        remote_monitors: layout.remote_monitors.iter().map(rect_to_payload).collect(),
        edge: EdgeConfigPayload {
            local_exit: border_str(layout.edge.local_exit),
            remote_entry: border_str(layout.edge.remote_entry),
            exit_local_monitor_id: layout.edge.exit_local_monitor_id.map(|id| id.0),
        },
    }
}

pub fn monitor_layout_to_wire(layout: &MonitorLayout) -> MonitorLayoutPayload {
    monitor_layout_to_payload(layout)
}

pub fn monitor_layout_from_wire(payload: &MonitorLayoutPayload) -> MonitorLayout {
    monitor_layout_from_payload(payload)
}

pub fn rects_to_wire(rects: &[MonitorRect]) -> Vec<MonitorRectPayload> {
    rects
        .iter()
        .map(|r| MonitorRectPayload {
            id: r.id.0,
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
        })
        .collect()
}

pub fn rects_from_wire(payload: &[MonitorRectPayload]) -> Vec<MonitorRect> {
    payload
        .iter()
        .map(|r| MonitorRect {
            id: MonitorId(r.id),
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
        })
        .collect()
}

fn monitor_layout_from_payload(payload: &MonitorLayoutPayload) -> MonitorLayout {
    fn parse_border(s: &str) -> BorderSide {
        match s {
            "Left" => BorderSide::Left,
            "Top" => BorderSide::Top,
            "Bottom" => BorderSide::Bottom,
            _ => BorderSide::Right,
        }
    }

    fn rect_from_payload(r: &MonitorRectPayload) -> MonitorRect {
        MonitorRect {
            id: MonitorId(r.id),
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
        }
    }

    MonitorLayout {
        local_monitors: payload.local_monitors.iter().map(rect_from_payload).collect(),
        remote_peer: PeerId::from_uuid(payload.remote_peer),
        remote_virtual: rect_from_payload(&payload.remote_virtual),
        remote_monitors: payload.remote_monitors.iter().map(rect_from_payload).collect(),
        edge: EdgeConfig {
            local_exit: parse_border(&payload.edge.local_exit),
            remote_entry: parse_border(&payload.edge.remote_entry),
            exit_local_monitor_id: payload.edge.exit_local_monitor_id.map(MonitorId),
        },
    }
}
