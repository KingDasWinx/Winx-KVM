use std::net::Ipv4Addr;

pub struct NetworkInterfaceDescriptor {
    pub name: String,
    pub ipv4: Option<Ipv4Addr>,
}

pub trait NetworkInterfacesProvider: Send + Sync {
    fn list_active(&self) -> anyhow::Result<Vec<NetworkInterfaceDescriptor>>;
}
