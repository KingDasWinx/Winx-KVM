use serde::{Deserialize, Serialize};

/// Payload de sincronização de clipboard de texto no stream Data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardPayload {
    pub origin_peer_id: uuid::Uuid,
    pub content_hash: [u8; 32],
    pub text: String,
}
