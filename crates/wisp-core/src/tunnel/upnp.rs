//! Asking the router to let friends through (UPnP port mapping), so they reach this computer
//! directly even when their own router is strict. Most home routers allow it; when one doesn't,
//! the link still tries a direct connection the other way.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use igd_next::aio::Gateway;
use igd_next::aio::tokio::Tokio;
pub use igd_next::PortMappingProtocol as Protocol;

/// How long a mapping lasts before it's renewed (routers drop it after, if Wisp is gone).
pub const LEASE: Duration = Duration::from_secs(60 * 60);

pub struct Mapping {
    gateway: Gateway<Tokio>,
    protocol: Protocol,
    local: SocketAddr,
    external: SocketAddr,
    description: String,
}

impl Mapping {
    /// The address people outside use.
    pub fn external(&self) -> SocketAddr {
        self.external
    }

    /// Asks again for the same outside port, before the lease runs out.
    pub async fn renew(&self) -> bool {
        self.gateway.add_port(self.protocol, self.external.port(), self.local, LEASE.as_secs() as u32, &self.description).await.is_ok()
    }

    /// Takes the mapping off the router.
    pub async fn remove(&self) {
        let _ = self.gateway.remove_port(self.protocol, self.external.port()).await;
    }
}

/// Whether an address belongs to a private network (behind another router, so a mapping on this
/// one doesn't reach the internet).
pub fn is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified() || (ip.octets()[0] == 100 && (ip.octets()[1] & 0xC0) == 64),
        IpAddr::V6(ip) => ip.is_loopback() || ip.is_unspecified() || (ip.segments()[0] & 0xfe00) == 0xfc00 || (ip.segments()[0] & 0xffc0) == 0xfe80,
    }
}

/// Maps `local` (this computer's address on the network and the port) on the router, preferring
/// the same port outside. None when there's no UPnP router, it says no, or it's itself behind
/// another router.
pub async fn map(protocol: Protocol, local: SocketAddr, description: &str) -> Option<Mapping> {
    let mut options = igd_next::SearchOptions::default();
    options.timeout = Some(Duration::from_secs(2));
    let gateway = tokio::time::timeout(Duration::from_secs(3), igd_next::aio::tokio::search_gateway(options)).await.ok()?.ok()?;
    let ip = tokio::time::timeout(Duration::from_secs(3), gateway.get_external_ip()).await.ok()?.ok()?;
    if is_private(ip) {
        return None;
    }
    let lease = LEASE.as_secs() as u32;
    let port = match gateway.add_port(protocol, local.port(), local, lease, description).await {
        Ok(()) => local.port(),
        Err(_) => gateway.add_any_port(protocol, local, lease, description).await.ok()?,
    };
    Some(Mapping { gateway, protocol, local, external: SocketAddr::new(ip, port), description: description.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_private_addresses() {
        for private in ["10.0.0.1", "192.168.1.20", "172.16.4.4", "100.72.1.1", "127.0.0.1", "fd00::1", "fe80::1"] {
            assert!(is_private(private.parse().unwrap()), "{private}");
        }
        for public in ["203.0.113.5", "100.128.0.1", "2001:db8::1"] {
            assert!(!is_private(public.parse().unwrap()), "{public}");
        }
    }
}
