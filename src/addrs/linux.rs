//! Linux backend: RTM_GETADDR dump over rtnetlink.

use std::{collections::HashMap, io, net::IpAddr};

use futures_util::TryStreamExt;
use rtnetlink::packet_route::{
    AddressFamily,
    address::{AddressAttribute, AddressMessage},
    link::LinkAttribute,
};

use super::{Flags, RawAddress};

// Kernel IFA_F_* flags (linux/if_addr.h).
const IFA_F_TEMPORARY: u32 = 0x01;
const IFA_F_DADFAILED: u32 = 0x08;
const IFA_F_DEPRECATED: u32 = 0x20;
const IFA_F_TENTATIVE: u32 = 0x40;
const IFA_F_PERMANENT: u32 = 0x80;
const IFA_F_STABLE_PRIVACY: u32 = 0x800;

/// Lifetime value the kernel uses for "forever".
const INFINITY_LIFE_TIME: u32 = u32::MAX;

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
    let mut bits = u32::from(msg.header.flags.bits());
    let (mut preferred, mut valid) = (None, None);
    for attr in msg.attributes {
        match attr {
            AddressAttribute::Address(IpAddr::V6(a)) => address = Some(a),
            AddressAttribute::Local(IpAddr::V6(a)) => local = Some(a),
            AddressAttribute::Flags(f) => bits = f.bits(),
            AddressAttribute::CacheInfo(ci) => {
                preferred = Some(ci.ifa_preferred);
                valid = Some(ci.ifa_valid);
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
        flags: Flags {
            permanent: bits & IFA_F_PERMANENT != 0,
            temporary: bits & IFA_F_TEMPORARY != 0,
            deprecated: bits & IFA_F_DEPRECATED != 0,
            unusable: bits & (IFA_F_TENTATIVE | IFA_F_DADFAILED) != 0,
            stable_privacy: bits & IFA_F_STABLE_PRIVACY != 0,
        },
        preferred_lifetime: preferred.filter(|&v| v != INFINITY_LIFE_TIME),
        valid_lifetime: valid.filter(|&v| v != INFINITY_LIFE_TIME),
    })
}
