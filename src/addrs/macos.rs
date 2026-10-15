//! macOS backend: getifaddrs(3) for the address list, plus the
//! SIOCGIFAFLAG_IN6 / SIOCGIFALIFETIME_IN6 ioctls for per-address state
//! (the same source `ifconfig -L` uses).

use std::{
    ffi::CStr,
    io, mem,
    net::Ipv6Addr,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    ptr,
    time::{SystemTime, UNIX_EPOCH},
};

use super::{Flags, RawAddress};

// From <netinet6/in6_var.h>; these are stable kernel ABI.
const SIOCGIFAFLAG_IN6: libc::c_ulong = 0xc120_6949;
const SIOCGIFALIFETIME_IN6: libc::c_ulong = 0xc120_6951;

const IN6_IFF_TENTATIVE: libc::c_int = 0x02;
const IN6_IFF_DUPLICATED: libc::c_int = 0x04;
const IN6_IFF_DETACHED: libc::c_int = 0x08;
const IN6_IFF_DEPRECATED: libc::c_int = 0x10;
const IN6_IFF_AUTOCONF: libc::c_int = 0x40;
const IN6_IFF_TEMPORARY: libc::c_int = 0x80;
const IN6_IFF_DYNAMIC: libc::c_int = 0x100;
const IN6_IFF_SECURED: libc::c_int = 0x400;

#[repr(C)]
#[derive(Clone, Copy)]
struct In6AddrLifetime {
    expire: libc::time_t,
    preferred: libc::time_t,
    vltime: u32,
    pltime: u32,
}

#[repr(C)]
union In6IfreqData {
    addr: libc::sockaddr_in6,
    flags6: libc::c_int,
    lifetime: In6AddrLifetime,
    // The real union also holds per-interface statistics; this pads it to
    // the kernel's size, which is encoded in the ioctl numbers above.
    _size: [u64; 34],
}

#[repr(C)]
struct In6Ifreq {
    name: [libc::c_char; libc::IFNAMSIZ],
    data: In6IfreqData,
}

const _: () = assert!(mem::size_of::<In6Ifreq>() == 288);

pub async fn list() -> io::Result<Vec<RawAddress>> {
    list_sync()
}

fn list_sync() -> io::Result<Vec<RawAddress>> {
    // SAFETY: plain socket(2) call; the fd is owned immediately.
    let fd = unsafe { libc::socket(libc::AF_INET6, libc::SOCK_DGRAM, 0) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fd is a freshly created, valid descriptor.
    let sock = unsafe { OwnedFd::from_raw_fd(fd) };

    let ifaddrs = IfAddrs::new()?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as libc::time_t);

    let mut out = Vec::new();
    let mut cur = ifaddrs.0;
    while !cur.is_null() {
        // SAFETY: getifaddrs returns a valid linked list until freeifaddrs.
        let ifa = unsafe { &*cur };
        cur = ifa.ifa_next;
        if ifa.ifa_addr.is_null()
            || i32::from(unsafe { (*ifa.ifa_addr).sa_family }) != libc::AF_INET6
        {
            continue;
        }
        // SAFETY: sa_family is AF_INET6, so this is a sockaddr_in6.
        let sin6 = unsafe { ptr::read_unaligned(ifa.ifa_addr.cast::<libc::sockaddr_in6>()) };
        // SAFETY: ifa_name is a NUL-terminated string owned by the list.
        let name = unsafe { CStr::from_ptr(ifa.ifa_name) };

        // The address may disappear between getifaddrs and the ioctl; skip it then.
        let Some(flags6) = query(&sock, name, &sin6, SIOCGIFAFLAG_IN6, |d| unsafe {
            d.flags6
        }) else {
            continue;
        };
        let lifetime = query(&sock, name, &sin6, SIOCGIFALIFETIME_IN6, |d| unsafe {
            d.lifetime
        });

        let configured = flags6 & (IN6_IFF_AUTOCONF | IN6_IFF_DYNAMIC | IN6_IFF_TEMPORARY);
        out.push(RawAddress {
            interface: name.to_string_lossy().into_owned(),
            address: display_addr(sin6.sin6_addr.s6_addr),
            prefix_len: prefix_len(ifa.ifa_netmask),
            flags: Flags {
                permanent: configured == 0,
                temporary: flags6 & IN6_IFF_TEMPORARY != 0,
                deprecated: flags6 & IN6_IFF_DEPRECATED != 0,
                unusable: flags6 & (IN6_IFF_TENTATIVE | IN6_IFF_DUPLICATED | IN6_IFF_DETACHED) != 0,
                stable_privacy: flags6 & IN6_IFF_SECURED != 0,
            },
            preferred_lifetime: lifetime.and_then(|lt| remaining(lt.preferred, now)),
            valid_lifetime: lifetime.and_then(|lt| remaining(lt.expire, now)),
        });
    }
    Ok(out)
}

fn query<T>(
    sock: &OwnedFd,
    name: &CStr,
    addr: &libc::sockaddr_in6,
    request: libc::c_ulong,
    read: impl FnOnce(&In6IfreqData) -> T,
) -> Option<T> {
    let mut req = In6Ifreq {
        name: [0; libc::IFNAMSIZ],
        data: In6IfreqData { _size: [0; 34] },
    };
    let bytes = name.to_bytes();
    if bytes.len() >= libc::IFNAMSIZ {
        return None;
    }
    for (dst, &src) in req.name.iter_mut().zip(bytes) {
        *dst = src as libc::c_char;
    }
    req.data.addr = *addr;
    // SAFETY: req is a correctly sized in6_ifreq for this request.
    let rc = unsafe { libc::ioctl(sock.as_raw_fd(), request, &mut req) };
    (rc == 0).then(|| read(&req.data))
}

/// Absolute expiry time (0 = never) to remaining seconds (`None` = infinite).
fn remaining(at: libc::time_t, now: libc::time_t) -> Option<u32> {
    (at != 0).then(|| (at - now).clamp(0, i64::from(u32::MAX - 1)) as u32)
}

/// The kernel embeds the scope id in bytes 2..4 of link-local addresses.
fn display_addr(mut bytes: [u8; 16]) -> Ipv6Addr {
    if bytes[0] == 0xfe && bytes[1] & 0xc0 == 0x80 {
        bytes[2] = 0;
        bytes[3] = 0;
    }
    Ipv6Addr::from(bytes)
}

fn prefix_len(netmask: *const libc::sockaddr) -> u8 {
    if netmask.is_null() {
        return 128;
    }
    // BSD may shorten sa_len to drop trailing zero bytes of a mask.
    // SAFETY: netmask points to at least sa_len bytes.
    let len = usize::from(unsafe { (*netmask).sa_len });
    let start = mem::offset_of!(libc::sockaddr_in6, sin6_addr);
    let base = netmask.cast::<u8>();
    (start..len.min(start + 16))
        .map(|i| unsafe { *base.add(i) }.count_ones())
        .sum::<u32>() as u8
}

struct IfAddrs(*mut libc::ifaddrs);

impl IfAddrs {
    fn new() -> io::Result<Self> {
        let mut head = ptr::null_mut();
        // SAFETY: getifaddrs fills head on success.
        if unsafe { libc::getifaddrs(&mut head) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(head))
    }
}

impl Drop for IfAddrs {
    fn drop(&mut self) {
        // SAFETY: allocated by getifaddrs and freed exactly once.
        unsafe { libc::freeifaddrs(self.0) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaining_lifetimes() {
        assert_eq!(remaining(0, 1000), None);
        assert_eq!(remaining(1600, 1000), Some(600));
        assert_eq!(remaining(900, 1000), Some(0));
    }

    #[test]
    fn strips_embedded_scope() {
        let a: Ipv6Addr = "fe80:7::1".parse().unwrap();
        assert_eq!(
            display_addr(a.octets()),
            "fe80::1".parse::<Ipv6Addr>().unwrap()
        );
        let g: Ipv6Addr = "2001:db8::1".parse().unwrap();
        assert_eq!(display_addr(g.octets()), g);
    }
}
