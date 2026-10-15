//! Connection-level client allowlist.
//!
//! Peers outside the allowlist are dropped right after `accept()`, before any
//! HTTP parsing, so the check cannot be bypassed by how routes are written.

use std::{io, net::IpAddr, net::SocketAddr, sync::Arc};

use axum::serve::Listener;
use ipnet::IpNet;
use tokio::net::{TcpListener, TcpStream};

#[derive(Debug, Clone)]
pub struct Allowlist(Arc<[IpNet]>);

impl Allowlist {
    pub fn new(nets: &[IpNet]) -> Self {
        Self(nets.into())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        // A dual-stack `[::]` socket reports IPv4 clients as ::ffff:a.b.c.d.
        let ip = ip.to_canonical();
        self.0.iter().any(|net| net.contains(&ip))
    }
}

pub struct AllowlistListener {
    inner: TcpListener,
    allow: Allowlist,
}

impl AllowlistListener {
    pub fn new(inner: TcpListener, allow: Allowlist) -> Self {
        Self { inner, allow }
    }
}

impl Listener for AllowlistListener {
    type Io = TcpStream;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (stream, peer) = Listener::accept(&mut self.inner).await;
            if self.allow.contains(peer.ip()) {
                return (stream, peer);
            }
            tracing::debug!(%peer, "dropped connection from address outside allowlist");
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.inner.local_addr()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(nets: &[&str]) -> Allowlist {
        Allowlist::new(&nets.iter().map(|n| n.parse().unwrap()).collect::<Vec<_>>())
    }

    #[test]
    fn matches_v6_and_v4() {
        let a = list(&["2001:db8::/32", "192.168.1.0/24"]);
        assert!(a.contains("2001:db8::5".parse().unwrap()));
        assert!(!a.contains("2001:db9::5".parse().unwrap()));
        assert!(a.contains("192.168.1.7".parse().unwrap()));
        assert!(!a.contains("192.168.2.7".parse().unwrap()));
    }

    #[test]
    fn ipv4_mapped_is_canonicalized() {
        let a = list(&["127.0.0.0/8"]);
        assert!(a.contains("::ffff:127.0.0.1".parse().unwrap()));
        assert!(!list(&["::1/128"]).contains("::ffff:127.0.0.1".parse().unwrap()));
    }

    #[test]
    fn empty_denies_all() {
        assert!(!list(&[]).contains("::1".parse().unwrap()));
    }

    #[tokio::test]
    async fn drops_disallowed_peers_at_accept() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let inner = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = inner.local_addr().unwrap();
        // 127.0.0.1 is not in this list, so the connection must be dropped.
        let mut listener = AllowlistListener::new(inner, list(&["10.0.0.0/8"]));
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await;
            s.write_all(b"leak").await.unwrap();
        });

        let mut client = TcpStream::connect(addr).await.unwrap();
        let mut buf = Vec::new();
        let n = client.read_to_end(&mut buf).await.unwrap_or(0);
        assert_eq!(n, 0, "disallowed peer must receive nothing");
    }
}
