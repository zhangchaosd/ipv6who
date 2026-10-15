//! Discovery and filtering of the host's global IPv6 addresses.

use std::net::Ipv6Addr;

use ipnet::Ipv6Net;
use serde::Serialize;

use crate::config::InterfaceFilter;

// Kernel IFA_F_* flags (linux/if_addr.h).
pub const IFA_F_TEMPORARY: u32 = 0x01;
pub const IFA_F_DADFAILED: u32 = 0x08;
pub const IFA_F_DEPRECATED: u32 = 0x20;
pub const IFA_F_TENTATIVE: u32 = 0x40;
pub const IFA_F_PERMANENT: u32 = 0x80;
pub const IFA_F_STABLE_PRIVACY: u32 = 0x800;

/// Lifetime value the kernel uses for "forever".
const INFINITY_LIFE_TIME: u32 = u32::MAX;

/// An IPv6 address as reported by the kernel, before filtering.
#[derive(Debug, Clone)]
pub struct RawAddress {
    pub interface: String,
    pub address: Ipv6Addr,
    pub prefix_len: u8,
    pub flags: u32,
    pub preferred_lft: Option<u32>,
    pub valid_lft: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AddressType {
    /// Configured without an expiry (manually or by a static network config).
    Static,
    /// Has a finite lifetime: SLAAC or DHCPv6.
    Dynamic,
    /// RFC 8981 privacy-extension address.
    Temporary,
}

#[derive(Debug, Clone, Serialize)]
pub struct Address {
    pub address: Ipv6Addr,
    pub prefix_len: u8,
    pub interface: String,
    #[serde(rename = "type")]
    pub kind: AddressType,
    pub scope: &'static str,
    pub deprecated: bool,
    pub stable_privacy: bool,
    /// Seconds; `null` means infinite.
    pub preferred_lifetime: Option<u32>,
    /// Seconds; `null` means infinite.
    pub valid_lifetime: Option<u32>,
}

fn global_unicast() -> Ipv6Net {
    "2000::/3".parse().unwrap()
}

fn excluded_ranges() -> [Ipv6Net; 2] {
    [
        "fc00::/7".parse().unwrap(),  // unique local
        "fe80::/10".parse().unwrap(), // link local
    ]
}

/// Keeps usable global unicast addresses and orders them best-first.
pub fn select(
    raw: impl IntoIterator<Item = RawAddress>,
    interfaces: &InterfaceFilter,
    include_temporary: bool,
) -> Vec<Address> {
    let global = global_unicast();
    let excluded = excluded_ranges();
    let mut out: Vec<Address> = raw
        .into_iter()
        .filter(|r| global.contains(&r.address))
        .filter(|r| !excluded.iter().any(|n| n.contains(&r.address)))
        .filter(|r| r.flags & (IFA_F_TENTATIVE | IFA_F_DADFAILED) == 0)
        .filter(|r| include_temporary || r.flags & IFA_F_TEMPORARY == 0)
        .filter(|r| interfaces.matches(&r.interface))
        .map(|r| {
            let kind = if r.flags & IFA_F_TEMPORARY != 0 {
                AddressType::Temporary
            } else if r.flags & IFA_F_PERMANENT != 0 {
                AddressType::Static
            } else {
                AddressType::Dynamic
            };
            Address {
                address: r.address,
                prefix_len: r.prefix_len,
                interface: r.interface,
                kind,
                scope: "global",
                deprecated: r.flags & IFA_F_DEPRECATED != 0,
                stable_privacy: r.flags & IFA_F_STABLE_PRIVACY != 0,
                preferred_lifetime: r.preferred_lft.filter(|&v| v != INFINITY_LIFE_TIME),
                valid_lifetime: r.valid_lft.filter(|&v| v != INFINITY_LIFE_TIME),
            }
        })
        .collect();

    // Best first: not deprecated, most stable type, longest remaining lifetime.
    out.sort_by(|a, b| {
        a.deprecated
            .cmp(&b.deprecated)
            .then(a.kind.cmp(&b.kind))
            .then(lifetime_key(b.preferred_lifetime).cmp(&lifetime_key(a.preferred_lifetime)))
            .then_with(|| a.interface.cmp(&b.interface))
            .then(a.address.cmp(&b.address))
    });
    out
}

fn lifetime_key(lft: Option<u32>) -> u64 {
    lft.map_or(u64::MAX, u64::from)
}

#[cfg(target_os = "linux")]
pub use netlink::list;

#[cfg(not(target_os = "linux"))]
pub async fn list() -> std::io::Result<Vec<RawAddress>> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "address discovery requires Linux netlink",
    ))
}

#[cfg(target_os = "linux")]
mod netlink {
    use std::{collections::HashMap, io, net::IpAddr};

    use futures_util::TryStreamExt;
    use rtnetlink::packet_route::{
        AddressFamily,
        address::{AddressAttribute, AddressMessage},
        link::LinkAttribute,
    };

    use super::RawAddress;

    /// Dumps all IPv6 addresses via an RTM_GETADDR netlink request.
    pub async fn list() -> io::Result<Vec<RawAddress>> {
        let (connection, handle, _) = rtnetlink::new_connection()?;
        let task = tokio::spawn(connection);
        let result = async {
            let mut names = HashMap::new();
            let mut links = handle.link().get().execute();
            while let Some(link) = links.try_next().await.map_err(io::Error::other)? {
                for attr in link.attributes {
                    if let LinkAttribute::IfName(name) = attr {
                        names.insert(link.header.index, name);
                    }
                }
            }

            let mut out = Vec::new();
            let mut addrs = handle.address().get().execute();
            while let Some(msg) = addrs.try_next().await.map_err(io::Error::other)? {
                if msg.header.family == AddressFamily::Inet6
                    && let Some(raw) = to_raw(msg, &names)
                {
                    out.push(raw);
                }
            }
            Ok(out)
        }
        .await;
        task.abort();
        result
    }

    fn to_raw(msg: AddressMessage, names: &HashMap<u32, String>) -> Option<RawAddress> {
        let mut address = None;
        let mut local = None;
        // IFA_FLAGS (32-bit) supersedes the 8-bit header flags when present.
        let mut flags = u32::from(msg.header.flags.bits());
        let (mut preferred_lft, mut valid_lft) = (None, None);
        for attr in msg.attributes {
            match attr {
                AddressAttribute::Address(IpAddr::V6(a)) => address = Some(a),
                AddressAttribute::Local(IpAddr::V6(a)) => local = Some(a),
                AddressAttribute::Flags(f) => flags = f.bits(),
                AddressAttribute::CacheInfo(ci) => {
                    preferred_lft = Some(ci.ifa_preferred);
                    valid_lft = Some(ci.ifa_valid);
                }
                _ => {}
            }
        }
        let index = msg.header.index;
        Some(RawAddress {
            interface: names
                .get(&index)
                .cloned()
                .unwrap_or_else(|| index.to_string()),
            // On point-to-point links IFA_LOCAL is our address and IFA_ADDRESS the peer.
            address: local.or(address)?,
            prefix_len: msg.header.prefix_len,
            flags,
            preferred_lft,
            valid_lft,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(iface: &str, addr: &str, flags: u32, preferred: u32) -> RawAddress {
        RawAddress {
            interface: iface.into(),
            address: addr.parse().unwrap(),
            prefix_len: 64,
            flags,
            preferred_lft: Some(preferred),
            valid_lft: Some(preferred),
        }
    }

    fn addrs(v: &[Address]) -> Vec<String> {
        v.iter().map(|a| a.address.to_string()).collect()
    }

    #[test]
    fn keeps_only_global_unicast() {
        let input = vec![
            raw("eth0", "2001:db8::1", IFA_F_PERMANENT, u32::MAX),
            raw("eth0", "fe80::1", IFA_F_PERMANENT, u32::MAX),
            raw("eth0", "fd00::1", IFA_F_PERMANENT, u32::MAX),
            raw("lo", "::1", IFA_F_PERMANENT, u32::MAX),
            raw("eth0", "ff02::1", 0, 100),
        ];
        let out = select(input, &InterfaceFilter::default(), false);
        assert_eq!(addrs(&out), ["2001:db8::1"]);
        assert_eq!(out[0].kind, AddressType::Static);
        assert_eq!(out[0].preferred_lifetime, None);
    }

    #[test]
    fn temporary_excluded_by_default() {
        let input = vec![
            raw("eth0", "2001:db8::aaaa", IFA_F_TEMPORARY, 3600),
            raw("eth0", "2001:db8::1", 0, 1800),
        ];
        let out = select(input.clone(), &InterfaceFilter::default(), false);
        assert_eq!(addrs(&out), ["2001:db8::1"]);

        let out = select(input, &InterfaceFilter::default(), true);
        assert_eq!(addrs(&out), ["2001:db8::1", "2001:db8::aaaa"]);
        assert_eq!(out[1].kind, AddressType::Temporary);
    }

    #[test]
    fn skips_tentative_and_dadfailed() {
        let input = vec![
            raw("eth0", "2001:db8::1", IFA_F_TENTATIVE, 100),
            raw("eth0", "2001:db8::2", IFA_F_DADFAILED, 100),
        ];
        assert!(select(input, &InterfaceFilter::default(), false).is_empty());
    }

    #[test]
    fn ordering_prefers_stable_and_fresh() {
        let input = vec![
            raw("eth0", "2001:db8::d", IFA_F_DEPRECATED, 0),
            raw("eth0", "2001:db8::a", 0, 100),
            raw("eth0", "2001:db8::b", 0, 900),
            raw("eth1", "2001:db8::c", IFA_F_PERMANENT, u32::MAX),
        ];
        let out = select(input, &InterfaceFilter::default(), false);
        assert_eq!(
            addrs(&out),
            ["2001:db8::c", "2001:db8::b", "2001:db8::a", "2001:db8::d"]
        );
        assert!(out[3].deprecated);
    }

    #[test]
    fn interface_filter_applies() {
        let filter = InterfaceFilter {
            include: vec!["eth*".into()],
            exclude: vec!["eth1".into()],
        };
        let input = vec![
            raw("eth0", "2001:db8::1", 0, 100),
            raw("eth1", "2001:db8::2", 0, 100),
            raw("wg0", "2001:db8::3", 0, 100),
        ];
        assert_eq!(addrs(&select(input, &filter, false)), ["2001:db8::1"]);
    }
}
