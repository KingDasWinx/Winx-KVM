use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use tokio::net::UdpSocket;
use tracing::info;
use winx_domain::shared::ids::PeerId;
use winx_protocol::DiagPing;

use crate::{
    ports::{pairing::pairing_socket_addr, IdentityStore},
    use_cases::{DiscoveryService, InputControlService, TransportService},
};

#[derive(Debug, Clone, Serialize)]
pub struct ProbeResult {
    pub service: String,
    pub ok: bool,
    pub latency_ms: Option<u32>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LabProbeResults {
    pub peer_id: String,
    pub probes: Vec<ProbeResult>,
    pub ran_at_ms: u64,
}

pub struct ConnectionLabService {
    discovery: Arc<DiscoveryService>,
    transport: Arc<TransportService>,
    input: Arc<InputControlService>,
    identity: Arc<dyn IdentityStore>,
}

impl ConnectionLabService {
    pub fn new(
        discovery: Arc<DiscoveryService>,
        transport: Arc<TransportService>,
        input: Arc<InputControlService>,
        identity: Arc<dyn IdentityStore>,
    ) -> Self {
        Self {
            discovery,
            transport,
            input,
            identity,
        }
    }

    pub async fn run_suite(&self, peer_id: PeerId) -> LabProbeResults {
        let mut probes = Vec::new();
        probes.push(self.probe_mdns(peer_id).await);
        probes.push(self.probe_pairing_udp(peer_id).await);
        probes.push(self.probe_quic(peer_id).await);
        probes.push(self.probe_quic_control(peer_id).await);
        probes.push(self.probe_input_stream(peer_id).await);

        LabProbeResults {
            peer_id: peer_id.to_string(),
            probes,
            ran_at_ms: now_ms(),
        }
    }

    async fn probe_mdns(&self, peer_id: PeerId) -> ProbeResult {
        let found = self
            .discovery
            .list_peers_enriched(self.identity.as_ref())
            .await
            .ok()
            .is_some_and(|peers| peers.iter().any(|p| p.peer.id == peer_id));

        ProbeResult {
            service: "mdns".into(),
            ok: found,
            latency_ms: None,
            detail: if found {
                "peer visível na rede".into()
            } else {
                "peer não encontrado no registry mDNS".into()
            },
        }
    }

    async fn probe_pairing_udp(&self, peer_id: PeerId) -> ProbeResult {
        let Some(addr) = self.resolve_peer_addr(peer_id).await else {
            return ProbeResult {
                service: "pairing_udp".into(),
                ok: false,
                latency_ms: None,
                detail: "endereço do peer não resolvido".into(),
            };
        };

        let target = pairing_socket_addr(addr);
        let nonce = now_ms();
        let ping = DiagPing { nonce };

        match probe_udp_ping(target, ping, Duration::from_secs(2)).await {
            Ok(latency_ms) => ProbeResult {
                service: "pairing_udp".into(),
                ok: true,
                latency_ms: Some(latency_ms),
                detail: format!("pong nonce={nonce}"),
            },
            Err(err) => ProbeResult {
                service: "pairing_udp".into(),
                ok: false,
                latency_ms: None,
                detail: err.to_string(),
            },
        }
    }

    async fn probe_quic(&self, peer_id: PeerId) -> ProbeResult {
        if !self.transport.is_peer_connected(peer_id).await {
            return ProbeResult {
                service: "quic".into(),
                ok: false,
                latency_ms: None,
                detail: "peer não conectado via QUIC".into(),
            };
        }

        match self.transport.get_stats(peer_id).await {
            Ok(stats) => ProbeResult {
                service: "quic".into(),
                ok: true,
                latency_ms: Some(stats.rtt_ms),
                detail: format!(
                    "tx={} rx={} lost={}",
                    stats.tx_bytes, stats.rx_bytes, stats.lost_packets
                ),
            },
            Err(err) => ProbeResult {
                service: "quic".into(),
                ok: false,
                latency_ms: None,
                detail: err.message.clone(),
            },
        }
    }

    async fn probe_quic_control(&self, peer_id: PeerId) -> ProbeResult {
        if !self.transport.is_peer_connected(peer_id).await {
            return ProbeResult {
                service: "quic_control".into(),
                ok: false,
                latency_ms: None,
                detail: "peer não conectado".into(),
            };
        }

        let transport_rtt = self
            .transport
            .get_stats(peer_id)
            .await
            .ok()
            .map(|s| s.rtt_ms);

        match self
            .transport
            .probe_control_heartbeat_for_peer(peer_id)
            .await
        {
            Ok(rtt) => {
                let detail = match transport_rtt {
                    Some(trtt) => format!(
                        "HeartbeatAck recebido (RTT heartbeat={rtt} ms, RTT transporte={trtt} ms)"
                    ),
                    None => format!("HeartbeatAck recebido (RTT heartbeat={rtt} ms)"),
                };
                ProbeResult {
                    service: "quic_control".into(),
                    ok: true,
                    latency_ms: Some(rtt),
                    detail,
                }
            }
            Err(err) => ProbeResult {
                service: "quic_control".into(),
                ok: false,
                latency_ms: None,
                detail: format!("control_stream_no_ack: {}", err.message),
            },
        }
    }

    async fn probe_input_stream(&self, peer_id: PeerId) -> ProbeResult {
        if !self.transport.is_peer_connected(peer_id).await {
            return ProbeResult {
                service: "input_stream".into(),
                ok: false,
                latency_ms: None,
                detail: "peer não conectado".into(),
            };
        }

        let started = std::time::Instant::now();
        match self.input.send_lab_ping(peer_id).await {
            Ok(()) => ProbeResult {
                service: "input_stream".into(),
                ok: true,
                latency_ms: Some(
                    u32::try_from(started.elapsed().as_millis().min(u128::from(u32::MAX)))
                        .unwrap_or(u32::MAX),
                ),
                detail: "ping Input enviado no stream".into(),
            },
            Err(err) => ProbeResult {
                service: "input_stream".into(),
                ok: false,
                latency_ms: None,
                detail: format!("input_stream_send_failed: {}", err.message),
            },
        }
    }

    async fn resolve_peer_addr(&self, peer_id: PeerId) -> Option<SocketAddr> {
        let peers = self
            .discovery
            .list_peers_enriched(self.identity.as_ref())
            .await
            .ok()?;
        peers
            .into_iter()
            .find(|p| p.peer.id == peer_id)
            .and_then(|p| p.peer.addresses.first().copied())
    }

    pub fn input_control(&self) -> Arc<InputControlService> {
        Arc::clone(&self.input)
    }
}

async fn probe_udp_ping(
    target: SocketAddr,
    ping: DiagPing,
    timeout: Duration,
) -> anyhow::Result<u32> {
    let sock = UdpSocket::bind("0.0.0.0:0").await?;
    let started = std::time::Instant::now();
    sock.send_to(&ping.encode(), target).await?;

    let mut buf = [0u8; 64];
    let result = tokio::time::timeout(timeout, sock.recv_from(&mut buf)).await;
    match result {
        Ok(Ok((len, _))) => {
            let data = &buf[..len];
            let nonce = DiagPing::decode_pong(data)
                .ok_or_else(|| anyhow::anyhow!("resposta WINP inválida"))?;
            if nonce != ping.nonce {
                anyhow::bail!("nonce do pong não confere");
            }
            let latency_ms = u32::try_from(started.elapsed().as_millis().min(u128::from(u32::MAX)))
                .unwrap_or(u32::MAX);
            info!(%latency_ms, "probe pairing UDP ok");
            Ok(latency_ms)
        }
        Ok(Err(err)) => Err(err.into()),
        Err(_) => anyhow::bail!("timeout aguardando pong UDP"),
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use std::{
        net::{IpAddr, Ipv4Addr},
        sync::Arc,
    };

    use tokio::sync::Mutex;
    use uuid::Uuid;
    use winx_domain::{
        discovery::{DiscoveredPeer, DiscoveryRegistry},
        identity::{PublicKey, TrustedPeer},
        shared::ids::PeerId,
        transport::StreamKind,
    };

    use super::*;
    use crate::{
        bus::EventBus,
        ports::{
            transport::{ActiveConnection, IncomingConnection, StreamReceiver, StreamSender},
            IdentityStore, InputBackend, MonitorBackend, TransportAdapter,
        },
        use_cases::{InputControlService, TransportService},
    };

    #[tokio::test]
    async fn mdns_probe_ok_when_peer_in_registry() {
        let peer_id = PeerId::from_uuid(Uuid::new_v4());
        let registry = Arc::new(Mutex::new(DiscoveryRegistry::new()));
        registry.lock().await.appeared(DiscoveredPeer::new(
            peer_id,
            "Peer",
            "AA:BB:CC:DD:EE:FF:00:11",
            vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7878)],
        ));

        let bus = EventBus::new();
        let discovery = Arc::new(DiscoveryService::new(
            Arc::new(NoopDiscovery),
            Arc::clone(&registry),
            bus,
        ));
        let transport = Arc::new(TransportService::new(
            Arc::new(NoopTransportAdapter),
            Arc::new(MockIdentity {
                peers: vec![TrustedPeer::new(peer_id, "p", PublicKey::new([0u8; 32]))],
            }),
            Arc::clone(&registry),
            EventBus::new(),
        ));
        let input = Arc::new(InputControlService::new(
            Arc::new(NoopInput),
            Arc::new(NoopMonitors),
            Arc::clone(&transport),
            EventBus::new(),
        ));

        let identity = Arc::new(MockIdentity {
            peers: vec![TrustedPeer::new(peer_id, "p", PublicKey::new([0u8; 32]))],
        });
        let lab = ConnectionLabService::new(discovery, transport, input, identity);

        let result = lab.probe_mdns(peer_id).await;
        assert!(result.ok);
    }

    struct NoopDiscovery;

    #[async_trait::async_trait]
    impl crate::ports::DiscoveryAdapter for NoopDiscovery {
        async fn announce(&self, _: &crate::ports::discovery::AnnounceInfo) -> anyhow::Result<()> {
            Ok(())
        }

        async fn reannounce(
            &self,
            _: &crate::ports::discovery::AnnounceInfo,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn stop_announcing(&self) -> anyhow::Result<()> {
            Ok(())
        }

        async fn start_browsing(
            &self,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<crate::ports::discovery::DiscoveryEvent>>
        {
            let (_, rx) = tokio::sync::mpsc::channel(1);
            Ok(rx)
        }

        async fn stop_browsing(&self) -> anyhow::Result<()> {
            Ok(())
        }

        async fn set_interfaces(&self, _: &[String]) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct NoopTransportAdapter;

    #[async_trait::async_trait]
    impl TransportAdapter for NoopTransportAdapter {
        async fn listen(
            &self,
            _: u16,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<IncomingConnection>> {
            let (_, rx) = tokio::sync::mpsc::channel(1);
            Ok(rx)
        }
        async fn connect(&self, _: SocketAddr, _: [u8; 32]) -> anyhow::Result<ActiveConnection> {
            let (_, rx) = tokio::sync::mpsc::channel(8);
            Ok(ActiveConnection {
                conn_id: winx_domain::shared::ids::SessionId::new(),
                inbound_streams: rx,
            })
        }
        async fn open_stream(
            &self,
            _: winx_domain::shared::ids::SessionId,
            _: StreamKind,
        ) -> anyhow::Result<(StreamSender, StreamReceiver)> {
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

    struct MockIdentity {
        peers: Vec<TrustedPeer>,
    }

    #[async_trait::async_trait]
    impl IdentityStore for MockIdentity {
        async fn load_device(&self) -> anyhow::Result<Option<winx_domain::identity::Device>> {
            Ok(None)
        }
        async fn save_device(&self, _: &winx_domain::identity::Device) -> anyhow::Result<()> {
            Ok(())
        }
        async fn load_peers(&self) -> anyhow::Result<Vec<TrustedPeer>> {
            Ok(self.peers.clone())
        }
        async fn save_peer(&self, _: &TrustedPeer) -> anyhow::Result<()> {
            Ok(())
        }
        async fn remove_peer(&self, _: PeerId) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct NoopInput;

    #[async_trait::async_trait]
    impl InputBackend for NoopInput {
        async fn start_capture(
            &self,
            _: Box<dyn Fn(winx_domain::input_control::InputEvent) + Send + Sync>,
            _: Box<dyn Fn(winx_domain::input_control::HotkeyAction) + Send + Sync>,
        ) -> anyhow::Result<crate::ports::CaptureHandle> {
            Ok(crate::ports::CaptureHandle { id: 1 })
        }
        async fn stop_capture(&self, _: crate::ports::CaptureHandle) -> anyhow::Result<()> {
            Ok(())
        }
        async fn inject(&self, _: winx_domain::input_control::InputEvent) -> anyhow::Result<()> {
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
    async fn pairing_probe_fails_when_no_pong() {
        let target = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 59999);
        let ping = DiagPing { nonce: 42 };
        assert!(
            probe_udp_ping(target, ping, Duration::from_millis(200))
                .await
                .is_err(),
            "sem listener WINP na porta, o probe deve falhar"
        );
    }

    struct NoopMonitors;

    #[async_trait::async_trait]
    impl MonitorBackend for NoopMonitors {
        async fn enumerate_local_monitors(
            &self,
        ) -> anyhow::Result<Vec<winx_domain::input_control::MonitorRect>> {
            Ok(vec![])
        }
    }
}
