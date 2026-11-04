//! Where the gateway may listen (PLAN-SERVER-CLIENT §4.1 layer 1): a LAN or
//! tailnet address of this machine. Every interface (`0.0.0.0`) only by an
//! explicit choice; never QuantMCP's port, which stays on loopback.

use serde::Serialize;
use std::net::{IpAddr, SocketAddr};

/// QuantMCP's port (loopback only, on every machine).
pub const QUANTMCP_PORT: u16 = 3100;

#[derive(Debug, Clone, Serialize)]
pub struct Interface {
    pub name: String,
    pub address: String,
    /// `tailnet`, `lan`, `loopback` or `other`.
    pub kind: &'static str,
}

/// Tailscale hands out 100.64.0.0/10 (CGNAT) and fd7a:115c:a1e0::/48.
pub fn kind_of(ip: &IpAddr) -> &'static str {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            if v4.is_loopback() {
                "loopback"
            } else if o[0] == 100 && (o[1] & 0xc0) == 64 {
                "tailnet"
            } else if v4.is_private() {
                "lan"
            } else {
                "other"
            }
        }
        IpAddr::V6(v6) => {
            let s = v6.segments();
            if v6.is_loopback() {
                "loopback"
            } else if s[0] == 0xfd7a && s[1] == 0x115c && s[2] == 0xa1e0 {
                "tailnet"
            } else if (s[0] & 0xfe00) == 0xfc00 {
                "lan"
            } else {
                "other"
            }
        }
    }
}

/// This machine's addresses: tailnet first, then LAN; link-local left out.
pub fn interfaces() -> Vec<Interface> {
    let mut out: Vec<Interface> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| match i.ip() {
            IpAddr::V4(v4) => !v4.is_link_local(),
            IpAddr::V6(v6) => (v6.segments()[0] & 0xffc0) != 0xfe80,
        })
        .map(|i| {
            let ip = i.ip();
            Interface { name: i.name.clone(), address: ip.to_string(), kind: kind_of(&ip) }
        })
        .collect();
    let rank = |k: &str| match k {
        "tailnet" => 0,
        "lan" => 1,
        "other" => 2,
        _ => 3,
    };
    out.sort_by(|a, b| rank(a.kind).cmp(&rank(b.kind)).then(a.address.cmp(&b.address)));
    out
}

/// Refuses a listen address the plan rules out.
pub fn check_listen(addr: &SocketAddr, every_interface_ok: bool) -> Result<(), String> {
    if addr.port() == QUANTMCP_PORT {
        return Err(format!("Port {QUANTMCP_PORT} is QuantMCP's and stays on this PC only. Choose another port."));
    }
    if addr.port() == 0 {
        return Err("Choose a port.".into());
    }
    if addr.ip().is_unspecified() && !every_interface_ok {
        return Err("Listening on every network (0.0.0.0) needs your explicit choice: anyone who reaches this PC can knock. Prefer the tailnet or LAN address.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_classified() {
        let k = |s: &str| kind_of(&s.parse().unwrap());
        assert_eq!(k("100.101.102.103"), "tailnet");
        assert_eq!(k("100.128.0.1"), "other", "outside 100.64/10");
        assert_eq!(k("192.168.1.20"), "lan");
        assert_eq!(k("10.0.0.5"), "lan");
        assert_eq!(k("127.0.0.1"), "loopback");
        assert_eq!(k("8.8.8.8"), "other");
        assert_eq!(k("fd7a:115c:a1e0::1"), "tailnet");
    }

    #[test]
    fn listen_rules() {
        assert!(check_listen(&"100.101.102.103:47100".parse().unwrap(), false).is_ok());
        assert!(check_listen(&"0.0.0.0:47100".parse().unwrap(), false).is_err());
        assert!(check_listen(&"0.0.0.0:47100".parse().unwrap(), true).is_ok());
        assert!(check_listen(&"192.168.1.20:3100".parse().unwrap(), true).is_err(), "QuantMCP's port");
    }
}
