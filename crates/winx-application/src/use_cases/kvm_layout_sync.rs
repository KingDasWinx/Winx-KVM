//! Sync de layout/monitores via stream Data (single connection).

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tracing::{debug, info, warn};
use winx_domain::{
    input_control::{events::PeerMonitorsUpdated, MonitorLayout, MonitorRect},
    shared::ids::PeerId,
    transport::StreamKind,
    DomainEvent,
};
use winx_protocol::{encode, Frame, Payload, PeerMonitorsPayload};

use crate::{
    bus::EventBus,
    ports::{transport::StreamSender, KvmLayoutStore},
    workspace_layout_wire::{monitor_layout_to_wire, rects_from_wire, rects_to_wire},
    use_cases::TransportService,
};

use super::input_control::MouseSendState;

const LAYOUT_SYNC_WAIT: Duration = Duration::from_secs(6);

async fn send_frame(tx: &StreamSender, payload: Payload) {
    let frame = Frame::new(payload);
    if let Ok(bytes) = encode(&frame) {
        if tx.send(bytes).await.is_err() {
            warn!("falha ao enviar frame de layout KVM no stream Data");
        }
    }
}

async fn send_announcements(
    tx: &StreamSender,
    local_monitors: &[MonitorRect],
    saved: Option<&MonitorLayout>,
) {
    send_frame(
        tx,
        Payload::PeerMonitorsAnnounce(PeerMonitorsPayload {
            monitors: rects_to_wire(local_monitors),
        }),
    )
    .await;

    if let Some(layout) = saved {
        send_frame(tx, Payload::KvmLayoutShare(monitor_layout_to_wire(layout))).await;
    }
}

pub async fn store_peer_monitors(
    store: &Arc<Mutex<Option<Arc<dyn KvmLayoutStore>>>>,
    bus: &EventBus,
    peer_id: PeerId,
    monitors: Vec<MonitorRect>,
) {
    let count = monitors.len();
    if count == 0 {
        return;
    }
    if let Some(s) = store.lock().await.as_ref() {
        if let Err(err) = s.save_peer_monitors(peer_id, &monitors).await {
            warn!(?err, %peer_id, "falha ao persistir monitores do peer");
        }
    }
    bus.publish(DomainEvent::PeerMonitorsUpdated(PeerMonitorsUpdated {
        peer_id,
        monitor_count: count,
    }));
    info!(%peer_id, count, "monitores do peer recebidos via sync");
}

async fn handle_layout_frame(
    store: &Arc<Mutex<Option<Arc<dyn KvmLayoutStore>>>>,
    bus: &EventBus,
    layout: &Arc<Mutex<Option<MonitorLayout>>>,
    enabled: &Arc<Mutex<bool>>,
    active_peer: &Arc<Mutex<Option<PeerId>>>,
    mouse_send: &Arc<MouseSendState>,
    peer_id: PeerId,
    bytes: Vec<u8>,
) {
    let Ok(frame) = winx_protocol::decode(&bytes) else {
        return;
    };
    match frame.payload {
        Payload::PeerMonitorsAnnounce(p) => {
            let monitors = rects_from_wire(&p.monitors);
            store_peer_monitors(store, bus, peer_id, monitors).await;
        }
        Payload::KvmLayoutShare(wire) => {
            let received = crate::workspace_layout_wire::monitor_layout_from_wire(&wire);
            if !received.local_monitors.is_empty() {
                store_peer_monitors(
                    store,
                    bus,
                    peer_id,
                    received.local_monitors.clone(),
                )
                .await;
            }
            if *enabled.lock().await && *active_peer.lock().await == Some(peer_id) {
                let mut current = layout.lock().await;
                if let Some(ref mut active) = *current {
                    if active.remote_monitors.is_empty() {
                        active.remote_monitors = received.local_monitors;
                        active.infer_edges_from_geometry();
                        let scale = active.remote_mouse_scale();
                        mouse_send
                            .scale_q8
                            .store((scale * 256.0).round() as i32, Ordering::SeqCst);
                        info!(%peer_id, "layout remoto atualizado a partir de KvmLayoutShare");
                    }
                }
            }
        }
        _ => {}
    }
}

async fn recv_layout_loop(
    store: Arc<Mutex<Option<Arc<dyn KvmLayoutStore>>>>,
    bus: EventBus,
    layout: Arc<Mutex<Option<MonitorLayout>>>,
    enabled: Arc<Mutex<bool>>,
    active_peer: Arc<Mutex<Option<PeerId>>>,
    mouse_send: Arc<MouseSendState>,
    peer_id: PeerId,
    mut rx: crate::ports::transport::StreamReceiver,
) {
    while let Some(bytes) = rx.recv().await {
        handle_layout_frame(
            &store,
            &bus,
            &layout,
            &enabled,
            &active_peer,
            &mouse_send,
            peer_id,
            bytes,
        )
        .await;
    }
    debug!(%peer_id, "layout sync stream encerrado");
}

/// Anuncia monitores/layout locais e escuta respostas do peer (stream Data dedicado).
pub async fn start_kvm_layout_sync(
    transport: Arc<TransportService>,
    store: Arc<Mutex<Option<Arc<dyn KvmLayoutStore>>>>,
    bus: EventBus,
    layout: Arc<Mutex<Option<MonitorLayout>>>,
    enabled: Arc<Mutex<bool>>,
    active_peer: Arc<Mutex<Option<PeerId>>>,
    mouse_send: Arc<MouseSendState>,
    peer_id: PeerId,
    local_monitors: Vec<MonitorRect>,
) {
    let saved = {
        let guard = store.lock().await;
        if let Some(s) = guard.as_ref() {
            s.get(peer_id).await.ok().flatten()
        } else {
            None
        }
    };

    let saved_inbound = saved.clone();
    let monitors_inbound = local_monitors.clone();

    let store_in = Arc::clone(&store);
    let bus_in = bus.clone();
    let layout_in = Arc::clone(&layout);
    let enabled_in = Arc::clone(&enabled);
    let active_in = Arc::clone(&active_peer);
    let mouse_in = Arc::clone(&mouse_send);
    let transport_in = Arc::clone(&transport);
    tokio::spawn(async move {
        if let Some((tx, rx)) = transport_in
            .wait_inbound_stream(peer_id, StreamKind::Data, LAYOUT_SYNC_WAIT)
            .await
        {
            send_announcements(&tx, &monitors_inbound, saved_inbound.as_ref()).await;
            recv_layout_loop(
                store_in,
                bus_in,
                layout_in,
                enabled_in,
                active_in,
                mouse_in,
                peer_id,
                rx,
            )
            .await;
        }
    });

    match transport
        .open_stream_for_peer(peer_id, StreamKind::Data)
        .await
    {
        Ok((tx, rx)) => {
            send_announcements(&tx, &local_monitors, saved.as_ref()).await;
            recv_layout_loop(store, bus, layout, enabled, active_peer, mouse_send, peer_id, rx)
                .await;
        }
        Err(err) => {
            warn!(?err, %peer_id, "falha ao abrir stream Data para layout sync");
        }
    }
}

pub async fn get_peer_monitors_from_store(
    store: &Arc<Mutex<Option<Arc<dyn KvmLayoutStore>>>>,
    peer_id: PeerId,
) -> Vec<MonitorRect> {
    let guard = store.lock().await;
    let Some(s) = guard.as_ref() else {
        return Vec::new();
    };
    s.get_peer_monitors(peer_id)
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// Reenvia layout local ao peer (após salvar no editor).
pub async fn broadcast_kvm_layout(
    transport: Arc<TransportService>,
    _store: Arc<Mutex<Option<Arc<dyn KvmLayoutStore>>>>,
    peer_id: PeerId,
    local_monitors: Vec<MonitorRect>,
    layout: &MonitorLayout,
) {
    if !transport.is_peer_connected(peer_id).await {
        return;
    }
    if let Ok((tx, _rx)) = transport
        .open_stream_for_peer(peer_id, StreamKind::Data)
        .await
    {
        send_announcements(&tx, &local_monitors, Some(layout)).await;
    }
}
