//! DTOs compartilhados para layout de monitores (workspace + single KVM).

use serde::{Deserialize, Serialize};
use winx_domain::input_control::layout::{BorderSide, EdgeConfig};
use winx_domain::input_control::{MonitorId, MonitorLayout, MonitorRect};
use winx_domain::shared::ids::PeerId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorLayoutDto {
    pub local_monitors: Vec<MonitorRectDto>,
    pub remote_peer: String,
    pub remote_virtual: MonitorRectDto,
    #[serde(default)]
    pub remote_monitors: Vec<MonitorRectDto>,
    pub edge: EdgeConfigDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorRectDto {
    pub id: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeConfigDto {
    pub local_exit: String,
    pub remote_entry: String,
    #[serde(default)]
    pub exit_local_monitor_id: Option<u32>,
}

pub fn layout_to_dto(layout: &MonitorLayout) -> MonitorLayoutDto {
    fn border_str(side: BorderSide) -> String {
        match side {
            BorderSide::Right => "Right".to_string(),
            BorderSide::Left => "Left".to_string(),
            BorderSide::Top => "Top".to_string(),
            BorderSide::Bottom => "Bottom".to_string(),
        }
    }

    fn rect_to_dto(r: &MonitorRect) -> MonitorRectDto {
        MonitorRectDto {
            id: r.id.0,
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
        }
    }

    MonitorLayoutDto {
        local_monitors: layout.local_monitors.iter().map(rect_to_dto).collect(),
        remote_peer: layout.remote_peer.to_string(),
        remote_virtual: rect_to_dto(&layout.remote_virtual),
        remote_monitors: layout.remote_monitors.iter().map(rect_to_dto).collect(),
        edge: EdgeConfigDto {
            local_exit: border_str(layout.edge.local_exit),
            remote_entry: border_str(layout.edge.remote_entry),
            exit_local_monitor_id: layout.edge.exit_local_monitor_id.map(|id| id.0),
        },
    }
}

pub fn dto_to_layout(dto: MonitorLayoutDto) -> Result<MonitorLayout, String> {
    fn parse_border(s: &str) -> Result<BorderSide, String> {
        match s {
            "Right" => Ok(BorderSide::Right),
            "Left" => Ok(BorderSide::Left),
            "Top" => Ok(BorderSide::Top),
            "Bottom" => Ok(BorderSide::Bottom),
            other => Err(format!("border side inválido: {other}")),
        }
    }

    fn rect_from_dto(r: MonitorRectDto) -> MonitorRect {
        MonitorRect {
            id: MonitorId(r.id),
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
        }
    }

    let remote_peer = uuid::Uuid::parse_str(&dto.remote_peer)
        .map(PeerId::from_uuid)
        .map_err(|e| format!("remote_peer inválido: {e}"))?;

    Ok(MonitorLayout {
        local_monitors: dto.local_monitors.into_iter().map(rect_from_dto).collect(),
        remote_peer,
        remote_virtual: rect_from_dto(dto.remote_virtual),
        remote_monitors: dto.remote_monitors.into_iter().map(rect_from_dto).collect(),
        edge: EdgeConfig {
            local_exit: parse_border(&dto.edge.local_exit)?,
            remote_entry: parse_border(&dto.edge.remote_entry)?,
            exit_local_monitor_id: dto.edge.exit_local_monitor_id.map(MonitorId),
        },
    })
}

pub fn rect_to_dto(r: &MonitorRect) -> MonitorRectDto {
    MonitorRectDto {
        id: r.id.0,
        x: r.x,
        y: r.y,
        width: r.width,
        height: r.height,
    }
}
