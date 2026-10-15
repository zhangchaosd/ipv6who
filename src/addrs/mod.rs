//! Discovery and filtering of the host's global IPv6 addresses.

use std::net::Ipv6Addr;

use ipnet::Ipv6Net;
use serde::Serialize;

use crate::config::InterfaceFilter;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as os;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as os;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as os;

/// OS-independent view of the per-address state the kernel reports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Flags {
    /// Configured statically rather than by SLAAC or DHCPv6.
    pub permanent: bool,
    /// RFC 8981 privacy-extension address.
    pub temporary: bool,
    pub deprecated: bool,
    /// Tentative, duplicate, DAD failed or detached: not usable as a source.
    pub unusable: bool,
    /// Interface identifier generated per RFC 7217 or an equivalent scheme.
    pub stable_privacy: bool,
}

/// An IPv6 address as reported by the OS, before filtering.
#[derive(Debug, Clone, Serialize)]
pub struct RawAddress {
    pub interface: String,
    pub address: Ipv6Addr,
    pub prefix_len: u8,
    pub flags: Flags,
    /// Remaining seconds; `None` means infinite.
    pub preferred_lifetime: Option<u32>,
    /// Remaining seconds; `None` means infinite.
    pub valid_lifetime: Option<u32>,
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

/// Lists every IPv6 address on the host, unfiltered.
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub async fn list() -> std::io::Result<Vec<RawAddress>> {
    os::list().await
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub async fn list() -> std::io::Result<Vec<RawAddress>> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "address discovery is not implemented on this OS",
    ))
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
        .filter(|r| !r.flags.unusable)
        .filter(|r| include_temporary || !r.flags.temporary)
        .filter(|r| interfaces.matches(&r.interface))
        .map(|r| {
            let kind = if r.flags.temporary {
                AddressType::Temporary
            } else if r.flags.permanent {
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
                deprecated: r.flags.deprecated,
                stable_privacy: r.flags.stable_privacy,
                preferred_lifetime: r.preferred_lifetime,
                valid_lifetime: r.valid_lifetime,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn flags(f: impl FnOnce(&mut Flags)) -> Flags {
        let mut flags = Flags::default();
        f(&mut flags);
        flags
    }

    fn permanent() -> Flags {
        flags(|f| f.permanent = true)
    }

    fn raw(iface: &str, addr: &str, flags: Flags, preferred: Option<u32>) -> RawAddress {
        RawAddress {
            interface: iface.into(),
            address: addr.parse().unwrap(),
            prefix_len: 64,
            flags,
            preferred_lifetime: preferred,
            valid_lifetime: preferred,
        }
    }

    fn addrs(v: &[Address]) -> Vec<String> {
        v.iter().map(|a| a.address.to_string()).collect()
    }

    #[test]
    fn keeps_only_global_unicast() {
        let input = vec![
            raw("eth0", "2001:db8::1", permanent(), None),
            raw("eth0", "fe80::1", permanent(), None),
            raw("eth0", "fd00::1", permanent(), None),
            raw("lo", "::1", permanent(), None),
            raw("eth0", "ff02::1", Flags::default(), Some(100)),
        ];
        let out = select(input, &InterfaceFilter::default(), false);
        assert_eq!(addrs(&out), ["2001:db8::1"]);
        assert_eq!(out[0].kind, AddressType::Static);
        assert_eq!(out[0].preferred_lifetime, None);
    }

    #[test]
    fn temporary_excluded_by_default() {
        let input = vec![
            raw(
                "eth0",
                "2001:db8::aaaa",
                flags(|f| f.temporary = true),
                Some(3600),
            ),
            raw("eth0", "2001:db8::1", Flags::default(), Some(1800)),
        ];
        let out = select(input.clone(), &InterfaceFilter::default(), false);
        assert_eq!(addrs(&out), ["2001:db8::1"]);

        let out = select(input, &InterfaceFilter::default(), true);
        assert_eq!(addrs(&out), ["2001:db8::1", "2001:db8::aaaa"]);
        assert_eq!(out[1].kind, AddressType::Temporary);
    }

    #[test]
    fn skips_unusable() {
        let input = vec![raw(
            "eth0",
            "2001:db8::1",
            flags(|f| f.unusable = true),
            Some(100),
        )];
        assert!(select(input, &InterfaceFilter::default(), false).is_empty());
    }

    #[test]
    fn ordering_prefers_stable_and_fresh() {
        let input = vec![
            raw(
                "eth0",
                "2001:db8::d",
                flags(|f| f.deprecated = true),
                Some(0),
            ),
            raw("eth0", "2001:db8::a", Flags::default(), Some(100)),
            raw("eth0", "2001:db8::b", Flags::default(), Some(900)),
            raw("eth1", "2001:db8::c", permanent(), None),
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
            raw("eth0", "2001:db8::1", Flags::default(), Some(100)),
            raw("eth1", "2001:db8::2", Flags::default(), Some(100)),
            raw("wg0", "2001:db8::3", Flags::default(), Some(100)),
        ];
        assert_eq!(addrs(&select(input, &filter, false)), ["2001:db8::1"]);
    }

    /// Exercises the real OS backend; the host may have no global address,
    /// but listing must succeed and loopback must be present.
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    #[tokio::test]
    async fn os_backend_lists_loopback() {
        let all = list().await.expect("listing addresses");
        assert!(
            all.iter().any(|a| a.address == Ipv6Addr::LOCALHOST),
            "::1 not found in {all:?}"
        );
    }
}
