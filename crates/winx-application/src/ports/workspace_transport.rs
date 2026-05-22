use async_trait::async_trait;
use std::net::SocketAddr;
use tokio::sync::mpsc;
use winx_protocol::workspace::WorkspaceInviteMessage;

/// Port for workspace invite transport layer (UDP).
pub const WORKSPACE_INVITE_PORT: u16 = 7880;

/// Decoded message received from peer.
pub struct DecodedWorkspaceInviteMessage {
    pub from: SocketAddr,
    pub sender_pubkey: [u8; 32],
    pub message: WorkspaceInviteMessage,
}

/// Port: transport layer for workspace invites.
/// Implementations must sign messages with the local signing key.
#[async_trait]
pub trait WorkspaceInviteTransport: Send + Sync + 'static {
    /// Start listening for incoming invites. Returns a receiver that yields
    /// decoded messages. Receiver closed when transport shuts down.
    async fn listen(&self) -> anyhow::Result<mpsc::Receiver<DecodedWorkspaceInviteMessage>>;

    /// Send a message to a peer at the given address.
    async fn send_to(&self, addr: SocketAddr, msg: WorkspaceInviteMessage) -> anyhow::Result<()>;
}
