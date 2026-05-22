use async_trait::async_trait;
use ed25519_dalek::SigningKey;
use std::net::SocketAddr;
use tokio::sync::mpsc;
use winx_protocol::workspace::WorkspaceInviteMessage;

/// Porta UDP para mensagens de workspace invite.
pub const WORKSPACE_INVITE_PORT: u16 = 7880;

/// Mensagem decodificada recebida do peer.
///
/// `sender_pubkey` é o pubkey declarado no payload **já verificado** contra
/// a assinatura Ed25519 do datagrama. Se `signature_valid == false`, o adapter
/// não emite a mensagem (mesmo padrão do `pairing`).
pub struct DecodedWorkspaceInviteMessage {
    pub from: SocketAddr,
    pub sender_pubkey: [u8; 32],
    pub message: WorkspaceInviteMessage,
}

/// Port: transporte de workspace invites sobre UDP autenticado.
///
/// Cada datagrama carrega uma assinatura Ed25519 do payload (igual `pairing`),
/// permitindo TOFU sem pareamento prévio: o destinatário verifica que o
/// `sender_pubkey` declarado no payload assinou o datagrama, então aceita o
/// pubkey via TOFU formal no `accept_invite`.
#[async_trait]
pub trait WorkspaceInviteTransport: Send + Sync + 'static {
    /// Inicia o listener UDP. Receiver entrega apenas mensagens com assinatura válida.
    async fn listen(&self) -> anyhow::Result<mpsc::Receiver<DecodedWorkspaceInviteMessage>>;

    /// Envia mensagem assinada para um peer.
    ///
    /// `signing_key` é usada para assinar o payload serializado. O receptor
    /// verifica a assinatura contra o `sender_pubkey` declarado no payload.
    async fn send_to(
        &self,
        addr: SocketAddr,
        msg: &WorkspaceInviteMessage,
        signing_key: &SigningKey,
    ) -> anyhow::Result<()>;
}
