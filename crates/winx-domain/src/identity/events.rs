use serde::{Deserialize, Serialize};

use crate::shared::ids::DeviceId;

/// Device criado pela primeira vez neste PC (geração de keypair).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCreated {
    pub device_id: DeviceId,
    /// Fingerprint da chave pública para exibição (ex: `"A3:4F:B2:1C:D9:E5:07:AB"`).
    pub fingerprint: String,
}

/// Peer removido da lista de confiança ("Esquecer dispositivo").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerForgotten {
    pub peer_id: crate::shared::ids::PeerId,
}
