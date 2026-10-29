//! The inspector's Browser tab (card 14): one child webview in the main
//! window, made on the first navigation and destroyed when the tab closes;
//! the page gets no app access (only `main` may call these commands, and
//! the guest can never load the app's own origin, which the capabilities
//! would treat as local). Navigation, the element picker — a script whose
//! result waits in a page global the app reads by evaluation, polled only
//! while armed — the visible-viewport screenshot (WebView2 CapturePreview)
//! and the list of local listeners for the start page. Cookies and history
//! are ephemeral (incognito).

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Url, Webview};

pub const LABEL: &str = "pilot-browser";
const STATE_EVENT: &str = "pilot-browser-state";
pub const ELEMENT_EVENT: &str = "pilot-browser-element";

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub picking: bool,
    pub zoom: f64,
}

#[derive(Default)]
pub struct BrowserState {
    status: Mutex<Status>,
    lifecycle: tokio::sync::Mutex<()>,
    /// Bumped per guest: callbacks of a destroyed one stand down.
    view: AtomicU64,
    /// Bumped per pick: an old pick's poll stands down.
    pick: AtomicU64,
    pick_key: Mutex<Option<String>>,
}

impl BrowserState {
    /// One browser call at a time: a navigation never races a close.
    pub fn serial(&self) -> &tokio::sync::Mutex<()> {
        &self.lifecycle
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Only the app's own page may drive the browser.
pub fn guard(caller: &Webview) -> Result<(), String> {
    if caller.label() == "main" {
        Ok(())
    } else {
        Err("browser content has no app access".into())
    }
}

fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme() && a.host_str() == b.host_str() && a.port_or_known_default() == b.port_or_known_default()
}

/// HTTP(S) with a host, no credentials, never the app's own origin.
pub fn allowed(url: &Url, own: Option<&Url>) -> bool {
    matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && !own.is_some_and(|o| same_origin(url, o))
}

/// The address bar: a URL, a host (https), `localhost:3000` (http), or a bare port.
pub fn normalize_url(input: &str, own: Option<&Url>) -> Result<Url, String> {
    let input = input.trim();
    if input.is_empty() || input.len() > 4096 || input.chars().any(char::is_control) {
        return Err("Enter a web address or a port.".into());
    }
    let port_only = input.trim_start_matches(':');
    let candidate = if !port_only.is_empty() && port_only.len() <= 5 && port_only.chars().all(|c| c.is_ascii_digit()) {
        format!("http://localhost:{port_only}")
    } else if input.contains("://") {
        input.to_string()
    } else if input.starts_with("localhost") || input.starts_with("127.") || input.starts_with("[::1]") || input.starts_with("0.0.0.0") {
        format!("http://{}", input.replacen("0.0.0.0", "localhost", 1))
    } else {
        format!("https://{input}")
    };
    let url = Url::parse(&candidate).map_err(|_| "That is not a web address.".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Only http and https pages open here.".into());
    }
    if !allowed(&url, own) {
        return Err(if own.is_some_and(|o| same_origin(&url, o)) {
            "That is QuantSuite itself — it does not open in its own preview.".into()
        } else {
            "Only http and https addresses without credentials open here.".into()
        });
    }
    Ok(url)
}

/// The app's own origin (the main page): the guest never loads it.
fn own_origin(app: &AppHandle) -> Option<Url> {
    app.get_webview("main").and_then(|w| w.url().ok())
}

/// Bounds inside the window, at least one pixel.
pub fn clamp(b: Bounds, window: (f64, f64)) -> Result<Bounds, String> {
    if ![b.x, b.y, b.width, b.height].iter().all(|v| v.is_finite()) || b.width < 1.0 || b.height < 1.0 {
        return Err("the browser area has no size".into());
    }
    let x = b.x.clamp(0.0, (window.0 - 1.0).max(0.0));
    let y = b.y.clamp(0.0, (window.1 - 1.0).max(0.0));
    Ok(Bounds { x, y, width: b.width.min(window.0 - x).max(1.0), height: b.height.min(window.1 - y).max(1.0) })
}

fn emit(app: &AppHandle, change: impl FnOnce(&mut Status)) {
    let state = app.state::<BrowserState>();
    let snapshot = {
        let mut s = lock(&state.status);
        change(&mut s);
        s.clone()
    };
    let _ = app.emit_to("main", STATE_EVENT, snapshot);
}

fn current(app: &AppHandle, generation: u64) -> bool {
    app.state::<BrowserState>().view.load(Ordering::SeqCst) == generation
}

fn guest(app: &AppHandle) -> Result<Webview, String> {
    app.get_webview(LABEL).ok_or_else(|| "open a page first".into())
}

fn window_size(caller: &Webview) -> Result<(f64, f64), String> {
    let window = caller.window();
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let size = window.inner_size().map_err(|e| e.to_string())?.to_logical::<f64>(scale);
    Ok((size.width, size.height))
}

fn place(view: &Webview, b: Bounds) -> Result<(), String> {
    view.set_bounds(tauri::Rect { position: tauri::LogicalPosition::new(b.x, b.y).into(), size: tauri::LogicalSize::new(b.width, b.height).into() })
        .map_err(|e| e.to_string())
}

/// Back/forward availability from WebView2.
fn sync_history(view: &Webview, generation: u64) {
    #[cfg(windows)]
    {
        let app = view.app_handle().clone();
        let _ = view.with_webview(move |platform| unsafe {
            if !current(&app, generation) {
                return;
            }
            let Ok(core) = platform.controller().CoreWebView2() else {
                return;
            };
            let (mut back, mut forward) = (windows::core::BOOL(0), windows::core::BOOL(0));
            let _ = core.CanGoBack(&mut back);
            let _ = core.CanGoForward(&mut forward);
            emit(&app, |s| {
                s.can_go_back = back.as_bool();
                s.can_go_forward = forward.as_bool();
            });
        });
    }
    #[cfg(not(windows))]
    let _ = (view, generation);
}

/// Show `url` in the guest — made now when there is none.
pub fn open(app: &AppHandle, caller: &Webview, url: &str, bounds: Bounds, zoom: f64) -> Result<Status, String> {
    let own = own_origin(app);
    let url = normalize_url(url, own.as_ref())?;
    let bounds = clamp(bounds, window_size(caller)?)?;
    cancel_pick(app);
    emit(app, |s| {
        s.url = url.to_string();
        s.title = url.host_str().unwrap_or_default().to_string();
        s.loading = true;
    });
    let zoom = if zoom.is_finite() { zoom.clamp(0.25, 5.0) } else { 1.0 };
    if let Some(view) = app.get_webview(LABEL) {
        place(&view, bounds)?;
        view.show().map_err(|e| e.to_string())?;
        view.navigate(url).map_err(|e| e.to_string())?;
    } else {
        let state = app.state::<BrowserState>();
        let generation = state.view.fetch_add(1, Ordering::SeqCst) + 1;
        let nav = app.clone();
        let own_nav = own.clone();
        let builder = tauri::webview::WebviewBuilder::new(LABEL, tauri::WebviewUrl::External(url))
            .incognito(true)
            .devtools(false)
            .on_navigation(move |u| {
                if !current(&nav, generation) || !allowed(u, own_nav.as_ref()) {
                    return false;
                }
                cancel_pick(&nav);
                emit(&nav, |s| {
                    s.url = u.to_string();
                    s.loading = true;
                });
                true
            })
            // Links that open a window open here instead: no second window, no popups.
            .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
            .on_download(|_, _| false)
            .on_document_title_changed(move |view, title| {
                if current(view.app_handle(), generation) {
                    emit(view.app_handle(), |s| s.title = clip(&title, 300));
                }
            })
            .on_page_load(move |view, payload| {
                if !current(view.app_handle(), generation) {
                    return;
                }
                let done = matches!(payload.event(), tauri::webview::PageLoadEvent::Finished);
                emit(view.app_handle(), |s| {
                    s.url = payload.url().to_string();
                    s.loading = !done;
                });
                sync_history(&view, generation);
            });
        let view = caller
            .window()
            .add_child(builder, tauri::LogicalPosition::new(bounds.x, bounds.y), tauri::LogicalSize::new(bounds.width, bounds.height))
            .map_err(|e| format!("the browser could not open: {e}"))?;
        view.show().map_err(|e| e.to_string())?;
    }
    if let Ok(view) = guest(app) {
        let _ = view.set_zoom(zoom);
    }
    emit(app, |s| s.zoom = zoom);
    Ok(lock(&app.state::<BrowserState>().status).clone())
}

/// Follow the tab's area; hidden while an app menu or dialog covers it (a native view is always on top).
pub fn set_bounds(app: &AppHandle, caller: &Webview, bounds: Bounds, visible: bool) -> Result<(), String> {
    let Some(view) = app.get_webview(LABEL) else {
        return Ok(());
    };
    if visible {
        place(&view, clamp(bounds, window_size(caller)?)?)?;
        view.show().map_err(|e| e.to_string())
    } else {
        cancel_pick(app);
        view.hide().map_err(|e| e.to_string())
    }
}

pub fn action(app: &AppHandle, what: &str) -> Result<(), String> {
    if !matches!(what, "back" | "forward" | "reload" | "stop") {
        return Err(format!("unknown browser action '{what}'"));
    }
    cancel_pick(app);
    let view = guest(app)?;
    let generation = app.state::<BrowserState>().view.load(Ordering::SeqCst);
    #[cfg(windows)]
    {
        let what = what.to_string();
        view.with_webview(move |platform| unsafe {
            if let Ok(core) = platform.controller().CoreWebView2() {
                let _ = match what.as_str() {
                    "back" => core.GoBack(),
                    "forward" => core.GoForward(),
                    "reload" => core.Reload(),
                    _ => core.Stop(),
                };
            }
        })
        .map_err(|e| e.to_string())?;
    }
    #[cfg(not(windows))]
    view.eval(match what {
        "back" => "history.back()",
        "forward" => "history.forward()",
        "reload" => "location.reload()",
        _ => "window.stop()",
    })
    .map_err(|e| e.to_string())?;
    sync_history(&view, generation);
    Ok(())
}

pub fn zoom(app: &AppHandle, factor: f64) -> Result<f64, String> {
    let factor = if factor.is_finite() { (factor * 100.0).round() / 100.0 } else { 1.0 }.clamp(0.25, 5.0);
    if let Ok(view) = guest(app) {
        view.set_zoom(factor).map_err(|e| e.to_string())?;
    }
    emit(app, |s| s.zoom = factor);
    Ok(factor)
}

/// The tab closed: the guest goes, and with it the page's session.
pub fn close(app: &AppHandle) -> Result<(), String> {
    cancel_pick(app);
    app.state::<BrowserState>().view.fetch_add(1, Ordering::SeqCst);
    if let Some(view) = app.get_webview(LABEL) {
        view.close().map_err(|e| e.to_string())?;
    }
    emit(app, |s| *s = Status { zoom: 1.0, ..Default::default() });
    Ok(())
}

// ── Element picker ──

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Element {
    pub url: String,
    pub title: String,
    pub selector: String,
    pub tag: String,
    pub text: String,
    pub html: String,
    pub bounds: Bounds,
}

/// Printable characters only, at most `max`.
pub fn clip(value: &str, max: usize) -> String {
    value.chars().filter(|c| !c.is_control() || *c == '\n').take(max).collect()
}

/// A second line of defence after the page script: no script or style
/// bodies, no event handlers, no values, no `javascript:` links.
pub fn sanitize_markup(html: &str) -> String {
    let mut out = html.to_string();
    for tag in ["script", "style", "noscript", "template"] {
        let re = regex::Regex::new(&format!(r"(?is)<{tag}\b.*?</{tag}\s*>|<{tag}\b[^>]*/?>")).expect("static pattern");
        out = re.replace_all(&out, "").into_owned();
    }
    let attrs = regex::Regex::new(r#"(?i)\s(?:on[a-z]+|value|srcdoc)\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+)"#).expect("static pattern");
    out = attrs.replace_all(&out, "").into_owned();
    let js = regex::Regex::new(r#"(?i)(href|src)\s*=\s*(["']?)\s*javascript:[^"'\s>]*"#).expect("static pattern");
    out = js.replace_all(&out, "$1=$2#").into_owned();
    clip(&out, 3000)
}

/// What the page returned, checked and bounded before it reaches the composer.
pub fn clean_element(mut e: Element, own: Option<&Url>) -> Option<Element> {
    let url = Url::parse(&e.url).ok()?;
    if !allowed(&url, own) || e.url.len() > 4096 || ![e.bounds.x, e.bounds.y, e.bounds.width, e.bounds.height].iter().all(|v| v.is_finite()) {
        return None;
    }
    e.title = clip(&e.title, 300);
    e.selector = clip(&e.selector, 1000);
    e.tag = clip(&e.tag, 40).to_lowercase();
    e.text = clip(&e.text, 1200);
    e.html = sanitize_markup(&e.html);
    Some(e)
}

fn cancel_pick(app: &AppHandle) {
    let state = app.state::<BrowserState>();
    state.pick.fetch_add(1, Ordering::SeqCst);
    let key = lock(&state.pick_key).take();
    if let (Some(key), Some(view)) = (key, app.get_webview(LABEL)) {
        let key = serde_json::to_string(&key).unwrap_or_default();
        let _ = view.eval(format!("(()=>{{const s=globalThis[{key}];if(s&&s.result===null)s.cancel();delete globalThis[{key}];}})()"));
        emit(app, |s| s.picking = false);
    }
}

async fn evaluate(view: &Webview, script: String) -> Result<String, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = Mutex::new(Some(tx));
    view.eval_with_callback(script, move |value| {
        if let Some(tx) = lock(&tx).take() {
            let _ = tx.send(value);
        }
    })
    .map_err(|e| e.to_string())?;
    tokio::time::timeout(Duration::from_secs(3), rx).await.map_err(|_| "the page did not answer".to_string())?.map_err(|_| "the browser closed".into())
}

/// Arm the picker; the chosen element arrives as `pilot-browser-element`.
pub async fn pick(app: AppHandle, enabled: bool) -> Result<(), String> {
    cancel_pick(&app);
    if !enabled {
        return Ok(());
    }
    let view = guest(&app)?;
    let state = app.state::<BrowserState>();
    let generation = state.pick.load(Ordering::SeqCst);
    let key = format!("__qp_pick_{}", uuid::Uuid::new_v4().simple());
    *lock(&state.pick_key) = Some(key.clone());
    let encoded = serde_json::to_string(&key).map_err(|e| e.to_string())?;
    view.eval(include_str!("browser_picker.js").replace("__QP_PICK_KEY__", &encoded)).map_err(|e| e.to_string())?;
    emit(&app, |s| s.picking = true);
    let _ = view.set_focus();
    let own = own_origin(&app);
    tauri::async_runtime::spawn(async move {
        let stale = |app: &AppHandle| app.state::<BrowserState>().pick.load(Ordering::SeqCst) != generation;
        // Read only while armed, for at most a minute.
        for _ in 0..240 {
            tokio::time::sleep(Duration::from_millis(250)).await;
            if stale(&app) {
                return;
            }
            let script = format!("(()=>{{const s=globalThis[{encoded}];return s?s.result:{{cancelled:true}}}})()");
            let Ok(raw) = evaluate(&view, script).await else {
                break;
            };
            if stale(&app) {
                return;
            }
            // WebView2 hands the result back as JSON; a string result may come encoded twice.
            let value: serde_json::Value = match serde_json::from_str(&raw) {
                Ok(serde_json::Value::String(inner)) => serde_json::from_str(&inner).unwrap_or(serde_json::Value::Null),
                Ok(v) => v,
                Err(_) => break,
            };
            if value.is_null() {
                continue;
            }
            if value.get("cancelled").and_then(|v| v.as_bool()) == Some(true) || raw.len() > 32_000 {
                break;
            }
            if let Some(e) = serde_json::from_value::<Element>(value).ok().and_then(|e| clean_element(e, own.as_ref())) {
                let _ = app.emit_to("main", ELEMENT_EVENT, e);
            }
            break;
        }
        if !stale(&app) {
            cancel_pick(&app);
            emit(&app, |s| s.picking = false);
        }
    });
    Ok(())
}

// ── Screenshot ──

/// The visible viewport as PNG (WebView2 CapturePreview).
#[cfg(windows)]
pub async fn capture(app: &AppHandle) -> Result<Vec<u8>, String> {
    use std::{cell::RefCell, rc::Rc};
    use webview2_com::{CapturePreviewCompletedHandler, Microsoft::Web::WebView2::Win32::COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG};
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::Com::{StructuredStorage::CreateStreamOnHGlobal, STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET};
    cancel_pick(app);
    let view = guest(app)?;
    // A round trip first, so the picker's highlight is gone from the next frame.
    let _ = evaluate(&view, "null".into()).await;
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<Vec<u8>, String>>();
    view.with_webview(move |platform| unsafe {
        let tx = Rc::new(RefCell::new(Some(tx)));
        let reply = tx.clone();
        let started = (|| -> windows::core::Result<()> {
            let stream = CreateStreamOnHGlobal(HGLOBAL::default(), true)?;
            let reader = stream.clone();
            let handler = CapturePreviewCompletedHandler::create(Box::new(move |result| {
                let bytes = (|| -> Result<Vec<u8>, String> {
                    result.map_err(|e| e.to_string())?;
                    let mut meta = STATSTG::default();
                    reader.Stat(&mut meta, STATFLAG_NONAME).map_err(|e| e.to_string())?;
                    if meta.cbSize == 0 || meta.cbSize > 32 * 1024 * 1024 {
                        return Err("the screenshot is too large — make the browser smaller".into());
                    }
                    reader.Seek(0, STREAM_SEEK_SET, None).map_err(|e| e.to_string())?;
                    let mut bytes = vec![0u8; meta.cbSize as usize];
                    let mut read = 0u32;
                    reader.Read(bytes.as_mut_ptr().cast(), bytes.len() as u32, Some(&mut read)).ok().map_err(|e| e.to_string())?;
                    bytes.truncate(read as usize);
                    Ok(bytes)
                })();
                if let Some(tx) = reply.borrow_mut().take() {
                    let _ = tx.send(bytes);
                }
                Ok(())
            }));
            platform.controller().CoreWebView2()?.CapturePreview(COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG, &stream, &handler)
        })();
        if let Err(e) = started {
            if let Some(tx) = tx.borrow_mut().take() {
                let _ = tx.send(Err(e.to_string()));
            }
        }
    })
    .map_err(|e| e.to_string())?;
    tokio::time::timeout(Duration::from_secs(10), rx).await.map_err(|_| "the screenshot timed out".to_string())?.map_err(|_| "the browser closed".to_string())?
}

#[cfg(not(windows))]
pub async fn capture(_app: &AppHandle) -> Result<Vec<u8>, String> {
    Err("screenshots of the preview need Windows (WebView2)".into())
}

// ── Local listeners (the start page) ──

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Listener {
    pub port: u16,
    pub pid: u32,
    pub process: String,
}

/// TCP listeners from `netstat -ano` and process names from `tasklist /FO CSV /NH`.
pub fn parse_listeners(netstat: &str, tasklist: &str) -> Vec<Listener> {
    let names: std::collections::HashMap<u32, String> = tasklist
        .lines()
        .filter_map(|l| {
            let cols: Vec<&str> = l.split("\",\"").map(|c| c.trim_matches('"')).collect();
            Some((cols.get(1)?.trim().parse().ok()?, cols.first()?.to_string()))
        })
        .collect();
    let mut out: Vec<Listener> = Vec::new();
    for line in netstat.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 5 || !cols[0].eq_ignore_ascii_case("TCP") || !cols[3].eq_ignore_ascii_case("LISTENING") {
            continue;
        }
        let (Some(port), Ok(pid)) = (cols[1].rsplit(':').next().and_then(|p| p.parse::<u16>().ok()), cols[4].parse::<u32>()) else {
            continue;
        };
        // System, svchost-style low ports and the RPC range are not dev servers.
        if pid <= 4 || port < 1024 || out.iter().any(|l| l.port == port) {
            continue;
        }
        out.push(Listener { port, pid, process: names.get(&pid).cloned().unwrap_or_else(|| "unknown".into()) });
    }
    out.sort_by_key(|l| l.port);
    out
}

pub fn listeners() -> Result<Vec<Listener>, String> {
    let run = |program: &str, args: &[&str]| -> Result<String, String> {
        let mut cmd = std::process::Command::new(program);
        cmd.args(args);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let (ok, out, err) = crate::providers::run_capture_split(cmd, Duration::from_secs(10))?;
        if ok {
            Ok(out)
        } else {
            Err(err)
        }
    };
    let netstat = run("netstat", &["-ano", "-p", "TCP"])?;
    let tasks = run("tasklist", &["/FO", "CSV", "/NH"]).unwrap_or_default();
    Ok(parse_listeners(&netstat, &tasks))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_normalize_and_the_app_origin_is_refused() {
        let own = Url::parse("http://localhost:1420/").unwrap();
        let n = |s: &str| normalize_url(s, Some(&own)).map(|u| u.to_string());
        assert_eq!(n("5173").unwrap(), "http://localhost:5173/");
        assert_eq!(n(":3000").unwrap(), "http://localhost:3000/");
        assert_eq!(n("localhost:8080/app").unwrap(), "http://localhost:8080/app");
        assert_eq!(n("0.0.0.0:4000").unwrap(), "http://localhost:4000/");
        assert_eq!(n("127.0.0.1:9000").unwrap(), "http://127.0.0.1:9000/");
        assert_eq!(n("example.com/docs").unwrap(), "https://example.com/docs");
        assert_eq!(n("http://example.com").unwrap(), "http://example.com/");
        assert!(n("1420").unwrap_err().contains("QuantSuite itself"));
        assert!(n("http://localhost:1420/pilot").is_err());
        for bad in ["", "javascript:alert(1)", "file:///C:/x", "tauri://localhost", "http://user:pw@example.com", "ftp://x.org", "http://\u{7}x"] {
            assert!(n(bad).is_err(), "{bad}");
        }
        assert!(normalize_url("1420", None).is_ok(), "without a known own origin a port is just a port");
    }

    #[test]
    fn bounds_stay_inside_the_window() {
        let b = clamp(Bounds { x: 900.0, y: -5.0, width: 400.0, height: 2000.0 }, (1200.0, 800.0)).unwrap();
        assert_eq!(b, Bounds { x: 900.0, y: 0.0, width: 300.0, height: 800.0 });
        assert!(clamp(Bounds { x: 0.0, y: 0.0, width: 0.0, height: 10.0 }, (100.0, 100.0)).is_err());
        assert!(clamp(Bounds { x: f64::NAN, y: 0.0, width: 10.0, height: 10.0 }, (100.0, 100.0)).is_err());
    }

    #[test]
    fn markup_and_elements_are_sanitized_and_bounded() {
        let html = r#"<div class="a" onclick="steal()"><script>evil()</script><style>x{}</style><input value="secret" name="pw"><a href="javascript:run()">go</a><a href='/ok'>ok</a></div>"#;
        let clean = sanitize_markup(html);
        for gone in ["onclick", "evil", "x{}", "secret", "javascript:"] {
            assert!(!clean.contains(gone), "{gone} in {clean}");
        }
        assert!(clean.contains(r#"name="pw""#) && clean.contains("href='/ok'"), "{clean}");
        let own = Url::parse("http://localhost:1420/").unwrap();
        let e = Element {
            url: "http://localhost:5173/".into(),
            title: "T\u{7}itle".into(),
            selector: "#app > button".into(),
            tag: "BUTTON".into(),
            text: "x".repeat(5000),
            html: html.into(),
            bounds: Bounds { x: 1.0, y: 2.0, width: 3.0, height: 4.0 },
        };
        let c = clean_element(e.clone(), Some(&own)).unwrap();
        assert_eq!((c.title.as_str(), c.tag.as_str(), c.text.chars().count()), ("Title", "button", 1200));
        assert!(clean_element(Element { url: "http://localhost:1420/".into(), ..e.clone() }, Some(&own)).is_none(), "the app's own page");
        assert!(clean_element(Element { url: "file:///x".into(), ..e.clone() }, Some(&own)).is_none());
        assert!(clean_element(Element { bounds: Bounds { x: f64::INFINITY, ..e.bounds }, ..e }, Some(&own)).is_none());
    }

    #[test]
    fn listeners_come_from_netstat_and_tasklist() {
        let netstat = "\n  Proto  Local Address          Foreign Address        State           PID\n  TCP    0.0.0.0:135            0.0.0.0:0              LISTENING       1196\n  TCP    0.0.0.0:445            0.0.0.0:0              LISTENING       4\n  TCP    127.0.0.1:5173         0.0.0.0:0              LISTENING       4242\n  TCP    [::1]:5173             [::]:0                 LISTENING       4242\n  TCP    0.0.0.0:3000           0.0.0.0:0              LISTENING       777\n  TCP    127.0.0.1:52000        127.0.0.1:3000         ESTABLISHED     888\n";
        let tasks = "\"System\",\"4\",\"Services\",\"0\",\"144 K\"\n\"node.exe\",\"4242\",\"Console\",\"1\",\"80,000 K\"\n\"python.exe\",\"777\",\"Console\",\"1\",\"30,000 K\"\n";
        assert_eq!(
            parse_listeners(netstat, tasks),
            vec![Listener { port: 3000, pid: 777, process: "python.exe".into() }, Listener { port: 5173, pid: 4242, process: "node.exe".into() }]
        );
    }
}
