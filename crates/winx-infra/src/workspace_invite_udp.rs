#![allow(unsafe_code)]

use anyhow::{anyhow, Result};
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tracing::warn;
use winx_application::ports::{
    DecodedWorkspaceInviteMessage, WorkspaceInviteTransport, WORKSPACE_INVITE_PORT,
};
use winx_protocol::workspace::{WorkspaceInviteMessage, WORKSPACE_INVITE_MAGIC};

/// UDP adapter for workspace invites on port 7880.
pub struct UdpWorkspaceInviteTransport {
    socket: UdpSocket,
}

impl UdpWorkspaceInviteTransport {
    /// Bind to the workspace invite port. Returns an error if binding fails.
    pub async fn bind(port: u16) -> Result<Self> {
        let addr = format!("0.0.0.0:{}", port);
        let socket = UdpSocket::bind(&addr).await?;
        Ok(Self { socket })
    }

    /// Encode a workspace invite datagram.
    /// Format: [MAGIC 4][version u16][len u32][body bincode]
    fn encode_datagram(msg: &WorkspaceInviteMessage) -> Result<Vec<u8>> {
        let body = serde_json::to_vec(msg)?;
        let len = body.len() as u32;

        let mut datagram = Vec::with_capacity(4 + 2 + 4 + body.len());
        datagram.extend_from_slice(&WORKSPACE_INVITE_MAGIC);
        datagram.extend_from_slice(&1u16.to_le_bytes()); // version
        datagram.extend_from_slice(&len.to_le_bytes());
        datagram.extend_from_slice(&body);
        Ok(datagram)
    }

    /// Decode a workspace invite datagram.
    fn decode_datagram(buf: &[u8]) -> Result<WorkspaceInviteMessage> {
        if buf.len() < 4 + 2 + 4 {
            return Err(anyhow!("datagram too short"));
        }

        // Verify magic
        if &buf[0..4] != &WORKSPACE_INVITE_MAGIC {
            return Err(anyhow!("invalid magic"));
        }

        // Parse version
        let version = u16::from_le_bytes([buf[4], buf[5]]);
        if version != 1 {
            return Err(anyhow!("unsupported version: {}", version));
        }

        // Parse length
        let len = u32::from_le_bytes([buf[6], buf[7], buf[8], buf[9]]) as usize;
        if buf.len() != 10 + len {
            return Err(anyhow!(
                "size mismatch: expected {}, got {}",
                10 + len,
                buf.len()
            ));
        }

        // Decode body
        let body_buf = &buf[10..];
        let msg = serde_json::from_slice(body_buf)?;
        Ok(msg)
    }
}

#[async_trait::async_trait]
impl WorkspaceInviteTransport for UdpWorkspaceInviteTransport {
    async fn listen(&self) -> Result<mpsc::Receiver<DecodedWorkspaceInviteMessage>> {
        let (tx, rx) = mpsc::channel(32);

        let socket = UdpSocket::bind(format!("0.0.0.0:{}", WORKSPACE_INVITE_PORT)).await?;

        tokio::spawn(async move {
            let mut buf = vec![0u8; 4096];
            loop {
                match socket.recv_from(&mut buf).await {
                    Ok((n, from)) => match Self::decode_datagram(&buf[..n]) {
                        Ok(message) => {
                            let sender_pubkey = match &message {
                                WorkspaceInviteMessage::Invite(p) => p.sender_pubkey,
                                WorkspaceInviteMessage::Response(p) => p.responder_pubkey,
                                WorkspaceInviteMessage::Cancel(_) => {
                                    warn!(%from, "received Cancel without pubkey");
                                    continue;
                                }
                            };
                            let decoded = DecodedWorkspaceInviteMessage {
                                from,
                                sender_pubkey,
                                message,
                            };
                            if tx.send(decoded).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            warn!(%from, ?e, "failed to decode workspace invite");
                        }
                    },
                    Err(e) => {
                        warn!(?e, "recv_from error");
                        break;
                    }
                }
            }
        });

        Ok(rx)
    }

    async fn send_to(&self, addr: SocketAddr, msg: WorkspaceInviteMessage) -> Result<()> {
        let datagram = Self::encode_datagram(&msg)?;
        self.socket.send_to(&datagram, addr).await?;
        Ok(())
    }
}
