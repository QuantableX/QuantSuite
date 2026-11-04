//! The gateway's security, end to end over loopback sockets (card Server 3/7
//! acceptance): pairing, the four words, roles, step-up, revocation, versions,
//! wrong keys, brute force — and no secret in anything a device receives.

use qs_gateway::client::{self, ClientInfo, ConnectError};
use qs_gateway::protocol::Refusal;
use qs_gateway::registry::{DeviceState, Registry};
use qs_gateway::server::{CallFuture, Dispatch, Hub, HubConfig, Listener};
use qs_gateway::{DeviceRole, Identity, Manifest, PROTOCOL};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const MANIFEST: &str = r#"{
  "commands": {
    "plugin:notes|list_notes": "server:read",
    "plugin:notes|create_note": "server:write",
    "plugin:algo|start_bot": "server:external",
    "plugin:algo|wallet_export_key": "deny:read",
    "plugin:pilot|send_message": "client:external"
  },
  "plugins": {},
  "events": []
}"#;

/// Answers every call with what it was asked; records the calls.
#[derive(Default)]
struct Echo(Mutex<Vec<String>>);

impl Dispatch for Echo {
    fn call(&self, cmd: String, args: Value) -> CallFuture {
        self.0.lock().unwrap().push(cmd.clone());
        Box::pin(async move { Ok(json!({ "cmd": cmd, "args": args })) })
    }
}

struct Server {
    hub: Arc<Hub>,
    listener: Listener,
    identity_pkcs8: Vec<u8>,
    echo: Arc<Echo>,
}

impl Server {
    fn addr(&self) -> String {
        self.listener.local_addr().to_string()
    }
}

async fn server() -> Server {
    let identity = Identity::generate().unwrap();
    let identity_pkcs8 = identity.pkcs8().to_vec();
    let manifest = Arc::new(Manifest::parse(MANIFEST).unwrap());
    let config = HubConfig { server_name: "homeserver".into(), app_version: "1.0.45".into() };
    let hub = Hub::new(identity, Registry::in_memory().unwrap(), manifest, config).unwrap();
    let echo = Arc::new(Echo::default());
    hub.set_dispatch(echo.clone());
    let listener = hub.listen("127.0.0.1:0".parse().unwrap()).await.unwrap();
    Server { hub, listener, identity_pkcs8, echo }
}

fn me(name: &str) -> ClientInfo {
    ClientInfo { name: name.into(), app: "1.0.45".into(), manifest: Manifest::parse(MANIFEST).unwrap().hash().into() }
}

/// Pairs `device` through the whole flow, the server's user choosing `role`.
async fn pair(s: &Server, device: &Identity, role: DeviceRole) -> [u8; 32] {
    let code = s.hub.start_pairing().unwrap().code.unwrap();
    let attempt = client::pair(&s.addr(), device, &code, &me("Laptop")).await.unwrap();
    let status = s.hub.pairing_status();
    assert_eq!(status.pending.len(), 1);
    assert_eq!(status.pending[0].words, attempt.words.to_vec(), "both screens show the same four words");
    s.hub.confirm_pairing(&status.pending[0].id, Some(role)).unwrap();
    let server_key = attempt.server_key;
    let (_, got) = attempt.finish().await.unwrap();
    assert_eq!(got, role);
    server_key
}

#[tokio::test]
async fn pair_use_and_roles() {
    let s = server().await;
    let device = Identity::generate().unwrap();
    let server_key = pair(&s, &device, DeviceRole::Full).await;
    assert_eq!(server_key, s.hub.public_key());

    let c = client::connect(&s.addr(), &device, server_key, &me("Laptop")).await.unwrap();
    assert_eq!(c.welcome.role, DeviceRole::Full);
    assert_eq!(c.welcome.server, "homeserver");
    let r = c.call("plugin:notes|list_notes", json!({ "q": 1 })).await.unwrap();
    assert_eq!(r["cmd"], "plugin:notes|list_notes");
    assert_eq!(c.call("plugin:notes|create_note", json!({})).await.unwrap()["cmd"], "plugin:notes|create_note");

    // Never: secrets, the client's own commands, anything unknown.
    for cmd in ["plugin:algo|wallet_export_key", "plugin:pilot|send_message", "plugin:notes|drop_all", "quit"] {
        let e = c.call(cmd, json!({})).await.unwrap_err();
        assert_eq!(e.code, "denied", "{cmd}");
    }
    assert!(!s.echo.0.lock().unwrap().iter().any(|c| c.contains("wallet") || c.contains("pilot") || c.contains("drop")));

    // A new role applies to the next call.
    let id = c.welcome.device_id.clone();
    s.hub.set_device_role(&id, DeviceRole::ReadOnly).unwrap();
    assert!(c.call("plugin:notes|list_notes", json!({})).await.is_ok());
    assert_eq!(c.call("plugin:notes|create_note", json!({})).await.unwrap_err().code, "denied");
    s.hub.set_device_role(&id, DeviceRole::NoTrading).unwrap();
    assert!(c.call("plugin:notes|create_note", json!({})).await.is_ok());
    assert_eq!(c.call("plugin:algo|start_bot", json!({})).await.unwrap_err().code, "denied");

    // Every call is in the audit log with its outcome.
    let audit = s.hub.audit_log(100).unwrap();
    assert!(audit.iter().any(|a| a.command == "plugin:algo|wallet_export_key" && a.outcome == "denied" && a.device == "Laptop"));
    assert!(audit.iter().any(|a| a.command == "plugin:notes|list_notes" && a.outcome == "ok"));
    assert!(audit.iter().any(|a| a.command == "pair" && a.outcome == "paired"));
}

#[tokio::test]
async fn external_calls_need_the_server_pin() {
    let s = server().await;
    let device = Identity::generate().unwrap();
    let key = pair(&s, &device, DeviceRole::Full).await;
    let c = client::connect(&s.addr(), &device, key, &me("Laptop")).await.unwrap();

    assert_eq!(c.call("plugin:algo|start_bot", json!({})).await.unwrap_err().code, "step_up_required");
    assert!(c.step_up("482915").await.unwrap_err().contains("No server PIN"));
    s.hub.set_pin(Some("482915")).unwrap();
    assert!(c.step_up("111111").await.unwrap_err().contains("Wrong PIN"));
    assert_eq!(c.call("plugin:algo|start_bot", json!({})).await.unwrap_err().code, "step_up_required");
    assert_eq!(c.step_up("482915").await.unwrap(), 600);
    assert!(c.call("plugin:algo|start_bot", json!({})).await.is_ok());

    // A new PIN ends every grant.
    s.hub.set_pin(Some("777777")).unwrap();
    assert_eq!(c.call("plugin:algo|start_bot", json!({})).await.unwrap_err().code, "step_up_required");

    // No PIN in anything stored or shown.
    let audit = serde_json::to_string(&s.hub.audit_log(100).unwrap()).unwrap();
    assert!(!audit.contains("482915") && !audit.contains("111111") && !audit.contains("777777"));
}

#[tokio::test]
async fn a_wrong_server_key_is_refused() {
    let s = server().await;
    let device = Identity::generate().unwrap();
    pair(&s, &device, DeviceRole::Full).await;
    let impostor = Identity::generate().unwrap().public_key();
    match client::connect(&s.addr(), &device, impostor, &me("Laptop")).await {
        Err(ConnectError::Tls { key_changed: true, .. }) => {}
        other => panic!("expected a refused server key, got {:?}", other.map(|c| c.welcome.clone())),
    }
}

#[tokio::test]
async fn unpaired_devices_may_only_pair() {
    let s = server().await;
    let stranger = Identity::generate().unwrap();
    let r = client::connect(&s.addr(), &stranger, s.hub.public_key(), &me("Stranger")).await;
    assert_eq!(r.err(), Some(ConnectError::Unpaired));
    assert!(s.echo.0.lock().unwrap().is_empty());
    // Pairing without an open code fails.
    let e = client::pair(&s.addr(), &stranger, "12345678", &me("Stranger")).await.err().unwrap();
    assert!(e.contains("No pairing code"), "{e}");
}

#[tokio::test]
async fn a_revoked_device_is_dropped_at_once_and_refused() {
    let s = server().await;
    let device = Identity::generate().unwrap();
    let key = pair(&s, &device, DeviceRole::Full).await;
    let c = client::connect(&s.addr(), &device, key, &me("Laptop")).await.unwrap();
    assert!(c.call("plugin:notes|list_notes", json!({})).await.is_ok());
    assert_eq!(s.hub.connected(), vec![c.welcome.device_id.clone()]);

    s.hub.revoke_device(&c.welcome.device_id).unwrap();
    tokio::time::timeout(Duration::from_secs(3), c.closed()).await.expect("the connection drops at once");
    assert!(c.call("plugin:notes|list_notes", json!({})).await.is_err());

    match client::connect(&s.addr(), &device, key, &me("Laptop")).await {
        Err(ConnectError::Refused { reason: Refusal::Revoked, .. }) => {}
        other => panic!("expected revoked, got {:?}", other.map(|c| c.welcome.clone())),
    }
    assert_eq!(s.hub.devices().unwrap()[0].state, DeviceState::Revoked);
}

#[tokio::test]
async fn pairing_brute_force_locks() {
    let s = server().await;
    let code = s.hub.start_pairing().unwrap().code.unwrap();
    let wrong = if code == "00000000" { "11111111" } else { "00000000" };
    for _ in 0..4 {
        let e = client::pair(&s.addr(), &Identity::generate().unwrap(), wrong, &me("Guess")).await.err().unwrap();
        assert!(e.contains("Wrong pairing code"), "{e}");
    }
    let e = client::pair(&s.addr(), &Identity::generate().unwrap(), wrong, &me("Guess")).await.err().unwrap();
    assert!(e.contains("locked"), "{e}");
    let e = client::pair(&s.addr(), &Identity::generate().unwrap(), &code, &me("Guess")).await.err().unwrap();
    assert!(e.contains("locked"), "the right code no longer helps: {e}");
    assert!(s.hub.start_pairing().is_err(), "and no new code for 15 minutes");
    assert!(s.hub.pairing_status().locked_secs > 800);
    assert!(s.hub.devices().unwrap().is_empty());
}

#[tokio::test]
async fn the_server_user_can_turn_a_device_away() {
    let s = server().await;
    let device = Identity::generate().unwrap();
    let code = s.hub.start_pairing().unwrap().code.unwrap();
    let attempt = client::pair(&s.addr(), &device, &code, &me("Laptop")).await.unwrap();
    let id = s.hub.pairing_status().pending[0].id.clone();
    s.hub.confirm_pairing(&id, None).unwrap();
    assert!(attempt.finish().await.unwrap_err().contains("did not confirm"));
    assert!(s.hub.devices().unwrap().is_empty());

    // A device that gives up (its user saw other words) leaves nothing behind.
    let code = s.hub.start_pairing().unwrap().code.unwrap();
    let attempt = client::pair(&s.addr(), &device, &code, &me("Laptop")).await.unwrap();
    drop(attempt);
    for _ in 0..50 {
        if s.hub.pairing_status().pending.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(s.hub.pairing_status().pending.is_empty());
    assert!(s.hub.devices().unwrap().is_empty());
}

#[tokio::test]
async fn versions_must_match() {
    let s = server().await;
    let device = Identity::generate().unwrap();
    let key = pair(&s, &device, DeviceRole::Full).await;
    let refusal = |r: Result<client::Session, ConnectError>| match r {
        Err(ConnectError::Refused { reason, .. }) => reason,
        Err(e) => panic!("{e}"),
        Ok(_) => panic!("connected"),
    };
    let old = client::connect_speaking(&s.addr(), &device, key, &me("Laptop"), PROTOCOL - 1).await;
    assert_eq!(refusal(old), Refusal::UpdateClient);
    let new = client::connect_speaking(&s.addr(), &device, key, &me("Laptop"), PROTOCOL + 1).await;
    assert_eq!(refusal(new), Refusal::UpdateServer);
    let mut other = me("Laptop");
    other.manifest = "0000000000000000".into();
    assert_eq!(refusal(client::connect(&s.addr(), &device, key, &other).await), Refusal::VersionMismatch);
}

#[tokio::test]
async fn no_secret_in_anything_a_device_or_the_screen_receives() {
    let s = server().await;
    let device = Identity::generate().unwrap();
    let key = pair(&s, &device, DeviceRole::Full).await;
    s.hub.set_pin(Some("482915")).unwrap();
    let c = client::connect(&s.addr(), &device, key, &me("Laptop")).await.unwrap();
    let _ = c.step_up("482915").await;
    let welcome = serde_json::to_string(&c.welcome).unwrap();
    let shown = [
        welcome,
        serde_json::to_string(&s.hub.devices().unwrap()).unwrap(),
        serde_json::to_string(&s.hub.audit_log(1000).unwrap()).unwrap(),
        serde_json::to_string(&s.hub.pairing_status()).unwrap(),
    ]
    .join("\n");
    let device_secret = hex::encode(device.pkcs8());
    let server_secret = hex::encode(&s.identity_pkcs8);
    // The 32-byte private seed inside ring's PKCS#8 v2 document (bytes 16..48).
    let seed = |p: &[u8]| hex::encode(&p[16..48]);
    for secret in [device_secret, server_secret, seed(device.pkcs8()), seed(&s.identity_pkcs8), "482915".into()] {
        assert!(!shown.contains(&secret), "a secret leaked");
    }
    assert!(!s.hub.pin_set() || !shown.contains("argon2"), "not even the PIN's hash");
}
