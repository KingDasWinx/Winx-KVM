//! Bounded context: **Discovery**.
//!
//! Anuncia este device na LAN via mDNS e mantém registry de peers
//! descobertos (não necessariamente confiáveis).
//!
//! Ver [docs/PLANNING.md][p] épico 3.
//!
//! [p]: ../../../../../docs/PLANNING.md

pub mod discovered_peer;
pub mod events;
pub mod registry;

pub use discovered_peer::DiscoveredPeer;
pub use events::{PeerAppeared, PeerDisappeared, PeerUpdated};
pub use registry::DiscoveryRegistry;
