use serde::{Deserialize, Serialize};

use crate::shared::ids::PeerId;

use super::content_hash::ContentHash;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardChanged {
    pub hash: ContentHash,
    pub byte_len: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardReceived {
    pub from_peer: PeerId,
    pub hash: ContentHash,
}
