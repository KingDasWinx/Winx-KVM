use serde::{Deserialize, Serialize};

use super::content_hash::ContentHash;

/// Tamanho máximo de texto UTF-8 sincronizado (5 MB).
pub const MAX_CLIPBOARD_TEXT_BYTES: usize = 5 * 1024 * 1024;

/// Texto de clipboard com hash pré-calculado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipboardText {
    pub text: String,
    pub hash: ContentHash,
}

impl ClipboardText {
    #[must_use]
    pub fn new(text: String) -> Self {
        let hash = ContentHash::of_text(&text);
        Self { text, hash }
    }
}
