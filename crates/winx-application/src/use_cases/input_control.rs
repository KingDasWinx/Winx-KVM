use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::Mutex;
use tracing::{error, info, warn};
use winx_domain::{
    input_control::{
        accumulate_return_left, apply_focus_target, events::{FocusSwitched, HotkeyTriggered, InputBlocked},
        should_switch_to_remote, toggle_lock_mode, EdgeDetectInput, FocusState, FocusTarget,
        HotkeyAction, InputEvent, MonitorLayout,
    },
    shared::{ids::PeerId, DomainErrorCode},
    DomainError, DomainEvent,
};

use crate::{
    bus::EventBus,
    ports::{transport::StreamSender, InputBackend, MonitorBackend},
    protocol_convert::{encode_input_payload, input_event_from_dto},
    use_cases::{input_streams, TransportService},
};

static FIRST_INJECT: AtomicBool = AtomicBool::new(true);

#[derive(Debug, Clone, Serialize)]
pub struct KeyboardMirrorStatus {
    pub active: bool,
    pub seconds_left: u32,
    pub keys_sent: u64,
}

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
    remote_dx_accum: Arc<AtomicI32>,
    keyboard_mirror: Arc<AtomicBool>,
    mirror_keys_sent: Arc<AtomicU64>,
    mirror_deadline: Arc<Mutex<Option<Instant>>>,
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
            remote_dx_accum: Arc::new(AtomicI32::new(0)),
            keyboard_mirror: Arc::new(AtomicBool::new(false)),
            mirror_keys_sent: Arc::new(AtomicU64::new(0)),
            mirror_deadline: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn start_keyboard_mirror(
        &self,
        peer_id: PeerId,
        duration_secs: u32,
    ) -> Result<(), DomainError> {
        if !self.transport.is_peer_connected(peer_id).await {
            return Err(DomainError::new(
                DomainErrorCode::TransportConnectionFailed,
                "peer não conectado via QUIC",
            ));
        }

        if !*self.enabled.lock().await {
            self.enable_for_peer(peer_id).await?;
        } else {
            let active = *self.active_peer.lock().await;
            if active != Some(peer_id) {
                return Err(DomainError::new(
                    DomainErrorCode::InternalError,
                    "input control ativo para outro peer",
                ));
            }
        }

        let duration = Duration::from_secs(u64::from(duration_secs.max(1)));
        self.mirror_keys_sent.store(0, Ordering::SeqCst);
        *self.mirror_deadline.lock().await = Some(Instant::now() + duration);
        self.keyboard_mirror.store(true, Ordering::SeqCst);

        let mirror_flag = Arc::clone(&self.keyboard_mirror);
        let deadline_store = Arc::clone(&self.mirror_deadline);
        tokio::spawn(async move {
            tokio::time::sleep(duration).await;
            mirror_flag.store(false, Ordering::SeqCst);
            *deadline_store.lock().await = None;
            info!("keyboard mirror encerrado");
        });

        info!(%peer_id, duration_secs, "keyboard mirror iniciado");
        Ok(())
    }

    pub async fn get_keyboard_mirror_status(&self) -> KeyboardMirrorStatus {
        let active = self.keyboard_mirror.load(Ordering::SeqCst);
        let keys_sent = self.mirror_keys_sent.load(Ordering::SeqCst);
        let seconds_left = if active {
            let guard = self.mirror_deadline.lock().await;
            guard
                .map(|deadline| {
                    deadline
                        .saturating_duration_since(Instant::now())
                        .as_secs()
                })
                .unwrap_or(0)
        } else {
            0
        };
        KeyboardMirrorStatus {
            active,
            seconds_left: u32::try_from(seconds_left).unwrap_or(0),
            keys_sent,
        }
    }

    pub async fn send_test_click(&self, peer_id: PeerId) -> Result<(), DomainError> {
        if !self.transport.is_peer_connected(peer_id).await {
            return Err(DomainError::new(
                DomainErrorCode::TransportConnectionFailed,
                "peer não conectado via QUIC",
            ));
        }
        if !*self.enabled.lock().await {
            self.enable_for_peer(peer_id).await?;
        }

        let events = [
            InputEvent::MouseButton {
                button: winx_domain::input_control::MouseButton::Left,
                pressed: true,
            },
            InputEvent::MouseButton {
                button: winx_domain::input_control::MouseButton::Left,
                pressed: false,
            },
        ];

        let input_tx = self.input_tx.lock().await;
        let Some(tx) = input_tx.as_ref() else {
            return Err(DomainError::new(
                DomainErrorCode::TransportConnectionFailed,
                "stream Input não disponível",
            ));
        };

        for ev in events {
            let n = self.seq.fetch_add(1, Ordering::SeqCst);
            let bytes = encode_input_payload(n, &ev).map_err(|e| internal_err(&e.to_string()))?;
            tx.send(bytes)
                .await
                .map_err(|_| internal_err("falha ao enviar clique de teste"))?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
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

        let (tx, mut rx) = input_streams::acquire_input_streams(&self.transport, peer_id).await?;
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
                let service_remote_dx = Arc::clone(&self.remote_dx_accum);
        let service_mirror = Arc::clone(&self.keyboard_mirror);
        let service_mirror_keys = Arc::clone(&self.mirror_keys_sent);

        // Hooks Win32 disparam em std::thread — não usar `tokio::spawn` direto no callback.
        let runtime = tokio::runtime::Handle::current();

        let on_event = {
            let runtime = runtime.clone();
            move |ev: InputEvent| {
                let focus = service_focus.clone();
                let layout = service_layout.clone();
                let input_tx = service_input_tx.clone();
                let transport = service_transport.clone();
                let bus = service_bus.clone();
                let active = service_active.clone();
                let input_be = service_self_input.clone();
                let enabled = service_enabled.clone();
                let remote_dx = service_remote_dx.clone();
                let mirror = service_mirror.clone();
                let mirror_keys = service_mirror_keys.clone();
                let seq = Arc::clone(&service_seq);
                runtime.spawn(async move {
                    if !*enabled.lock().await && !mirror.load(Ordering::SeqCst) {
                        return;
                    }
                    handle_local_input(
                        ev,
                        focus,
                        layout,
                        input_tx,
                        transport,
                        bus,
                        active,
                        input_be,
                        &seq,
                        remote_dx,
                        mirror,
                        mirror_keys,
                    )
                    .await;
                });
            }
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
                {
                    let runtime = runtime.clone();
                    Box::new(move |action| {
                        let focus = focus_hk.clone();
                        let layout = layout_hk.clone();
                        let input = input_hk.clone();
                        let bus = bus_hk.clone();
                        let active = active_hk.clone();
                        let input_tx = input_tx_hk.clone();
                        runtime.spawn(async move {
                            handle_hotkey(action, focus, layout, input, bus, active, input_tx)
                                .await;
                        });
                    })
                },
            )
            .await
            .map_err(|e| internal_err(&e.to_string()))?;

        FIRST_INJECT.store(true, Ordering::SeqCst);
        self.remote_dx_accum.store(0, Ordering::SeqCst);

        let inject_input = Arc::clone(&self.input);
        tokio::spawn(async move {
            while let Some(bytes) = rx.recv().await {
                match winx_protocol::decode(&bytes) {
                    Ok(frame) => {
                        if let winx_protocol::Payload::Input(p) = frame.payload {
                            if FIRST_INJECT.swap(false, Ordering::SeqCst) {
                                info!("primeiro evento de input remoto recebido");
                            }
                            let ev = input_event_from_dto(&p.event);
                            tracing::debug!(?ev, "input remoto injetado");
                            if inject_input.inject(ev).await.is_err() {
                                warn!("falha ao injetar input remoto");
                            }
                        }
                    }
                    Err(err) => {
                        warn!(?err, len = bytes.len(), "frame de input inválido");
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

    /// Restaura cursor/foco local após desconexão (libera `ClipCursor`).
    pub async fn reset_after_disconnect(&self, peer_id: PeerId) {
        let was_active = {
            let guard = self.active_peer.lock().await;
            *guard == Some(peer_id)
        };
        if !was_active {
            return;
        }

        *self.active_peer.lock().await = None;
        *self.enabled.lock().await = false;
        *self.input_tx.lock().await = None;
        self.remote_dx_accum.store(0, Ordering::SeqCst);

        {
            let mut f = self.focus.lock().await;
            f.target = FocusTarget::Local;
            f.lock_mode = false;
        }

        self.input.set_pass_through(true);
        if self.input.set_cursor_clipped(None).await.is_err() {
            warn!("falha ao liberar ClipCursor após desconexão");
        } else {
            let _ = self.input.set_cursor_visible(true).await;
            self.input.reset_mouse_delta_baseline();
            info!(%peer_id, "input local restaurado após desconexão");
        }
        FIRST_INJECT.store(true, Ordering::SeqCst);
    }

    fn spawn_bus_subscriber(&self) {
        let mut rx = self.bus.subscribe();
        let active = Arc::clone(&self.active_peer);
        let focus = Arc::clone(&self.focus);
        let input = Arc::clone(&self.input);
        let input_tx = Arc::clone(&self.input_tx);
        let enabled = Arc::clone(&self.enabled);
        let remote_dx = Arc::clone(&self.remote_dx_accum);

        tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                if let DomainEvent::ConnectionLost(e) = event {
                    let guard = active.lock().await;
                    if *guard == Some(e.peer_id) {
                        drop(guard);
                        *active.lock().await = None;
                        *enabled.lock().await = false;
                        *input_tx.lock().await = None;
                        remote_dx.store(0, Ordering::SeqCst);
                        let mut f = focus.lock().await;
                        f.target = FocusTarget::Local;
                        f.lock_mode = false;
                        input.set_pass_through(true);
                        let _ = input.set_cursor_clipped(None).await;
                        input.reset_mouse_delta_baseline();
                        FIRST_INJECT.store(true, Ordering::SeqCst);
                        info!(peer_id = %e.peer_id, "foco e cursor restaurados (connection lost)");
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
            let _ = toggle_lock_mode(&mut f);
            bus.publish(DomainEvent::HotkeyTriggered(HotkeyTriggered {
                action: HotkeyAction::ToggleLock,
            }));
        }
        HotkeyAction::ForceReset => {
            force_local_reset(focus, layout, input, bus).await;
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
    let input_warp = Arc::clone(&input);
    let layout_warp = Arc::clone(&layout);
    switch_focus(
        FocusTarget::Local,
        from,
        Arc::clone(&focus),
        Arc::clone(&layout),
        input,
        bus.clone(),
    )
    .await;
    if let Some(layout) = layout_warp.lock().await.as_ref() {
        let x = layout.local_right_edge_x().saturating_sub(8);
        let y = layout.local_monitors.first().map_or(540, |m| {
            m.y + i32::try_from(m.height).unwrap_or(1080) / 2
        });
        if input_warp.warp_cursor(x, y).await.is_err() {
            warn!("falha ao reposicionar cursor no panic local");
        }
    }
    input_warp.reset_mouse_delta_baseline();
    bus.publish(DomainEvent::HotkeyTriggered(HotkeyTriggered {
        action: HotkeyAction::PanicLocal,
    }));
}

async fn force_local_reset(
    focus: Arc<Mutex<FocusState>>,
    layout: Arc<Mutex<Option<MonitorLayout>>>,
    input: Arc<dyn InputBackend>,
    bus: EventBus,
) {
    // Force reset: libera tudo e volta pra local, ignorando estado de conexão.
    {
        let mut f = focus.lock().await;
        f.target = FocusTarget::Local;
        f.lock_mode = false;
    }

    input.set_pass_through(true);
    let _ = input.set_cursor_clipped(None).await;
    let _ = input.set_cursor_visible(true).await;
    input.reset_mouse_delta_baseline();

    if let Some(layout) = layout.lock().await.as_ref() {
        let x = layout.local_right_edge_x().saturating_sub(8);
        let y = layout.local_monitors.first().map_or(540, |m| {
            m.y + i32::try_from(m.height).unwrap_or(1080) / 2
        });
        let _ = input.warp_cursor(x, y).await;
    }

    bus.publish(DomainEvent::HotkeyTriggered(HotkeyTriggered {
        action: HotkeyAction::ForceReset,
    }));
    info!("force reset ativado — foco e cursor restaurados");
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
    remote_dx_accum: Arc<AtomicI32>,
    keyboard_mirror: Arc<AtomicBool>,
    mirror_keys_sent: Arc<AtomicU64>,
) {
    if keyboard_mirror.load(Ordering::SeqCst) {
        if let InputEvent::Key { .. } = &ev {
            if let Some(tx) = input_tx.lock().await.as_ref() {
                let n = seq.fetch_add(1, Ordering::SeqCst);
                if let Ok(bytes) = encode_input_payload(n, &ev) {
                    if tx.send(bytes).await.is_ok() {
                        mirror_keys_sent.fetch_add(1, Ordering::SeqCst);
                    }
                }
            }
        }
        return;
    }

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
                let layout_guard = layout.lock().await;
                if let Some(layout_data) = layout_guard.as_ref() {
                    if should_switch_to_remote(
                        EdgeDetectInput {
                            screen_x,
                            lock_mode: state.lock_mode,
                        },
                        layout_data,
                    ) {
                        drop(layout_guard);
                        try_edge_switch(screen_x, focus, layout, input, bus, active, input_tx)
                            .await;
                    }
                }
            }
        }
        FocusTarget::Remote(_peer) => {
            input.set_pass_through(false);
            if let InputEvent::MouseMove { dx, .. } = &ev {
                let current = remote_dx_accum.load(Ordering::SeqCst);
                let (new_acc, go_back) = accumulate_return_left(*dx, current);
                remote_dx_accum.store(new_acc, Ordering::SeqCst);
                if go_back {
                    try_switch_back_to_local(
                        Arc::clone(&focus),
                        Arc::clone(&layout),
                        Arc::clone(&input),
                        bus.clone(),
                        Arc::clone(&active),
                    )
                    .await;
                    return;
                }
            }
            // Enviar input para o remoto
            if let Some(tx) = input_tx.lock().await.as_ref() {
                let n = seq.fetch_add(1, Ordering::SeqCst);
                if let Ok(bytes) = encode_input_payload(n, &ev) {
                    tracing::debug!(?ev, seq=n, "input enviado ao remoto peer");
                    if tx.send(bytes).await.is_err() {
                        warn!("falha ao enviar input no stream");
                    }
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
    if !should_switch_to_remote(
        EdgeDetectInput {
            screen_x,
            lock_mode: focus.lock().await.lock_mode,
        },
        layout_data,
    ) {
        return;
    }
    let Some(peer) = *active.lock().await else {
        return;
    };

    let current = focus.lock().await.target.clone();
    if matches!(&current, FocusTarget::Remote(p) if *p == peer) {
        return;
    }

    let edge_x = layout_data.local_right_edge_x();
    let primary = layout_data.local_monitors.first().copied().unwrap_or_else(|| {
        use winx_domain::input_control::MonitorRect;
        MonitorRect {
            id: winx_domain::input_control::MonitorId(0),
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        }
    });

    drop(layout_guard);

    info!(
        %screen_x,
        edge = edge_x,
        %peer,
        "borda direita atingida — trocando foco para remoto"
    );

    let from = current;
    let safe_x = primary.x + i32::try_from(primary.width).unwrap_or(1920) / 2;
    let safe_y = primary.y + i32::try_from(primary.height).unwrap_or(1080) / 2;

    // 1. Transição FÍSICA de cursor PRIMEIRO (Hide → Warp → Clip)
    // Se falhar, não mudar foco
    if let Err(err) = input.transition_to_remote(safe_x, safe_y).await {
        error!(?err, "falha na transição para remoto — mantendo foco local");
        let _ = input.set_cursor_clipped(None).await;
        return;
    }

    // 2. Transição LÓGICA de foco — só após sucesso físico
    switch_focus(
        FocusTarget::Remote(peer),
        from,
        Arc::clone(&focus),
        Arc::clone(&layout),
        Arc::clone(&input),
        bus,
    )
    .await;

    let _ = input_tx;
}

async fn try_switch_back_to_local(
    focus: Arc<Mutex<FocusState>>,
    layout: Arc<Mutex<Option<MonitorLayout>>>,
    input: Arc<dyn InputBackend>,
    bus: EventBus,
    _active: Arc<Mutex<Option<PeerId>>>,
) {
    let current = focus.lock().await.target.clone();
    let FocusTarget::Remote(_peer) = &current else {
        return;
    };

    let layout_guard = layout.lock().await;
    let Some(layout_data) = layout_guard.as_ref() else {
        return;
    };

    let edge_x = layout_data.local_right_edge_x();
    let primary = layout_data.local_monitors.first().copied().unwrap_or_else(|| {
        use winx_domain::input_control::MonitorRect;
        MonitorRect {
            id: winx_domain::input_control::MonitorId(0),
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        }
    });
    let primary_center_x = primary.x + i32::try_from(primary.width).unwrap_or(1920) / 2;
    let primary_center_y = primary.y + i32::try_from(primary.height).unwrap_or(1080) / 2;

    drop(layout_guard);

    if let Err(err) = input.transition_to_local(edge_x, primary_center_x, primary_center_y).await {
        warn!(?err, "falha ao retornar para local via borda esquerda");
        return;
    }

    info!("voltando para foco local via borda esquerda acumulada");

    switch_focus(
        FocusTarget::Local,
        current,
        focus,
        layout,
        input,
        bus,
    )
    .await;
}

async fn switch_focus(
    to: FocusTarget,
    _from: FocusTarget,
    focus: Arc<Mutex<FocusState>>,
    layout: Arc<Mutex<Option<MonitorLayout>>>,
    input: Arc<dyn InputBackend>,
    bus: EventBus,
) {
    let transition = {
        let mut f = focus.lock().await;
        apply_focus_target(&mut f, to.clone())
    };

    match &to {
        FocusTarget::Local => {
            input.set_pass_through(true);
            let _ = input.set_cursor_clipped(None).await;
            let _ = input.set_cursor_visible(true).await;
            input.reset_mouse_delta_baseline();
        }
        FocusTarget::Remote(_) => {
            input.set_pass_through(false);
        }
    }

    bus.publish(DomainEvent::FocusSwitched(FocusSwitched {
        from: transition.from,
        to: transition.to,
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
    use winx_domain::transport::StreamKind;

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
        async fn warp_cursor(&self, _: i32, _: i32) -> anyhow::Result<()> {
            Ok(())
        }
        fn reset_mouse_delta_baseline(&self) {}
        async fn set_cursor_visible(&self, _: bool) -> anyhow::Result<()> {
            Ok(())
        }
        async fn hide_cursor_system(&self) -> anyhow::Result<()> {
            Ok(())
        }
        async fn restore_cursor_system(&self) -> anyhow::Result<()> {
            Ok(())
        }
        async fn warp_cursor_signed(&self, _: i32, _: i32) -> anyhow::Result<()> {
            Ok(())
        }
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

    #[tokio::test]
    async fn lock_mode_prevents_edge_switch() {
        let peer_id = PeerId::from_uuid(Uuid::new_v4());
        let bus = EventBus::new();
        let focus = Arc::new(Mutex::new(FocusState::default()));
        {
            let mut f = focus.lock().await;
            f.lock_mode = true;
        }
        let layout = Arc::new(Mutex::new(Some(MonitorLayout::default_side_by_side(
            vec![MonitorRect {
                id: winx_domain::input_control::MonitorId(1),
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            }],
            peer_id,
        ))));
        let active = Arc::new(Mutex::new(Some(peer_id)));
        let input_tx = Arc::new(Mutex::new(None));

        try_edge_switch(
            1920,
            Arc::clone(&focus),
            Arc::clone(&layout),
            Arc::new(MockInput),
            bus,
            active,
            input_tx,
        )
        .await;

        let f = focus.lock().await;
        assert!(matches!(f.target, FocusTarget::Local));
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
            let (_, rx) = tokio::sync::mpsc::channel(8);
            Ok(crate::ports::transport::ActiveConnection {
                conn_id: winx_domain::shared::ids::SessionId::new(),
                inbound_streams: rx,
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

        async fn probe_control_heartbeat(
            &self,
            _: winx_domain::shared::ids::SessionId,
        ) -> anyhow::Result<u32> {
            Ok(1)
        }
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
