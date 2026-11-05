//! Where the gateway may listen (PLAN-SERVER-CLIENT §4.1 layer 1): a LAN or
//! tailnet address of this machine. Every interface (`0.0.0.0`) only by an
//! explicit choice; never QuantMCP's port, which stays on loopback.
//!
//! And the tailnet itself (§4.2, card Server 6/7): whether Tailscale is
//! installed and connected on this PC, its name in the tailnet (MagicDNS) and
//! the other devices — from `tailscale status --json`.

use serde::Serialize;
use serde_json::Value;
use std::io::Read;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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

/// Whether this machine has `ip` right now (a tailnet address appears only
/// once Tailscale is connected).
pub fn has_address(ip: &IpAddr) -> bool {
    ip.is_unspecified() || ip.is_loopback() || interfaces().iter().any(|i| i.address == ip.to_string())
}

/// An address the whole internet may reach (not private, tailnet, loopback
/// or link-local).
pub fn is_public(ip: &IpAddr) -> bool {
    !ip.is_unspecified() && kind_of(ip) == "other" && !matches!(ip, IpAddr::V4(v4) if v4.is_link_local())
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TailnetPeer {
    pub name: String,
    /// The MagicDNS name (`homeserver.tail1234.ts.net`), when MagicDNS is on.
    pub dns_name: Option<String>,
    pub ips: Vec<String>,
    pub online: bool,
    pub os: Option<String>,
}

/// This PC in the tailnet.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Tailnet {
    pub installed: bool,
    /// Tailscale's backend state: `Running`, `NeedsLogin`, `Stopped`, …
    pub state: Option<String>,
    pub running: bool,
    pub name: Option<String>,
    pub dns_name: Option<String>,
    pub ips: Vec<String>,
    pub magic_dns: bool,
    pub peers: Vec<TailnetPeer>,
    pub error: Option<String>,
}

fn peer(p: &Value) -> TailnetPeer {
    let text = |k: &str| p.get(k).and_then(Value::as_str).map(String::from).filter(|s| !s.is_empty());
    TailnetPeer {
        name: text("HostName").unwrap_or_default(),
        dns_name: text("DNSName").map(|d| d.trim_end_matches('.').to_string()),
        ips: p.get("TailscaleIPs").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect()).unwrap_or_default(),
        online: p.get("Online").and_then(Value::as_bool).unwrap_or(false),
        os: text("OS"),
    }
}

/// Reads `tailscale status --json`.
pub fn parse_tailscale_status(json: &str) -> Result<Tailnet, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("tailscale status: {e}"))?;
    let state = v.get("BackendState").and_then(Value::as_str).map(String::from);
    let me = v.get("Self").map(peer).unwrap_or_default();
    let mut peers: Vec<TailnetPeer> = v.get("Peer").and_then(Value::as_object).map(|m| m.values().map(peer).collect()).unwrap_or_default();
    peers.sort_by(|a, b| b.online.cmp(&a.online).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    let magic_dns = v
        .pointer("/CurrentTailnet/MagicDNSEnabled")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| v.get("MagicDNSSuffix").and_then(Value::as_str).is_some_and(|s| !s.is_empty()));
    Ok(Tailnet {
        installed: true,
        running: state.as_deref() == Some("Running"),
        state,
        name: Some(me.name).filter(|n| !n.is_empty()),
        dns_name: me.dns_name.filter(|_| magic_dns),
        ips: me.ips,
        magic_dns,
        peers,
        error: None,
    })
}

fn tailscale_exe() -> Option<PathBuf> {
    let on_path = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .flat_map(|d| [d.join("tailscale.exe"), d.join("tailscale")]);
    let known = [
        r"C:\Program Files\Tailscale\tailscale.exe",
        r"C:\Program Files (x86)\Tailscale\tailscale.exe",
        "/usr/bin/tailscale",
        "/usr/local/bin/tailscale",
        "/opt/homebrew/bin/tailscale",
        "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
    ]
    .into_iter()
    .map(PathBuf::from);
    on_path.chain(known).find(|p| p.is_file())
}

/// Tailscale on this PC, asked through its command line (at most 3 s).
pub fn tailscale_status() -> Tailnet {
    let Some(exe) = tailscale_exe() else { return Tailnet::default() };
    let failed = |e: String| Tailnet { installed: true, error: Some(e), ..Default::default() };
    let mut cmd = Command::new(&exe);
    cmd.args(["status", "--json"]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window flashing up.
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return failed(format!("{}: {e}", display(&exe))),
    };
    let mut out = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(o) = out.as_mut() {
            let _ = o.read_to_string(&mut s);
        }
        s
    });
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(30)),
            _ => {
                let _ = child.kill();
                return failed("Tailscale did not answer.".into());
            }
        }
    }
    let text = reader.join().unwrap_or_default();
    parse_tailscale_status(&text).unwrap_or_else(failed)
}

fn display(p: &Path) -> String {
    p.display().to_string()
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
    fn public_addresses_are_recognised() {
        let p = |s: &str| is_public(&s.parse().unwrap());
        assert!(p("8.8.8.8") && p("2a01:4f8::1"));
        assert!(!p("100.101.102.103") && !p("192.168.1.20") && !p("127.0.0.1") && !p("0.0.0.0") && !p("169.254.1.1"));
        assert!(has_address(&"127.0.0.1".parse().unwrap()));
        assert!(!has_address(&"100.127.255.254".parse().unwrap()), "no tailnet here");
    }

    #[test]
    fn tailscale_status_is_read() {
        let json = r#"{
          "Version": "1.90.0", "BackendState": "Running", "MagicDNSSuffix": "tail1234.ts.net",
          "CurrentTailnet": { "Name": "you@example.com", "MagicDNSSuffix": "tail1234.ts.net", "MagicDNSEnabled": true },
          "Self": { "HostName": "homeserver", "DNSName": "homeserver.tail1234.ts.net.", "TailscaleIPs": ["100.101.102.103", "fd7a:115c:a1e0::1"], "Online": true, "OS": "windows" },
          "Peer": {
            "nodekey:a": { "HostName": "phone", "DNSName": "phone.tail1234.ts.net.", "TailscaleIPs": ["100.64.0.9"], "Online": false, "OS": "iOS" },
            "nodekey:b": { "HostName": "Laptop", "DNSName": "laptop.tail1234.ts.net.", "TailscaleIPs": ["100.64.0.7"], "Online": true, "OS": "windows" }
          }
        }"#;
        let t = parse_tailscale_status(json).unwrap();
        assert!(t.installed && t.running && t.magic_dns);
        assert_eq!(t.name.as_deref(), Some("homeserver"));
        assert_eq!(t.dns_name.as_deref(), Some("homeserver.tail1234.ts.net"));
        assert_eq!(t.ips[0], "100.101.102.103");
        assert_eq!(t.peers.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), vec!["Laptop", "phone"], "online first");
        let logged_out = parse_tailscale_status(r#"{"BackendState":"NeedsLogin","Self":{"HostName":"pc","TailscaleIPs":[]},"Peer":null}"#).unwrap();
        assert!(!logged_out.running && logged_out.peers.is_empty() && logged_out.dns_name.is_none());
        assert!(parse_tailscale_status("not json").is_err());
    }

    #[test]
    fn listen_rules() {
        assert!(check_listen(&"100.101.102.103:47100".parse().unwrap(), false).is_ok());
        assert!(check_listen(&"0.0.0.0:47100".parse().unwrap(), false).is_err());
        assert!(check_listen(&"0.0.0.0:47100".parse().unwrap(), true).is_ok());
        assert!(check_listen(&"192.168.1.20:3100".parse().unwrap(), true).is_err(), "QuantMCP's port");
    }
}
