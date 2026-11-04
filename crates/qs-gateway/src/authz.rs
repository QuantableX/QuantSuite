//! Roles against the remote manifest (PLAN-SERVER-CLIENT §4.1 layers 5–6).
//!
//! Every remote call names a command; the server looks it up in the remote
//! manifest (`remote-manifest.json`, generated from every `module.json` by
//! `scripts/remote-inventory.mjs --write`). Only `server` commands run for a
//! remote device; `client` commands act on the PC you sit at and `deny`
//! commands never leave the server. Anything not listed is denied.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::OnceLock;

/// What a paired device may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceRole {
    /// Everything; `external` calls need the server PIN.
    Full,
    /// Read and change data, never trade or touch keys (`external`).
    NoTrading,
    /// Read and compute only.
    ReadOnly,
}

impl DeviceRole {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "full" => Some(Self::Full),
            "no_trading" => Some(Self::NoTrading),
            "read_only" => Some(Self::ReadOnly),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::NoTrading => "no_trading",
            Self::ReadOnly => "read_only",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    Server,
    Client,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Read,
    Compute,
    Write,
    External,
}

fn entry(v: &str) -> Option<(Placement, Effect)> {
    let (run, effect) = v.split_once(':')?;
    let run = match run {
        "server" => Placement::Server,
        "client" => Placement::Client,
        "deny" => Placement::Deny,
        _ => return None,
    };
    let effect = match effect {
        "read" => Effect::Read,
        "compute" => Effect::Compute,
        "write" => Effect::Write,
        "external" => Effect::External,
        _ => return None,
    };
    Some((run, effect))
}

#[derive(Deserialize)]
struct Raw {
    #[serde(default)]
    commands: HashMap<String, String>,
    #[serde(default)]
    plugins: HashMap<String, String>,
    #[serde(default)]
    events: Vec<String>,
}

pub struct Manifest {
    commands: HashMap<String, (Placement, Effect)>,
    plugins: HashMap<String, (Placement, Effect)>,
    events: Vec<String>,
    hash: String,
}

impl Manifest {
    pub fn parse(json: &str) -> Result<Self, String> {
        let raw: Raw = serde_json::from_str(json).map_err(|e| format!("remote manifest: {e}"))?;
        let mut commands = HashMap::new();
        for (k, v) in raw.commands {
            let e = entry(&v).ok_or_else(|| format!("remote manifest: {k} = {v}"))?;
            commands.insert(k, e);
        }
        let mut plugins = HashMap::new();
        for (k, v) in raw.plugins {
            let e = entry(&v).ok_or_else(|| format!("remote manifest: plugin {k} = {v}"))?;
            plugins.insert(k, e);
        }
        let normalized = json.replace("\r\n", "\n");
        let hash = hex::encode(&Sha256::digest(normalized.as_bytes())[..8]);
        Ok(Self { commands, plugins, events: raw.events, hash })
    }

    /// The manifest this build was made with.
    pub fn bundled() -> &'static Manifest {
        static BUNDLED: OnceLock<Manifest> = OnceLock::new();
        BUNDLED.get_or_init(|| {
            Manifest::parse(include_str!("../remote-manifest.json")).unwrap_or_else(|e| {
                log::error!("{e}");
                Manifest { commands: HashMap::new(), plugins: HashMap::new(), events: Vec::new(), hash: "invalid".into() }
            })
        })
    }

    /// Where a call runs and what it does; `None` when the manifest does not
    /// list it.
    pub fn lookup(&self, cmd: &str) -> Option<(Placement, Effect)> {
        if let Some(e) = self.commands.get(cmd) {
            return Some(*e);
        }
        let plugin = cmd.strip_prefix("plugin:")?.split('|').next()?;
        self.plugins.get(plugin).or_else(|| self.plugins.get(&format!("core:{plugin}"))).copied()
    }

    /// Client and server must authorize against the same manifest (§6.3.4
    /// risk 9): its hash goes into the handshake.
    pub fn hash(&self) -> &str {
        &self.hash
    }

    /// The Tauri events server-placed modules emit (card 4 forwards them).
    pub fn events(&self) -> &[String] {
        &self.events
    }

    pub fn command_count(&self) -> usize {
        self.commands.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    /// Allowed once the device has entered the server PIN.
    StepUp,
    Deny(String),
}

pub fn decide(manifest: &Manifest, role: DeviceRole, cmd: &str) -> Decision {
    let Some((run, effect)) = manifest.lookup(cmd) else {
        return Decision::Deny(format!("{cmd} is not offered to other devices"));
    };
    match run {
        Placement::Deny => return Decision::Deny(format!("{cmd} never runs for another device")),
        Placement::Client => return Decision::Deny(format!("{cmd} runs on the PC you sit at, not on the server")),
        Placement::Server => {}
    }
    match (role, effect) {
        (_, Effect::Read | Effect::Compute) => Decision::Allow,
        (DeviceRole::ReadOnly, _) => Decision::Deny("this device may only read".into()),
        (DeviceRole::NoTrading, Effect::External) => Decision::Deny("this device may not trade or change keys".into()),
        (_, Effect::Write) => Decision::Allow,
        (DeviceRole::Full, Effect::External) => Decision::StepUp,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub const TEST_MANIFEST: &str = r#"{
      "commands": {
        "plugin:notes|list_notes": "server:read",
        "plugin:systems|run_backtest": "server:compute",
        "plugin:notes|create_note": "server:write",
        "plugin:algo|start_bot": "server:external",
        "plugin:algo|wallet_export_key": "deny:read",
        "plugin:pilot|send_message": "client:external"
      },
      "plugins": { "core:window": "client:write", "shell": "client:external" },
      "events": ["bot:log"]
    }"#;

    #[test]
    fn roles_against_the_manifest() {
        let m = Manifest::parse(TEST_MANIFEST).unwrap();
        use Decision::*;
        let full = DeviceRole::Full;
        assert_eq!(decide(&m, full, "plugin:notes|list_notes"), Allow);
        assert_eq!(decide(&m, full, "plugin:notes|create_note"), Allow);
        assert_eq!(decide(&m, full, "plugin:algo|start_bot"), StepUp);
        assert!(matches!(decide(&m, full, "plugin:algo|wallet_export_key"), Deny(_)), "secrets never leave");
        assert!(matches!(decide(&m, full, "plugin:pilot|send_message"), Deny(_)), "client commands never run remotely");
        assert!(matches!(decide(&m, full, "plugin:window|set_title"), Deny(_)), "core plugins by their plugin entry");
        assert!(matches!(decide(&m, full, "plugin:shell|execute"), Deny(_)));
        assert!(matches!(decide(&m, full, "plugin:notes|drop_everything"), Deny(_)), "unknown is denied");
        assert!(matches!(decide(&m, full, "something_else"), Deny(_)));

        let nt = DeviceRole::NoTrading;
        assert_eq!(decide(&m, nt, "plugin:notes|create_note"), Allow);
        assert!(matches!(decide(&m, nt, "plugin:algo|start_bot"), Deny(_)));

        let ro = DeviceRole::ReadOnly;
        assert_eq!(decide(&m, ro, "plugin:notes|list_notes"), Allow);
        assert_eq!(decide(&m, ro, "plugin:systems|run_backtest"), Allow);
        assert!(matches!(decide(&m, ro, "plugin:notes|create_note"), Deny(_)));
        assert!(matches!(decide(&m, ro, "plugin:algo|start_bot"), Deny(_)));
    }

    #[test]
    fn the_bundled_manifest_is_complete_enough_to_trust() {
        let m = Manifest::bundled();
        assert!(m.command_count() > 600, "{}", m.command_count());
        assert_ne!(m.hash(), "invalid");
        assert_eq!(m.lookup("plugin:algo|wallet_export_key"), Some((Placement::Deny, Effect::Read)));
        assert_eq!(m.lookup("plugin:algo|start_bot").map(|e| e.0), Some(Placement::Server));
        assert_eq!(m.lookup("plugin:qs|server_status"), Some((Placement::Server, Effect::Read)));
        assert_eq!(m.lookup("plugin:fs|read_file").map(|e| e.0), Some(Placement::Client));
        assert!(m.events().iter().any(|e| e == "bot:log"));
        // A manifest is the same whatever line endings the checkout has.
        assert_eq!(Manifest::parse(&TEST_MANIFEST.replace('\n', "\r\n")).unwrap().hash(), Manifest::parse(TEST_MANIFEST).unwrap().hash());
    }

    #[test]
    fn roles_round_trip() {
        for r in [DeviceRole::Full, DeviceRole::NoTrading, DeviceRole::ReadOnly] {
            assert_eq!(DeviceRole::parse(r.as_str()), Some(r));
        }
        assert_eq!(DeviceRole::parse("admin"), None);
    }
}
