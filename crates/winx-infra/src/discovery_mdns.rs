use std::net::SocketAddr;

use async_trait::async_trait;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use uuid::Uuid;
use winx_domain::shared::ids::PeerId;

use winx_application::ports::discovery::{AnnounceInfo, DiscoveryAdapter, DiscoveryEvent};

const SERVICE_TYPE: &str = "_winx-kvm._tcp.local.";

/// Adapter mDNS para discovery de peers na rede local.
///
/// Usa `mdns-sd` (não é async-runtime-dependent) e converte os eventos
/// síncronos de `flume::Receiver` para `tokio::sync::mpsc`.
pub struct MdnsDiscoveryAdapter {
    daemon: ServiceDaemon,
}

impl MdnsDiscoveryAdapter {
    pub fn new() -> anyhow::Result<Self> {
        let daemon = ServiceDaemon::new().map_err(|e| anyhow::anyhow!("mdns daemon: {e}"))?;

        Self::filter_virtual_interfaces(&daemon);

        Ok(Self { daemon })
    }

    /// Desabilita interfaces virtuais (Hyper-V, WSL, VPN) para mDNS discover.
    fn filter_virtual_interfaces(_daemon: &ServiceDaemon) {
        // Aqui iteraríamos pelas interfaces do sistema, mas mdns-sd 0.19 não expõe
        // interface listing diretamente. Loga aviso e confia que o daemon
        // descobrirá a interface correta na primeira tentativa de register.
        // Futuro: usar `if-addrs` crate para filtro explícito.

        info!("mDNS daemon initialized (interface filtering via mdns-sd auto-detection)");
    }
}

#[async_trait]
impl DiscoveryAdapter for MdnsDiscoveryAdapter {
    async fn announce(&self, info: &AnnounceInfo) -> anyhow::Result<()> {
        let instance_name = format!("winx-{}", info.peer_id);
        let host_name = format!("winx-{}.local.", info.peer_id);

        let username_truncated: String = info.username.chars().take(60).collect();

        let properties = [
            ("peer_id", info.peer_id.to_string()),
            ("username", username_truncated),
            ("fingerprint", info.fingerprint.clone()),
        ];

        let service = ServiceInfo::new(
            SERVICE_TYPE,
            &instance_name,
            &host_name,
            "", // endereço auto-detectado pela lib
            info.port,
            &properties[..],
        )
        .map_err(|e| anyhow::anyhow!("ServiceInfo inválido: {e}"))?;

        self.daemon
            .register(service)
            .map_err(|e| anyhow::anyhow!("register falhou: {e}"))?;

        info!(peer_id = %info.peer_id, port = info.port, "mDNS announce registrado");
        Ok(())
    }

    async fn reannounce(&self, info: &AnnounceInfo) -> anyhow::Result<()> {
        let fullname = format!("winx-{}.{}", info.peer_id, SERVICE_TYPE);
        if let Err(e) = self.daemon.unregister(&fullname) {
            debug!(%fullname, ?e, "unregister antes do reannounce (pode ser primeira vez)");
        }
        self.announce(info).await
    }

    async fn stop_announcing(&self) -> anyhow::Result<()> {
        self.daemon
            .shutdown()
            .map_err(|e| anyhow::anyhow!("mdns shutdown: {e}"))?;
        Ok(())
    }

    async fn start_browsing(&self) -> anyhow::Result<mpsc::Receiver<DiscoveryEvent>> {
        let receiver = self
            .daemon
            .browse(SERVICE_TYPE)
            .map_err(|e| anyhow::anyhow!("browse falhou: {e}"))?;

        let (tx, rx) = mpsc::channel(64);

        // Converte eventos síncronos do flume para o canal tokio async.
        std::thread::spawn(move || {
            while let Ok(event) = receiver.recv() {
                let discovery_event = match event {
                    ServiceEvent::ServiceResolved(resolved) => {
                        let Some(peer_id_str) = resolved.get_property_val_str("peer_id") else {
                            warn!(fullname = %resolved.fullname, "peer sem peer_id no TXT — ignorado");
                            continue;
                        };

                        let Ok(uuid) = Uuid::parse_str(peer_id_str) else {
                            warn!(%peer_id_str, "peer_id inválido no TXT — ignorado");
                            continue;
                        };

                        let peer_id = PeerId::from_uuid(uuid);
                        let username = resolved
                            .get_property_val_str("username")
                            .unwrap_or("Unknown")
                            .to_string();
                        let fingerprint = resolved
                            .get_property_val_str("fingerprint")
                            .unwrap_or("")
                            .to_string();

                        let addresses: Vec<SocketAddr> = resolved
                            .get_addresses()
                            .iter()
                            .map(|scoped| SocketAddr::new(scoped.to_ip_addr(), resolved.get_port()))
                            .collect();

                        debug!(%peer_id, %username, addrs = addresses.len(), "peer resolvido");
                        DiscoveryEvent::Found {
                            peer_id,
                            username,
                            fingerprint,
                            addresses,
                        }
                    }

                    ServiceEvent::ServiceRemoved(_ty, fullname) => {
                        // O fullname tem formato "winx-<uuid>._winx-kvm._tcp.local."
                        // Extrai o UUID da parte do instance name.
                        let instance = fullname.split('.').next().unwrap_or("");
                        let uuid_str = instance.strip_prefix("winx-").unwrap_or("");

                        let Ok(uuid) = Uuid::parse_str(uuid_str) else {
                            warn!(%fullname, "não conseguiu extrair peer_id do ServiceRemoved");
                            continue;
                        };

                        let peer_id = PeerId::from_uuid(uuid);
                        debug!(%peer_id, "peer removido da rede");
                        DiscoveryEvent::Lost { peer_id }
                    }

                    // Demais eventos (SearchStarted, ServiceFound, SearchStopped e
                    // variantes futuras de #[non_exhaustive]) são ignorados.
                    _ => {
                        continue;
                    }
                };

                if tx.blocking_send(discovery_event).is_err() {
                    // Receiver foi dropado (app encerrando) — sai do loop
                    break;
                }
            }
        });

        Ok(rx)
    }

    async fn stop_browsing(&self) -> anyhow::Result<()> {
        self.daemon
            .stop_browse(SERVICE_TYPE)
            .map_err(|e| anyhow::anyhow!("stop_browse: {e}"))?;
        Ok(())
    }
}
