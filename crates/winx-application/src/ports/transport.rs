use std::net::SocketAddr;

use async_trait::async_trait;
use tokio::sync::mpsc;
use winx_domain::{
    shared::ids::SessionId,
    transport::{ConnectionStats, StreamKind},
};

pub type StreamSender = tokio::sync::mpsc::Sender<Vec<u8>>;
pub type StreamReceiver = tokio::sync::mpsc::Receiver<Vec<u8>>;
pub type InboundStreamsReceiver =
    tokio::sync::mpsc::Receiver<(StreamKind, StreamSender, StreamReceiver)>;

/// Conexão QUIC aceita de um peer remoto.
#[derive(Debug)]
pub struct IncomingConnection {
    pub conn_id: SessionId,
    pub peer_addr: SocketAddr,
    pub peer_public_key: [u8; 32],
    pub inbound_streams: InboundStreamsReceiver,
}

/// Conexão QUIC ativa iniciada localmente.
#[derive(Debug)]
pub struct ActiveConnection {
    pub conn_id: SessionId,
    pub inbound_streams: InboundStreamsReceiver,
}

/// Port de transporte QUIC entre peers pareados.
#[async_trait]
pub trait TransportAdapter: Send + Sync + 'static {
    /// Inicia o listener QUIC neste device.
    async fn listen(&self, port: u16) -> anyhow::Result<mpsc::Receiver<IncomingConnection>>;

    /// Conecta a um peer remoto já pareado.
    async fn connect(
        &self,
        peer_addr: SocketAddr,
        peer_public_key: [u8; 32],
    ) -> anyhow::Result<ActiveConnection>;

    /// Abre um stream nomeado em uma conexão existente.
    async fn open_stream(
        &self,
        conn_id: SessionId,
        kind: StreamKind,
    ) -> anyhow::Result<(StreamSender, StreamReceiver)>;

    /// Retorna estatísticas da conexão ativa.
    async fn get_stats(&self, conn_id: SessionId) -> anyhow::Result<ConnectionStats>;

    /// Fecha uma conexão.
    async fn close(&self, conn_id: SessionId, reason: &str) -> anyhow::Result<()>;

    /// Adiciona chave Ed25519 de um peer recém-pareado ao verificador TLS.
    async fn add_trusted_key(&self, key: [u8; 32]);

    /// Envia `Heartbeat` num stream Control efêmero e mede RTT até `HeartbeatAck`.
    async fn probe_control_heartbeat(&self, conn_id: SessionId) -> anyhow::Result<u32>;
}
