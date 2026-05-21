use std::time::Duration;

use winx_domain::{
    shared::{ids::PeerId, DomainErrorCode},
    transport::StreamKind,
    DomainError,
};

use super::TransportService;
use crate::ports::transport::{StreamReceiver, StreamSender};

const INBOUND_WAIT: Duration = Duration::from_secs(5);

pub async fn acquire_input_streams(
    transport: &TransportService,
    peer_id: PeerId,
) -> Result<(StreamSender, StreamReceiver), DomainError> {
    if !transport.is_peer_connected(peer_id).await {
        return Err(DomainError::new(
            DomainErrorCode::TransportConnectionFailed,
            "peer não conectado",
        ));
    }

    if transport.is_peer_outbound(peer_id).await {
        transport
            .open_stream_for_peer(peer_id, StreamKind::Input)
            .await
    } else {
        transport
            .wait_inbound_stream(peer_id, StreamKind::Input, INBOUND_WAIT)
            .await
            .ok_or_else(|| {
                DomainError::new(
                    DomainErrorCode::TransportConnectionFailed,
                    "stream Input inbound não recebido a tempo",
                )
            })
    }
}
