//! Sync de layout/monitores via stream Data compartilhado com clipboard.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::info;
use winx_domain::{
    input_control::{events::PeerMonitorsUpdated, MonitorLayout, MonitorRect},
    shared::ids::PeerId,
    DomainEvent,
};
use winx_protocol::{Payload, PeerMonitorsPayload};

use crate::{
    bus::EventBus,
    ports::KvmLayoutStore,
    use_cases::clipboard::{ClipboardService, LayoutDataHandler},
    workspace_layout_wire::{monitor_layout_to_wire, rects_from_wire, rects_to_wire},
};

use super::input_control::MouseSendState;

pub struct KvmLayoutSyncDeps {
    pub store: Arc<Mutex<Option<Arc<dyn KvmLayoutStore>>>>,
    pub bus: EventBus,
    pub layout: Arc<Mutex<Option<MonitorLayout>>>,
    pub enabled: Arc<Mutex<bool>>,
    pub active_peer: Arc<Mutex<Option<PeerId>>>,
    pub mouse_send: Arc<MouseSendState>,
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
    }

    pub async fn announce_to_peer(
        &self,
        clipboard: &ClipboardService,
        peer_id: PeerId,
        local_monitors: &[MonitorRect],
    ) -> Result<(), winx_domain::DomainError> {
        let saved = {
            let guard = self.store.lock().await;
            if let Some(s) = guard.as_ref() {
                s.get(peer_id).await.ok().flatten()
            } else {
                None
            }
        };

        clipboard
            .send_data_payload(Payload::PeerMonitorsAnnounce(PeerMonitorsPayload {
                monitors: rects_to_wire(local_monitors),
            }))
            .await?;

        if let Some(layout) = saved.as_ref() {
            clipboard
                .send_data_payload(Payload::KvmLayoutShare(monitor_layout_to_wire(layout)))
                .await?;
        }
        Ok(())
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
            tracing::warn!(?err, %peer_id, "falha ao persistir monitores do peer");
        }
    }
    bus.publish(DomainEvent::PeerMonitorsUpdated(PeerMonitorsUpdated {
        peer_id,
        monitor_count: count,
    }));
    info!(%peer_id, count, "monitores do peer recebidos via sync");
}

async fn handle_layout_payload(deps: &KvmLayoutSyncDeps, peer_id: PeerId, payload: Payload) {
    match payload {
        Payload::PeerMonitorsAnnounce(p) => {
            let monitors = rects_from_wire(&p.monitors);
            store_peer_monitors(&deps.store, &deps.bus, peer_id, monitors.clone()).await;
            if *deps.enabled.lock().await && *deps.active_peer.lock().await == Some(peer_id) {
                let mut current = deps.layout.lock().await;
                if let Some(ref mut active) = *current {
                    active.remote_monitors = monitors;
                    active.infer_edges_from_geometry();
                    let scale = active.remote_mouse_scale();
                    deps.mouse_send
                        .scale_q8
                        .store((scale * 256.0).round() as i32, Ordering::SeqCst);
                }
            }
        }
        Payload::KvmLayoutShare(wire) => {
            let received = crate::workspace_layout_wire::monitor_layout_from_wire(&wire);
            if !received.local_monitors.is_empty() {
                store_peer_monitors(
                    &deps.store,
                    &deps.bus,
                    peer_id,
                    received.local_monitors.clone(),
                )
                .await;
            }
            if *deps.enabled.lock().await && *deps.active_peer.lock().await == Some(peer_id) {
                let mut current = deps.layout.lock().await;
                if let Some(ref mut active) = *current {
                    active.remote_monitors = received.local_monitors;
                    active.infer_edges_from_geometry();
                    let scale = active.remote_mouse_scale();
                    deps.mouse_send
                        .scale_q8
                        .store((scale * 256.0).round() as i32, Ordering::SeqCst);
                    info!(%peer_id, "layout remoto atualizado a partir de KvmLayoutShare");
                }
            }
        }
        _ => {}
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

pub async fn broadcast_kvm_layout(
    deps: &KvmLayoutSyncDeps,
    clipboard: &ClipboardService,
    peer_id: PeerId,
    local_monitors: Vec<MonitorRect>,
    layout: &MonitorLayout,
) {
    let _ = clipboard
        .send_data_payload(Payload::PeerMonitorsAnnounce(PeerMonitorsPayload {
            monitors: rects_to_wire(&local_monitors),
        }))
        .await;
    let _ = clipboard
        .send_data_payload(Payload::KvmLayoutShare(monitor_layout_to_wire(layout)))
        .await;
    let _ = (deps, peer_id);
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicI32, AtomicU64};
    use std::sync::{Arc, Mutex as StdMutex};

    use tokio::sync::{Mutex, Notify};
    use uuid::Uuid;
    use winx_domain::input_control::MonitorId;
    use winx_protocol::PeerMonitorsPayload;

    use crate::use_cases::{input_control::MouseSendState, mouse_coalesce::MouseCoalescer};
    use super::*;

    struct MockKvmLayoutStore {
        peer_monitors: StdMutex<HashMap<PeerId, Vec<MonitorRect>>>,
        layouts: StdMutex<HashMap<PeerId, MonitorLayout>>,
    }

    impl Default for MockKvmLayoutStore {
        fn default() -> Self {
            Self {
                peer_monitors: StdMutex::new(HashMap::new()),
                layouts: StdMutex::new(HashMap::new()),
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

    fn test_deps(
        peer_id: PeerId,
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
        }
    }

    #[tokio::test]
    async fn peer_monitors_announce_persists_publishes_and_updates_active_layout() {
        let peer_id = PeerId::from_uuid(Uuid::new_v4());
        let monitors = remote_monitors_pair();
        let mock = Arc::new(MockKvmLayoutStore::default());
        let bus = EventBus::new();
        let mut rx = bus.subscribe();
        let deps = test_deps(peer_id, Arc::clone(&mock), bus);

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

        let active = deps.layout.lock().await;
        let active = active.as_ref().expect("layout ativo");
        assert_eq!(active.remote_monitors.len(), 2);
        assert_eq!(active.remote_monitors, monitors);

        let mut expected = MonitorLayout::default_side_by_side(
            vec![MonitorRect {
                id: MonitorId(10),
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            }],
            peer_id,
        );
        expected.remote_monitors = monitors;
        expected.infer_edges_from_geometry();
        assert_eq!(active.edge, expected.edge);
    }
}
