use winx_application::ports::network::{NetworkInterfaceDescriptor, NetworkInterfacesProvider};

pub struct IfAddrsNetworkProvider;

impl NetworkInterfacesProvider for IfAddrsNetworkProvider {
    fn list_active(&self) -> anyhow::Result<Vec<NetworkInterfaceDescriptor>> {
        super::network_interfaces::list_active().map(|interfaces| {
            interfaces
                .into_iter()
                .map(|iface| NetworkInterfaceDescriptor {
                    name: iface.name,
                    ipv4: iface.ipv4,
                })
                .collect()
        })
    }
}
