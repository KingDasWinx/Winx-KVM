//! Commands do contexto de discovery.

use std::sync::Arc;

use serde::Serialize;
use tauri::State;
use winx_infra::TomlConfigStore;

use crate::app_state::{DiscoveryState, IdentityState};

/// DTO de um peer descoberto na rede para o frontend.
#[derive(Debug, Serialize)]
pub struct DiscoveredPeerDto {
    pub id: String,
    pub username: String,
    pub fingerprint: String,
    pub addresses: Vec<String>,
    pub is_paired: bool,
}

/// Retorna snapshot dos peers atualmente visíveis na rede.
#[tauri::command]
pub async fn list_discovered_peers(
    discovery: State<'_, DiscoveryState>,
    identity: State<'_, IdentityState>,
) -> Result<Vec<DiscoveredPeerDto>, String> {
    let peers = discovery
        .discovery
        .list_peers_enriched(identity.identity_store.as_ref())
        .await
        .map_err(|e| format!("falha ao listar peers: {e}"))?;

    Ok(peers
        .into_iter()
        .map(|p| DiscoveredPeerDto {
            id: p.peer.id.to_string(),
            username: p.peer.username,
            fingerprint: p.peer.fingerprint,
            addresses: p.peer.addresses.iter().map(ToString::to_string).collect(),
            is_paired: p.is_paired,
        })
        .collect())
}

/// DTO de uma interface de rede para o frontend.
#[derive(Debug, Serialize)]
pub struct NetworkInterfaceDto {
    pub name: String,
    pub ipv4: Option<String>,
}

/// Lista interfaces de rede ativas.
#[tauri::command]
pub async fn list_network_interfaces() -> Result<Vec<NetworkInterfaceDto>, String> {
    let interfaces = winx_infra::network_interfaces::list_active()
        .map_err(|e| format!("falha ao enumerar interfaces: {}", e))?;

    Ok(interfaces
        .into_iter()
        .map(|iface| NetworkInterfaceDto {
            name: iface.name,
            ipv4: iface.ipv4.map(|ip| ip.to_string()),
        })
        .collect())
}

/// Retorna lista de interfaces atualmente habilitadas para mDNS.
#[tauri::command]
pub async fn get_discovery_interfaces(
    config_store: State<'_, Arc<TomlConfigStore>>,
) -> Result<Vec<String>, String> {
    let cfg = config_store
        .load_or_create()
        .map_err(|e| format!("falha ao carregar config: {}", e))?;
    Ok(cfg.discovery.interfaces)
}

/// Redefine as interfaces de rede para mDNS e reanuncia.
#[tauri::command]
pub async fn set_discovery_interfaces(
    discovery: State<'_, DiscoveryState>,
    config_store: State<'_, Arc<TomlConfigStore>>,
    interfaces: Vec<String>,
) -> Result<(), String> {
    config_store
        .save_discovery_interfaces(&interfaces)
        .map_err(|e| format!("falha ao salvar config: {}", e))?;

    discovery
        .discovery
        .set_discovery_interfaces(&interfaces)
        .await
        .map_err(|e| format!("falha ao reconfigurar mDNS: {}", e))
}
