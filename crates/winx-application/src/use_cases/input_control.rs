use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::{Mutex, Notify};
use tracing::{debug, error, info, warn};
use winx_domain::{
    input_control::{
        apply_focus_target,
        events::{FocusSwitched, HotkeyTriggered, InputBlocked},
        remote_inland_px, should_return_to_local, should_switch_to_remote,
        approaching_local_exit_edge, toggle_lock_mode,
        EdgeDetectInput, FocusState, FocusTarget, HotkeyAction, InputEvent, MonitorLayout,
        RemoteCursorEst, SessionDesktopLayout, MOUSE_COALESCE_FLUSH_MS, MOUSE_SEND_MIN_MANHATTAN,
        REMOTE_MIN_INLAND_PX,
    },
    shared::{ids::{DeviceId, PeerId}, DomainErrorCode},
    DomainError, DomainEvent,
};

use winx_protocol::Payload;

use crate::{
    bus::EventBus,
    ports::{transport::StreamSender, InputBackend, KvmLayoutStore, MonitorBackend, WorkspaceGlobalCursor},
    protocol_convert::{encode_input_payload, input_event_from_dto},
    use_cases::{input_streams, mouse_coalesce::MouseCoalescer, TransportService},
    workspace_layout_wire::monitor_layout_to_session,
};

/// Contexto compartilhado do cursor unificado da sessão KVM.
#[derive(Clone)]
struct SessionCursorCtx {
    x: Arc<AtomicI32>,
    y: Arc<AtomicI32>,
    seq: Arc<AtomicU64>,
    ready: Arc<AtomicBool>,
    host: Arc<Mutex<Option<DeviceId>>>,
    peer_controls_us: Arc<AtomicBool>,
    local_device_id: Arc<Mutex<Option<DeviceId>>>,
    clipboard: Arc<Mutex<Option<Arc<super::ClipboardService>>>>,
}

impl SessionCursorCtx {
    async fn store(&self, host: DeviceId, x: i32, y: i32, seq: u64) {
        self.x.store(x, Ordering::SeqCst);
        self.y.store(y, Ordering::SeqCst);
        self.seq.store(seq, Ordering::SeqCst);
        *self.host.lock().await = Some(host);
        self.ready.store(true, Ordering::SeqCst);
    }

    async fn broadcast(&self, peer_id: PeerId, host: DeviceId, x: i32, y: i32, seq: u64) {
        let Some(clipboard) = self.clipboard.lock().await.clone() else {
            return;
        };
        let _ = clipboard
            .send_data_payload(Payload::SessionCursorSync(
                winx_protocol::SessionCursorSyncPayload {
                    device_id: host.as_uuid(),
                    x,
                    y,
                    seq,
                },
            ))
            .await;
        let _ = peer_id;
    }

    async fn maybe_handoff(
        &self,
        input: &Arc<dyn InputBackend>,
        layout: Option<&MonitorLayout>,
        screen_x: i32,
        screen_y: i32,
    ) -> bool {
        if !self.ready.load(Ordering::SeqCst) {
            return false;
        }
        if layout.is_some_and(|l| approaching_local_exit_edge(l, screen_x, screen_y)) {
            return false;
        }
        let local_device = match *self.local_device_id.lock().await {
            Some(d) => d,
            None => return false,
        };
        if *self.host.lock().await != Some(local_device) {
            return false;
        }
        let sx = self.x.load(Ordering::SeqCst);
        let sy = self.y.load(Ordering::SeqCst);
        let jump = (screen_x - sx).abs().saturating_add((screen_y - sy).abs());
        if jump <= SESSION_HANDOFF_MANHATTAN_PX {
            return false;
        }
        if input.warp_cursor_signed(sx, sy).await.is_err() {
            return false;
        }
        input.reset_mouse_delta_baseline();
        info!(sx, sy, jump, "session cursor: handoff");
        true
    }

    async fn update_local(
        &self,
        input: &Arc<dyn InputBackend>,
        peer_id: PeerId,
        screen_x: i32,
        screen_y: i32,
    ) {
        let local_device = match *self.local_device_id.lock().await {
            Some(d) => d,
            None => return,
        };
        let seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        self.store(local_device, screen_x, screen_y, seq).await;
        self.broadcast(peer_id, local_device, screen_x, screen_y, seq)
            .await;
        let _ = input;
    }

    async fn send_takeover(&self, peer_id: PeerId, screen_x: i32, screen_y: i32) {
        let local_device = match *self.local_device_id.lock().await {
            Some(d) => d,
            None => return,
        };
        let seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        self.store(local_device, screen_x, screen_y, seq).await;
        let Some(clipboard) = self.clipboard.lock().await.clone() else {
            return;
        };
        let _ = clipboard
            .send_data_payload(Payload::SessionInputTakeover(
                winx_protocol::SessionInputTakeoverPayload {
                    device_id: local_device.as_uuid(),
                    x: screen_x,
                    y: screen_y,
                    seq,
                },
            ))
            .await;
        self.peer_controls_us.store(false, Ordering::SeqCst);
        info!(%peer_id, screen_x, screen_y, "session cursor: takeover enviado");
    }
}

/// Estado de envio agregado de mouse para o peer remoto.
pub struct MouseSendState {
    pub coalesce: Mutex<MouseCoalescer>,
    pub flush_notify: Notify,
    /// Escala de sensibilidade remota × 256 (1.0 = 256).
    pub scale_q8: AtomicI32,
    pub frames_sent: AtomicU64,
}

/// Distância Manhattan mínima para detectar troca de mouse físico (handoff).
const SESSION_HANDOFF_MANHATTAN_PX: i32 = 80;
/// Movimento mínimo para retomar controle enquanto o peer injeta input.
const SESSION_TAKEOVER_MANHATTAN_PX: i32 = 10;

static FIRST_INJECT: AtomicBool = AtomicBool::new(true);

/// Período após cruzar para remoto em que retorno pela borda oposta fica desabilitado.
const REMOTE_SWITCH_GRACE_MS: u64 = 500;

#[derive(Debug, Clone, Serialize)]
pub struct KeyboardMirrorStatus {
    pub active: bool,
    pub seconds_left: u32,
    pub keys_sent: u64,
    pub keys_hooked: u64,
    pub keys_send_errors: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct InputDebugStats {
    pub mirror_active: bool,
    pub keys_sent: u64,
    pub keys_hooked: u64,
    pub keys_send_errors: u64,
    pub remote_frames_received: u64,
    pub remote_inject_ok: u64,
    pub remote_inject_fail: u64,
    pub mouse_frames_sent: u64,
    pub input_enabled: bool,
    pub has_input_tx: bool,
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
    remote_cursor_x_est: Arc<AtomicI32>,
    remote_cursor_y_est: Arc<AtomicI32>,
    keyboard_mirror: Arc<AtomicBool>,
    mirror_keys_sent: Arc<AtomicU64>,
    mirror_keys_hooked: Arc<AtomicU64>,
    mirror_keys_send_errors: Arc<AtomicU64>,
    remote_frames_received: Arc<AtomicU64>,
    remote_inject_ok: Arc<AtomicU64>,
    remote_inject_fail: Arc<AtomicU64>,
    mirror_deadline: Arc<Mutex<Option<Instant>>>,
    mouse_send: Arc<MouseSendState>,
    workspace_cursor: Arc<Mutex<Option<Arc<dyn WorkspaceGlobalCursor>>>>,
    kvm_layout_store: Arc<Mutex<Option<Arc<dyn KvmLayoutStore>>>>,
    remote_switch_grace: Arc<Mutex<Option<Instant>>>,
    remote_return_armed: Arc<AtomicBool>,
    /// Serializa ida/volta local↔remoto — evita transições duplicadas por eventos de mouse paralelos.
    focus_transition: Arc<Mutex<()>>,
    layout_sync_deps: Arc<Mutex<Option<Arc<super::kvm_layout_sync::KvmLayoutSyncDeps>>>>,
    clipboard: Arc<Mutex<Option<Arc<super::ClipboardService>>>>,
    local_device_id: Arc<Mutex<Option<DeviceId>>>,
    session_cursor_x: Arc<AtomicI32>,
    session_cursor_y: Arc<AtomicI32>,
    session_cursor_seq: Arc<AtomicU64>,
    session_cursor_ready: Arc<AtomicBool>,
    session_cursor_host: Arc<Mutex<Option<DeviceId>>>,
    peer_controls_us: Arc<AtomicBool>,
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
            remote_cursor_x_est: Arc::new(AtomicI32::new(0)),
            remote_cursor_y_est: Arc::new(AtomicI32::new(0)),
            keyboard_mirror: Arc::new(AtomicBool::new(false)),
            mirror_keys_sent: Arc::new(AtomicU64::new(0)),
            mirror_keys_hooked: Arc::new(AtomicU64::new(0)),
            mirror_keys_send_errors: Arc::new(AtomicU64::new(0)),
            remote_frames_received: Arc::new(AtomicU64::new(0)),
            remote_inject_ok: Arc::new(AtomicU64::new(0)),
            remote_inject_fail: Arc::new(AtomicU64::new(0)),
            mirror_deadline: Arc::new(Mutex::new(None)),
            mouse_send: Arc::new(MouseSendState {
                coalesce: Mutex::new(MouseCoalescer::new()),
                flush_notify: Notify::new(),
                scale_q8: AtomicI32::new(256),
                frames_sent: AtomicU64::new(0),
            }),
            workspace_cursor: Arc::new(Mutex::new(None)),
            kvm_layout_store: Arc::new(Mutex::new(None)),
            remote_switch_grace: Arc::new(Mutex::new(None)),
            remote_return_armed: Arc::new(AtomicBool::new(false)),
            focus_transition: Arc::new(Mutex::new(())),
            layout_sync_deps: Arc::new(Mutex::new(None)),
            clipboard: Arc::new(Mutex::new(None)),
            local_device_id: Arc::new(Mutex::new(None)),
            session_cursor_x: Arc::new(AtomicI32::new(0)),
            session_cursor_y: Arc::new(AtomicI32::new(0)),
            session_cursor_seq: Arc::new(AtomicU64::new(0)),
            session_cursor_ready: Arc::new(AtomicBool::new(false)),
            session_cursor_host: Arc::new(Mutex::new(None)),
            peer_controls_us: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn set_local_device_id(&self, device_id: DeviceId) {
        *self.local_device_id.lock().await = Some(device_id);
    }

    async fn require_local_device_id(&self) -> Result<DeviceId, DomainError> {
        self.local_device_id
            .lock()
            .await
            .ok_or_else(|| internal_err("local_device_id não configurado"))
    }

    fn session_ctx(&self) -> SessionCursorCtx {
        SessionCursorCtx {
            x: Arc::clone(&self.session_cursor_x),
            y: Arc::clone(&self.session_cursor_y),
            seq: Arc::clone(&self.session_cursor_seq),
            ready: Arc::clone(&self.session_cursor_ready),
            host: Arc::clone(&self.session_cursor_host),
            peer_controls_us: Arc::clone(&self.peer_controls_us),
            local_device_id: Arc::clone(&self.local_device_id),
            clipboard: Arc::clone(&self.clipboard),
        }
    }

    pub async fn attach_clipboard(&self, clipboard: Arc<super::ClipboardService>) {
        *self.clipboard.lock().await = Some(clipboard);
    }

    pub async fn init_layout_sync(self: &Arc<Self>, clipboard: Arc<super::ClipboardService>) {
        let deps = Arc::new(super::kvm_layout_sync::KvmLayoutSyncDeps {
            store: Arc::clone(&self.kvm_layout_store),
            bus: self.bus.clone(),
            layout: Arc::clone(&self.layout),
            enabled: Arc::clone(&self.enabled),
            active_peer: Arc::clone(&self.active_peer),
            mouse_send: Arc::clone(&self.mouse_send),
            monitors: Arc::clone(&self.monitors),
            focus: Arc::clone(&self.focus),
            local_device_id: Arc::clone(&self.local_device_id),
        });
        *self.layout_sync_deps.lock().await = Some(Arc::clone(&deps));
        let weak = Arc::downgrade(self);
        let session_handler: super::clipboard::LayoutDataHandler = Arc::new(move |peer_id, payload| {
            if let Some(input) = weak.upgrade() {
                tokio::spawn(async move {
                    input.handle_session_data_payload(peer_id, payload).await;
                });
            }
        });
        deps.register_handler(clipboard.as_ref(), Some(session_handler))
            .await;
        self.attach_clipboard(clipboard).await;
        info!("layout sync: deps inicializados e ligados ao clipboard");
    }

    pub async fn announce_layout_sync(&self, peer_id: PeerId) -> Result<(), DomainError> {
        let local = self.list_local_monitors().await?;
        info!(
            %peer_id,
            count = local.len(),
            monitors = %super::kvm_layout_sync::monitors_summary(&local),
            "layout sync: announce_layout_sync iniciado"
        );
        let clipboard = self
            .clipboard
            .lock()
            .await
            .clone()
            .ok_or_else(|| {
                warn!(%peer_id, "layout sync: clipboard não configurado");
                internal_err("clipboard não configurado")
            })?;
        if !clipboard.is_data_stream_open().await {
            warn!(
                %peer_id,
                "layout sync: stream Data fechado — announce falhará (ordem: clipboard.enable antes de announce)"
            );
        }
        let deps = self
            .layout_sync_deps
            .lock()
            .await
            .clone()
            .ok_or_else(|| {
                warn!(%peer_id, "layout sync: deps não configurados");
                internal_err("layout sync não configurado")
            })?;
        deps.announce_to_peer(clipboard.as_ref(), peer_id, &local)
            .await
            .map_err(|e| {
                warn!(?e, %peer_id, "layout sync: announce_layout_sync falhou");
                e
            })?;
        info!(%peer_id, "layout sync: announce_layout_sync concluído");
        Ok(())
    }

    pub async fn attach_kvm_layout_store(&self, store: Arc<dyn KvmLayoutStore>) {
        *self.kvm_layout_store.lock().await = Some(store);
    }

    pub async fn list_local_monitors(
        &self,
    ) -> Result<Vec<winx_domain::input_control::MonitorRect>, DomainError> {
        self.monitors
            .enumerate_local_monitors()
            .await
            .map_err(|e| internal_err(&e.to_string()))
    }

    pub async fn get_peer_monitors(
        &self,
        peer_id: PeerId,
    ) -> Result<Vec<winx_domain::input_control::MonitorRect>, DomainError> {
        Ok(super::kvm_layout_sync::get_peer_monitors_from_store(
            &self.kvm_layout_store,
            peer_id,
        )
        .await)
    }

    pub async fn get_kvm_session_layout(
        &self,
        peer_id: PeerId,
    ) -> Result<Option<SessionDesktopLayout>, DomainError> {
        let local_device = self.require_local_device_id().await?;
        let local_os = self.list_local_monitors().await?;
        let peer_mons = self.get_peer_monitors(peer_id).await?;

        let store = self.kvm_layout_store.lock().await;
        let Some(store) = store.as_ref() else {
            return Ok(None);
        };

        let mut session = if let Some(saved) = store
            .get_session(peer_id)
            .await
            .map_err(|e| internal_err(&e.to_string()))?
        {
            if !saved.per_device.is_empty() {
                saved
            } else if let Some(legacy) = store
                .get(peer_id)
                .await
                .map_err(|e| internal_err(&e.to_string()))?
            {
                monitor_layout_to_session(&legacy, local_device)
            } else {
                SessionDesktopLayout::empty()
            }
        } else if let Some(legacy) = store
            .get(peer_id)
            .await
            .map_err(|e| internal_err(&e.to_string()))?
        {
            monitor_layout_to_session(&legacy, local_device)
        } else {
            SessionDesktopLayout::empty()
        };

        if session.device_monitors(local_device).is_empty() && !local_os.is_empty() {
            session.merge_announced_monitors(local_device, &local_os, None);
        }
        let remote_device = DeviceId::from_uuid(peer_id.as_uuid());
        if session.device_monitors(remote_device).is_empty() && !peer_mons.is_empty() {
            session.merge_announced_monitors(remote_device, &peer_mons, Some(local_device));
        }

        refresh_device_geometry(&mut session, local_device, &local_os);
        refresh_device_geometry(&mut session, remote_device, &peer_mons);

        if session.per_device.is_empty() {
            Ok(None)
        } else {
            Ok(Some(session))
        }
    }

    pub async fn save_kvm_session_layout(
        &self,
        peer_id: PeerId,
        mut session: SessionDesktopLayout,
    ) -> Result<(), DomainError> {
        let local_device = self.require_local_device_id().await?;
        let local = self.list_local_monitors().await?;
        refresh_device_geometry(&mut session, local_device, &local);

        {
            let store = self.kvm_layout_store.lock().await;
            let Some(store) = store.as_ref() else {
                return Err(internal_err("kvm layout store não configurado"));
            };
            store
                .save_session(peer_id, &session)
                .await
                .map_err(|e| internal_err(&e.to_string()))?;
        }

        let runtime = session.derive_runtime_layout(local_device, peer_id, &local);
        if *self.enabled.lock().await && *self.active_peer.lock().await == Some(peer_id) {
            self.apply_monitor_layout(runtime).await;
        }

        if let Err(err) = self.ensure_layout_sync_for_peer(peer_id).await {
            warn!(
                ?err,
                %peer_id,
                "layout sync: falha ao garantir canal Data — broadcast pode falhar"
            );
        }

        let layout_deps = self.layout_sync_deps.lock().await.clone();
        let clipboard_svc = self.clipboard.lock().await.clone();
        if let (Some(deps), Some(clipboard)) = (layout_deps, clipboard_svc) {
            super::kvm_layout_sync::broadcast_session_layout(
                deps.as_ref(),
                clipboard.as_ref(),
                peer_id,
                local,
                &session,
            )
            .await;
        } else {
            warn!(
                %peer_id,
                "layout sync: save session concluído mas broadcast omitido (deps ou clipboard ausente)"
            );
        }

        Ok(())
    }

    pub async fn get_kvm_layout(
        &self,
        peer_id: PeerId,
    ) -> Result<Option<MonitorLayout>, DomainError> {
        let local_device = match self.require_local_device_id().await {
            Ok(id) => id,
            Err(_) => {
                return self.get_kvm_layout_legacy(peer_id).await;
            }
        };
        let local = self.list_local_monitors().await?;
        if let Some(session) = self.get_kvm_session_layout(peer_id).await? {
            return Ok(Some(session.derive_runtime_layout(local_device, peer_id, &local)));
        }
        self.get_kvm_layout_legacy(peer_id).await
    }

    async fn get_kvm_layout_legacy(
        &self,
        peer_id: PeerId,
    ) -> Result<Option<MonitorLayout>, DomainError> {
        let local = self.list_local_monitors().await?;
        let store = self.kvm_layout_store.lock().await;
        let Some(store) = store.as_ref() else {
            return Ok(None);
        };
        let mut layout = store
            .get(peer_id)
            .await
            .map_err(|e| internal_err(&e.to_string()))?;
        if let Some(ref mut saved) = layout {
            saved.finalize_for_runtime(local, peer_id);
            if saved.remote_monitors.is_empty() {
                if let Ok(Some(peer_mons)) = store.get_peer_monitors(peer_id).await {
                    if !peer_mons.is_empty() {
                        saved.remote_monitors = peer_mons;
                        saved.infer_edges_from_geometry();
                    }
                }
            }
        }
        Ok(layout)
    }

    pub async fn save_kvm_layout(
        &self,
        peer_id: PeerId,
        mut layout: MonitorLayout,
    ) -> Result<(), DomainError> {
        if let Ok(local_device) = self.require_local_device_id().await {
            let local = self.list_local_monitors().await?;
            layout.finalize_for_runtime(local.clone(), peer_id);
            let session = monitor_layout_to_session(&layout, local_device);
            return self.save_kvm_session_layout(peer_id, session).await;
        }

        let local = self.list_local_monitors().await?;
        layout.finalize_for_runtime(local.clone(), peer_id);
        {
            let store = self.kvm_layout_store.lock().await;
            let Some(store) = store.as_ref() else {
                return Err(internal_err("kvm layout store não configurado"));
            };
            store
                .save(peer_id, &layout)
                .await
                .map_err(|e| internal_err(&e.to_string()))?;
        }

        if *self.enabled.lock().await && *self.active_peer.lock().await == Some(peer_id) {
            self.apply_monitor_layout(layout.clone()).await;
        }

        if let Err(err) = self.ensure_layout_sync_for_peer(peer_id).await {
            warn!(
                ?err,
                %peer_id,
                "layout sync: falha ao garantir canal Data — broadcast pode falhar"
            );
        }

        let layout_deps = self.layout_sync_deps.lock().await.clone();
        let clipboard_svc = self.clipboard.lock().await.clone();
        if let (Some(deps), Some(clipboard)) = (layout_deps, clipboard_svc) {
            let local_device = self.require_local_device_id().await?;
            let session = monitor_layout_to_session(&layout, local_device);
            super::kvm_layout_sync::broadcast_session_layout(
                deps.as_ref(),
                clipboard.as_ref(),
                peer_id,
                local,
                &session,
            )
            .await;
        } else {
            warn!(
                %peer_id,
                "layout sync: save concluído mas broadcast omitido (deps ou clipboard ausente)"
            );
        }

        Ok(())
    }

    pub async fn attach_workspace_cursor(&self, bridge: Arc<dyn WorkspaceGlobalCursor>) {
        *self.workspace_cursor.lock().await = Some(bridge);
    }

    pub async fn get_input_debug_stats(&self) -> InputDebugStats {
        InputDebugStats {
            mirror_active: self.keyboard_mirror.load(Ordering::SeqCst),
            keys_sent: self.mirror_keys_sent.load(Ordering::SeqCst),
            keys_hooked: self.mirror_keys_hooked.load(Ordering::SeqCst),
            keys_send_errors: self.mirror_keys_send_errors.load(Ordering::SeqCst),
            remote_frames_received: self.remote_frames_received.load(Ordering::SeqCst),
            remote_inject_ok: self.remote_inject_ok.load(Ordering::SeqCst),
            remote_inject_fail: self.remote_inject_fail.load(Ordering::SeqCst),
            mouse_frames_sent: self.mouse_send.frames_sent.load(Ordering::SeqCst),
            input_enabled: *self.enabled.lock().await,
            has_input_tx: self.input_tx.lock().await.is_some(),
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

        if *self.enabled.lock().await {
            let active = *self.active_peer.lock().await;
            if active != Some(peer_id) {
                return Err(DomainError::new(
                    DomainErrorCode::InternalError,
                    "input control ativo para outro peer",
                ));
            }
        } else {
            self.enable_for_peer(peer_id).await?;
        }

        let duration = Duration::from_secs(u64::from(duration_secs.max(1)));
        self.mirror_keys_sent.store(0, Ordering::SeqCst);
        self.mirror_keys_hooked.store(0, Ordering::SeqCst);
        self.mirror_keys_send_errors.store(0, Ordering::SeqCst);
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
                .map(|deadline| deadline.saturating_duration_since(Instant::now()).as_secs())
                .unwrap_or(0)
        } else {
            0
        };
        KeyboardMirrorStatus {
            active,
            seconds_left: u32::try_from(seconds_left).unwrap_or(0),
            keys_sent,
            keys_hooked: self.mirror_keys_hooked.load(Ordering::SeqCst),
            keys_send_errors: self.mirror_keys_send_errors.load(Ordering::SeqCst),
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

    /// Envia um frame Input mínimo no stream para validar conectividade (Lab).
    pub async fn send_lab_ping(&self, peer_id: PeerId) -> Result<(), DomainError> {
        if !self.transport.is_peer_connected(peer_id).await {
            return Err(DomainError::new(
                DomainErrorCode::TransportConnectionFailed,
                "peer não conectado via QUIC",
            ));
        }

        if self.input_tx.lock().await.is_none() {
            info!(%peer_id, "lab ping: habilitando input (stream único)");
            self.enable_for_peer(peer_id).await?;
        } else {
            info!(%peer_id, stream_reused = true, "lab ping input");
        }

        let tx = self.input_tx.lock().await.clone();
        let Some(tx) = tx else {
            return Err(DomainError::new(
                DomainErrorCode::TransportConnectionFailed,
                "stream Input não disponível",
            ));
        };

        let ev = InputEvent::Key {
            code: winx_domain::input_control::PortableKeyCode(0),
            pressed: false,
            modifiers: winx_domain::input_control::KeyModifiers {
                ctrl: false,
                alt: false,
                shift: false,
                meta: false,
            },
        };
        let n = self.seq.fetch_add(1, Ordering::SeqCst);
        let bytes = encode_input_payload(n, &ev).map_err(|e| internal_err(&e.to_string()))?;
        tx.send(bytes)
            .await
            .map_err(|_| internal_err("falha ao enviar ping Input no stream"))?;
        Ok(())
    }

    /// Atualiza layout/escala sem reiniciar hooks (workspace conectado ou layout editado).
    pub async fn apply_monitor_layout(&self, layout: MonitorLayout) {
        let scale = layout.remote_mouse_scale();
        self.mouse_send
            .scale_q8
            .store((scale * 256.0).round() as i32, Ordering::SeqCst);
        *self.layout.lock().await = Some(layout);
    }

    async fn resolve_layout_for_peer(
        &self,
        peer_id: PeerId,
        local_monitors: &[winx_domain::input_control::MonitorRect],
    ) -> MonitorLayout {
        if let Some(bridge) = self.workspace_cursor.lock().await.as_ref() {
            if let Some(layout) = bridge
                .input_layout_for_peer(peer_id, local_monitors.to_vec())
                .await
            {
                return layout;
            }
        }
        if let Some(store) = self.kvm_layout_store.lock().await.as_ref() {
            let local_device = self.local_device_id.lock().await;
            if let Some(local_device) = *local_device {
                if let Ok(Some(session)) = store.get_session(peer_id).await {
                    if !session.per_device.is_empty() {
                        return session.derive_runtime_layout(
                            local_device,
                            peer_id,
                            local_monitors,
                        );
                    }
                }
            }
            if let Ok(Some(mut saved)) = store.get(peer_id).await {
                saved.finalize_for_runtime(local_monitors.to_vec(), peer_id);
                if saved.remote_monitors.is_empty() {
                    if let Ok(Some(peer_mons)) = store.get_peer_monitors(peer_id).await {
                        if !peer_mons.is_empty() {
                            saved.remote_monitors = peer_mons;
                            saved.infer_edges_from_geometry();
                        }
                    }
                }
                return saved;
            }
        }
        let mut layout =
            MonitorLayout::default_side_by_side(local_monitors.to_vec(), peer_id);
        layout.infer_edges_from_geometry();
        layout
    }

    #[allow(clippy::too_many_lines)]
    pub async fn enable_for_peer(&self, peer_id: PeerId) -> Result<(), DomainError> {
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

        let layout = self.resolve_layout_for_peer(peer_id, &local).await;

        if *self.enabled.lock().await {
            let active = *self.active_peer.lock().await;
            if active == Some(peer_id) {
                self.apply_monitor_layout(layout).await;
                return Ok(());
            }
        }

        let scale = layout.remote_mouse_scale();
        self.mouse_send
            .scale_q8
            .store((scale * 256.0).round() as i32, Ordering::SeqCst);
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
        let service_remote_dx = Arc::clone(&self.remote_cursor_x_est);
        let service_remote_dy = Arc::clone(&self.remote_cursor_y_est);
        let service_mirror = Arc::clone(&self.keyboard_mirror);
        let service_mirror_keys = Arc::clone(&self.mirror_keys_sent);
        let service_mirror_hooked = Arc::clone(&self.mirror_keys_hooked);
        let service_mirror_send_errors = Arc::clone(&self.mirror_keys_send_errors);
        let service_mouse_send = Arc::clone(&self.mouse_send);
        let service_workspace_cursor = Arc::clone(&self.workspace_cursor);
        let service_remote_switch_grace = Arc::clone(&self.remote_switch_grace);
        let service_remote_return_armed = Arc::clone(&self.remote_return_armed);
        let service_focus_transition = Arc::clone(&self.focus_transition);
        let service_session = self.session_ctx();

        let mouse_send_flush = Arc::clone(&self.mouse_send);
        let input_tx_flush = Arc::clone(&self.input_tx);
        let seq_flush = Arc::clone(&self.seq);
        let enabled_flush = Arc::clone(&self.enabled);
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(Duration::from_millis(MOUSE_COALESCE_FLUSH_MS));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = interval.tick() => {}
                    () = mouse_send_flush.flush_notify.notified() => {}
                }
                if !*enabled_flush.lock().await {
                    continue;
                }
                flush_mouse_to_peer(&mouse_send_flush, &input_tx_flush, &seq_flush).await;
            }
        });

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
                let remote_dy = service_remote_dy.clone();
                let mirror = service_mirror.clone();
                let mirror_keys = service_mirror_keys.clone();
                let mirror_hooked = service_mirror_hooked.clone();
                let mirror_send_errors = service_mirror_send_errors.clone();
                let mouse_send = service_mouse_send.clone();
                let workspace_cursor = service_workspace_cursor.clone();
                let remote_switch_grace = service_remote_switch_grace.clone();
                let remote_return_armed = service_remote_return_armed.clone();
                let focus_transition = service_focus_transition.clone();
                let session = service_session.clone();
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
                        remote_dy,
                        mirror,
                        mirror_keys,
                        mirror_hooked,
                        mirror_send_errors,
                        mouse_send,
                        workspace_cursor,
                        remote_switch_grace,
                        remote_return_armed,
                        focus_transition,
                        session,
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
        let workspace_cursor_hk = Arc::clone(&self.workspace_cursor);

        self.input
            .start_capture(Box::new(on_event), {
                let runtime = runtime.clone();
                Box::new(move |action| {
                    let focus = focus_hk.clone();
                    let layout = layout_hk.clone();
                    let input = input_hk.clone();
                    let bus = bus_hk.clone();
                    let active = active_hk.clone();
                    let input_tx = input_tx_hk.clone();
                    let workspace_cursor = workspace_cursor_hk.clone();
                    runtime.spawn(async move {
                        handle_hotkey(
                            action,
                            focus,
                            layout,
                            input,
                            bus,
                            active,
                            input_tx,
                            workspace_cursor,
                        )
                        .await;
                    });
                })
            })
            .await
            .map_err(|e| internal_err(&e.to_string()))?;

        FIRST_INJECT.store(true, Ordering::SeqCst);
        self.remote_cursor_x_est.store(0, Ordering::SeqCst);
        self.remote_cursor_y_est.store(0, Ordering::SeqCst);

        let inject_input = Arc::clone(&self.input);
        let remote_frames = Arc::clone(&self.remote_frames_received);
        let remote_ok = Arc::clone(&self.remote_inject_ok);
        let remote_fail = Arc::clone(&self.remote_inject_fail);
        let inject_layout = Arc::clone(&self.layout);
        let inject_focus = Arc::clone(&self.focus);
        let inject_active = Arc::clone(&self.active_peer);
        let inject_session = self.session_ctx();
        let inject_peer_controls = Arc::clone(&self.peer_controls_us);
        tokio::spawn(async move {
            while let Some(bytes) = rx.recv().await {
                match winx_protocol::decode(&bytes) {
                    Ok(frame) => {
                        if let winx_protocol::Payload::Input(p) = frame.payload {
                            let mut ev = input_event_from_dto(&p.event);
                            if ev.is_noop_mouse_move()
                                || ev.is_noise_mouse_move(MOUSE_SEND_MIN_MANHATTAN)
                            {
                                continue;
                            }
                            if let InputEvent::MouseWarpAbsolute { x, y } = ev {
                                if let Some(layout) = inject_layout.lock().await.as_ref() {
                                    let (ox, oy) = layout.map_remote_relative_to_os(x, y);
                                    ev = InputEvent::MouseWarpAbsolute { x: ox, y: oy };
                                }
                            }
                            remote_frames.fetch_add(1, Ordering::SeqCst);
                            if FIRST_INJECT.swap(false, Ordering::SeqCst) {
                                info!(
                                    target: "winx::input::remote",
                                    seq = p.seq,
                                    "primeiro frame Input remoto recebido"
                                );
                            }
                            match inject_input.inject(ev.clone()).await {
                                Ok(()) => {
                                    remote_ok.fetch_add(1, Ordering::SeqCst);
                                }
                                Err(err) => {
                                    remote_fail.fetch_add(1, Ordering::SeqCst);
                                    warn!(
                                        target: "winx::input::remote",
                                        seq = p.seq,
                                        ?err,
                                        "falha ao injetar input remoto"
                                    );
                                }
                            }
                            if matches!(
                                ev,
                                InputEvent::MouseMove { .. }
                                    | InputEvent::MouseWarpAbsolute { .. }
                            ) {
                                inject_peer_controls.store(true, Ordering::SeqCst);
                                inject_input.set_pass_through(false);
                                if matches!(inject_focus.lock().await.target, FocusTarget::Local) {
                                    if let Some(peer_id) = *inject_active.lock().await {
                                        if let Ok((x, y)) = inject_input.get_cursor_pos().await {
                                            inject_session
                                                .update_local(&inject_input, peer_id, x, y)
                                                .await;
                                        }
                                    }
                                }
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
        if let Err(err) = self.register_session_cursor_at_connection(peer_id).await {
            warn!(?err, %peer_id, "session cursor: falha ao registrar na conexão");
        }
        info!(%peer_id, "input control habilitado");
        Ok(())
    }

    pub async fn get_focus_state(&self) -> FocusState {
        self.focus.lock().await.clone()
    }

    pub async fn is_active_for_peer(&self, peer_id: PeerId) -> bool {
        *self.enabled.lock().await && *self.active_peer.lock().await == Some(peer_id)
    }

    /// Garante stream Data aberto para broadcast de layout quando o peer está conectado.
    pub async fn ensure_layout_sync_for_peer(&self, peer_id: PeerId) -> Result<(), DomainError> {
        if !self.transport.is_peer_connected(peer_id).await {
            return Ok(());
        }
        let Some(clipboard) = self.clipboard.lock().await.clone() else {
            return Ok(());
        };
        if clipboard.is_data_stream_open().await {
            return Ok(());
        }
        info!(
            %peer_id,
            "layout sync: stream Data fechado — reabilitando KVM para sync de layout"
        );
        super::single_connection::enable_kvm_for_peer(clipboard.as_ref(), self, peer_id).await
    }

    fn clear_session_cursor(&self) {
        self.session_cursor_ready.store(false, Ordering::SeqCst);
        self.session_cursor_x.store(0, Ordering::SeqCst);
        self.session_cursor_y.store(0, Ordering::SeqCst);
        self.session_cursor_seq.store(0, Ordering::SeqCst);
        self.peer_controls_us.store(false, Ordering::SeqCst);
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
        self.remote_cursor_x_est.store(0, Ordering::SeqCst);

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
        *self.session_cursor_host.lock().await = None;
        self.clear_session_cursor();
        FIRST_INJECT.store(true, Ordering::SeqCst);
    }

    fn spawn_bus_subscriber(&self) {
        let mut rx = self.bus.subscribe();
        let active = Arc::clone(&self.active_peer);
        let focus = Arc::clone(&self.focus);
        let input = Arc::clone(&self.input);
        let input_tx = Arc::clone(&self.input_tx);
        let enabled = Arc::clone(&self.enabled);
        let remote_dx = Arc::clone(&self.remote_cursor_x_est);
        let remote_return_armed = Arc::clone(&self.remote_return_armed);
        let reload_enabled = Arc::clone(&self.enabled);
        let reload_active = Arc::clone(&self.active_peer);
        let reload_focus = Arc::clone(&self.focus);
        let reload_layout = Arc::clone(&self.layout);
        let reload_store = Arc::clone(&self.kvm_layout_store);
        let reload_monitors = Arc::clone(&self.monitors);
        let reload_mouse = Arc::clone(&self.mouse_send);
        let reload_return_armed = Arc::clone(&self.remote_return_armed);
        let reload_local_device = Arc::clone(&self.local_device_id);
        let reload_session_ready = Arc::clone(&self.session_cursor_ready);
        let reload_session_host = Arc::clone(&self.session_cursor_host);
        let reload_peer_controls = Arc::clone(&self.peer_controls_us);
        let reload_session_x = Arc::clone(&self.session_cursor_x);
        let reload_session_y = Arc::clone(&self.session_cursor_y);
        let reload_session_seq = Arc::clone(&self.session_cursor_seq);

        tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                match event {
                    DomainEvent::ConnectionLost(e) => {
                        let guard = active.lock().await;
                        if *guard == Some(e.peer_id) {
                            drop(guard);
                            *active.lock().await = None;
                            *enabled.lock().await = false;
                            *input_tx.lock().await = None;
                            remote_dx.store(0, Ordering::SeqCst);
                            remote_return_armed.store(false, Ordering::SeqCst);
                            let mut f = focus.lock().await;
                            f.target = FocusTarget::Local;
                            f.lock_mode = false;
                            input.set_pass_through(true);
                            let _ = input.set_cursor_clipped(None).await;
                            input.reset_mouse_delta_baseline();
                            reload_session_ready.store(false, Ordering::SeqCst);
                            reload_session_x.store(0, Ordering::SeqCst);
                            reload_session_y.store(0, Ordering::SeqCst);
                            reload_session_seq.store(0, Ordering::SeqCst);
                            reload_peer_controls.store(false, Ordering::SeqCst);
                            *reload_session_host.lock().await = None;
                            FIRST_INJECT.store(true, Ordering::SeqCst);
                            info!(peer_id = %e.peer_id, "foco e cursor restaurados (connection lost)");
                        }
                    }
                    DomainEvent::KvmLayoutUpdated(e) => {
                        if !*reload_enabled.lock().await {
                            continue;
                        }
                        if *reload_active.lock().await != Some(e.peer_id) {
                            continue;
                        }
                        if matches!(reload_focus.lock().await.target, FocusTarget::Remote(_)) {
                            debug!(
                                peer_id = %e.peer_id,
                                "layout sync: KvmLayoutUpdated adiado (foco remoto ativo)"
                            );
                            continue;
                        }
                        apply_stored_layout_to_runtime(
                            &reload_store,
                            &reload_monitors,
                            &reload_layout,
                            &reload_mouse,
                            &reload_return_armed,
                            &reload_local_device,
                            e.peer_id,
                        )
                        .await;
                    }
                    DomainEvent::FocusSwitched(e) => {
                        if !matches!(e.to, FocusTarget::Local) {
                            continue;
                        }
                        if !*reload_enabled.lock().await {
                            continue;
                        }
                        let Some(peer_id) = *reload_active.lock().await else {
                            continue;
                        };
                        apply_stored_layout_to_runtime(
                            &reload_store,
                            &reload_monitors,
                            &reload_layout,
                            &reload_mouse,
                            &reload_return_armed,
                            &reload_local_device,
                            peer_id,
                        )
                        .await;
                    }
                    _ => {}
                }
            }
        });
    }
}

fn refresh_device_geometry(
    session: &mut SessionDesktopLayout,
    device_id: DeviceId,
    os_monitors: &[winx_domain::input_control::MonitorRect],
) {
    if os_monitors.is_empty() {
        return;
    }
    let Some(existing) = session.per_device.get(&device_id).cloned() else {
        session.merge_announced_monitors(device_id, os_monitors, None);
        return;
    };
    let mut updated = Vec::with_capacity(os_monitors.len());
    for os in os_monitors {
        if let Some(saved) = existing.iter().find(|m| m.id == os.id) {
            updated.push(winx_domain::input_control::MonitorRect {
                id: os.id,
                x: saved.x,
                y: saved.y,
                width: os.width,
                height: os.height,
            });
        } else {
            updated.push(*os);
        }
    }
    session.set_device_monitors(device_id, updated);
}

async fn apply_stored_layout_to_runtime(
    kvm_layout_store: &Arc<Mutex<Option<Arc<dyn KvmLayoutStore>>>>,
    monitors: &Arc<dyn MonitorBackend>,
    layout: &Arc<Mutex<Option<MonitorLayout>>>,
    mouse_send: &Arc<MouseSendState>,
    remote_return_armed: &Arc<AtomicBool>,
    local_device_id: &Arc<Mutex<Option<DeviceId>>>,
    peer_id: PeerId,
) {
    let Some(store) = kvm_layout_store.lock().await.clone() else {
        return;
    };
    let local = monitors.enumerate_local_monitors().await.unwrap_or_default();
    if local.is_empty() {
        return;
    }
    let Some(local_device) = *local_device_id.lock().await else {
        return;
    };

    if let Ok(Some(session)) = store.get_session(peer_id).await {
        if !session.per_device.is_empty() {
            let runtime = session.derive_runtime_layout(local_device, peer_id, &local);
            let scale = runtime.remote_mouse_scale();
            mouse_send
                .scale_q8
                .store((scale * 256.0).round() as i32, Ordering::SeqCst);
            *layout.lock().await = Some(runtime);
            remote_return_armed.store(false, Ordering::SeqCst);
            info!(%peer_id, "layout runtime recarregado do session store");
            return;
        }
    }

    if let Ok(Some(mut saved)) = store.get(peer_id).await {
        saved.finalize_for_runtime(local, peer_id);
        if saved.remote_monitors.is_empty() {
            if let Ok(Some(peer_mons)) = store.get_peer_monitors(peer_id).await {
                if !peer_mons.is_empty() {
                    saved.remote_monitors = peer_mons;
                    saved.infer_edges_from_geometry();
                }
            }
        }
        let scale = saved.remote_mouse_scale();
        mouse_send
            .scale_q8
            .store((scale * 256.0).round() as i32, Ordering::SeqCst);
        *layout.lock().await = Some(saved);
        remote_return_armed.store(false, Ordering::SeqCst);
        info!(%peer_id, "layout runtime recarregado do store");
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
    workspace_cursor: Arc<Mutex<Option<Arc<dyn WorkspaceGlobalCursor>>>>,
) {
    match action {
        HotkeyAction::PanicLocal => {
            panic_local(
                focus,
                layout,
                input,
                bus,
                active,
                input_tx,
                workspace_cursor,
            )
            .await;
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
        HotkeyAction::OpenActiveWorkspace => {
            bus.publish(DomainEvent::HotkeyTriggered(HotkeyTriggered {
                action: HotkeyAction::OpenActiveWorkspace,
            }));
        }
    }
}

async fn panic_local(
    focus: Arc<Mutex<FocusState>>,
    layout: Arc<Mutex<Option<MonitorLayout>>>,
    input: Arc<dyn InputBackend>,
    bus: EventBus,
    active: Arc<Mutex<Option<PeerId>>>,
    _input_tx: Arc<Mutex<Option<StreamSender>>>,
    workspace_cursor: Arc<Mutex<Option<Arc<dyn WorkspaceGlobalCursor>>>>,
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
        workspace_cursor,
        active,
    )
    .await;
    if let Some(layout) = layout_warp.lock().await.as_ref() {
        let (x, y) = layout.local_return_warp_point();
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
        let (x, y) = layout.local_return_warp_point();
        let _ = input.warp_cursor(x, y).await;
    }

    bus.publish(DomainEvent::HotkeyTriggered(HotkeyTriggered {
        action: HotkeyAction::ForceReset,
    }));
    info!("force reset ativado — foco e cursor restaurados");
}

fn scale_mouse_delta(dx: i32, dy: i32, scale_q8: i32) -> (i32, i32) {
    let sx = ((i64::from(dx) * i64::from(scale_q8)) / 256) as i32;
    let sy = ((i64::from(dy) * i64::from(scale_q8)) / 256) as i32;
    (sx, sy)
}

async fn flush_mouse_to_peer(
    mouse_send: &MouseSendState,
    input_tx: &Arc<Mutex<Option<StreamSender>>>,
    seq: &Arc<AtomicU64>,
) {
    let taken = {
        let mut c = mouse_send.coalesce.lock().await;
        c.take_if_significant(MOUSE_SEND_MIN_MANHATTAN)
    };
    let Some((dx, dy)) = taken else {
        return;
    };
    let scale_q8 = mouse_send.scale_q8.load(Ordering::SeqCst);
    let (dx, dy) = scale_mouse_delta(dx, dy, scale_q8);
    if dx == 0 && dy == 0 {
        return;
    }
    let ev = InputEvent::MouseMove {
        dx,
        dy,
        screen_x: 0,
        screen_y: 0,
    };
    let guard = input_tx.lock().await;
    let Some(tx) = guard.as_ref() else {
        return;
    };
    let n = seq.fetch_add(1, Ordering::SeqCst);
    if let Ok(bytes) = encode_input_payload(n, &ev) {
        if tx.send(bytes).await.is_ok() {
            mouse_send.frames_sent.fetch_add(1, Ordering::SeqCst);
        } else {
            warn!("falha ao enviar mouse agregado no stream");
        }
    }
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
    remote_cursor_x_est: Arc<AtomicI32>,
    remote_cursor_y_est: Arc<AtomicI32>,
    keyboard_mirror: Arc<AtomicBool>,
    mirror_keys_sent: Arc<AtomicU64>,
    mirror_keys_hooked: Arc<AtomicU64>,
    mirror_keys_send_errors: Arc<AtomicU64>,
    mouse_send: Arc<MouseSendState>,
    workspace_cursor: Arc<Mutex<Option<Arc<dyn WorkspaceGlobalCursor>>>>,
    remote_switch_grace: Arc<Mutex<Option<Instant>>>,
    remote_return_armed: Arc<AtomicBool>,
    focus_transition: Arc<Mutex<()>>,
    session: SessionCursorCtx,
) {
    if keyboard_mirror.load(Ordering::SeqCst) {
        if let InputEvent::Key { code, pressed, .. } = &ev {
            mirror_keys_hooked.fetch_add(1, Ordering::SeqCst);
            if let Some(tx) = input_tx.lock().await.as_ref() {
                let n = seq.fetch_add(1, Ordering::SeqCst);
                match encode_input_payload(n, &ev) {
                    Ok(bytes) => {
                        let bytes_len = bytes.len();
                        match tx.send(bytes).await {
                            Ok(()) => {
                                mirror_keys_sent.fetch_add(1, Ordering::SeqCst);
                                debug!(
                                    target: "winx::input::mirror",
                                    seq = n,
                                    ?code,
                                    pressed,
                                    bytes_len,
                                    "tecla espelhada enviada"
                                );
                            }
                            Err(_) => {
                                mirror_keys_send_errors.fetch_add(1, Ordering::SeqCst);
                                warn!(
                                    target: "winx::input::mirror",
                                    seq = n,
                                    "falha ao enviar tecla no stream Input"
                                );
                            }
                        }
                    }
                    Err(err) => {
                        mirror_keys_send_errors.fetch_add(1, Ordering::SeqCst);
                        warn!(target: "winx::input::mirror", ?err, "falha ao codificar tecla");
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
            if !session.peer_controls_us.load(Ordering::SeqCst) {
                input.set_pass_through(true);
            }
            if let InputEvent::MouseMove {
                screen_x, screen_y, ..
            } = ev
            {
                if ev.is_noop_mouse_move() {
                    return;
                }
                if session.peer_controls_us.load(Ordering::SeqCst) {
                    let sx = session.x.load(Ordering::SeqCst);
                    let sy = session.y.load(Ordering::SeqCst);
                    let jump = (screen_x - sx).abs().saturating_add((screen_y - sy).abs());
                    if jump >= SESSION_TAKEOVER_MANHATTAN_PX {
                        if let Some(peer_id) = *active.lock().await {
                            session
                                .send_takeover(peer_id, screen_x, screen_y)
                                .await;
                            input.set_pass_through(true);
                        }
                    } else {
                        return;
                    }
                }
                let layout_snapshot = layout.lock().await.clone();
                if let Some(ref layout_data) = layout_snapshot {
                    if should_switch_to_remote(
                        EdgeDetectInput {
                            screen_x,
                            screen_y,
                            lock_mode: state.lock_mode,
                        },
                        layout_data,
                    ) {
                        try_edge_switch(
                            screen_x,
                            screen_y,
                            focus.clone(),
                            layout.clone(),
                            input.clone(),
                            bus.clone(),
                            active.clone(),
                            input_tx.clone(),
                            Arc::clone(&remote_cursor_x_est),
                            Arc::clone(&remote_cursor_y_est),
                            workspace_cursor.clone(),
                            remote_switch_grace.clone(),
                            remote_return_armed.clone(),
                            focus_transition.clone(),
                            session.clone(),
                        )
                        .await;
                        if !matches!(focus.lock().await.target, FocusTarget::Local) {
                            return;
                        }
                    }
                }
                if session
                    .maybe_handoff(
                        &input,
                        layout_snapshot.as_ref(),
                        screen_x,
                        screen_y,
                    )
                    .await
                {
                    return;
                }
                if let Some(bridge) = workspace_cursor.lock().await.as_ref() {
                    bridge.publish_local_cursor(screen_x, screen_y).await;
                }
                if let Some(peer_id) = *active.lock().await {
                    session
                        .update_local(&input, peer_id, screen_x, screen_y)
                        .await;
                }
            }
        }
        FocusTarget::Remote(_peer) => {
            input.set_pass_through(false);
            match &ev {
                InputEvent::MouseMove { dx, dy, .. } => {
                    if ev.is_noop_mouse_move() || ev.is_noise_mouse_move(MOUSE_SEND_MIN_MANHATTAN) {
                        return;
                    }
                    // Rastrear posição X estimada do cursor no espaço do monitor remoto.
                    // scaled_dx usa a mesma escala aplicada ao delta enviado ao peer.
                    let scale_q8 = mouse_send.scale_q8.load(Ordering::SeqCst);
                    let scaled_dx = ((i64::from(*dx) * i64::from(scale_q8)) / 256) as i32;
                    let scaled_dy = ((i64::from(*dy) * i64::from(scale_q8)) / 256) as i32;
                    let in_grace = remote_switch_grace.lock().await.is_some_and(|t| {
                        t.elapsed() < Duration::from_millis(REMOTE_SWITCH_GRACE_MS)
                    });

                    let go_back = {
                        let layout_guard = layout.lock().await;
                        if let Some(layout_data) = layout_guard.as_ref() {
                            let remote_bounds = layout_data.placed_remote_bounds();
                            let remote_w = remote_bounds.width as i32;
                            let remote_h = remote_bounds.height as i32;
                            let max_x = remote_w.saturating_sub(1);
                            let max_y = remote_h.saturating_sub(1);
                            let old_x = remote_cursor_x_est.load(Ordering::SeqCst);
                            let old_y = remote_cursor_y_est.load(Ordering::SeqCst);
                            let new_x = (old_x + scaled_dx).clamp(0, max_x);
                            let new_y = (old_y + scaled_dy).clamp(0, max_y);
                            remote_cursor_x_est.store(new_x, Ordering::SeqCst);
                            remote_cursor_y_est.store(new_y, Ordering::SeqCst);

                            if in_grace {
                                false
                            } else {
                                let est = RemoteCursorEst { x: new_x, y: new_y };
                                if !remote_return_armed.load(Ordering::SeqCst) {
                                    if remote_inland_px(est, layout_data) >= REMOTE_MIN_INLAND_PX {
                                        remote_return_armed.store(true, Ordering::SeqCst);
                                    }
                                }
                                remote_return_armed.load(Ordering::SeqCst)
                                    && should_return_to_local(est, layout_data)
                            }
                        } else {
                            false
                        }
                    };

                    if in_grace {
                        let mut c = mouse_send.coalesce.lock().await;
                        c.push(*dx, *dy);
                        mouse_send.flush_notify.notify_one();
                        return;
                    }

                    if go_back {
                        let est_x = remote_cursor_x_est.load(Ordering::SeqCst);
                        let est_y = remote_cursor_y_est.load(Ordering::SeqCst);
                        try_switch_back_to_local(
                            Arc::clone(&focus),
                            Arc::clone(&layout),
                            Arc::clone(&input),
                            bus.clone(),
                            Arc::clone(&active),
                            workspace_cursor.clone(),
                            remote_switch_grace.clone(),
                            remote_return_armed.clone(),
                            est_x,
                            est_y,
                            focus_transition.clone(),
                            session.clone(),
                        )
                        .await;
                        return;
                    }
                    {
                        let mut c = mouse_send.coalesce.lock().await;
                        c.push(*dx, *dy);
                    }
                    mouse_send.flush_notify.notify_one();
                }
                InputEvent::MouseButton { .. } | InputEvent::MouseScroll { .. } => {
                    flush_mouse_to_peer(&mouse_send, &input_tx, seq).await;
                    if let Some(tx) = input_tx.lock().await.as_ref() {
                        let n = seq.fetch_add(1, Ordering::SeqCst);
                        if let Ok(bytes) = encode_input_payload(n, &ev) {
                            debug!(?ev, seq = n, "input enviado ao remoto peer");
                            if tx.send(bytes).await.is_err() {
                                warn!("falha ao enviar input no stream");
                            }
                        }
                    }
                }
                InputEvent::Key { .. } => {
                    if let Some(tx) = input_tx.lock().await.as_ref() {
                        let n = seq.fetch_add(1, Ordering::SeqCst);
                        if let Ok(bytes) = encode_input_payload(n, &ev) {
                            debug!(?ev, seq = n, "tecla encaminhada ao remoto");
                            if tx.send(bytes).await.is_err() {
                                warn!("falha ao enviar tecla no stream");
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

async fn try_edge_switch(
    screen_x: i32,
    screen_y: i32,
    focus: Arc<Mutex<FocusState>>,
    layout: Arc<Mutex<Option<MonitorLayout>>>,
    input: Arc<dyn InputBackend>,
    bus: EventBus,
    active: Arc<Mutex<Option<PeerId>>>,
    input_tx: Arc<Mutex<Option<StreamSender>>>,
    remote_cursor_x_est: Arc<AtomicI32>,
    remote_cursor_y_est: Arc<AtomicI32>,
    workspace_cursor: Arc<Mutex<Option<Arc<dyn WorkspaceGlobalCursor>>>>,
    remote_switch_grace: Arc<Mutex<Option<Instant>>>,
    remote_return_armed: Arc<AtomicBool>,
    focus_transition: Arc<Mutex<()>>,
    session: SessionCursorCtx,
) {
    let Ok(_transition_guard) = focus_transition.try_lock() else {
        debug!("transição local→remoto já em andamento — ignorando");
        return;
    };

    let layout_guard = layout.lock().await;
    let Some(layout_data) = layout_guard.as_ref() else {
        return;
    };
    if !should_switch_to_remote(
        EdgeDetectInput {
            screen_x,
            screen_y,
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

    let edge_x = layout_data.local_exit_edge_coord();
    let (clip_x, clip_y, clip_w, clip_h) = layout_data.local_clip_rect_while_remote();
    let safe_x = clip_x + i32::try_from(clip_w).unwrap_or(1920) / 2;
    let safe_y = clip_y + i32::try_from(clip_h).unwrap_or(1080) / 2;

    // Ponto de entrada no monitor remoto (proporção preservada no eixo perpendicular)
    let remote_entry = layout_data.map_crossing_point(screen_x, screen_y);

    remote_cursor_x_est.store(remote_entry.0, Ordering::SeqCst);
    remote_cursor_y_est.store(remote_entry.1, Ordering::SeqCst);

    drop(layout_guard);

    info!(
        %screen_x,
        %screen_y,
        edge = edge_x,
        remote_entry_x = remote_entry.0,
        remote_entry_y = remote_entry.1,
        %peer,
        "borda atingida — trocando foco para remoto"
    );

    let from = current;
    let clip_rect = (clip_x, clip_y, clip_w, clip_h);

    // Grace ANTES de trocar foco — evita race onde o 1º MouseMove remoto dispara go_back.
    remote_return_armed.store(false, Ordering::SeqCst);
    *remote_switch_grace.lock().await = Some(Instant::now());

    if let Err(err) = input.transition_to_remote(safe_x, safe_y, clip_rect).await {
        error!(?err, "falha na transição para remoto — mantendo foco local");
        *remote_switch_grace.lock().await = None;
        let _ = input.set_cursor_clipped(None).await;
        return;
    }

    if let Some(peer_id) = *active.lock().await {
        session
            .update_local(&input, peer_id, screen_x, screen_y)
            .await;
    }

    // Transição lógica de foco — só após sucesso físico
    switch_focus(
        FocusTarget::Remote(peer),
        from,
        Arc::clone(&focus),
        Arc::clone(&layout),
        Arc::clone(&input),
        bus,
        workspace_cursor,
        active,
    )
    .await;

    input.reset_mouse_delta_baseline();

    // Enviar warp absoluto como primeiro frame para posicionar cursor no receiver
    if let Some(tx) = input_tx.lock().await.as_ref() {
        let warp_ev = InputEvent::MouseWarpAbsolute {
            x: remote_entry.0,
            y: remote_entry.1,
        };
        let n = 0u64; // seq não importa para warp inicial; será sobrescrito pelo flush normal
        if let Ok(bytes) = encode_input_payload(n, &warp_ev) {
            if tx.send(bytes).await.is_err() {
                warn!("falha ao enviar warp absoluto inicial ao remoto");
            }
        }
    }
}

async fn try_switch_back_to_local(
    focus: Arc<Mutex<FocusState>>,
    layout: Arc<Mutex<Option<MonitorLayout>>>,
    input: Arc<dyn InputBackend>,
    bus: EventBus,
    active: Arc<Mutex<Option<PeerId>>>,
    workspace_cursor: Arc<Mutex<Option<Arc<dyn WorkspaceGlobalCursor>>>>,
    remote_switch_grace: Arc<Mutex<Option<Instant>>>,
    remote_return_armed: Arc<AtomicBool>,
    remote_x: i32,
    remote_y: i32,
    focus_transition: Arc<Mutex<()>>,
    session: SessionCursorCtx,
) {
    if !remote_return_armed.swap(false, Ordering::SeqCst) {
        return;
    }

    let Ok(_transition_guard) = focus_transition.try_lock() else {
        remote_return_armed.store(true, Ordering::SeqCst);
        return;
    };

    let current = focus.lock().await.target.clone();
    let FocusTarget::Remote(_peer) = &current else {
        return;
    };

    let layout_guard = layout.lock().await;
    let Some(layout_data) = layout_guard.as_ref() else {
        remote_return_armed.store(true, Ordering::SeqCst);
        return;
    };

    let (warp_x, warp_y) = layout_data.map_return_point(remote_x, remote_y);
    drop(layout_guard);

    if let Err(err) = input.set_cursor_clipped(None).await {
        warn!(?err, "falha ao liberar clip ao retornar para local");
        remote_return_armed.store(true, Ordering::SeqCst);
        return;
    }
    if let Err(err) = input.restore_cursor_system().await {
        warn!(?err, "falha ao restaurar cursor ao retornar para local");
        remote_return_armed.store(true, Ordering::SeqCst);
        return;
    }
    if let Err(err) = input.warp_cursor(warp_x, warp_y).await {
        warn!(?err, "falha ao reposicionar cursor ao retornar para local");
        remote_return_armed.store(true, Ordering::SeqCst);
        return;
    }
    input.reset_mouse_delta_baseline();

    info!(warp_x, warp_y, "voltando para foco local via borda oposta");

    session.peer_controls_us.store(false, Ordering::SeqCst);
    if let Some(peer_id) = *active.lock().await {
        session
            .update_local(&input, peer_id, warp_x, warp_y)
            .await;
    }

    switch_focus(
        FocusTarget::Local,
        current,
        focus,
        layout,
        input,
        bus,
        workspace_cursor,
        active,
    )
    .await;
    *remote_switch_grace.lock().await = None;
    remote_return_armed.store(false, Ordering::SeqCst);
}

async fn switch_focus(
    to: FocusTarget,
    _from: FocusTarget,
    focus: Arc<Mutex<FocusState>>,
    layout: Arc<Mutex<Option<MonitorLayout>>>,
    input: Arc<dyn InputBackend>,
    bus: EventBus,
    workspace_cursor: Arc<Mutex<Option<Arc<dyn WorkspaceGlobalCursor>>>>,
    active_kvm: Arc<Mutex<Option<PeerId>>>,
) {
    let transition = {
        let mut f = focus.lock().await;
        apply_focus_target(&mut f, to.clone())
    };

    let kvm_active = active_kvm.lock().await.is_some();

    match &to {
        FocusTarget::Local => {
            input.set_raw_mouse_capture(false);
            input.set_pass_through(true);
            let _ = input.set_cursor_clipped(None).await;
            let _ = input.set_cursor_visible(true).await;
            input.reset_mouse_delta_baseline();
            // Com KVM single-connection ativo, não restaurar cursor global do workspace —
            // o warp de retorno KVM já posicionou na borda correta.
            if !kvm_active {
                if let Some(bridge) = workspace_cursor.lock().await.as_ref() {
                    if let Some((x, y)) = bridge.restore_cursor_on_focus().await {
                        if input.warp_cursor_signed(x, y).await.is_err() {
                            warn!("falha ao restaurar cursor global do workspace");
                        }
                    }
                }
            }
        }
        FocusTarget::Remote(_) => {
            input.set_pass_through(false);
            input.set_raw_mouse_capture(true);
            input.reset_mouse_delta_baseline();
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

impl InputControlService {
    async fn register_session_cursor_at_connection(&self, peer_id: PeerId) -> Result<(), DomainError> {
        let ctx = self.session_ctx();
        let local_device = self.require_local_device_id().await?;
        let (x, y) = self
            .input
            .get_cursor_pos()
            .await
            .map_err(|e| internal_err(&e.to_string()))?;
        let seq = ctx.seq.fetch_add(1, Ordering::SeqCst) + 1;
        ctx.store(local_device, x, y, seq).await;
        ctx.broadcast(peer_id, local_device, x, y, seq).await;
        info!(%peer_id, x, y, seq, "session cursor: registrado na conexão");
        Ok(())
    }

    pub async fn handle_session_data_payload(&self, peer_id: PeerId, payload: Payload) {
        let ctx = self.session_ctx();
        match payload {
            Payload::SessionCursorSync(p) => {
                let host = DeviceId::from_uuid(p.device_id);
                let local_device = match *self.local_device_id.lock().await {
                    Some(d) => d,
                    None => return,
                };
                if p.seq <= ctx.seq.load(Ordering::SeqCst) {
                    return;
                }
                ctx.store(host, p.x, p.y, p.seq).await;
                if host == local_device && self.peer_controls_us.load(Ordering::SeqCst) {
                    let _ = self.input.warp_cursor_signed(p.x, p.y).await;
                    self.input.reset_mouse_delta_baseline();
                    self.peer_controls_us.store(false, Ordering::SeqCst);
                    self.input.set_pass_through(true);
                } else if host != local_device {
                    let focus = self.focus.lock().await.target.clone();
                    if matches!(focus, FocusTarget::Remote(pid) if pid == peer_id) {
                        if let Some(layout) = self.layout.lock().await.as_ref() {
                            let (rx, ry) =
                                layout.peer_cursor_os_to_remote_relative(p.x, p.y);
                            self.remote_cursor_x_est.store(rx, Ordering::SeqCst);
                            self.remote_cursor_y_est.store(ry, Ordering::SeqCst);
                        }
                    }
                }
                info!(%peer_id, x = p.x, y = p.y, seq = p.seq, ?host, "session cursor: sync recebido");
            }
            Payload::SessionInputTakeover(p) => {
                let taker = DeviceId::from_uuid(p.device_id);
                let local_device = match *self.local_device_id.lock().await {
                    Some(d) => d,
                    None => return,
                };
                if taker == local_device {
                    return;
                }
                if p.seq <= ctx.seq.load(Ordering::SeqCst) {
                    return;
                }
                ctx.store(taker, p.x, p.y, p.seq).await;
                let focus = self.focus.lock().await.target.clone();
                if matches!(focus, FocusTarget::Remote(pid) if pid == peer_id) {
                    info!(%peer_id, "session cursor: peer retomou controle — voltando foco local");
                    drop(focus);
                    let mut f = self.focus.lock().await;
                    f.target = FocusTarget::Local;
                    drop(f);
                    self.input.set_pass_through(true);
                    self.input.set_raw_mouse_capture(false);
                    let _ = self.input.set_cursor_clipped(None).await;
                    let _ = self.input.set_cursor_visible(true).await;
                    self.input.reset_mouse_delta_baseline();
                    self.peer_controls_us.store(false, Ordering::SeqCst);
                    self.remote_return_armed.store(false, Ordering::SeqCst);
                    *self.remote_switch_grace.lock().await = None;
                }
            }
            _ => {}
        }
    }
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
        async fn get_cursor_pos(&self) -> anyhow::Result<(i32, i32)> {
            Ok((0, 0))
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
            1919,
            540,
            Arc::clone(&svc.focus),
            Arc::clone(&svc.layout),
            Arc::new(MockInput),
            svc.bus.clone(),
            Arc::clone(&svc.active_peer),
            Arc::clone(&svc.input_tx),
            Arc::new(AtomicI32::new(0)),
            Arc::new(AtomicI32::new(0)),
            Arc::clone(&svc.workspace_cursor),
            Arc::clone(&svc.remote_switch_grace),
            Arc::clone(&svc.remote_return_armed),
            Arc::clone(&svc.focus_transition),
            svc.session_ctx(),
        )
        .await;

        let f = svc.get_focus_state().await;
        assert!(matches!(f.target, FocusTarget::Remote(_)));
    }

    #[tokio::test]
    async fn lock_mode_prevents_edge_switch() {
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
            bus.clone(),
        );
        let focus = Arc::clone(&svc.focus);
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
            1919,
            540,
            Arc::clone(&focus),
            Arc::clone(&layout),
            Arc::new(MockInput),
            bus,
            active,
            input_tx,
            Arc::new(AtomicI32::new(0)),
            Arc::new(AtomicI32::new(0)),
            Arc::new(Mutex::new(None)),
            Arc::new(Mutex::new(None)),
            Arc::new(AtomicBool::new(false)),
            Arc::clone(&svc.focus_transition),
            svc.session_ctx(),
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
