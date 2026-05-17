use anyhow::Result;
use std::net::IpAddr;
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, info};

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone)]
pub enum NetworkEvent {
    InterfaceChanged { alias: String, ipv4: Option<IpAddr> },
    InterfaceLost { alias: String },
}

pub struct NetworkWatcher {
    _join_handle: std::thread::JoinHandle<()>,
}

impl NetworkWatcher {
    pub fn start() -> Result<(Self, mpsc::UnboundedReceiver<NetworkEvent>)> {
        let (tx, rx) = mpsc::unbounded_channel();

        let join_handle = std::thread::spawn(move || {
            let mut last_state: Vec<String> = Vec::new();
            let mut first_run = true;

            loop {
                std::thread::sleep(Duration::from_secs(3));

                match get_active_interfaces() {
                    Ok(current_state) => {
                        // Na primeira execução, apenas registra o estado sem emitir eventos
                        // para evitar false positives (todas as interfaces sendo "novas")
                        if first_run {
                            debug!(
                                "NetworkWatcher: inicial state captured, {} interfaces",
                                current_state.len()
                            );
                            for alias in &current_state {
                                debug!("  - {}", alias);
                            }
                            last_state = current_state;
                            first_run = false;
                            continue;
                        }

                        // Detecta interfaces que apareceram
                        for alias in &current_state {
                            if !last_state.contains(alias) {
                                info!("Network interface appeared (real change): {}", alias);
                                let _ = tx.send(NetworkEvent::InterfaceChanged {
                                    alias: alias.clone(),
                                    ipv4: None,
                                });
                            }
                        }

                        // Detecta interfaces que desapareceram
                        for alias in &last_state {
                            if !current_state.contains(alias) {
                                info!("Network interface lost: {}", alias);
                                let _ = tx.send(NetworkEvent::InterfaceLost {
                                    alias: alias.clone(),
                                });
                            }
                        }

                        last_state = current_state;
                    }
                    Err(e) => {
                        debug!("Failed to get network interfaces: {}", e);
                    }
                }
            }
        });

        Ok((
            NetworkWatcher {
                _join_handle: join_handle,
            },
            rx,
        ))
    }
}

fn get_active_interfaces() -> Result<Vec<String>> {
    let output = Command::new("powershell")
        .creation_flags(CREATE_NO_WINDOW)
        .args(&[
            "-NoProfile",
            "-Command",
            r#"
            Get-NetIPConfiguration -Detailed -ErrorAction SilentlyContinue |
            Where-Object { $_.NetAdapter.Status -eq 'Up' } |
            Where-Object { $_.IPv4Address -ne $null } |
            Select-Object -ExpandProperty InterfaceAlias |
            ConvertTo-Json -Compress
            "#,
        ])
        .output()?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8(output.stdout)?;
    if stdout.trim().is_empty() {
        return Ok(Vec::new());
    }

    match serde_json::from_str::<Vec<String>>(&stdout) {
        Ok(interfaces) => Ok(interfaces),
        Err(_) => {
            if let Ok(single) = serde_json::from_str::<String>(&stdout) {
                Ok(vec![single])
            } else {
                Ok(Vec::new())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_active_interfaces() {
        match get_active_interfaces() {
            Ok(ifaces) => {
                println!("Active interfaces: {:?}", ifaces);
                assert!(
                    !ifaces.is_empty(),
                    "Should find at least one active interface"
                );
            }
            Err(e) => {
                println!("get_active_interfaces error (may be OK in CI): {}", e);
            }
        }
    }
}
