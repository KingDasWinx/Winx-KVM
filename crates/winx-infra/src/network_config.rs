use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{info, warn};

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
        if self.firewall_rules.is_empty() {
            return true;
        }

        let exe_path = self.current_exe.to_string_lossy().to_lowercase();

        for rule in &self.firewall_rules {
            let rule_program = rule
                .program
                .as_ref()
                .map(|p| p.to_lowercase())
                .unwrap_or_default();

            if rule_program != exe_path {
                return true;
            }

            if rule.enabled.to_lowercase() != "true" {
                return true;
            }

            if !rule.profile.contains("Private") && !rule.profile.contains("Domain") {
                return true;
            }
        }

        false
    }

    pub fn issues(&self) -> Vec<String> {
        let mut issues = Vec::new();

        if self.firewall_rules.is_empty() {
            issues.push("No firewall rules found for Winx-KVM".to_string());
            return issues;
        }

        let exe_path = self.current_exe.to_string_lossy().to_lowercase();

        for rule in &self.firewall_rules {
            let rule_program = rule
                .program
                .as_ref()
                .map(|p| p.to_lowercase())
                .unwrap_or_default();

            if rule_program != exe_path {
                issues.push(format!(
                    "Rule '{}' has stale path: {} (current: {})",
                    rule.name, rule_program, exe_path
                ));
            }

            if rule.enabled.to_lowercase() != "true" {
                issues.push(format!("Rule '{}' is disabled", rule.name));
            }

            if !rule.profile.contains("Private") && !rule.profile.contains("Domain") {
                issues.push(format!(
                    "Rule '{}' has wrong profile: {} (need Private or Domain)",
                    rule.name, rule.profile
                ));
            }
        }

        issues
    }
}

pub fn is_elevated() -> bool {
    use std::process::Command;

    let output = Command::new("net")
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

    let has_private_or_domain_network = network_profiles
        .iter()
        .any(|p| p.category == NetworkCategory::Private || p.category == NetworkCategory::DomainAuthenticated);

    Ok(NetworkConfigStatus {
        current_exe,
        firewall_rules,
        network_profiles,
        has_private_or_domain_network,
    })
}

fn get_firewall_rules() -> Result<Vec<FirewallRule>> {
    let output = Command::new("powershell")
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
        warn!("Failed to get firewall rules: {}", String::from_utf8_lossy(&output.stderr));
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
        .args(&[
            "-NoProfile",
            "-Command",
            r#"
            Get-NetConnectionProfile -ErrorAction SilentlyContinue | Select-Object Name, InterfaceAlias, NetworkCategory, IPv4Connectivity | ConvertTo-Json -Compress
            "#,
        ])
        .output()?;

    if !output.status.success() {
        warn!("Failed to get network profiles: {}", String::from_utf8_lossy(&output.stderr));
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

            Some(NetworkProfileInfo {
                interface_alias,
                ipv4: None,
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
    let _ = Command::new("netsh")
        .args(&["advfirewall", "firewall", "delete", "rule", "name=Winx-KVM"])
        .output();

    info!("Adding UDP inbound rule");
    Command::new("netsh")
        .args(&[
            "advfirewall",
            "firewall",
            "add",
            "rule",
            "name=Winx-KVM",
            "dir=in",
            "action=allow",
            &format!("program={}", exe_str),
            "protocol=UDP",
            "profile=private,domain",
            "enable=yes",
        ])
        .output()?;

    info!("Adding TCP inbound rule");
    Command::new("netsh")
        .args(&[
            "advfirewall",
            "firewall",
            "add",
            "rule",
            "name=Winx-KVM",
            "dir=in",
            "action=allow",
            &format!("program={}", exe_str),
            "protocol=TCP",
            "profile=private,domain",
            "enable=yes",
        ])
        .output()?;

    info!("Firewall rules reconfigured successfully");
    Ok(())
}

pub fn request_firewall_setup_via_uac() -> Result<i32> {
    let exe_path = std::env::current_exe()?;

    let script = format!(
        r#"
        Start-Process -FilePath '{}' -ArgumentList '--setup-firewall' -Verb RunAs -Wait
        exit $LASTEXITCODE
        "#,
        exe_path.to_string_lossy().replace("'", "''")
    );

    let output = Command::new("powershell")
        .args(&["-NoProfile", "-Command", &script])
        .output()?;

    Ok(output.status.code().unwrap_or(1))
}
