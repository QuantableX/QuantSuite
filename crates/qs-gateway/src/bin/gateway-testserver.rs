//! A gateway server in its own process, for `tests/two_process.rs` (card
//! Server 4/7): a hub on 127.0.0.1 with a test dispatcher, the server PIN
//! 482915, and every pairing device allowed as Full. It prints one JSON line
//! (`addr`, `key`, `code`, `manifest`) and runs until its stdin closes.
//!
//!     gateway-testserver <dir>
//!
//! `QS_IDLE_MS` sets the idle timeout, `QS_RATE=<per sec>,<burst>` the rate
//! limit.

use qs_gateway::files::FileRoots;
use qs_gateway::protocol::EventKind;
use qs_gateway::registry::Registry;
use qs_gateway::server::{CallFuture, Dispatch, Hub, HubConfig};
use qs_gateway::{DeviceRole, Identity, Manifest};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;

const MANIFEST: &str = r#"{
  "commands": {
    "plugin:notes|list_notes": "server:read",
    "plugin:notes|count_runs": "server:read",
    "plugin:notes|stream": "server:read",
    "plugin:systems|slow_read": "server:read",
    "plugin:systems|run_backtest": "server:compute",
    "plugin:notes|create_note": "server:write",
    "plugin:algo|start_bot": "server:external",
    "plugin:algo|wallet_export_key": "deny:read"
  },
  "plugins": {},
  "events": ["backtest:progress"],
  "topics": ["notes.note.updated"]
}"#;

#[derive(Default)]
struct TestDispatch {
    hub: OnceLock<Weak<Hub>>,
    runs: Mutex<HashMap<String, u64>>,
}

impl TestDispatch {
    fn hub(&self) -> Option<Arc<Hub>> {
        self.hub.get().and_then(Weak::upgrade)
    }
}

fn arg_u64(args: &Value, key: &str, default: u64) -> u64 {
    args.get(key).and_then(Value::as_u64).unwrap_or(default)
}

/// The dispatcher as the hub holds it.
struct Handle(Arc<TestDispatch>);

impl Dispatch for Handle {
    fn call(&self, cmd: String, args: Value) -> CallFuture {
        let me = self.0.clone();
        Box::pin(async move {
            match cmd.as_str() {
                "plugin:notes|list_notes" => Ok(args),
                "plugin:systems|slow_read" => {
                    tokio::time::sleep(Duration::from_secs(arg_u64(&args, "secs", 10))).await;
                    Ok(json!("slow"))
                }
                "plugin:systems|run_backtest" => {
                    let n = arg_u64(&args, "n", 10);
                    for i in 0..n {
                        if let Some(h) = me.hub() {
                            h.publish(EventKind::Tauri, "backtest:progress", json!({ "i": i, "of": n }));
                        }
                        tokio::time::sleep(Duration::from_millis(2)).await;
                    }
                    Ok(json!({ "done": n }))
                }
                "plugin:notes|create_note" => {
                    tokio::time::sleep(Duration::from_millis(arg_u64(&args, "delay_ms", 0))).await;
                    let note = args.get("note").and_then(Value::as_str).unwrap_or("").to_string();
                    let runs = {
                        let mut r = me.runs.lock().unwrap();
                        let n = r.entry(note.clone()).or_insert(0);
                        *n += 1;
                        *n
                    };
                    Ok(json!({ "note": note, "runs": runs }))
                }
                "plugin:notes|count_runs" => {
                    let note = args.get("note").and_then(Value::as_str).unwrap_or("");
                    Ok(json!(me.runs.lock().unwrap().get(note).copied().unwrap_or(0)))
                }
                "plugin:notes|stream" => {
                    // `per_ms` events each millisecond (a Windows timer sleeps
                    // at least that long).
                    let (n, per_ms) = (arg_u64(&args, "n", 100), arg_u64(&args, "per_ms", 5).max(1));
                    let me2 = me.clone();
                    tokio::spawn(async move {
                        for i in 0..n {
                            if let Some(h) = me2.hub() {
                                h.publish(EventKind::Bus, "notes.note.updated", json!({ "i": i }));
                            }
                            if (i + 1) % per_ms == 0 {
                                tokio::time::sleep(Duration::from_millis(1)).await;
                            }
                        }
                    });
                    Ok(Value::Null)
                }
                "plugin:algo|start_bot" => Ok(json!("started")),
                other => Err(Value::String(format!("no such test command {other}"))),
            }
        })
    }
}

#[tokio::main]
async fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).expect("usage: gateway-testserver <dir>"));
    std::fs::create_dir_all(dir.join("files")).expect("dir");
    let manifest = Arc::new(Manifest::parse(MANIFEST).expect("manifest"));
    let mut config = HubConfig::new("testserver", "test");
    config.files = Some(FileRoots {
        roots: vec![("Files".into(), dir.join("files"))],
        deny: vec![dir.join("files").join("secret")],
        transfers: dir.join("transfers"),
    });
    if let Some(ms) = std::env::var("QS_IDLE_MS").ok().and_then(|v| v.parse().ok()) {
        config.idle_timeout = Duration::from_millis(ms);
    }
    if let Some((rate, burst)) = std::env::var("QS_RATE").ok().and_then(|v| v.split_once(',').map(|(a, b)| (a.parse().ok(), b.parse().ok()))) {
        config.calls_per_sec = rate.unwrap_or(config.calls_per_sec);
        config.burst = burst.unwrap_or(config.burst);
    }
    let hub = Hub::new(Identity::generate().expect("key"), Registry::open(&dir.join("gateway.db")).expect("db"), manifest.clone(), config)
        .expect("hub");
    let dispatch = Arc::new(TestDispatch::default());
    let _ = dispatch.hub.set(Arc::downgrade(&hub));
    hub.set_dispatch(Arc::new(Handle(dispatch)));
    hub.set_pin(Some("482915")).expect("pin");
    let listener = hub.listen("127.0.0.1:0".parse().expect("addr")).await.expect("listen");
    let code = hub.start_pairing().expect("code").code.expect("code");
    println!(
        "{}",
        json!({ "addr": listener.local_addr().to_string(), "key": hex::encode(hub.public_key()), "code": code, "manifest": manifest.hash() })
    );

    // Every device that pairs is allowed as Full.
    let h = hub.clone();
    tokio::spawn(async move {
        loop {
            for p in h.pairing_status().pending {
                let _ = h.confirm_pairing(&p.id, Some(DeviceRole::Full));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });
    // Until the test closes stdin (or dies).
    let _ = tokio::task::spawn_blocking(|| {
        let mut sink = Vec::new();
        let _ = std::io::stdin().read_to_end(&mut sink);
    })
    .await;
    drop(listener);
}
