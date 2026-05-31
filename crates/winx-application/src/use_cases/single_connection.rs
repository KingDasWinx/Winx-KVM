//! Modo Single Connection — habilitação simétrica de KVM (clipboard + input + layout sync).

use std::sync::Arc;

use tracing::{info, warn};
use winx_domain::{shared::ids::PeerId, DomainError, DomainEvent};

use crate::bus::EventBus;

use super::{ClipboardService, InputControlService};

/// Habilita clipboard, input control e anuncia monitores locais ao peer.
pub async fn enable_kvm_for_peer(
    clipboard: &ClipboardService,
    input: &InputControlService,
    peer_id: PeerId,
) -> Result<(), DomainError> {
    info!(
        %peer_id,
        "single connection: habilitando clipboard → input → layout announce"
    );
    clipboard.enable_for_peer(peer_id).await?;
    input.enable_for_peer(peer_id).await?;
    input.announce_layout_sync(peer_id).await?;
    info!(%peer_id, "single connection: KVM + layout sync habilitados");
    Ok(())
}

/// No peer **inbound** (aceitou QUIC), habilita KVM automaticamente — o initiator
/// já chama `enable_kvm_for_peer` via `open_connection`.
pub async fn run_inbound_auto_enable(
    bus: EventBus,
    clipboard: Arc<ClipboardService>,
    input: Arc<InputControlService>,
) {
    let mut rx = bus.subscribe();
    loop {
        match rx.recv().await {
            Ok(DomainEvent::ConnectionEstablished(e))
                if e.is_inbound && e.via_workspace_id.is_none() =>
            {
                let peer_id = e.peer_id;
                info!(
                    %peer_id,
                    username = %e.peer_username,
                    "single connection: peer inbound conectou — auto-habilitando KVM"
                );
                if let Err(err) =
                    enable_kvm_for_peer(clipboard.as_ref(), input.as_ref(), peer_id).await
                {
                    warn!(
                        ?err,
                        %peer_id,
                        "single connection: falha ao auto-habilitar inbound"
                    );
                }
            }
            Ok(DomainEvent::ConnectionEstablished(e)) if !e.is_inbound => {
                info!(
                    peer_id = %e.peer_id,
                    "single connection: conexão outbound — KVM via open_connection"
                );
            }
            Ok(_) => {}
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
}
