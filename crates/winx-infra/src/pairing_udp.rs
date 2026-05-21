//! Adapter UDP para mensagens de pareamento.

use std::{net::SocketAddr, sync::Arc};

use async_trait::async_trait;
use ed25519_dalek::{Signer, SigningKey};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tracing::{info, warn};
use winx_application::ports::pairing::{
    DecodedPairingMessage, PairingTransport, WINX_KVM_PAIRING_PORT,
};
use winx_protocol::{
    decode_pairing_datagram, encode_pairing_body, encode_pairing_datagram,
    message_requires_signature, PairingMessage, PairingProtocolError,
};

pub struct UdpPairingTransport {
    socket: Arc<UdpSocket>,
}

impl UdpPairingTransport {
    pub async fn bind() -> anyhow::Result<Self> {
        let addr = SocketAddr::from(([0, 0, 0, 0], WINX_KVM_PAIRING_PORT));
        let socket = UdpSocket::bind(addr).await?;
        info!(%addr, "listener UDP de pairing ativo");
        Ok(Self {
            socket: Arc::new(socket),
        })
    }
}

fn sign_body(signing_key: &SigningKey, body: &[u8]) -> [u8; 64] {
    signing_key.sign(body).to_bytes()
}

#[async_trait]
impl PairingTransport for UdpPairingTransport {
    async fn listen(&self) -> anyhow::Result<mpsc::Receiver<DecodedPairingMessage>> {
        let (tx, rx) = mpsc::channel(64);
        let socket = Arc::clone(&self.socket);
        tokio::spawn(async move {
            let mut buf = vec![0u8; 2048];
            loop {
                match socket.recv_from(&mut buf).await {
                    Ok((len, from_addr)) => {
                        let data = &buf[..len];
                        if let Some(ping) = winx_protocol::diagnostics::DiagPing::decode(data) {
                            let _ = socket.send_to(&ping.pong_bytes(), from_addr).await;
                            continue;
                        }
                        match decode_pairing_datagram(data) {
                            Ok((message, signature)) => {
                                if message_requires_signature(&message) && signature.is_none() {
                                    warn!(%from_addr, "pairing: mensagem assinada sem signature");
                                    continue;
                                }
                                if tx
                                    .send(DecodedPairingMessage {
                                        from_addr,
                                        message,
                                        signature,
                                    })
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            }
                            Err(err) => {
                                warn!(%from_addr, ?err, "pairing: datagrama inválido");
                            }
                        }
                    }
                    Err(err) => {
                        warn!(?err, "pairing: recv_from falhou");
                    }
                }
            }
        });
        Ok(rx)
    }

    async fn send_to(
        &self,
        addr: SocketAddr,
        msg: &PairingMessage,
        sign: Option<&SigningKey>,
    ) -> anyhow::Result<()> {
        let signature = if message_requires_signature(msg) {
            let key = sign.ok_or_else(|| anyhow::anyhow!("mensagem requer assinatura"))?;
            let body = encode_pairing_body(msg).map_err(|e| anyhow::anyhow!("{e}"))?;
            Some(sign_body(key, &body))
        } else {
            None
        };
        let bytes = encode_pairing_datagram(msg, signature.as_ref())
            .map_err(|e: PairingProtocolError| anyhow::anyhow!("{e}"))?;
        self.socket.send_to(&bytes, addr).await?;
        Ok(())
    }
}
