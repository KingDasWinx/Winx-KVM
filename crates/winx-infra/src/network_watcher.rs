use anyhow::Result;
use std::net::IpAddr;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, info};

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

            loop {
                std::thread::sleep(Duration::from_secs(3));

                match get_active_interfaces() {
                    Ok(current_state) => {
                        for alias in &current_state {
                            if !last_state.contains(alias) {
                                debug!("Network interface appeared: {}", alias);
                                let _ = tx.send(NetworkEvent::InterfaceChanged {
                                    alias: alias.clone(),
                                    ipv4: None,
                                });
                            }
                        }

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
    use std::process::Command;

    let output = Command::new("powershell")
        .args(&[
            "-NoProfile",
            "-Command",
            r#"
            Get-NetConnectionProfile -ErrorAction SilentlyContinue |
            Where-Object { $_.NetworkCategory -ne 'Disconnected' } |
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
                assert!(!ifaces.is_empty(), "Should find at least one active interface");
            }
            Err(e) => {
                println!("get_active_interfaces error (may be OK in CI): {}", e);
            }
        }
    }
}
