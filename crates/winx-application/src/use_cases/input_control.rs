use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::{info, warn};
use winx_domain::{
    input_control::{
        events::{FocusSwitched, HotkeyTriggered, InputBlocked},
        FocusState, FocusTarget, HotkeyAction, InputEvent, MonitorLayout,
    },
    shared::{ids::PeerId, DomainErrorCode},
    transport::StreamKind,
    DomainError, DomainEvent,
};

use crate::{
    bus::EventBus,
    ports::{transport::StreamSender, InputBackend, MonitorBackend},
    protocol_convert::{encode_input_payload, input_event_from_dto},
    use_cases::TransportService,
};

pub struct InputControlService {
    focus: Arc<Mutex<FocusState>>,
    layout: Arc<Mutex<Option<MonitorLayout>>>,
    input: Arc<dyn InputBackend>,
    monitors: Arc<dyn MonitorBackend>,
    transport: Arc<TransportService>,
    bus: EventBus,
    active_peer: Arc<Mutex<Option<PeerId>>>,
    input_tx: Arc<Mutex<Option<StreamSender>>>,
    seq: Arc<AtomicU64>,
    enabled: Arc<Mutex<bool>>,
}

impl InputControlService {
    pub fn new(
        input: Arc<dyn InputBackend>,
        monitors: Arc<dyn MonitorBackend>,
        transport: Arc<TransportService>,
        bus: EventBus,
    ) -> Self {
        Self {
            focus: Arc::new(Mutex::new(FocusState::default())),
            layout: Arc::new(Mutex::new(None)),
            input,
            monitors,
            transport,
            bus,
            active_peer: Arc::new(Mutex::new(None)),
            input_tx: Arc::new(Mutex::new(None)),
            seq: Arc::new(AtomicU64::new(0)),
            enabled: Arc::new(Mutex::new(false)),
        }
    }

    pub async fn enable_for_peer(&self, peer_id: PeerId) -> Result<(), DomainError> {
        if *self.enabled.lock().await {
            let active = *self.active_peer.lock().await;
            if active == Some(peer_id) {
                return Ok(());
            }
        }

        if !self.transport.is_peer_connected(peer_id).await {
            return Err(DomainError::new(
                DomainErrorCode::TransportConnectionFailed,
                "peer não conectado via QUIC",
            ));
        }

        let local = self
            .monitors
            .enumerate_local_monitors()
            .await
            .map_err(|e| internal_err(&e.to_string()))?;

        let layout = MonitorLayout::default_side_by_side(local, peer_id);
        *self.layout.lock().await = Some(layout);
        *self.active_peer.lock().await = Some(peer_id);

        let (tx, mut rx) = self
            .transport
            .open_stream_for_peer(peer_id, StreamKind::Input)
            .await?;
        *self.input_tx.lock().await = Some(tx);

        let service_focus = Arc::clone(&self.focus);
        let service_layout = Arc::clone(&self.layout);
        let service_transport = Arc::clone(&self.transport);
        let service_bus = self.bus.clone();
        let service_input_tx = Arc::clone(&self.input_tx);
        let service_seq = Arc::clone(&self.seq);
        let service_active = Arc::clone(&self.active_peer);
        let service_self_input = Arc::clone(&self.input);
        let service_enabled = Arc::clone(&self.enabled);

        let on_event = move |ev: InputEvent| {
            let focus = service_focus.clone();
            let layout = service_layout.clone();
            let input_tx = service_input_tx.clone();
            let transport = service_transport.clone();
            let bus = service_bus.clone();
            let active = service_active.clone();
            let input_be = service_self_input.clone();
            let enabled = service_enabled.clone();
            let seq = Arc::clone(&service_seq);
            tokio::spawn(async move {
                if !*enabled.lock().await {
                    return;
                }
                handle_local_input(
                    ev, focus, layout, input_tx, transport, bus, active, input_be, &seq,
                )
                .await;
            });
        };

        let focus_hk = Arc::clone(&self.focus);
        let layout_hk = Arc::clone(&self.layout);
        let input_hk = Arc::clone(&self.input);
        let bus_hk = self.bus.clone();
        let active_hk = Arc::clone(&self.active_peer);
        let input_tx_hk = Arc::clone(&self.input_tx);

        self.input
            .start_capture(
                Box::new(on_event),
                Box::new(move |action| {
                    let focus = focus_hk.clone();
                    let layout = layout_hk.clone();
                    let input = input_hk.clone();
                    let bus = bus_hk.clone();
                    let active = active_hk.clone();
                    let input_tx = input_tx_hk.clone();
                    tokio::spawn(async move {
                        handle_hotkey(action, focus, layout, input, bus, active, input_tx).await;
                    });
                }),
            )
            .await
            .map_err(|e| internal_err(&e.to_string()))?;

        let inject_input = Arc::clone(&self.input);
        tokio::spawn(async move {
            while let Some(bytes) = rx.recv().await {
                if let Ok(frame) = winx_protocol::decode(&bytes) {
                    if let winx_protocol::Payload::Input(p) = frame.payload {
                        let ev = input_event_from_dto(&p.event);
                        if inject_input.inject(ev).await.is_err() {
                            warn!("falha ao injetar input remoto");
                        }
                    }
                }
            }
        });

        self.spawn_bus_subscriber();
        *self.enabled.lock().await = true;
        info!(%peer_id, "input control habilitado");
        Ok(())
    }

    pub async fn get_focus_state(&self) -> FocusState {
        self.focus.lock().await.clone()
    }

    fn spawn_bus_subscriber(&self) {
        let mut rx = self.bus.subscribe();
        let active = Arc::clone(&self.active_peer);
        let focus = Arc::clone(&self.focus);
        let input = Arc::clone(&self.input);

        tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                if let DomainEvent::ConnectionLost(e) = event {
                    let guard = active.lock().await;
                    if *guard == Some(e.peer_id) {
                        drop(guard);
                        let mut f = focus.lock().await;
                        if matches!(f.target, FocusTarget::Remote(pid) if pid == e.peer_id) {
                            f.target = FocusTarget::Local;
                            input.set_pass_through(true);
                            let _ = input.set_cursor_clipped(None).await;
                        }
                    }
                }
            }
        });
    }
}

async fn handle_hotkey(
    action: HotkeyAction,
    focus: Arc<Mutex<FocusState>>,
    layout: Arc<Mutex<Option<MonitorLayout>>>,
    input: Arc<dyn InputBackend>,
    bus: EventBus,
    active: Arc<Mutex<Option<PeerId>>>,
    input_tx: Arc<Mutex<Option<StreamSender>>>,
) {
    match action {
        HotkeyAction::PanicLocal => {
            panic_local(focus, layout, input, bus, active, input_tx).await;
        }
        HotkeyAction::ToggleLock => {
            let mut f = focus.lock().await;
            f.lock_mode = !f.lock_mode;
            bus.publish(DomainEvent::HotkeyTriggered(HotkeyTriggered {
                action: HotkeyAction::ToggleLock,
            }));
        }
    }
}

async fn panic_local(
    focus: Arc<Mutex<FocusState>>,
    layout: Arc<Mutex<Option<MonitorLayout>>>,
    input: Arc<dyn InputBackend>,
    bus: EventBus,
    _active: Arc<Mutex<Option<PeerId>>>,
    _input_tx: Arc<Mutex<Option<StreamSender>>>,
) {
    let from = focus.lock().await.target.clone();
    switch_focus(
        FocusTarget::Local,
        from,
        Arc::clone(&focus),
        Arc::clone(&layout),
        input,
        bus.clone(),
    )
    .await;
    bus.publish(DomainEvent::HotkeyTriggered(HotkeyTriggered {
        action: HotkeyAction::PanicLocal,
    }));
}

#[allow(clippy::too_many_arguments)]
async fn handle_local_input(
    ev: InputEvent,
    focus: Arc<Mutex<FocusState>>,
    layout: Arc<Mutex<Option<MonitorLayout>>>,
    input_tx: Arc<Mutex<Option<StreamSender>>>,
    _transport: Arc<TransportService>,
    bus: EventBus,
    active: Arc<Mutex<Option<PeerId>>>,
    input: Arc<dyn InputBackend>,
    seq: &Arc<AtomicU64>,
) {
    let state = focus.lock().await.clone();
    if state.lock_mode {
        return;
    }

    match &state.target {
        FocusTarget::Local => {
            input.set_pass_through(true);
            if let InputEvent::MouseMove {
                dx,
                dy,
                screen_x,
                screen_y: _,
            } = ev
            {
                if dx == 0 && dy == 0 {
                    return;
                }
                try_edge_switch(screen_x, focus, layout, input, bus, active, input_tx).await;
            }
        }
        FocusTarget::Remote(_peer) => {
            input.set_pass_through(false);
            if let Some(tx) = input_tx.lock().await.as_ref() {
                let n = seq.fetch_add(1, Ordering::SeqCst);
                if let Ok(bytes) = encode_input_payload(n, &ev) {
                    let len = u32::try_from(bytes.len()).unwrap_or(u32::MAX).to_be_bytes();
                    let mut frame = len.to_vec();
                    frame.extend(bytes);
                    let _ = tx.send(frame).await;
                }
            }
        }
    }
}

async fn try_edge_switch(
    screen_x: i32,
    focus: Arc<Mutex<FocusState>>,
    layout: Arc<Mutex<Option<MonitorLayout>>>,
    input: Arc<dyn InputBackend>,
    bus: EventBus,
    active: Arc<Mutex<Option<PeerId>>>,
    input_tx: Arc<Mutex<Option<StreamSender>>>,
) {
    let layout_guard = layout.lock().await;
    let Some(layout_data) = layout_guard.as_ref() else {
        return;
    };
    let edge = layout_data.local_right_edge_x();
    if screen_x < edge - 2 {
        return;
    }
    let Some(peer) = *active.lock().await else {
        return;
    };
    let remote = layout_data.remote_virtual;
    drop(layout_guard);

    let from = focus.lock().await.target.clone();
    switch_focus(
        FocusTarget::Remote(peer),
        from,
        Arc::clone(&focus),
        Arc::clone(&layout),
        Arc::clone(&input),
        bus,
    )
    .await;

    let _ = input
        .set_cursor_clipped(Some((
            remote.x + i32::try_from(remote.width).unwrap_or(i32::MAX) / 2,
            remote.y + i32::try_from(remote.height).unwrap_or(i32::MAX) / 2,
            1,
            1,
        )))
        .await;
    let _ = input_tx;
}

async fn switch_focus(
    to: FocusTarget,
    from: FocusTarget,
    focus: Arc<Mutex<FocusState>>,
    layout: Arc<Mutex<Option<MonitorLayout>>>,
    input: Arc<dyn InputBackend>,
    bus: EventBus,
) {
    {
        let mut f = focus.lock().await;
        f.target = to.clone();
    }

    match &to {
        FocusTarget::Local => {
            input.set_pass_through(true);
            let _ = input.set_cursor_clipped(None).await;
        }
        FocusTarget::Remote(_) => {
            input.set_pass_through(false);
        }
    }

    bus.publish(DomainEvent::FocusSwitched(FocusSwitched {
        from,
        to: to.clone(),
    }));
    bus.publish(DomainEvent::InputBlocked(InputBlocked {
        blocked: matches!(to, FocusTarget::Remote(_)),
        peer_id: match to {
            FocusTarget::Remote(p) => Some(p),
            FocusTarget::Local => None,
        },
    }));
    let _ = layout;
}

fn internal_err(msg: &str) -> DomainError {
    DomainError::new(DomainErrorCode::InternalError, msg)
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;
    use winx_domain::input_control::MonitorRect;

    use super::*;

    struct MockInput;

    #[async_trait::async_trait]
    impl InputBackend for MockInput {
        async fn start_capture(
            &self,
            _: Box<dyn Fn(InputEvent) + Send + Sync>,
            _: Box<dyn Fn(HotkeyAction) + Send + Sync>,
        ) -> anyhow::Result<crate::ports::CaptureHandle> {
            Ok(crate::ports::CaptureHandle { id: 1 })
        }
        async fn stop_capture(&self, _: crate::ports::CaptureHandle) -> anyhow::Result<()> {
            Ok(())
        }
        async fn inject(&self, _: InputEvent) -> anyhow::Result<()> {
            Ok(())
        }
        async fn set_cursor_clipped(&self, _: Option<(i32, i32, u32, u32)>) -> anyhow::Result<()> {
            Ok(())
        }
        fn set_pass_through(&self, _: bool) {}
    }

    #[tokio::test]
    async fn crossing_right_edge_switches_focus_to_remote() {
        let peer_id = PeerId::from_uuid(Uuid::new_v4());
        let bus = EventBus::new();
        let svc = InputControlService::new(
            Arc::new(MockInput),
            Arc::new(MockMonitors),
            Arc::new(TransportService::new(
                Arc::new(NoopTransport),
                Arc::new(NoopIdentity),
                Arc::new(Mutex::new(winx_domain::discovery::DiscoveryRegistry::new())),
                bus.clone(),
            )),
            bus,
        );

        *svc.active_peer.lock().await = Some(peer_id);
        *svc.layout.lock().await = Some(MonitorLayout::default_side_by_side(
            vec![MonitorRect {
                id: winx_domain::input_control::MonitorId(1),
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            }],
            peer_id,
        ));

        try_edge_switch(
            1920,
            Arc::clone(&svc.focus),
            Arc::clone(&svc.layout),
            Arc::new(MockInput),
            svc.bus.clone(),
            Arc::clone(&svc.active_peer),
            Arc::clone(&svc.input_tx),
        )
        .await;

        let f = svc.get_focus_state().await;
        assert!(matches!(f.target, FocusTarget::Remote(_)));
    }

    struct MockMonitors;

    #[async_trait::async_trait]
    impl MonitorBackend for MockMonitors {
        async fn enumerate_local_monitors(&self) -> anyhow::Result<Vec<MonitorRect>> {
            Ok(vec![])
        }
    }

    struct NoopTransport;

    #[async_trait::async_trait]
    impl crate::ports::TransportAdapter for NoopTransport {
        async fn listen(
            &self,
            _: u16,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<crate::ports::transport::IncomingConnection>>
        {
            let (_, rx) = tokio::sync::mpsc::channel(1);
            Ok(rx)
        }
        async fn connect(
            &self,
            _: std::net::SocketAddr,
            _: [u8; 32],
        ) -> anyhow::Result<crate::ports::transport::ActiveConnection> {
            Ok(crate::ports::transport::ActiveConnection {
                conn_id: winx_domain::shared::ids::SessionId::new(),
            })
        }
        async fn open_stream(
            &self,
            _: winx_domain::shared::ids::SessionId,
            _: StreamKind,
        ) -> anyhow::Result<(
            crate::ports::transport::StreamSender,
            crate::ports::transport::StreamReceiver,
        )> {
            let (tx, rx) = tokio::sync::mpsc::channel(8);
            Ok((tx, rx))
        }
        async fn get_stats(
            &self,
            _: winx_domain::shared::ids::SessionId,
        ) -> anyhow::Result<winx_domain::transport::ConnectionStats> {
            Ok(winx_domain::transport::ConnectionStats::default())
        }
        async fn close(
            &self,
            _: winx_domain::shared::ids::SessionId,
            _: &str,
        ) -> anyhow::Result<()> {
            Ok(())
        }
        async fn add_trusted_key(&self, _: [u8; 32]) {}
    }

    struct NoopIdentity;

    #[async_trait::async_trait]
    impl crate::ports::IdentityStore for NoopIdentity {
        async fn load_device(&self) -> anyhow::Result<Option<winx_domain::identity::Device>> {
            Ok(None)
        }
        async fn save_device(&self, _: &winx_domain::identity::Device) -> anyhow::Result<()> {
            Ok(())
        }
        async fn load_peers(&self) -> anyhow::Result<Vec<winx_domain::identity::TrustedPeer>> {
            Ok(vec![])
        }
        async fn save_peer(&self, _: &winx_domain::identity::TrustedPeer) -> anyhow::Result<()> {
            Ok(())
        }
        async fn remove_peer(&self, _: winx_domain::shared::ids::PeerId) -> anyhow::Result<()> {
            Ok(())
        }
    }
}
