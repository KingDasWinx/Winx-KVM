use std::net::Ipv4Addr;

#[derive(Debug, Clone)]
pub struct NetworkInterfaceInfo {
    pub name: String,
    pub ipv4: Option<Ipv4Addr>,
}

pub fn list_active() -> anyhow::Result<Vec<NetworkInterfaceInfo>> {
    use std::net::Ipv4Addr;

    let mut result: Vec<NetworkInterfaceInfo> = Vec::new();

    for iface in if_addrs::get_if_addrs()? {
        if iface.is_loopback() {
            continue;
        }

        let ipv4 = match iface.ip() {
            std::net::IpAddr::V4(v4) => Some(v4),
            std::net::IpAddr::V6(_) => None,
        };

        let name = iface.name.clone();
        let entry = result.iter_mut().find(|e| e.name == name);

        if let Some(entry) = entry {
            if entry.ipv4.is_none() && ipv4.is_some() {
                entry.ipv4 = ipv4;
            }
        } else if ipv4.is_some() {
            result.push(NetworkInterfaceInfo { name, ipv4 });
        }
    }

    result.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(result)
}
