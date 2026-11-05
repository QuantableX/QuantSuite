//! The gateway hosted in a (mock) Tauri app (card Server 4/7): authorized
//! calls of a paired device run through the main webview's IPC
//! (`qs_core::gateway::webview_dispatch`, option B), and the app's events —
//! the manifest's Tauri events and bus topics, nothing else — reach the device.

use qs_core::gateway::{forward_events, webview_dispatch};
use qs_gateway::client::{self, ClientEvent, ClientInfo};
use qs_gateway::protocol::EventKind;
use qs_gateway::registry::Registry;
use qs_gateway::server::{Hub, HubConfig};
use qs_gateway::{DeviceRole, Identity, Manifest};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::test::{mock_builder, mock_context, noop_assets};
use tauri::{Emitter, State, WebviewWindowBuilder};

#[derive(Default)]
struct Store(Mutex<Vec<String>>);

#[tauri::command(async)]
fn list_items(store: State<'_, Store>) -> Vec<String> {
    store.0.lock().unwrap().clone()
}

#[tauri::command(async)]
fn add_item(store: State<'_, Store>, name: String) -> usize {
    let mut items = store.0.lock().unwrap();
    items.push(name);
    items.len()
}

const MANIFEST: &str = r#"{
  "commands": { "add_item": "server:write", "list_items": "server:read", "secret_item": "deny:read" },
  "plugins": {},
  "events": ["spike://tick"],
  "topics": ["notes.note.created"]
}"#;

#[tokio::test(flavor = "multi_thread")]
async fn calls_run_in_the_app_and_its_events_reach_the_device() {
    let app = mock_builder()
        .manage(Store::default())
        .invoke_handler(tauri::generate_handler![list_items, add_item])
        .build(mock_context(noop_assets()))
        .expect("mock app");
    let _window = WebviewWindowBuilder::new(&app, "main", Default::default()).build().expect("main webview");
    let handle = app.handle().clone();

    let manifest = Arc::new(Manifest::parse(MANIFEST).unwrap());
    let hub = Hub::new(Identity::generate().unwrap(), Registry::in_memory().unwrap(), manifest.clone(), HubConfig::new("mock", "test")).unwrap();
    hub.set_dispatch(webview_dispatch(&handle));
    forward_events(&handle, &hub);
    let listener = hub.listen("127.0.0.1:0".parse().unwrap()).await.unwrap();
    let addr = listener.local_addr().to_string();

    let device = Identity::generate().unwrap();
    let me = ClientInfo { name: "Laptop".into(), app: "test".into(), manifest: manifest.hash().into() };
    let code = hub.start_pairing().unwrap().code.unwrap();
    let attempt = client::pair(&addr, &device, &code, &me).await.unwrap();
    let pending = hub.pairing_status().pending[0].id.clone();
    hub.confirm_pairing(&pending, Some(DeviceRole::Full)).unwrap();
    attempt.finish().await.unwrap();

    let c = client::connect(&addr, &device, hub.public_key(), &me).await.unwrap();
    let mut events = c.take_events().unwrap();
    assert_eq!(c.call("add_item", json!({ "name": "first" })).await.unwrap(), json!(1));
    assert_eq!(c.call("list_items", json!({})).await.unwrap(), json!(["first"]));
    assert_eq!(c.call("secret_item", json!({})).await.unwrap_err().code, "denied");

    handle.emit("spike://tick", 7).unwrap();
    let bus = |id: &str, topic: &str, n: u32| json!({ "id": id, "topic": topic, "source": "notes", "ts": 1, "payload": { "n": n } });
    handle.emit("qs://event", bus("a", "notes.note.created", 1)).unwrap();
    handle.emit("qs://event", bus("b", "pilot.thread.created", 2)).unwrap();
    handle.emit("other://event", 3).unwrap();
    handle.emit("qs://event", bus("c", "notes.note.created", 3)).unwrap();

    let mut got = Vec::new();
    while got.len() < 3 {
        match tokio::time::timeout(Duration::from_secs(5), events.recv()).await.expect("events arrive").unwrap() {
            ClientEvent::Event { kind, topic, payload, .. } => got.push((kind, topic, payload)),
            ClientEvent::Reload { .. } => panic!("no reload"),
        }
    }
    assert_eq!(got[0], (EventKind::Tauri, "spike://tick".to_string(), json!(7)));
    assert_eq!((got[1].0, got[1].1.as_str(), &got[1].2["payload"]["n"]), (EventKind::Bus, "notes.note.created", &json!(1)));
    assert_eq!(got[2].2["payload"]["n"], 3, "the client-placed topic and the unlisted event were not forwarded");
    assert!(tokio::time::timeout(Duration::from_millis(300), events.recv()).await.is_err(), "nothing else");
}
