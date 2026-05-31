//! Sync de layout/monitores via stream Data compartilhado com clipboard.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::{debug, info, warn};
use winx_domain::{
    input_control::{
        events::{KvmLayoutUpdated, PeerMonitorsUpdated},
        FocusState, FocusTarget, MonitorLayout, MonitorRect, SessionDesktopLayout,
    },
    shared::ids::{DeviceId, PeerId},
    DomainEvent,
};
use winx_protocol::{Payload, PeerMonitorsPayload};

use crate::{
    bus::EventBus,
    ports::{KvmLayoutStore, MonitorBackend},
    use_cases::clipboard::{ClipboardService, LayoutDataHandler},
    workspace_layout_wire::{
        monitor_layout_to_session, rects_from_wire, rects_to_wire, session_layout_from_wire,
        session_layout_to_wire,
    },
};

use super::input_control::MouseSendState;

/// Nome estável do payload para logs de diagnóstico.
#[must_use]
pub fn payload_kind(payload: &Payload) -> &'static str {
    match payload {
        Payload::PeerMonitorsAnnounce(_) => "PeerMonitorsAnnounce",
        Payload::KvmLayoutShare(_) => "KvmLayoutShare",
        Payload::Clipboard(_) => "Clipboard",
        Payload::Heartbeat => "Heartbeat",
        Payload::HeartbeatAck => "HeartbeatAck",
        Payload::Input(_) => "Input",
        Payload::OpenStream { .. } => "OpenStream",
        Payload::CloseStream { .. } => "CloseStream",
        Payload::FocusSwitch { .. } => "FocusSwitch",
        _ => "Unknown",
    }
}

/// Resumo legível de monitores para logs.
#[must_use]
pub fn monitors_summary(monitors: &[MonitorRect]) -> String {
    if monitors.is_empty() {
        return "(vazio)".to_string();
    }
    monitors
        .iter()
        .map(|m| format!("#{} {}x{}@{},{}", m.id.0, m.width, m.height, m.x, m.y))
        .collect::<Vec<_>>()
        .join("; ")
}

pub struct KvmLayoutSyncDeps {
    pub store: Arc<Mutex<Option<Arc<dyn KvmLayoutStore>>>>,
    pub bus: EventBus,
    pub layout: Arc<Mutex<Option<MonitorLayout>>>,
    pub enabled: Arc<Mutex<bool>>,
    pub active_peer: Arc<Mutex<Option<PeerId>>>,
    pub mouse_send: Arc<MouseSendState>,
    pub monitors: Arc<dyn MonitorBackend>,
    pub focus: Arc<Mutex<FocusState>>,
    pub local_device_id: Arc<Mutex<Option<DeviceId>>>,
}

impl KvmLayoutSyncDeps {
    pub async fn register_handler(self: Arc<Self>, clipboard: &ClipboardService) {
        let deps = Arc::clone(&self);
        let handler: LayoutDataHandler = Arc::new(move |peer_id, payload| {
            let deps = Arc::clone(&deps);
            tokio::spawn(async move {
                handle_layout_payload(&deps, peer_id, payload).await;
            });
        });
        clipboard.set_layout_handler(Some(handler)).await;
        info!("layout sync: handler registrado no stream Data do clipboard");
    }

    pub async fn announce_to_peer(
        &self,
        clipboard: &ClipboardService,
        peer_id: PeerId,
        local_monitors: &[MonitorRect],
    ) -> Result<(), winx_domain::DomainError> {
        let count = local_monitors.len();
        info!(
            %peer_id,
            count,
            monitors = %monitors_summary(local_monitors),
            "layout sync: anunciando monitores locais ao peer"
        );

        let local_device = match *self.local_device_id.lock().await {
            Some(id) => id,
            None => {
                warn!(%peer_id, "layout sync: local_device_id ausente — announce abortado");
                return Ok(());
            }
        };

        let mut session = load_session_from_store(&self.store, peer_id, local_device).await;
        session.merge_announced_monitors(local_device, local_monitors, None);
        persist_session(&self.store, peer_id, &session).await;

        clipboard
            .send_data_payload(Payload::PeerMonitorsAnnounce(PeerMonitorsPayload {
                monitors: rects_to_wire(local_monitors),
            }))
            .await?;
        info!(%peer_id, count, "layout sync: PeerMonitorsAnnounce enviado");

        clipboard
            .send_data_payload(Payload::KvmLayoutShare(session_layout_to_wire(&session)))
            .await?;
        info!(
            %peer_id,
            devices = session.per_device.len(),
            "layout sync: KvmLayoutShare enviado (layout canônico)"
        );
        Ok(())
    }
}

fn peer_as_device(peer_id: PeerId) -> DeviceId {
    DeviceId::from_uuid(peer_id.as_uuid())
}

async fn load_session_from_store(
    store: &Arc<Mutex<Option<Arc<dyn KvmLayoutStore>>>>,
    peer_id: PeerId,
    local_device: DeviceId,
) -> SessionDesktopLayout {
    let store_ref = {
        let guard = store.lock().await;
        guard.as_ref().cloned()
    };
    let Some(s) = store_ref else {
        return SessionDesktopLayout::empty();
    };

    if let Ok(Some(session)) = s.get_session(peer_id).await {
        if !session.per_device.is_empty() {
            return session;
        }
    }

    if let Ok(Some(legacy)) = s.get(peer_id).await {
        return monitor_layout_to_session(&legacy, local_device);
    }

    SessionDesktopLayout::empty()
}

async fn persist_session(
    store: &Arc<Mutex<Option<Arc<dyn KvmLayoutStore>>>>,
    peer_id: PeerId,
    session: &SessionDesktopLayout,
) {
    if let Some(s) = store.lock().await.as_ref() {
        if let Err(err) = s.save_session(peer_id, session).await {
            warn!(?err, %peer_id, "layout sync: falha ao persistir session layout");
        }
    }
}

async fn apply_session_to_runtime(
    deps: &KvmLayoutSyncDeps,
    peer_id: PeerId,
    session: &SessionDesktopLayout,
) {
    let local_device = match *deps.local_device_id.lock().await {
        Some(id) => id,
        None => return,
    };
    let runtime = session.derive_runtime_layout(local_device, peer_id);
    let scale = runtime.remote_mouse_scale();
    deps.mouse_send
        .scale_q8
        .store((scale * 256.0).round() as i32, Ordering::SeqCst);
    *deps.layout.lock().await = Some(runtime);
}

pub async fn store_peer_monitors(
    store: &Arc<Mutex<Option<Arc<dyn KvmLayoutStore>>>>,
    bus: &EventBus,
    peer_id: PeerId,
    monitors: Vec<MonitorRect>,
) {
    let count = monitors.len();
    if count == 0 {
        warn!(%peer_id, "layout sync: announce recebido sem monitores — ignorado");
        return;
    }
    if let Some(s) = store.lock().await.as_ref() {
        match s.save_peer_monitors(peer_id, &monitors).await {
            Ok(()) => debug!(%peer_id, count, "layout sync: monitores persistidos no store"),
            Err(err) => warn!(?err, %peer_id, "layout sync: falha ao persistir monitores do peer"),
        }
    } else {
        warn!(%peer_id, "layout sync: kvm layout store não configurado — monitores não persistidos");
    }
    bus.publish(DomainEvent::PeerMonitorsUpdated(PeerMonitorsUpdated {
        peer_id,
        monitor_count: count,
    }));
    info!(
        %peer_id,
        count,
        monitors = %monitors_summary(&monitors),
        "layout sync: monitores do peer recebidos e publicados"
    );
}

async fn handle_layout_payload(deps: &KvmLayoutSyncDeps, peer_id: PeerId, payload: Payload) {
    let kind = payload_kind(&payload);
    debug!(%peer_id, payload = kind, "layout sync: frame recebido no handler");

    match payload {
        Payload::PeerMonitorsAnnounce(p) => {
            let monitors = rects_from_wire(&p.monitors);
            info!(
                %peer_id,
                count = monitors.len(),
                monitors = %monitors_summary(&monitors),
                "layout sync: processando PeerMonitorsAnnounce"
            );
            store_peer_monitors(&deps.store, &deps.bus, peer_id, monitors.clone()).await;

            let local_device = match *deps.local_device_id.lock().await {
                Some(id) => id,
                None => {
                    warn!(%peer_id, "layout sync: local_device_id ausente — announce ignorado para session");
                    return;
                }
            };
            let remote_device = peer_as_device(peer_id);
            let mut session = load_session_from_store(&deps.store, peer_id, local_device).await;
            let local_os = deps
                .monitors
                .enumerate_local_monitors()
                .await
                .unwrap_or_default();
            if !local_os.is_empty() {
                session.merge_announced_monitors(local_device, &local_os, None);
            }
            session.merge_announced_monitors(remote_device, &monitors, Some(local_device));
            persist_session(&deps.store, peer_id, &session).await;

            let input_enabled = *deps.enabled.lock().await;
            let active = *deps.active_peer.lock().await;
            if input_enabled && active == Some(peer_id) {
                apply_session_to_runtime(deps, peer_id, &session).await;
                info!(%peer_id, "layout sync: runtime atualizado após PeerMonitorsAnnounce");
            } else {
                debug!(
                    %peer_id,
                    input_enabled,
                    ?active,
                    "layout sync: session persistida; runtime inativo ou outro peer"
                );
            }
        }
        Payload::KvmLayoutShare(wire) => {
            let received = session_layout_from_wire(&wire);
            info!(
                %peer_id,
                devices = received.per_device.len(),
                "layout sync: processando KvmLayoutShare canônico"
            );

            persist_session(&deps.store, peer_id, &received).await;
            deps.bus
                .publish(DomainEvent::KvmLayoutUpdated(KvmLayoutUpdated { peer_id }));

            let session_active = *deps.enabled.lock().await
                && *deps.active_peer.lock().await == Some(peer_id);
            let focus_on_remote = matches!(
                deps.focus.lock().await.target,
                FocusTarget::Remote(p) if p == peer_id
            );

            if session_active && !focus_on_remote {
                apply_session_to_runtime(deps, peer_id, &received).await;
                info!(%peer_id, "layout sync: layout ativo substituído via KvmLayoutShare");
            } else if session_active && focus_on_remote {
                debug!(
                    %peer_id,
                    "layout sync: layout canônico persistido; runtime adiado (foco remoto ativo)"
                );
            } else {
                debug!(
                    %peer_id,
                    "layout sync: layout canônico salvo; runtime inativo ou outro peer"
                );
            }
        }
        other => {
            warn!(
                %peer_id,
                payload = payload_kind(&other),
                "layout sync: payload inesperado no handler"
            );
        }
    }
}

pub async fn broadcast_session_layout(
    deps: &KvmLayoutSyncDeps,
    clipboard: &ClipboardService,
    peer_id: PeerId,
    local_monitors: Vec<MonitorRect>,
    session: &SessionDesktopLayout,
) {
    info!(
        %peer_id,
        local_count = local_monitors.len(),
        devices = session.per_device.len(),
        "layout sync: broadcast session após save"
    );
    if let Err(err) = clipboard
        .send_data_payload(Payload::PeerMonitorsAnnounce(PeerMonitorsPayload {
            monitors: rects_to_wire(&local_monitors),
        }))
        .await
    {
        warn!(?err, %peer_id, "layout sync: falha ao broadcast PeerMonitorsAnnounce");
    }
    if let Err(err) = clipboard
        .send_data_payload(Payload::KvmLayoutShare(session_layout_to_wire(session)))
        .await
    {
        warn!(?err, %peer_id, "layout sync: falha ao broadcast KvmLayoutShare");
    }
    let _ = deps;
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicI32, AtomicU64};
    use std::sync::{Arc, Mutex as StdMutex};

    use tokio::sync::{Mutex, Notify};
    use uuid::Uuid;
    use winx_domain::input_control::{FocusState, MonitorId, SessionDesktopLayout};
    use winx_domain::shared::ids::DeviceId;
    use winx_protocol::PeerMonitorsPayload;

    use crate::use_cases::{input_control::MouseSendState, mouse_coalesce::MouseCoalescer};
    use super::*;

    struct MockKvmLayoutStore {
        peer_monitors: StdMutex<HashMap<PeerId, Vec<MonitorRect>>>,
        layouts: StdMutex<HashMap<PeerId, MonitorLayout>>,
        sessions: StdMutex<HashMap<PeerId, SessionDesktopLayout>>,
    }

    impl Default for MockKvmLayoutStore {
        fn default() -> Self {
            Self {
                peer_monitors: StdMutex::new(HashMap::new()),
                layouts: StdMutex::new(HashMap::new()),
                sessions: StdMutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl KvmLayoutStore for MockKvmLayoutStore {
        async fn get(&self, peer_id: PeerId) -> anyhow::Result<Option<MonitorLayout>> {
            Ok(self.layouts.lock().unwrap().get(&peer_id).cloned())
        }

        async fn save(&self, peer_id: PeerId, layout: &MonitorLayout) -> anyhow::Result<()> {
            self.layouts
                .lock()
                .unwrap()
                .insert(peer_id, layout.clone());
            Ok(())
        }

        async fn delete(&self, peer_id: PeerId) -> anyhow::Result<()> {
            self.layouts.lock().unwrap().remove(&peer_id);
            Ok(())
        }

        async fn get_peer_monitors(
            &self,
            peer_id: PeerId,
        ) -> anyhow::Result<Option<Vec<MonitorRect>>> {
            Ok(self.peer_monitors.lock().unwrap().get(&peer_id).cloned())
        }

        async fn save_peer_monitors(
            &self,
            peer_id: PeerId,
            monitors: &[MonitorRect],
        ) -> anyhow::Result<()> {
            self.peer_monitors
                .lock()
                .unwrap()
                .insert(peer_id, monitors.to_vec());
            Ok(())
        }

        async fn get_session(
            &self,
            peer_id: PeerId,
        ) -> anyhow::Result<Option<SessionDesktopLayout>> {
            Ok(self.sessions.lock().unwrap().get(&peer_id).cloned())
        }

        async fn save_session(
            &self,
            peer_id: PeerId,
            layout: &SessionDesktopLayout,
        ) -> anyhow::Result<()> {
            self.sessions
                .lock()
                .unwrap()
                .insert(peer_id, layout.clone());
            Ok(())
        }
    }

    fn remote_monitors_pair() -> Vec<MonitorRect> {
        vec![
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
        ]
    }

    use crate::ports::MonitorBackend;

    struct MockMonitors;

    #[async_trait::async_trait]
    impl MonitorBackend for MockMonitors {
        async fn enumerate_local_monitors(&self) -> anyhow::Result<Vec<MonitorRect>> {
            Ok(vec![MonitorRect {
                id: MonitorId(10),
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            }])
        }
    }

    fn test_deps(
        peer_id: PeerId,
        local_device: DeviceId,
        mock: Arc<MockKvmLayoutStore>,
        bus: EventBus,
    ) -> KvmLayoutSyncDeps {
        KvmLayoutSyncDeps {
            store: Arc::new(Mutex::new(Some(
                mock as Arc<dyn KvmLayoutStore>
            ))),
            bus,
            layout: Arc::new(Mutex::new(Some(MonitorLayout::default_side_by_side(
                vec![MonitorRect {
                    id: MonitorId(10),
                    x: 0,
                    y: 0,
                    width: 1920,
                    height: 1080,
                }],
                peer_id,
            )))),
            enabled: Arc::new(Mutex::new(true)),
            active_peer: Arc::new(Mutex::new(Some(peer_id))),
            mouse_send: Arc::new(MouseSendState {
                coalesce: Mutex::new(MouseCoalescer::new()),
                flush_notify: Notify::new(),
                scale_q8: AtomicI32::new(256),
                frames_sent: AtomicU64::new(0),
            }),
            monitors: Arc::new(MockMonitors),
            focus: Arc::new(Mutex::new(FocusState::default())),
            local_device_id: Arc::new(Mutex::new(Some(local_device))),
        }
    }

    #[tokio::test]
    async fn peer_monitors_announce_persists_publishes_and_updates_active_layout() {
        let peer_id = PeerId::from_uuid(Uuid::new_v4());
        let local_device = DeviceId::from_uuid(Uuid::new_v4());
        let monitors = remote_monitors_pair();
        let mock = Arc::new(MockKvmLayoutStore::default());
        let bus = EventBus::new();
        let mut rx = bus.subscribe();
        let deps = test_deps(peer_id, local_device, Arc::clone(&mock), bus);

        handle_layout_payload(
            &deps,
            peer_id,
            Payload::PeerMonitorsAnnounce(PeerMonitorsPayload {
                monitors: rects_to_wire(&monitors),
            }),
        )
        .await;

        let saved = mock
            .peer_monitors
            .lock()
            .unwrap()
            .get(&peer_id)
            .cloned()
            .expect("monitores persistidos");
        assert_eq!(saved.len(), 2);
        assert_eq!(saved, monitors);

        let evt = rx.recv().await.unwrap();
        match evt {
            DomainEvent::PeerMonitorsUpdated(e) => {
                assert_eq!(e.peer_id, peer_id);
                assert_eq!(e.monitor_count, 2);
            }
            other => panic!("evento inesperado: {other:?}"),
        }

        let session = mock
            .sessions
            .lock()
            .unwrap()
            .get(&peer_id)
            .cloned()
            .expect("session persistida");
        let remote_device = peer_as_device(peer_id);
        assert_eq!(session.device_monitors(remote_device).len(), 2);

        let active = deps.layout.lock().await;
        let active = active.as_ref().expect("layout ativo");
        assert_eq!(active.remote_monitors.len(), 2);
    }

    #[tokio::test]
    async fn kvm_layout_share_persists_session_and_publishes_event() {
        let peer_id = PeerId::from_uuid(Uuid::new_v4());
        let local_device = DeviceId::from_uuid(Uuid::new_v4());
        let remote_device = peer_as_device(peer_id);
        let mock = Arc::new(MockKvmLayoutStore::default());
        let bus = EventBus::new();
        let mut rx = bus.subscribe();
        let deps = test_deps(peer_id, local_device, Arc::clone(&mock), bus);

        let mut session = SessionDesktopLayout::empty();
        session.set_device_monitors(
            local_device,
            vec![MonitorRect {
                id: MonitorId(10),
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            }],
        );
        session.set_device_monitors(
            remote_device,
            vec![MonitorRect {
                id: MonitorId(1),
                x: 1920,
                y: 0,
                width: 1920,
                height: 1080,
            }],
        );

        handle_layout_payload(
            &deps,
            peer_id,
            Payload::KvmLayoutShare(session_layout_to_wire(&session)),
        )
        .await;

        let saved = mock
            .sessions
            .lock()
            .unwrap()
            .get(&peer_id)
            .cloned()
            .expect("session persistida");
        assert_eq!(saved.device_monitors(remote_device).len(), 1);

        loop {
            let evt = rx.recv().await.unwrap();
            if matches!(evt, DomainEvent::KvmLayoutUpdated(ref e) if e.peer_id == peer_id) {
                break;
            }
        }
    }
}
