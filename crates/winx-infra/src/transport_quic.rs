//! Adapter QUIC (`quinn` + `rustls`) com verificação de certificado pela chave Ed25519 pareada.

use std::{
    collections::HashMap,
    fmt,
    net::SocketAddr,
    path::Path,
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use ed25519_dalek::pkcs8::EncodePrivateKey;
use ed25519_dalek::SigningKey;
use quinn::{ClientConfig, Connection, Endpoint, ServerConfig};
use rcgen::{CertificateParams, KeyPair, PKCS_ED25519};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::DistinguishedName;
use rustls::{DigitallySignedStruct, Error as RustlsError, SignatureScheme};
use tokio::sync::{mpsc, Mutex};
use tracing::{info, warn};
use winx_application::ports::transport::{
    ActiveConnection, IncomingConnection, StreamReceiver, StreamSender, TransportAdapter,
};
use winx_domain::{
    shared::ids::SessionId,
    transport::{ConnectionStats, StreamKind},
};
use x509_parser::prelude::FromDer;

type QuicConnection = Connection;

/// Gera ou carrega certificado TLS derivado da signing key Ed25519 de identidade.
pub fn generate_or_load_quic_cert(
    signing_key_bytes: &[u8; 32],
    cert_path: &Path,
    key_path: &Path,
) -> anyhow::Result<(CertificateDer<'static>, PrivatePkcs8KeyDer<'static>)> {
    if cert_path.exists() && key_path.exists() {
        let cert_pem = std::fs::read(cert_path)?;
        let key_pem = std::fs::read(key_path)?;
        let cert = rustls_pemfile::certs(&mut cert_pem.as_slice())
            .next()
            .transpose()?
            .ok_or_else(|| anyhow::anyhow!("cert PEM vazio"))?;
        let key = rustls_pemfile::pkcs8_private_keys(&mut key_pem.as_slice())
            .next()
            .transpose()?
            .ok_or_else(|| anyhow::anyhow!("key PEM vazio"))?;
        return Ok((cert, key));
    }

    let signing = SigningKey::from_bytes(signing_key_bytes);
    let pkcs8 = signing.to_pkcs8_der()?;
    let key_pair = KeyPair::from_pkcs8_der_and_sign_algo(
        &PrivatePkcs8KeyDer::from(pkcs8.as_bytes().to_vec()),
        &PKCS_ED25519,
    )?;

    let mut params = CertificateParams::new(vec![])?;
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "winx-kvm");
    let cert = params.self_signed(&key_pair)?;

    let cert_der = cert.der().to_vec();
    let key_der = key_pair.serialize_der();

    if let Some(parent) = cert_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(cert_path, cert.pem())?;
    std::fs::write(key_path, key_pair.serialize_pem())?;

    Ok((
        CertificateDer::from(cert_der),
        PrivatePkcs8KeyDer::from(key_der),
    ))
}

/// Extrai os 32 bytes da chave pública Ed25519 do certificado DER.
pub fn extract_ed25519_public_key(cert_der: &[u8]) -> Option<[u8; 32]> {
    let (_, cert) = x509_parser::certificate::X509Certificate::from_der(cert_der).ok()?;
    let raw = cert.public_key().raw;
    if raw.len() >= 32 {
        return raw[raw.len() - 32..].try_into().ok();
    }
    None
}

fn verify_ed25519_signature(
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
) -> Result<HandshakeSignatureValid, RustlsError> {
    let Some(pubkey) = extract_ed25519_public_key(cert.as_ref()) else {
        return Err(RustlsError::InvalidCertificate(
            rustls::CertificateError::BadSignature,
        ));
    };
    let Ok(verifying) = ed25519_dalek::VerifyingKey::from_bytes(&pubkey) else {
        return Err(RustlsError::InvalidCertificate(
            rustls::CertificateError::BadSignature,
        ));
    };
    let signature = ed25519_dalek::Signature::from_slice(dss.signature())
        .map_err(|_| RustlsError::InvalidCertificate(rustls::CertificateError::BadSignature))?;
    verifying
        .verify_strict(message, &signature)
        .map_err(|_| RustlsError::InvalidCertificate(rustls::CertificateError::BadSignature))?;
    Ok(HandshakeSignatureValid::assertion())
}

/// Verificador TLS servidor (lado cliente) — valida cert do peer remoto.
#[derive(Debug)]
pub struct Ed25519PeerVerifier {
    trusted_keys: Arc<RwLock<Vec<[u8; 32]>>>,
}

impl Ed25519PeerVerifier {
    pub fn new(trusted_keys: Arc<RwLock<Vec<[u8; 32]>>>) -> Arc<Self> {
        Arc::new(Self { trusted_keys })
    }

    fn is_trusted_cert(&self, cert_der: &[u8]) -> bool {
        let Some(key) = extract_ed25519_public_key(cert_der) else {
            return false;
        };
        self.trusted_keys
            .read()
            .map(|keys| keys.contains(&key))
            .unwrap_or(false)
    }
}

impl ServerCertVerifier for Ed25519PeerVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        if self.is_trusted_cert(end_entity.as_ref()) {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(RustlsError::InvalidCertificate(
                rustls::CertificateError::UnknownIssuer,
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        verify_ed25519_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        verify_ed25519_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }
}

/// Verificador de certificado de cliente (lado servidor).
#[derive(Debug)]
pub struct Ed25519PeerClientVerifier {
    trusted_keys: Arc<RwLock<Vec<[u8; 32]>>>,
}

impl Ed25519PeerClientVerifier {
    pub fn new(trusted_keys: Arc<RwLock<Vec<[u8; 32]>>>) -> Arc<Self> {
        Arc::new(Self { trusted_keys })
    }
}

impl ClientCertVerifier for Ed25519PeerClientVerifier {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, RustlsError> {
        let Some(key) = extract_ed25519_public_key(end_entity.as_ref()) else {
            return Err(RustlsError::InvalidCertificate(
                rustls::CertificateError::BadEncoding,
            ));
        };
        let trusted = self
            .trusted_keys
            .read()
            .map(|keys| keys.contains(&key))
            .unwrap_or(false);
        if trusted {
            Ok(ClientCertVerified::assertion())
        } else {
            Err(RustlsError::InvalidCertificate(
                rustls::CertificateError::UnknownIssuer,
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        verify_ed25519_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        verify_ed25519_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }
}

struct ConnEntry {
    connection: QuicConnection,
    /// `true` se este lado iniciou o handshake QUIC (`connect`); `false` se aceitou (`listen`).
    #[allow(dead_code)]
    is_quic_client: bool,
    /// Sender para streams inbound classificados (kind, sender, receiver).
    #[allow(dead_code)]
    inbound_streams_tx: mpsc::Sender<(StreamKind, StreamSender, StreamReceiver)>,
}

pub struct QuicTransportAdapter {
    endpoint: Endpoint,
    connections: Arc<Mutex<HashMap<SessionId, ConnEntry>>>,
    trusted_keys: Arc<RwLock<Vec<[u8; 32]>>>,
}

impl fmt::Debug for QuicTransportAdapter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QuicTransportAdapter")
            .finish_non_exhaustive()
    }
}

impl QuicTransportAdapter {
    pub fn new(
        cert_der: CertificateDer<'static>,
        key_der: PrivatePkcs8KeyDer<'static>,
        trusted_keys: Arc<RwLock<Vec<[u8; 32]>>>,
    ) -> anyhow::Result<Self> {
        let _ = rustls::crypto::ring::default_provider().install_default();

        let server_verifier = Ed25519PeerVerifier::new(Arc::clone(&trusted_keys));
        let client_verifier = Ed25519PeerClientVerifier::new(Arc::clone(&trusted_keys));

        let certs = vec![cert_der.clone()];
        let server_key = PrivateKeyDer::Pkcs8(key_der.clone_key());
        let client_key = PrivateKeyDer::Pkcs8(key_der.clone_key());

        let server_crypto = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_client_cert_verifier(client_verifier)
        .with_single_cert(certs.clone(), server_key)?;

        let client_crypto = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .dangerous()
        .with_custom_certificate_verifier(server_verifier)
        .with_client_auth_cert(certs, client_key)?;

        let mut transport_server = quinn::TransportConfig::default();
        transport_server.keep_alive_interval(Some(std::time::Duration::from_secs(5)));
        transport_server.max_idle_timeout(Some(quinn::IdleTimeout::try_from(
            std::time::Duration::from_secs(15),
        )?));

        let mut transport_client = quinn::TransportConfig::default();
        transport_client.keep_alive_interval(Some(std::time::Duration::from_secs(5)));
        transport_client.max_idle_timeout(Some(quinn::IdleTimeout::try_from(
            std::time::Duration::from_secs(15),
        )?));

        let mut server_config = ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto)?,
        ));
        server_config.transport_config(Arc::new(transport_server));

        let mut client_config = ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto)?,
        ));
        client_config.transport_config(Arc::new(transport_client));

        let addr: SocketAddr = "0.0.0.0:0".parse()?;
        let mut endpoint = Endpoint::server(server_config, addr)?;
        endpoint.set_default_client_config(client_config);

        Ok(Self {
            endpoint,
            connections: Arc::new(Mutex::new(HashMap::new())),
            trusted_keys,
        })
    }

    async fn store_connection(
        &self,
        conn_id: SessionId,
        connection: QuicConnection,
        is_quic_client: bool,
        inbound_streams_tx: mpsc::Sender<(StreamKind, StreamSender, StreamReceiver)>,
    ) {
        self.connections.lock().await.insert(
            conn_id,
            ConnEntry {
                connection,
                is_quic_client,
                inbound_streams_tx,
            },
        );
    }
}

/// Emparelha um stream QUIC bidirecional com canais mpsc (length-prefix por chunk).
fn bridge_bi_streams(
    mut send: quinn::SendStream,
    mut recv: quinn::RecvStream,
) -> (StreamSender, StreamReceiver) {
    let (tx, rx_out) = mpsc::channel::<Vec<u8>>(64);
    let (tx_in, mut rx_in) = mpsc::channel::<Vec<u8>>(64);

    tokio::spawn(async move {
        while let Some(chunk) = rx_in.recv().await {
            let len = u32::try_from(chunk.len()).unwrap_or(u32::MAX).to_be_bytes();
            if send.write_all(&len).await.is_err() {
                break;
            }
            if send.write_all(&chunk).await.is_err() {
                break;
            }
        }
    });

    tokio::spawn(async move {
        loop {
            let mut len_buf = [0u8; 4];
            if recv.read_exact(&mut len_buf).await.is_err() {
                break;
            }
            let len = u32::from_be_bytes(len_buf) as usize;
            if len > 1_048_576 {
                break;
            }
            let mut buf = vec![0u8; len];
            if recv.read_exact(&mut buf).await.is_err() {
                break;
            }
            if tx_in.send(buf).await.is_err() {
                break;
            }
        }
    });

    (tx, rx_out)
}

#[async_trait]
impl TransportAdapter for QuicTransportAdapter {
    async fn listen(&self, port: u16) -> anyhow::Result<mpsc::Receiver<IncomingConnection>> {
        let addr: SocketAddr = format!("0.0.0.0:{port}").parse()?;
        let socket = std::net::UdpSocket::bind(addr)?;
        self.endpoint.rebind(socket)?;

        let (tx, rx) = mpsc::channel(32);
        let endpoint = self.endpoint.clone();
        let connections = Arc::clone(&self.connections);
        let trusted_keys = Arc::clone(&self.trusted_keys);

        tokio::spawn(async move {
            loop {
                let Some(connecting) = endpoint.accept().await else {
                    continue;
                };
                let tx = tx.clone();
                let connections = Arc::clone(&connections);
                let trusted_keys = Arc::clone(&trusted_keys);
                tokio::spawn(async move {
                    match connecting.await {
                        Ok(connection) => {
                            let Some(peer_key) = peer_key_from_connection(&connection) else {
                                warn!("conexão sem certificado de cliente");
                                connection.close(0u32.into(), b"no client cert");
                                return;
                            };
                            let trusted = trusted_keys
                                .read()
                                .map(|keys| keys.contains(&peer_key))
                                .unwrap_or(false);
                            if !trusted {
                                warn!("conexão rejeitada: peer não confiável");
                                connection.close(0u32.into(), b"untrusted");
                                return;
                            }
                            let conn_id = SessionId::new();
                            let peer_addr = connection.remote_address();
                            let (inbound_tx, inbound_rx) = mpsc::channel(8);
                            connections.lock().await.insert(
                                conn_id,
                                ConnEntry {
                                    connection: connection.clone(),
                                    is_quic_client: false,
                                    inbound_streams_tx: inbound_tx.clone(),
                                },
                            );
                            spawn_inbound_stream_loop(connection.clone(), inbound_tx);
                            let _ = tx
                                .send(IncomingConnection {
                                    conn_id,
                                    peer_addr,
                                    peer_public_key: peer_key,
                                    inbound_streams: inbound_rx,
                                })
                                .await;
                            spawn_control_loop_responder(connection);
                        }
                        Err(err) => warn!(?err, "falha ao aceitar conexão QUIC"),
                    }
                });
            }
        });

        info!(%port, "listener QUIC ativo");
        Ok(rx)
    }

    async fn connect(
        &self,
        peer_addr: SocketAddr,
        peer_public_key: [u8; 32],
    ) -> anyhow::Result<ActiveConnection> {
        {
            let mut keys = self
                .trusted_keys
                .write()
                .map_err(|e| anyhow::anyhow!("lock: {e}"))?;
            if !keys.contains(&peer_public_key) {
                keys.push(peer_public_key);
            }
        }

        let connection = self.endpoint.connect(peer_addr, "winx-kvm.local")?.await?;
        let conn_id = SessionId::new();
        let (inbound_tx, inbound_rx) = mpsc::channel(8);
        self.store_connection(conn_id, connection.clone(), true, inbound_tx.clone())
            .await;
        spawn_inbound_stream_loop(connection.clone(), inbound_tx);
        spawn_control_loop_initiator(connection);
        Ok(ActiveConnection {
            conn_id,
            inbound_streams: inbound_rx,
        })
    }

    async fn open_stream(
        &self,
        conn_id: SessionId,
        kind: StreamKind,
    ) -> anyhow::Result<(StreamSender, StreamReceiver)> {
        let connection = {
            let guard = self.connections.lock().await;
            let entry = guard
                .get(&conn_id)
                .ok_or_else(|| anyhow::anyhow!("conexão não encontrada"))?;
            entry.connection.clone()
        };

        let (mut send, recv) = connection.open_bi().await?;

        let mut hdr = [0u8; 5];
        hdr[..4].copy_from_slice(winx_protocol::STREAM_HEADER_MAGIC);
        hdr[4] = kind.as_u8();
        send.write_all(&hdr).await?;

        info!(?kind, "stream QUIC outbound aberto com header");
        Ok(bridge_bi_streams(send, recv))
    }

    async fn get_stats(&self, conn_id: SessionId) -> anyhow::Result<ConnectionStats> {
        let connection = {
            let guard = self.connections.lock().await;
            guard
                .get(&conn_id)
                .map(|e| e.connection.clone())
                .ok_or_else(|| anyhow::anyhow!("conexão não encontrada"))?
        };

        let rtt = connection.rtt();
        let stats = connection.stats();

        Ok(ConnectionStats {
            rtt_ms: u32::try_from(rtt.as_millis().min(u128::from(u32::MAX))).unwrap_or(u32::MAX),
            tx_bytes: stats.udp_tx.bytes,
            rx_bytes: stats.udp_rx.bytes,
            lost_packets: stats.path.lost_packets,
        })
    }

    async fn close(&self, conn_id: SessionId, reason: &str) -> anyhow::Result<()> {
        if let Some(entry) = self.connections.lock().await.remove(&conn_id) {
            entry.connection.close(0u32.into(), reason.as_bytes());
        }
        Ok(())
    }

    async fn add_trusted_key(&self, key: [u8; 32]) {
        if let Ok(mut keys) = self.trusted_keys.write() {
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
    }
}

fn peer_key_from_connection(connection: &QuicConnection) -> Option<[u8; 32]> {
    let cert = connection
        .peer_identity()?
        .downcast::<Vec<CertificateDer<'static>>>()
        .ok()?;
    let first = cert.first()?;
    extract_ed25519_public_key(first.as_ref())
}

fn spawn_control_loop_responder(connection: QuicConnection) {
    tokio::spawn(async move {
        let Ok((mut send, mut recv)) = connection.accept_bi().await else {
            return;
        };
        run_control_stream(&mut send, &mut recv).await;
    });
}

fn spawn_control_loop_initiator(connection: QuicConnection) {
    tokio::spawn(async move {
        let Ok((mut send, mut recv)) = connection.open_bi().await else {
            return;
        };
        run_control_stream(&mut send, &mut recv).await;
    });
}

async fn run_control_stream(send: &mut quinn::SendStream, recv: &mut quinn::RecvStream) {
    let mut heartbeat_tick = tokio::time::interval(std::time::Duration::from_secs(5));
    loop {
        tokio::select! {
            _ = heartbeat_tick.tick() => {
                let frame = winx_protocol::Frame::new(winx_protocol::Payload::Heartbeat);
                if let Ok(bytes) = winx_protocol::encode(&frame) {
                    let len = u32::try_from(bytes.len())
                        .unwrap_or(u32::MAX)
                        .to_be_bytes();
                    if send.write_all(&len).await.is_err() { break; }
                    if send.write_all(&bytes).await.is_err() { break; }
                }
            }
            read = read_frame(recv) => {
                match read {
                    Ok(Some(payload)) => {
                        if matches!(payload, winx_protocol::Payload::Heartbeat) {
                            let ack = winx_protocol::Frame::new(winx_protocol::Payload::HeartbeatAck);
                            if let Ok(bytes) = winx_protocol::encode(&ack) {
                                let len = u32::try_from(bytes.len())
                        .unwrap_or(u32::MAX)
                        .to_be_bytes();
                                let _ = send.write_all(&len).await;
                                let _ = send.write_all(&bytes).await;
                            }
                        }
                    }
                    Ok(None) | Err(_) => break,
                }
            }
        }
    }
}

async fn read_frame(
    recv: &mut quinn::RecvStream,
) -> anyhow::Result<Option<winx_protocol::Payload>> {
    let mut len_buf = [0u8; 4];
    if recv.read_exact(&mut len_buf).await.is_err() {
        return Ok(None);
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 65_536 {
        anyhow::bail!("frame muito grande");
    }
    let mut buf = vec![0u8; len];
    recv.read_exact(&mut buf).await?;
    let frame = winx_protocol::decode(&buf)?;
    Ok(Some(frame.payload))
}

fn spawn_inbound_stream_loop(
    connection: QuicConnection,
    tx: mpsc::Sender<(StreamKind, StreamSender, StreamReceiver)>,
) {
    tokio::spawn(async move {
        loop {
            let Ok((send, mut recv)) = connection.accept_bi().await else {
                break;
            };
            let mut hdr = [0u8; 5];
            let read = tokio::time::timeout(
                std::time::Duration::from_secs(2),
                recv.read_exact(&mut hdr),
            )
            .await;
            let Ok(Ok(_)) = read else {
                warn!("stream entrante sem header válido — descartado");
                continue;
            };
            if &hdr[..4] != winx_protocol::STREAM_HEADER_MAGIC {
                warn!(?hdr, "magic inválido — descartado");
                continue;
            }
            let Some(kind) = StreamKind::from_u8(hdr[4]) else {
                warn!(byte = hdr[4], "kind desconhecido — descartado");
                continue;
            };
            info!(?kind, "stream entrante classificado");
            let (s, r) = bridge_bi_streams(send, recv);
            if tx.send((kind, s, r)).await.is_err() {
                break;
            }
        }
    });
}
