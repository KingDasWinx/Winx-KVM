use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::{info, warn};
use winx_domain::{
    data_exchange::{
        events::{ClipboardChanged, ClipboardReceived},
        ClipboardText, ContentHash, MAX_CLIPBOARD_TEXT_BYTES,
    },
    shared::{ids::PeerId, DomainError, DomainErrorCode, DomainEvent},
    transport::StreamKind,
};

use crate::{
    bus::EventBus,
    ports::{transport::StreamSender, ClipboardBackend, ClipboardWatcherHandle},
    protocol_convert::encode_clipboard_payload,
    use_cases::TransportService,
};

const POLL_MS: u64 = 200;

pub struct ClipboardService {
    clipboard: Arc<dyn ClipboardBackend>,
    transport: Arc<TransportService>,
    bus: EventBus,
    local_peer_id: PeerId,
    auto_sync: Arc<Mutex<bool>>,
    active_peer: Arc<Mutex<Option<PeerId>>>,
    data_tx: Arc<Mutex<Option<StreamSender>>>,
    last_sent_hash: Arc<Mutex<Option<ContentHash>>>,
    last_applied_hash: Arc<Mutex<Option<ContentHash>>>,
    suppress_local_emit: Arc<AtomicBool>,
    watcher: Arc<Mutex<Option<ClipboardWatcherHandle>>>,
}

impl ClipboardService {
    pub fn new(
        clipboard: Arc<dyn ClipboardBackend>,
        transport: Arc<TransportService>,
        bus: EventBus,
        local_peer_id: PeerId,
        auto_sync: bool,
    ) -> Self {
        Self {
            clipboard,
            transport,
            bus,
            local_peer_id,
            auto_sync: Arc::new(Mutex::new(auto_sync)),
            active_peer: Arc::new(Mutex::new(None)),
            data_tx: Arc::new(Mutex::new(None)),
            last_sent_hash: Arc::new(Mutex::new(None)),
            last_applied_hash: Arc::new(Mutex::new(None)),
            suppress_local_emit: Arc::new(AtomicBool::new(false)),
            watcher: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn get_auto_sync(&self) -> bool {
        *self.auto_sync.lock().await
    }

    pub async fn set_auto_sync(&self, enabled: bool) {
        *self.auto_sync.lock().await = enabled;
    }

    pub async fn enable_for_peer(&self, peer_id: PeerId) -> Result<(), DomainError> {
        if !self.transport.is_peer_connected(peer_id).await {
            return Err(DomainError::new(
                DomainErrorCode::TransportConnectionFailed,
                "peer não conectado via QUIC",
            ));
        }

        *self.active_peer.lock().await = Some(peer_id);

        let (tx, mut rx) = self
            .transport
            .open_stream_for_peer(peer_id, StreamKind::Data)
            .await?;
        *self.data_tx.lock().await = Some(tx);

        let clipboard_recv = Arc::clone(&self.clipboard);
        let local_peer = self.local_peer_id;
        let last_applied = Arc::clone(&self.last_applied_hash);
        let suppress = Arc::clone(&self.suppress_local_emit);
        let bus_recv = self.bus.clone();

        tokio::spawn(async move {
            while let Some(bytes) = rx.recv().await {
                if let Ok(frame) = winx_protocol::decode(&bytes) {
                    if let winx_protocol::Payload::Clipboard(p) = frame.payload {
                        if let Err(err) = handle_remote_payload(
                            &p,
                            local_peer,
                            &clipboard_recv,
                            &last_applied,
                            &suppress,
                            &bus_recv,
                            peer_id,
                        )
                        .await
                        {
                            warn!(?err, "falha ao aplicar clipboard remoto");
                        }
                    }
                }
            }
        });

        if self.watcher.lock().await.is_none() {
            let service_clipboard = Arc::clone(&self.clipboard);
            let auto_sync = Arc::clone(&self.auto_sync);
            let data_tx = Arc::clone(&self.data_tx);
            let active = Arc::clone(&self.active_peer);
            let last_sent = Arc::clone(&self.last_sent_hash);
            let last_applied = Arc::clone(&self.last_applied_hash);
            let suppress = Arc::clone(&self.suppress_local_emit);
            let bus_local = self.bus.clone();
            let local_peer = self.local_peer_id;

            let on_change = move |text: String| {
                let auto_sync = Arc::clone(&auto_sync);
                let data_tx = Arc::clone(&data_tx);
                let active = Arc::clone(&active);
                let last_sent = Arc::clone(&last_sent);
                let last_applied = Arc::clone(&last_applied);
                let suppress = Arc::clone(&suppress);
                let bus = bus_local.clone();
                let local_peer = local_peer;
                tokio::spawn(async move {
                    on_local_change(
                        text,
                        auto_sync,
                        data_tx,
                        active,
                        last_sent,
                        last_applied,
                        suppress,
                        bus,
                        local_peer,
                    )
                    .await;
                });
            };

            let handle = service_clipboard
                .start_watcher(POLL_MS, Box::new(on_change))
                .await
                .map_err(|e| internal_err(&e.to_string()))?;
            *self.watcher.lock().await = Some(handle);
        }

        self.spawn_bus_subscriber();
        info!(%peer_id, "clipboard sync habilitado");
        Ok(())
    }

    fn spawn_bus_subscriber(&self) {
        let mut rx = self.bus.subscribe();
        let active = Arc::clone(&self.active_peer);
        let data_tx = Arc::clone(&self.data_tx);

        tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                if let DomainEvent::ConnectionLost(e) = event {
                    let guard = active.lock().await;
                    if *guard == Some(e.peer_id) {
                        drop(guard);
                        *active.lock().await = None;
                        *data_tx.lock().await = None;
                    }
                }
            }
        });
    }
}

#[allow(clippy::too_many_arguments)]
async fn on_local_change(
    text: String,
    auto_sync: Arc<Mutex<bool>>,
    data_tx: Arc<Mutex<Option<StreamSender>>>,
    active: Arc<Mutex<Option<PeerId>>>,
    last_sent: Arc<Mutex<Option<ContentHash>>>,
    last_applied: Arc<Mutex<Option<ContentHash>>>,
    suppress: Arc<AtomicBool>,
    bus: EventBus,
    local_peer_id: PeerId,
) {
    if !*auto_sync.lock().await {
        return;
    }
    if suppress.load(Ordering::SeqCst) {
        return;
    }
    if active.lock().await.is_none() {
        return;
    }

    let clip = ClipboardText::new(text);
    if clip.text.len() > MAX_CLIPBOARD_TEXT_BYTES {
        warn!("clipboard local excede tamanho máximo");
        return;
    }

    {
        let applied = last_applied.lock().await;
        if *applied == Some(clip.hash) {
            return;
        }
    }
    {
        let sent = last_sent.lock().await;
        if *sent == Some(clip.hash) {
            return;
        }
    }

    let Some(tx) = data_tx.lock().await.clone() else {
        return;
    };

    let bytes = match encode_clipboard_payload(
        local_peer_id.as_uuid(),
        *clip.hash.as_bytes(),
        &clip.text,
    ) {
        Ok(b) => b,
        Err(err) => {
            warn!(?err, "falha ao codificar clipboard");
            return;
        }
    };

    if tx.send(bytes).await.is_err() {
        warn!("falha ao enviar clipboard no stream Data");
        return;
    }

    *last_sent.lock().await = Some(clip.hash);
    bus.publish(DomainEvent::ClipboardChanged(ClipboardChanged {
        hash: clip.hash,
        byte_len: clip.text.len(),
    }));
}

async fn handle_remote_payload(
    payload: &winx_protocol::ClipboardPayload,
    local_peer_id: PeerId,
    clipboard: &Arc<dyn ClipboardBackend>,
    last_applied: &Arc<Mutex<Option<ContentHash>>>,
    suppress: &Arc<AtomicBool>,
    bus: &EventBus,
    from_peer: PeerId,
) -> Result<(), DomainError> {
    if payload.origin_peer_id == local_peer_id.as_uuid() {
        return Ok(());
    }

    let hash = ContentHash::from_bytes(payload.content_hash);
    if *last_applied.lock().await == Some(hash) {
        return Ok(());
    }

    if payload.text.len() > MAX_CLIPBOARD_TEXT_BYTES {
        return Err(DomainError::new(
            DomainErrorCode::ClipboardPayloadTooLarge,
            "texto de clipboard excede 5MB",
        ));
    }

    suppress.store(true, Ordering::SeqCst);
    clipboard
        .set_text(payload.text.clone())
        .await
        .map_err(|e| internal_err(&e.to_string()))?;
    suppress.store(false, Ordering::SeqCst);

    *last_applied.lock().await = Some(hash);
    bus.publish(DomainEvent::ClipboardReceived(ClipboardReceived {
        from_peer,
        hash,
    }));
    Ok(())
}

fn internal_err(msg: &str) -> DomainError {
    DomainError::new(DomainErrorCode::InternalError, msg)
}

#[cfg(test)]
#[allow(dead_code)]
mod tests {
    use tokio::sync::mpsc;
    use uuid::Uuid;

    use super::*;
    use crate::ports::TransportAdapter;

    struct MockClipboard {
        text: Mutex<Option<String>>,
    }

    #[async_trait::async_trait]
    impl ClipboardBackend for MockClipboard {
        async fn start_watcher(
            &self,
            _: u64,
            _: Box<dyn Fn(String) + Send + Sync>,
        ) -> anyhow::Result<ClipboardWatcherHandle> {
            Ok(ClipboardWatcherHandle { id: 1 })
        }

        async fn stop_watcher(&self, _: ClipboardWatcherHandle) -> anyhow::Result<()> {
            Ok(())
        }

        async fn get_text(&self) -> anyhow::Result<Option<String>> {
            Ok(self.text.lock().await.clone())
        }

        async fn set_text(&self, text: String) -> anyhow::Result<()> {
            *self.text.lock().await = Some(text);
            Ok(())
        }
    }

    struct MockTransport {
        tx: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
    }

    #[async_trait::async_trait]
    impl TransportAdapter for MockTransport {
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
            let (_, rx) = mpsc::channel(8);
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
            let (tx, rx) = mpsc::channel(8);
            *self.tx.lock().await = Some(tx.clone());
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

    #[tokio::test]
    async fn does_not_retransmit_same_hash_after_remote_apply() {
        let local = PeerId::from_uuid(Uuid::new_v4());
        let remote = PeerId::from_uuid(Uuid::new_v4());
        let bus = EventBus::new();
        // Testa handle_remote + on_local_change isolados (anti-loop por hash).
        let clipboard: Arc<dyn ClipboardBackend> = Arc::new(MockClipboard {
            text: Mutex::new(None),
        });
        let last_applied = Arc::new(Mutex::new(None));
        let suppress = Arc::new(AtomicBool::new(false));

        let text = "hello peer".to_string();
        let hash = ContentHash::of_text(&text);
        let payload = winx_protocol::ClipboardPayload {
            origin_peer_id: remote.as_uuid(),
            content_hash: *hash.as_bytes(),
            text: text.clone(),
        };

        handle_remote_payload(
            &payload,
            local,
            &clipboard,
            &last_applied,
            &suppress,
            &bus,
            remote,
        )
        .await
        .unwrap();

        let (tx, _) = mpsc::channel(1);
        on_local_change(
            text,
            Arc::new(Mutex::new(true)),
            Arc::new(Mutex::new(Some(tx))),
            Arc::new(Mutex::new(Some(remote))),
            Arc::new(Mutex::new(None)),
            Arc::clone(&last_applied),
            suppress,
            bus,
            local,
        )
        .await;

        assert_eq!(*last_applied.lock().await, Some(hash));
    }
}
