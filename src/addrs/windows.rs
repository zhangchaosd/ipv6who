//! Windows backend: GetAdaptersAddresses(AF_INET6).

use std::{io, net::Ipv6Addr, ptr};

use windows_sys::Win32::{
    Foundation::{ERROR_BUFFER_OVERFLOW, NO_ERROR},
    NetworkManagement::IpHelper::{
        GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST,
        GetAdaptersAddresses, IP_ADAPTER_ADDRESS_DNS_ELIGIBLE, IP_ADAPTER_ADDRESSES_LH,
        IP_ADAPTER_UNICAST_ADDRESS_LH,
    },
    Networking::WinSock::{
        AF_INET6, IpDadStateDeprecated, IpDadStatePreferred, IpPrefixOriginManual,
        IpPrefixOriginRouterAdvertisement, IpPrefixOriginWellKnown, IpSuffixOriginRandom,
        SOCKADDR_IN6,
    },
};

use super::{Flags, RawAddress};

/// Lifetime value Windows uses for "forever".
const INFINITE: u32 = u32::MAX;

pub async fn list() -> io::Result<Vec<RawAddress>> {
    list_sync()
}

fn list_sync() -> io::Result<Vec<RawAddress>> {
    let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
    let mut size: u32 = 16 * 1024;
    // u64 elements keep the buffer aligned for IP_ADAPTER_ADDRESSES_LH.
    let mut buf: Vec<u64>;
    loop {
        buf = vec![0; (size as usize).div_ceil(8)];
        // SAFETY: buf holds `size` writable bytes; size is updated on overflow.
        let rc = unsafe {
            GetAdaptersAddresses(
                u32::from(AF_INET6),
                flags,
                ptr::null(),
                buf.as_mut_ptr().cast(),
                &mut size,
            )
        };
        match rc {
            NO_ERROR => break,
            ERROR_BUFFER_OVERFLOW => continue,
            err => return Err(io::Error::from_raw_os_error(err as i32)),
        }
    }

    let mut out = Vec::new();
    let mut adapter = buf.as_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
    while !adapter.is_null() {
        // SAFETY: the list lives in `buf` and is well formed on NO_ERROR.
        let a = unsafe { &*adapter };
        adapter = a.Next;
        let interface = unsafe { wide_to_string(a.FriendlyName) };
        let mut unicast = a.FirstUnicastAddress.cast_const();
        while !unicast.is_null() {
            let u = unsafe { &*unicast };
            unicast = u.Next;
            if let Some(raw) = to_raw(&interface, u) {
                out.push(raw);
            }
        }
    }
    Ok(out)
}

fn to_raw(interface: &str, u: &IP_ADAPTER_UNICAST_ADDRESS_LH) -> Option<RawAddress> {
    let sa = u.Address.lpSockaddr;
    if sa.is_null() || unsafe { (*sa).sa_family } != AF_INET6 {
        return None;
    }
    // SAFETY: the family is AF_INET6, so this is a SOCKADDR_IN6.
    let sin6 = unsafe { ptr::read_unaligned(sa.cast::<SOCKADDR_IN6>()) };
    let address = Ipv6Addr::from(unsafe { sin6.sin6_addr.u.Byte });

    let dns_eligible =
        unsafe { u.Anonymous.Anonymous.Flags } & IP_ADAPTER_ADDRESS_DNS_ELIGIBLE != 0;
    let random_suffix = u.SuffixOrigin == IpSuffixOriginRandom;
    let from_ra = u.PrefixOrigin == IpPrefixOriginRouterAdvertisement;
    // Windows has no explicit "temporary" flag. Privacy addresses are the
    // random-suffix SLAAC addresses that are not registered in DNS; the
    // stable random-suffix ("public") address is DNS eligible.
    let temporary = from_ra && random_suffix && !dns_eligible;

    Some(RawAddress {
        interface: interface.to_owned(),
        address,
        prefix_len: u.OnLinkPrefixLength,
        flags: Flags {
            permanent: u.PrefixOrigin == IpPrefixOriginManual
                || u.PrefixOrigin == IpPrefixOriginWellKnown,
            temporary,
            deprecated: u.DadState == IpDadStateDeprecated,
            unusable: u.DadState != IpDadStatePreferred && u.DadState != IpDadStateDeprecated,
            stable_privacy: from_ra && random_suffix && !temporary,
        },
        preferred_lifetime: (u.PreferredLifetime != INFINITE).then_some(u.PreferredLifetime),
        valid_lifetime: (u.ValidLifetime != INFINITE).then_some(u.ValidLifetime),
    })
}

/// # Safety
/// `p` must be null or point to a NUL-terminated UTF-16 string.
unsafe fn wide_to_string(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut len = 0;
    while unsafe { *p.add(len) } != 0 {
        len += 1;
    }
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, len) })
}
