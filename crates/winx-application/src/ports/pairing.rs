//! Port para transporte de mensagens de pareamento (UDP).

use std::net::SocketAddr;

use async_trait::async_trait;
use ed25519_dalek::SigningKey;
use tokio::sync::mpsc;
use winx_protocol::PairingMessage;

pub const WINX_KVM_PAIRING_PORT: u16 = 7879;

/// Mensagem decodificada recebida na rede.
pub struct DecodedPairingMessage {
    pub from_addr: SocketAddr,
    pub message: PairingMessage,
    pub signature: Option<[u8; 64]>,
}

#[async_trait]
pub trait PairingTransport: Send + Sync {
    /// Bind `0.0.0.0:WINX_KVM_PAIRING_PORT` e retorna canal de mensagens decodificadas.
    async fn listen(&self) -> anyhow::Result<mpsc::Receiver<DecodedPairingMessage>>;

    /// Envia datagrama para `addr` (porta do caller; use pairing port no resolver).
    async fn send_to(
        &self,
        addr: SocketAddr,
        msg: &PairingMessage,
        sign: Option<&SigningKey>,
    ) -> anyhow::Result<()>;
}

/// Substitui a porta do endereço pela porta de pairing.
#[must_use]
pub fn pairing_socket_addr(mut addr: SocketAddr) -> SocketAddr {
    addr.set_port(WINX_KVM_PAIRING_PORT);
    addr
}
