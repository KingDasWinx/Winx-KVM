use std::net::SocketAddr;

use async_trait::async_trait;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use tokio::sync::mpsc;
use tracing::{debug, info, trace, warn};
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
        info!("[MDNS INIT] criando ServiceDaemon...");

        let daemon = ServiceDaemon::new().map_err(|e| {
            warn!("[MDNS INIT] falha ao criar daemon: {}", e);
            anyhow::anyhow!("mdns daemon: {e}")
        })?;

        info!("[MDNS INIT] ServiceDaemon criado com sucesso");
        Self::filter_virtual_interfaces(&daemon);

        Ok(Self { daemon })
    }

    /// Desabilita interfaces virtuais (Hyper-V, WSL, VPN) para mDNS discover.
    fn filter_virtual_interfaces(_daemon: &ServiceDaemon) {
        // Aqui iteraríamos pelas interfaces do sistema, mas mdns-sd 0.19 não expõe
        // interface listing diretamente. Loga aviso e confia que o daemon
        // descobrirá a interface correta na primeira tentativa de register.
        // Futuro: usar `if-addrs` crate para filtro explícito.

        info!("[MDNS INIT] mDNS daemon initialized (interface auto-detection ativo)");
        debug!("[MDNS INIT] nota: mdns-sd 0.19 não expõe API de binding explícito — filtragem futura com if-addrs");
    }
}

#[async_trait]
impl DiscoveryAdapter for MdnsDiscoveryAdapter {
    async fn announce(&self, info: &AnnounceInfo) -> anyhow::Result<()> {
        let instance_name = format!("winx-{}", info.peer_id);
        let host_name = format!("winx-{}.local.", info.peer_id);

        let username_truncated: String = info.username.chars().take(60).collect();

        debug!("[MDNS ANNOUNCE] instance_name={}, host_name={}, port={}", instance_name, host_name, info.port);

        let properties = [
            ("peer_id", info.peer_id.to_string()),
            ("username", username_truncated.clone()),
            ("fingerprint", info.fingerprint.clone()),
        ];

        debug!("[MDNS ANNOUNCE] properties: peer_id={}, username={}, fingerprint={}", info.peer_id, username_truncated, info.fingerprint);

        let service = ServiceInfo::new(
            SERVICE_TYPE,
            &instance_name,
            &host_name,
            "", // endereço auto-detectado pela lib
            info.port,
            &properties[..],
        )
        .map_err(|e| {
            warn!("[MDNS ANNOUNCE] falha ao criar ServiceInfo: {}", e);
            anyhow::anyhow!("ServiceInfo inválido: {e}")
        })?;

        info!("[MDNS ANNOUNCE] ServiceInfo criado, registrando no daemon...");

        self.daemon
            .register(service)
            .map_err(|e| {
                warn!("[MDNS ANNOUNCE] daemon.register() falhou: {}", e);
                anyhow::anyhow!("register falhou: {e}")
            })?;

        info!("[MDNS ANNOUNCE] sucesso! peer_id={}, port={}, tipo={}", info.peer_id, info.port, SERVICE_TYPE);
        Ok(())
    }

    async fn reannounce(&self, info: &AnnounceInfo) -> anyhow::Result<()> {
        let fullname = format!("winx-{}.{}", info.peer_id, SERVICE_TYPE);
        info!("[MDNS REANNOUNCE] tentando unregister anterior: {}", fullname);
        if let Err(e) = self.daemon.unregister(&fullname) {
            debug!("[MDNS REANNOUNCE] unregister retornou erro (pode ser primeira vez): {}", e);
        } else {
            info!("[MDNS REANNOUNCE] unregister sucesso");
        }
        info!("[MDNS REANNOUNCE] chamando announce para {}", info.peer_id);
        self.announce(info).await
    }

    async fn stop_announcing(&self) -> anyhow::Result<()> {
        self.daemon
            .shutdown()
            .map_err(|e| anyhow::anyhow!("mdns shutdown: {e}"))?;
        Ok(())
    }

    async fn start_browsing(&self) -> anyhow::Result<mpsc::Receiver<DiscoveryEvent>> {
        info!("iniciando browse mDNS para tipo de serviço: {}", SERVICE_TYPE);

        let receiver = self
            .daemon
            .browse(SERVICE_TYPE)
            .map_err(|e| {
                warn!("browse falhou imediatamente: {}", e);
                anyhow::anyhow!("browse falhou: {e}")
            })?;

        info!("browse iniciado com sucesso, aguardando eventos...");

        let (tx, rx) = mpsc::channel(64);

        // Converte eventos síncronos do flume para o canal tokio async.
        std::thread::spawn(move || {
            debug!("[BROWSE THREAD] iniciado");

            let mut event_count = 0;
            loop {
                match receiver.recv() {
                    Ok(event) => {
                        event_count += 1;
                        trace!("[BROWSE THREAD] evento #{}: {:?}", event_count, event);

                        let discovery_event = match event {
                            ServiceEvent::ServiceResolved(resolved) => {
                                debug!("[BROWSE] ServiceResolved: fullname={}, port={}", resolved.fullname, resolved.get_port());

                                let Some(peer_id_str) = resolved.get_property_val_str("peer_id") else {
                                    warn!("[BROWSE] peer sem peer_id no TXT — ignorado. fullname={}", resolved.fullname);
                                    continue;
                                };

                                debug!("[BROWSE] peer_id extraído: {}", peer_id_str);

                                let Ok(uuid) = Uuid::parse_str(peer_id_str) else {
                                    warn!("[BROWSE] peer_id inválido (não é UUID): {}", peer_id_str);
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

                                debug!(%peer_id, %username, addrs = ?addresses.iter().map(|a| a.to_string()).collect::<Vec<_>>(), "peer resolvido com sucesso");
                                DiscoveryEvent::Found {
                                    peer_id,
                                    username,
                                    fingerprint,
                                    addresses,
                                }
                            }

                            ServiceEvent::ServiceRemoved(_ty, fullname) => {
                                debug!("[BROWSE] ServiceRemoved: fullname={}", fullname);

                                // O fullname tem formato "winx-<uuid>._winx-kvm._tcp.local."
                                // Extrai o UUID da parte do instance name.
                                let instance = fullname.split('.').next().unwrap_or("");
                                debug!("[BROWSE] instance extraído: {}", instance);

                                let uuid_str = instance.strip_prefix("winx-").unwrap_or("");
                                debug!("[BROWSE] uuid_str após strip_prefix: {}", uuid_str);

                                let Ok(uuid) = Uuid::parse_str(uuid_str) else {
                                    warn!("[BROWSE] não conseguiu extrair peer_id do ServiceRemoved. fullname={}, instance={}, uuid_str={}", fullname, instance, uuid_str);
                                    continue;
                                };

                                let peer_id = PeerId::from_uuid(uuid);
                                debug!(%peer_id, "peer removido da rede");
                                DiscoveryEvent::Lost { peer_id }
                            }

                            ServiceEvent::SearchStarted(_ty) => {
                                info!("[BROWSE] SearchStarted para tipo: {}", _ty);
                                continue;
                            }

                            ServiceEvent::ServiceFound(_ty, fullname) => {
                                debug!("[BROWSE] ServiceFound (antes de resolve): type={}, fullname={}", _ty, fullname);
                                continue;
                            }

                            ServiceEvent::SearchStopped(_ty) => {
                                warn!("[BROWSE] SearchStopped para tipo: {}", _ty);
                                continue;
                            }

                            // Variantes futuras de #[non_exhaustive]
                            _ => {
                                trace!("[BROWSE] evento desconhecido ignorado");
                                continue;
                            }
                        };

                        match tx.blocking_send(discovery_event) {
                            Ok(_) => trace!("[BROWSE] evento enviado para canal async"),
                            Err(e) => {
                                info!("[BROWSE] erro ao enviar evento: {}, encerrando thread", e);
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        warn!("[BROWSE THREAD] receiver.recv() retornou erro: {}, encerrando", e);
                        break;
                    }
                }
            }

            debug!("[BROWSE THREAD] finalizado após {} eventos", event_count);
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
