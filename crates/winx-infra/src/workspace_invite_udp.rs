//! Adapter UDP para mensagens de workspace invite, autenticadas via Ed25519.
//!
//! Mesmo modelo do `pairing_udp`: datagrama leva o body bincode + signature
//! de 64 bytes do `signing_key` do remetente. O verifier checa a assinatura
//! contra o `sender_pubkey` declarado no payload antes de emitir a mensagem,
//! garantindo que TOFU implícito só confie em payloads autenticados.

#![allow(unsafe_code)]

use anyhow::{anyhow, Result};
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tracing::warn;
use winx_application::ports::{DecodedWorkspaceInviteMessage, WorkspaceInviteTransport};
use winx_protocol::workspace::{WorkspaceInviteMessage, WORKSPACE_INVITE_MAGIC};

const PROTOCOL_VERSION: u16 = 1;
const SIGNATURE_LEN: usize = 64;
const HEADER_LEN: usize = 4 + 2 + 4; // MAGIC + version + body_len

/// UDP adapter for workspace invites on port 7880.
pub struct UdpWorkspaceInviteTransport {
    socket: Arc<UdpSocket>,
}

impl UdpWorkspaceInviteTransport {
    /// Bind to the workspace invite port. Returns an error if binding fails.
    pub async fn bind(port: u16) -> Result<Self> {
        let addr = format!("0.0.0.0:{}", port);
        let socket = UdpSocket::bind(&addr).await?;
        Ok(Self {
            socket: Arc::new(socket),
        })
    }

    /// Encode a workspace invite datagram with Ed25519 signature.
    ///
    /// Layout: `[MAGIC 4][version u16 LE][body_len u32 LE][body bincode][signature 64]`
    fn encode_signed_datagram(
        msg: &WorkspaceInviteMessage,
        signing_key: &SigningKey,
    ) -> Result<Vec<u8>> {
        let body = serde_json::to_vec(msg)?;
        let body_len = u32::try_from(body.len())
            .map_err(|_| anyhow!("body too large: {} bytes", body.len()))?;

        let signature = signing_key.sign(&body).to_bytes();

        let mut datagram = Vec::with_capacity(HEADER_LEN + body.len() + SIGNATURE_LEN);
        datagram.extend_from_slice(&WORKSPACE_INVITE_MAGIC);
        datagram.extend_from_slice(&PROTOCOL_VERSION.to_le_bytes());
        datagram.extend_from_slice(&body_len.to_le_bytes());
        datagram.extend_from_slice(&body);
        datagram.extend_from_slice(&signature);
        Ok(datagram)
    }

    /// Decode and verify a signed datagram. Returns `(message, sender_pubkey)`
    /// only if the signature matches the `sender_pubkey` declared in the payload.
    fn decode_and_verify_datagram(buf: &[u8]) -> Result<(WorkspaceInviteMessage, [u8; 32])> {
        if buf.len() < HEADER_LEN + SIGNATURE_LEN {
            return Err(anyhow!("datagram too short"));
        }

        if buf[0..4] != WORKSPACE_INVITE_MAGIC {
            return Err(anyhow!("invalid magic"));
        }

        let version = u16::from_le_bytes([buf[4], buf[5]]);
        if version != PROTOCOL_VERSION {
            return Err(anyhow!("unsupported version: {}", version));
        }

        let body_len = u32::from_le_bytes([buf[6], buf[7], buf[8], buf[9]]) as usize;
        if buf.len() != HEADER_LEN + body_len + SIGNATURE_LEN {
            return Err(anyhow!(
                "size mismatch: expected {}, got {}",
                HEADER_LEN + body_len + SIGNATURE_LEN,
                buf.len()
            ));
        }

        let body_buf = &buf[HEADER_LEN..HEADER_LEN + body_len];
        let signature_buf: &[u8; SIGNATURE_LEN] = buf
            [HEADER_LEN + body_len..HEADER_LEN + body_len + SIGNATURE_LEN]
            .try_into()
            .map_err(|_| anyhow!("invalid signature slice"))?;

        let msg: WorkspaceInviteMessage = serde_json::from_slice(body_buf)?;

        let sender_pubkey = match &msg {
            WorkspaceInviteMessage::Invite(p) => p.sender_pubkey,
            WorkspaceInviteMessage::Response(p) => p.responder_pubkey,
            WorkspaceInviteMessage::Sync(p) => p.sender_pubkey,
            WorkspaceInviteMessage::Delete(p) => p.sender_pubkey,
            WorkspaceInviteMessage::GlobalCursor(p) => p.sender_pubkey,
            WorkspaceInviteMessage::Cancel(_) => {
                return Err(anyhow!("Cancel message has no pubkey"));
            }
        };

        let verifying_key = VerifyingKey::from_bytes(&sender_pubkey)
            .map_err(|e| anyhow!("invalid sender_pubkey: {e}"))?;
        let signature = ed25519_dalek::Signature::from_bytes(signature_buf);
        verifying_key
            .verify(body_buf, &signature)
            .map_err(|e| anyhow!("signature verification failed: {e}"))?;

        Ok((msg, sender_pubkey))
    }
}

#[async_trait::async_trait]
impl WorkspaceInviteTransport for UdpWorkspaceInviteTransport {
    async fn listen(&self) -> Result<mpsc::Receiver<DecodedWorkspaceInviteMessage>> {
        let (tx, rx) = mpsc::channel(32);
        let socket = Arc::clone(&self.socket);

        tokio::spawn(async move {
            let mut buf = vec![0u8; 8192];
            loop {
                match socket.recv_from(&mut buf).await {
                    Ok((n, from)) => match Self::decode_and_verify_datagram(&buf[..n]) {
                        Ok((message, sender_pubkey)) => {
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
                            warn!(%from, ?e, "workspace invite: decode or verify failed");
                        }
                    },
                    Err(e) => {
                        warn!(?e, "workspace invite: recv_from error");
                        break;
                    }
                }
            }
        });

        Ok(rx)
    }

    async fn send_to(
        &self,
        addr: SocketAddr,
        msg: &WorkspaceInviteMessage,
        signing_key: &SigningKey,
    ) -> Result<()> {
        let datagram = Self::encode_signed_datagram(msg, signing_key)?;
        self.socket.send_to(&datagram, addr).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;
    use uuid::Uuid;
    use winx_protocol::workspace::{WorkspaceInviteCancelPayload, WorkspaceSnapshotPayload};

    fn make_test_signing_key() -> SigningKey {
        SigningKey::generate(&mut OsRng)
    }

    fn make_test_response_message(pubkey: [u8; 32]) -> WorkspaceInviteMessage {
        WorkspaceInviteMessage::Response(winx_protocol::workspace::WorkspaceInviteResponsePayload {
            invite_id: Uuid::new_v4(),
            responder_device_id: Uuid::new_v4(),
            responder_pubkey: pubkey,
            accepted: true,
            responder_username: "Remote".to_string(),
        })
    }

    #[test]
    fn roundtrip_signed_response() {
        let signing_key = make_test_signing_key();
        let pubkey = signing_key.verifying_key().to_bytes();
        let msg = make_test_response_message(pubkey);

        let datagram =
            UdpWorkspaceInviteTransport::encode_signed_datagram(&msg, &signing_key).unwrap();
        let (decoded, decoded_pubkey) =
            UdpWorkspaceInviteTransport::decode_and_verify_datagram(&datagram).unwrap();

        assert_eq!(decoded_pubkey, pubkey);
        if let WorkspaceInviteMessage::Response(p) = decoded {
            assert!(p.accepted);
        } else {
            panic!("expected Response variant");
        }
    }

    #[test]
    fn rejects_tampered_body() {
        let signing_key = make_test_signing_key();
        let pubkey = signing_key.verifying_key().to_bytes();
        let msg = make_test_response_message(pubkey);

        let mut datagram =
            UdpWorkspaceInviteTransport::encode_signed_datagram(&msg, &signing_key).unwrap();
        let body_start = HEADER_LEN;
        datagram[body_start] ^= 0xFF;

        let result = UdpWorkspaceInviteTransport::decode_and_verify_datagram(&datagram);
        assert!(result.is_err());
    }

    #[test]
    fn rejects_wrong_pubkey() {
        let signing_key = make_test_signing_key();
        let other_key = make_test_signing_key();
        let wrong_pubkey = other_key.verifying_key().to_bytes();
        let msg = make_test_response_message(wrong_pubkey);

        let datagram =
            UdpWorkspaceInviteTransport::encode_signed_datagram(&msg, &signing_key).unwrap();

        let result = UdpWorkspaceInviteTransport::decode_and_verify_datagram(&datagram);
        assert!(result.is_err(), "wrong pubkey should fail verification");
    }

    #[test]
    fn roundtrip_signed_sync() {
        let signing_key = make_test_signing_key();
        let pubkey = signing_key.verifying_key().to_bytes();
        let snapshot = WorkspaceSnapshotPayload {
            id: Uuid::new_v4(),
            name: "WS".to_string(),
            owner_device_id: Uuid::new_v4(),
            owner_username: "Owner".to_string(),
            version: 5,
            members: vec![],
            layout: Default::default(),
        };
        let msg = WorkspaceInviteMessage::Sync(winx_protocol::workspace::WorkspaceSyncPayload {
            workspace_id: snapshot.id,
            snapshot,
            sender_device_id: Uuid::new_v4(),
            sender_pubkey: pubkey,
        });
        let datagram =
            UdpWorkspaceInviteTransport::encode_signed_datagram(&msg, &signing_key).unwrap();
        let (decoded, decoded_pubkey) =
            UdpWorkspaceInviteTransport::decode_and_verify_datagram(&datagram).unwrap();
        assert_eq!(decoded_pubkey, pubkey);
        assert!(matches!(decoded, WorkspaceInviteMessage::Sync(_)));
    }

    #[test]
    fn roundtrip_signed_delete() {
        let signing_key = make_test_signing_key();
        let pubkey = signing_key.verifying_key().to_bytes();
        let msg =
            WorkspaceInviteMessage::Delete(winx_protocol::workspace::WorkspaceDeletePayload {
                workspace_id: Uuid::new_v4(),
                sender_device_id: Uuid::new_v4(),
                sender_pubkey: pubkey,
            });
        let datagram =
            UdpWorkspaceInviteTransport::encode_signed_datagram(&msg, &signing_key).unwrap();
        let (decoded, _) =
            UdpWorkspaceInviteTransport::decode_and_verify_datagram(&datagram).unwrap();
        assert!(matches!(decoded, WorkspaceInviteMessage::Delete(_)));
    }

    #[test]
    fn roundtrip_signed_global_cursor() {
        let signing_key = make_test_signing_key();
        let pubkey = signing_key.verifying_key().to_bytes();
        let msg =
            WorkspaceInviteMessage::GlobalCursor(winx_protocol::workspace::GlobalCursorPayload {
                workspace_id: Uuid::new_v4(),
                x: 100,
                y: 200,
                active_device_id: Uuid::new_v4(),
                monotonic_seq: 42,
                sender_device_id: Uuid::new_v4(),
                sender_pubkey: pubkey,
            });
        let datagram =
            UdpWorkspaceInviteTransport::encode_signed_datagram(&msg, &signing_key).unwrap();
        let (decoded, decoded_pubkey) =
            UdpWorkspaceInviteTransport::decode_and_verify_datagram(&datagram).unwrap();
        assert_eq!(decoded_pubkey, pubkey);
        if let WorkspaceInviteMessage::GlobalCursor(p) = decoded {
            assert_eq!(p.x, 100);
            assert_eq!(p.y, 200);
            assert_eq!(p.monotonic_seq, 42);
        } else {
            panic!("expected GlobalCursor variant");
        }
    }

    #[test]
    fn cancel_message_returns_error() {
        let signing_key = make_test_signing_key();
        let msg = WorkspaceInviteMessage::Cancel(WorkspaceInviteCancelPayload {
            invite_id: Uuid::new_v4(),
        });

        let datagram =
            UdpWorkspaceInviteTransport::encode_signed_datagram(&msg, &signing_key).unwrap();

        let result = UdpWorkspaceInviteTransport::decode_and_verify_datagram(&datagram);
        assert!(result.is_err());
    }
}
