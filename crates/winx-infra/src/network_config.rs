use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{info, warn};

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallRule {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Profile")]
    pub profile: String,
    #[serde(rename = "Protocol")]
    pub protocol: String,
    #[serde(rename = "Program")]
    pub program: Option<String>,
    #[serde(rename = "Enabled")]
    pub enabled: String,
    #[serde(rename = "Direction")]
    pub direction: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkCategory {
    Private,
    Public,
    DomainAuthenticated,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct NetworkProfileInfo {
    pub interface_alias: String,
    pub ipv4: Option<IpAddr>,
    pub category: NetworkCategory,
}

#[derive(Debug, Clone, Default)]
pub struct NetworkConfigStatus {
    pub current_exe: PathBuf,
    pub firewall_rules: Vec<FirewallRule>,
    pub network_profiles: Vec<NetworkProfileInfo>,
    pub has_private_or_domain_network: bool,
}

impl NetworkConfigStatus {
    pub fn needs_fix(&self) -> bool {
        let expected_rules = vec![
            "Winx-KVM mDNS UDP In",
            "Winx-KVM QUIC UDP In",
            "Winx-KVM Program UDP In",
            "Winx-KVM Program UDP Out",
        ];

        let found_names: Vec<&str> = self
            .firewall_rules
            .iter()
            .map(|r| r.name.as_str())
            .collect();

        for expected in &expected_rules {
            if !found_names.contains(expected) {
                return true;
            }
        }

        let exe_path = self.current_exe.to_string_lossy().to_lowercase();

        for rule in &self.firewall_rules {
            if rule.enabled.to_lowercase() != "true" {
                return true;
            }

            if let Some(prog) = &rule.program {
                if prog.to_lowercase() != exe_path {
                    return true;
                }
            }
        }

        false
    }

    pub fn issues(&self) -> Vec<String> {
        let mut issues = Vec::new();
        let expected_rules = vec![
            "Winx-KVM mDNS UDP In",
            "Winx-KVM QUIC UDP In",
            "Winx-KVM Program UDP In",
            "Winx-KVM Program UDP Out",
        ];

        let found_names: Vec<&str> = self
            .firewall_rules
            .iter()
            .map(|r| r.name.as_str())
            .collect();

        for expected in &expected_rules {
            if !found_names.contains(expected) {
                issues.push(format!("Missing firewall rule: {}", expected));
            }
        }

        if self.firewall_rules.is_empty() {
            issues.push("No firewall rules found for Winx-KVM".to_string());
            return issues;
        }

        let exe_path = self.current_exe.to_string_lossy().to_lowercase();

        for rule in &self.firewall_rules {
            if rule.enabled.to_lowercase() != "true" {
                issues.push(format!("Rule '{}' is disabled", rule.name));
            }

            if let Some(prog) = &rule.program {
                if prog.to_lowercase() != exe_path {
                    issues.push(format!(
                        "Rule '{}' has stale path: {} (current: {})",
                        rule.name, prog, exe_path
                    ));
                }
            }
        }

        issues
    }
}

pub fn is_elevated() -> bool {
    let output = Command::new("net")
        .creation_flags(CREATE_NO_WINDOW)
        .args(&["session"])
        .output();

    match output {
        Ok(o) => o.status.success(),
        Err(_) => false,
    }
}

pub fn inspect() -> Result<NetworkConfigStatus> {
    let current_exe = std::env::current_exe()?;

    let firewall_rules = get_firewall_rules()?;
    let network_profiles = get_network_profiles()?;

    let has_private_or_domain_network = network_profiles.iter().any(|p| {
        p.category == NetworkCategory::Private || p.category == NetworkCategory::DomainAuthenticated
    });

    Ok(NetworkConfigStatus {
        current_exe,
        firewall_rules,
        network_profiles,
        has_private_or_domain_network,
    })
}

fn get_firewall_rules() -> Result<Vec<FirewallRule>> {
    let output = Command::new("powershell")
        .creation_flags(CREATE_NO_WINDOW)
        .args(&[
            "-NoProfile",
            "-Command",
            r#"
            Get-NetFirewallRule -DisplayName "Winx-KVM*" -ErrorAction SilentlyContinue | ForEach-Object {
                $p = $_ | Get-NetFirewallPortFilter
                $a = $_ | Get-NetFirewallApplicationFilter
                @{
                    Name=$_.DisplayName;
                    Profile="$($_.Profile)";
                    Protocol="$($p.Protocol)";
                    Program=$a.Program;
                    Enabled="$($_.Enabled)";
                    Direction="$($_.Direction)"
                }
            } | ConvertTo-Json -Compress
            "#,
        ])
        .output()?;

    if !output.status.success() {
        warn!(
            "Failed to get firewall rules: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8(output.stdout)?;
    if stdout.trim().is_empty() {
        return Ok(Vec::new());
    }

    match serde_json::from_str::<Vec<FirewallRule>>(&stdout) {
        Ok(rules) => Ok(rules),
        Err(e) => {
            warn!("Failed to parse firewall rules JSON: {}", e);
            Ok(Vec::new())
        }
    }
}

fn get_network_profiles() -> Result<Vec<NetworkProfileInfo>> {
    let output = Command::new("powershell")
        .creation_flags(CREATE_NO_WINDOW)
        .args(&[
            "-NoProfile",
            "-Command",
            r#"
            Get-NetIPConfiguration -Detailed -ErrorAction SilentlyContinue |
            Where-Object { $_.NetAdapter.Status -eq 'Up' } |
            Select-Object @{n='InterfaceAlias';e={$_.InterfaceAlias}},
                          @{n='NetworkCategory';e={$_.NetConnectionProfile.NetworkCategory}},
                          @{n='IPv4Address';e={$_.IPv4Address.IPAddress}} |
            ConvertTo-Json -Compress
            "#,
        ])
        .output()?;

    if !output.status.success() {
        warn!(
            "Failed to get network profiles: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8(output.stdout)?;
    if stdout.trim().is_empty() {
        return Ok(Vec::new());
    }

    #[derive(Deserialize)]
    struct RawProfile {
        #[serde(rename = "InterfaceAlias")]
        interface_alias: Option<String>,
        #[serde(rename = "NetworkCategory")]
        network_category: Option<String>,
        #[serde(rename = "IPv4Address")]
        ipv4_address: Option<String>,
    }

    let profiles: Vec<RawProfile> = serde_json::from_str(&stdout).unwrap_or_default();
    let network_profiles = profiles
        .into_iter()
        .filter_map(|p| {
            let interface_alias = p.interface_alias?;
            let category = match p.network_category.as_deref() {
                Some("Private") => NetworkCategory::Private,
                Some("Public") => NetworkCategory::Public,
                Some("DomainAuthenticated") => NetworkCategory::DomainAuthenticated,
                _ => NetworkCategory::Unknown,
            };

            let ipv4 = p.ipv4_address.and_then(|ip| ip.parse::<IpAddr>().ok());

            Some(NetworkProfileInfo {
                interface_alias,
                ipv4,
                category,
            })
        })
        .collect();

    Ok(network_profiles)
}

pub fn reconfigure(exe_path: &Path) -> Result<()> {
    if !is_elevated() {
        return Err(anyhow!("Not elevated — cannot reconfigure firewall"));
    }

    let exe_str = exe_path
        .to_str()
        .ok_or_else(|| anyhow!("Invalid exe path"))?;

    info!("Removing old Winx-KVM firewall rules");
    let _ = Command::new("powershell")
        .creation_flags(CREATE_NO_WINDOW)
        .args(&[
            "-NoProfile",
            "-Command",
            r#"Remove-NetFirewallRule -DisplayName "Winx-KVM*" -ErrorAction SilentlyContinue"#,
        ])
        .output();

    let rules: Vec<(&str, String)> = vec![
        (
            "Winx-KVM mDNS UDP In",
            r#"
            New-NetFirewallRule -DisplayName "Winx-KVM mDNS UDP In" `
              -Direction Inbound -Action Allow -Protocol UDP `
              -LocalPort 5353 -Profile Any -Enabled True -ErrorAction Stop
            "#
            .to_string(),
        ),
        (
            "Winx-KVM QUIC UDP In",
            r#"
            New-NetFirewallRule -DisplayName "Winx-KVM QUIC UDP In" `
              -Direction Inbound -Action Allow -Protocol UDP `
              -LocalPort 7878 -Profile Any -Enabled True -ErrorAction Stop
            "#
            .to_string(),
        ),
        (
            "Winx-KVM Pairing UDP In",
            r#"
            New-NetFirewallRule -DisplayName "Winx-KVM Pairing UDP In" `
              -Direction Inbound -Action Allow -Protocol UDP `
              -LocalPort 7879 -Profile Any -Enabled True -ErrorAction Stop
            "#
            .to_string(),
        ),
        (
            "Winx-KVM Workspace UDP In",
            r#"
            New-NetFirewallRule -DisplayName "Winx-KVM Workspace UDP In" `
              -Direction Inbound -Action Allow -Protocol UDP `
              -LocalPort 7880 -Profile Any -Enabled True -ErrorAction Stop
            "#
            .to_string(),
        ),
        (
            "Winx-KVM Program UDP In",
            format!(
                r#"
                New-NetFirewallRule -DisplayName "Winx-KVM Program UDP In" `
                  -Direction Inbound -Action Allow -Protocol UDP `
                  -Program "{}" -Profile Any -Enabled True -ErrorAction Stop
                "#,
                exe_str
            ),
        ),
        (
            "Winx-KVM Program UDP Out",
            format!(
                r#"
                New-NetFirewallRule -DisplayName "Winx-KVM Program UDP Out" `
                  -Direction Outbound -Action Allow -Protocol UDP `
                  -Program "{}" -Profile Any -Enabled True -ErrorAction Stop
                "#,
                exe_str
            ),
        ),
    ];

    for (name, ps_cmd) in rules {
        info!("Adding firewall rule: {}", name);
        let output = Command::new("powershell")
            .creation_flags(CREATE_NO_WINDOW)
            .args(&["-NoProfile", "-Command", &ps_cmd])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("Failed to add rule '{}': {}", name, stderr);
            return Err(anyhow!("Failed to add rule '{}': {}", name, stderr));
        }

        info!("Successfully added rule: {}", name);
    }

    info!("Firewall rules reconfigured successfully");
    Ok(())
}

pub fn request_firewall_setup_via_uac() -> Result<i32> {
    let exe_path = std::env::current_exe()?;

    let script = format!(
        r#"
        Start-Process -FilePath '{}' -ArgumentList '--setup-firewall' -Verb RunAs -Wait -WindowStyle Hidden
        exit $LASTEXITCODE
        "#,
        exe_path.to_string_lossy().replace("'", "''")
    );

    let output = Command::new("powershell")
        .creation_flags(CREATE_NO_WINDOW)
        .args(&["-NoProfile", "-Command", &script])
        .output()?;

    Ok(output.status.code().unwrap_or(1))
}
