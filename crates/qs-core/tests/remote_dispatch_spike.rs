//! Spike for PLAN-SERVER-CLIENT §6.3 (card Server 1/7): the server performs
//! a remote client's call from Rust through Tauri's own IPC —
//! `Webview::on_message` with the app's invoke key (option B) — without
//! patching Tauri, and what a call costs.
//!
//! Runs on Tauri's mock runtime, so the numbers are the IPC machinery
//! (request → ACL → command → serialised response) without WebView2. Option
//! A (the hidden shell performs the invoke, like QuantMCP's agent broker)
//! adds two WebView2 crossings and a JS event round trip on top of exactly
//! this path; it cannot be measured here because the app window is not ours
//! to start. `cargo test -p tauri-plugin-qs --test remote_dispatch_spike -- --nocapture`
//! prints the numbers.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponse};
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::webview::InvokeRequest;
use tauri::{Emitter, Listener, Manager, State, Webview, WebviewWindowBuilder};

#[derive(Default)]
struct Store(Mutex<Vec<String>>);

#[tauri::command]
fn list_items(store: State<'_, Store>) -> Vec<String> {
    store.0.lock().unwrap().clone()
}

#[tauri::command]
fn add_item(store: State<'_, Store>, name: String) -> usize {
    let mut items = store.0.lock().unwrap();
    items.push(name);
    items.len()
}

/// A command that streams: events while it runs, then its result.
#[tauri::command]
fn stream(app: tauri::AppHandle<MockRuntime>, n: u32) -> u32 {
    for i in 0..n {
        app.emit("spike://tick", i).unwrap();
    }
    n
}

fn app() -> (tauri::App<MockRuntime>, Webview<MockRuntime>) {
    let app = mock_builder()
        .manage(Store::default())
        .invoke_handler(tauri::generate_handler![list_items, add_item, stream])
        .build(mock_context(noop_assets()))
        .expect("mock app");
    let window = WebviewWindowBuilder::new(&app, "main", Default::default()).build().expect("main webview");
    let webview = window.as_ref().clone();
    (app, webview)
}

/// What the server's dispatcher builds for a remote call: the app's own
/// invoke key and its local origin, so the ACL resolves exactly as for the
/// UI's own calls.
fn request(app: &tauri::App<MockRuntime>, cmd: &str, body: serde_json::Value, key: Option<&str>) -> InvokeRequest {
    InvokeRequest {
        cmd: cmd.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: if cfg!(windows) { "http://tauri.localhost" } else { "tauri://localhost" }.parse().unwrap(),
        body: InvokeBody::Json(body),
        headers: Default::default(),
        invoke_key: key.map(String::from).unwrap_or_else(|| app.handle().invoke_key().to_string()),
    }
}

/// One dispatch; `None` when Tauri dropped the request without an answer.
fn dispatch(webview: &Webview<MockRuntime>, req: InvokeRequest) -> Option<Result<serde_json::Value, serde_json::Value>> {
    let (tx, rx) = mpsc::sync_channel(1);
    webview.clone().on_message(
        req,
        Box::new(move |_w, _cmd, response, _cb, _err| {
            let _ = tx.send(response);
        }),
    );
    match rx.recv_timeout(Duration::from_secs(2)).ok()? {
        InvokeResponse::Ok(body) => Some(Ok(body.deserialize::<serde_json::Value>().unwrap())),
        InvokeResponse::Err(e) => Some(Err(e.0)),
    }
}

fn per_call(n: u32, mut f: impl FnMut(u32)) -> Duration {
    let t = Instant::now();
    for i in 0..n {
        f(i);
    }
    t.elapsed() / n
}

#[test]
fn option_b_dispatches_read_write_and_streaming_calls_without_patching_tauri() {
    let (app, webview) = app();

    // Write then read through the IPC, with the key the app hands out.
    let r = dispatch(&webview, request(&app, "add_item", serde_json::json!({"name": "first"}), None));
    assert_eq!(r, Some(Ok(serde_json::json!(1))));
    let r = dispatch(&webview, request(&app, "list_items", serde_json::json!({}), None));
    assert_eq!(r, Some(Ok(serde_json::json!(["first"]))));

    // Streaming: events reach listeners (the gateway forwards them as
    // `event` frames), then the result arrives.
    let ticks = Arc::new(Mutex::new(Vec::new()));
    let seen = ticks.clone();
    app.listen("spike://tick", move |e| seen.lock().unwrap().push(e.payload().to_string()));
    let r = dispatch(&webview, request(&app, "stream", serde_json::json!({"n": 5}), None));
    assert_eq!(r, Some(Ok(serde_json::json!(5))));
    assert_eq!(ticks.lock().unwrap().len(), 5);

    // A wrong invoke key is dropped without an answer: the key never leaves
    // the server, so only its own dispatcher can use this path.
    assert_eq!(dispatch(&webview, request(&app, "list_items", serde_json::json!({}), Some("guess"))), None);
    // An unknown command is refused, not crashed.
    assert!(matches!(dispatch(&webview, request(&app, "nope", serde_json::json!({}), None)), Some(Err(_))));

    // The cost per call (mock runtime, no WebView2).
    let n = 2_000;
    let read = per_call(n, |_| {
        dispatch(&webview, request(&app, "list_items", serde_json::json!({}), None)).unwrap().unwrap();
    });
    let write = per_call(n, |i| {
        dispatch(&webview, request(&app, "add_item", serde_json::json!({"name": format!("x{i}")}), None)).unwrap().unwrap();
    });
    let streaming = per_call(200, |_| {
        dispatch(&webview, request(&app, "stream", serde_json::json!({"n": 10}), None)).unwrap().unwrap();
    });
    println!("option B on the mock runtime: read {read:?}/call, write {write:?}/call, streaming (10 events) {streaming:?}/call");
    // Generous bounds — a regression guard, not a benchmark.
    assert!(read < Duration::from_millis(5), "{read:?}");
    assert!(write < Duration::from_millis(5), "{write:?}");
    let _ = app.state::<Store>();
}
