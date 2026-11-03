//! QuantCanvas's browser windows (rework 2026-11-03).
//!
//! Every tab is a child webview of the suite window that this module makes
//! itself, so the page's own events drive the browser: `on_navigation`,
//! `on_page_load`, `on_document_title_changed`, WebView2's history changes
//! (single-page apps) and `on_new_window` (a link that wants a window opens a
//! tab in the same browser window; a sized popup — a sign-in window — opens
//! as the page asked). Every change goes out as one `canvas-browser-state`
//! event; there is no polling and nothing is injected into the pages.
//!
//! A native webview is drawn above all HTML. The frontend therefore shows
//! the live page only while nothing lies on top of it and the canvas is at
//! rest; otherwise a picture of the page (`browser_tab_capture`, WebView2
//! CapturePreview) stands in its place in the DOM. Higher canvas windows are
//! cut out of a live page with a window region (`set_browser_clip_region`).
//!
//! Only the suite's own pages may drive the tabs: the pages in them get no
//! app access, and a tab never loads the suite's own origin.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Url, Webview};

pub const STATE_EVENT: &str = "canvas-browser-state";
pub const NEW_TAB_EVENT: &str = "canvas-browser-new-tab";
#[cfg(target_os = "windows")]
const FULLSCREEN_EVENT: &str = "browser-fullscreen-changed";
const LABEL_PREFIX: &str = "browser-";

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// What the browser window shows for one tab.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TabStatus {
    pub label: String,
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct NewTab {
    label: String,
    url: String,
}

/// The tabs that exist, by webview label. `generation` tells a tab's current
/// webview from an earlier one with the same label (a workspace switch closes
/// and rebuilds them): callbacks of the earlier one stand down.
#[derive(Default)]
pub struct BrowserTabs {
    status: Mutex<HashMap<String, TabStatus>>,
    generation: Mutex<HashMap<String, u64>>,
    next: AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Only the suite's own pages drive the tabs, never a page in one.
fn guard(caller: &Webview) -> Result<(), String> {
    let label = caller.label();
    if label == "main" || label.starts_with("suite-") {
        Ok(())
    } else {
        Err("browser content has no app access".into())
    }
}

fn check_label(label: &str) -> Result<(), String> {
    let ok = label.starts_with(LABEL_PREFIX)
        && label.len() <= 160
        && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if ok {
        Ok(())
    } else {
        Err(format!("'{label}' is not a browser tab"))
    }
}

fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme() && a.host_str() == b.host_str() && a.port_or_known_default() == b.port_or_known_default()
}

/// What a tab may show: web pages and the in-page kinds browsers keep in the
/// tab (about:blank, data:/blob: documents), never the suite's own origin,
/// files or app schemes.
pub fn allowed(url: &Url, own: Option<&Url>) -> bool {
    match url.scheme() {
        "http" | "https" => url.host_str().is_some() && !own.is_some_and(|o| same_origin(url, o)),
        "about" | "data" | "blob" => true,
        _ => false,
    }
}

fn own_origin(app: &AppHandle) -> Option<Url> {
    app.get_webview("main").and_then(|w| w.url().ok())
}

fn parse(url: &str, app: &AppHandle) -> Result<Url, String> {
    let parsed: Url = url.trim().parse().map_err(|e: url::ParseError| format!("not an address: {e}"))?;
    if !allowed(&parsed, own_origin(app).as_ref()) {
        return Err(format!("a browser tab cannot open {}", parsed.scheme()));
    }
    Ok(parsed)
}

fn current(app: &AppHandle, label: &str, generation: u64) -> bool {
    lock(&app.state::<BrowserTabs>().generation).get(label) == Some(&generation)
}

/// Change a tab's status and tell the browser window.
fn emit(app: &AppHandle, label: &str, change: impl FnOnce(&mut TabStatus)) {
    let state = app.state::<BrowserTabs>();
    let snapshot = {
        let mut all = lock(&state.status);
        let status = all.entry(label.to_string()).or_insert_with(|| TabStatus { label: label.to_string(), ..Default::default() });
        change(status);
        status.clone()
    };
    let _ = app.emit(STATE_EVENT, snapshot);
}

fn tab(app: &AppHandle, label: &str) -> Result<Webview, String> {
    app.get_webview(label).ok_or_else(|| format!("the tab '{label}' is not open"))
}

fn place(view: &Webview, b: Bounds) -> Result<(), String> {
    let width = b.width.max(1.0);
    let height = b.height.max(1.0);
    view.set_bounds(tauri::Rect { position: tauri::LogicalPosition::new(b.x, b.y).into(), size: tauri::LogicalSize::new(width, height).into() })
        .map_err(|e| e.to_string())
}

fn zoom_of(zoom: f64) -> f64 {
    if zoom.is_finite() { zoom.clamp(0.1, 5.0) } else { 1.0 }
}

/// Address, back/forward from WebView2 — also after a single-page app's
/// `pushState`, which no navigation event reports.
#[cfg(target_os = "windows")]
fn sync_history(view: &Webview, generation: u64) {
    let app = view.app_handle().clone();
    let label = view.label().to_string();
    let _ = view.with_webview(move |platform| unsafe {
        if !current(&app, &label, generation) {
            return;
        }
        let Ok(core) = platform.controller().CoreWebView2() else {
            return;
        };
        let (mut back, mut forward) = (windows::core::BOOL(0), windows::core::BOOL(0));
        let _ = core.CanGoBack(&mut back);
        let _ = core.CanGoForward(&mut forward);
        let mut source = windows::core::PWSTR::null();
        let url = core.Source(&mut source).ok().map(|_| webview2_com::take_pwstr(source));
        emit(&app, &label, |s| {
            s.can_go_back = back.as_bool();
            s.can_go_forward = forward.as_bool();
            if let Some(url) = url.filter(|u| !u.is_empty()) {
                s.url = url;
            }
        });
    });
}

#[cfg(not(target_os = "windows"))]
fn sync_history(_view: &Webview, _generation: u64) {}

/// WebView2 reports `pushState`/`replaceState` only as a history change.
#[cfg(target_os = "windows")]
fn watch_history(view: &Webview, generation: u64) {
    let hooked = view.clone();
    let _ = view.with_webview(move |platform| unsafe {
        let Ok(core) = platform.controller().CoreWebView2() else {
            return;
        };
        let handler = webview2_com::HistoryChangedEventHandler::create(Box::new(move |_, _| {
            sync_history(&hooked, generation);
            Ok(())
        }));
        let mut token = 0i64;
        let _ = core.add_HistoryChanged(&handler, &mut token);
    });
}

#[cfg(not(target_os = "windows"))]
fn watch_history(_view: &Webview, _generation: u64) {}

/// Open a tab's page — or reconnect to it after a reload of the suite page or
/// a remount of the browser window, which keep the webview by its label.
/// `visible: false` makes it off screen (a background tab, or a window that
/// shows its picture right now).
#[tauri::command(async)]
pub fn browser_tab_open(
    app: AppHandle,
    webview: Webview,
    label: String,
    url: String,
    bounds: Bounds,
    zoom: f64,
    visible: bool,
) -> Result<TabStatus, String> {
    guard(&webview)?;
    check_label(&label)?;
    let zoom = zoom_of(zoom);
    if let Some(view) = app.get_webview(&label) {
        if visible {
            place(&view, bounds)?;
            let _ = view.set_zoom(zoom);
            view.show().map_err(|e| e.to_string())?;
        } else {
            view.hide().map_err(|e| e.to_string())?;
        }
        let known = lock(&app.state::<BrowserTabs>().generation).get(&label).copied();
        match known {
            Some(generation) => sync_history(&view, generation),
            // A webview this run of the app did not make (it never happens
            // with the suite's own reload, which keeps the plugin state):
            // adopt it with a generation of its own.
            None => {
                let generation = app.state::<BrowserTabs>().next.fetch_add(1, Ordering::SeqCst) + 1;
                lock(&app.state::<BrowserTabs>().generation).insert(label.clone(), generation);
                let url = view.url().map(|u| u.to_string()).unwrap_or_default();
                emit(&app, &label, |s| s.url = url);
                sync_history(&view, generation);
            }
        }
        let status = lock(&app.state::<BrowserTabs>().status).get(&label).cloned();
        return Ok(status.unwrap_or_else(|| TabStatus { label: label.clone(), url: view.url().map(|u| u.to_string()).unwrap_or_default(), ..Default::default() }));
    }

    let start = parse(&url, &app)?;
    let state = app.state::<BrowserTabs>();
    let generation = state.next.fetch_add(1, Ordering::SeqCst) + 1;
    lock(&state.generation).insert(label.clone(), generation);
    lock(&state.status).insert(
        label.clone(),
        TabStatus { label: label.clone(), url: start.to_string(), title: start.host_str().unwrap_or_default().to_string(), loading: true, ..Default::default() },
    );

    let own = own_origin(&app);
    let (nav, opener, titles, loads) = (label.clone(), label.clone(), label.clone(), label.clone());
    let nav_app = app.clone();
    let opener_app = app.clone();
    let builder = tauri::webview::WebviewBuilder::new(&label, tauri::WebviewUrl::External(start))
        .zoom_hotkeys_enabled(false)
        .on_navigation(move |u| {
            if !current(&nav_app, &nav, generation) {
                return false;
            }
            if !allowed(u, own.as_ref()) {
                return false;
            }
            let url = u.to_string();
            emit(&nav_app, &nav, |s| {
                s.url = url;
                s.loading = true;
            });
            true
        })
        // A link that wants a window opens a tab here; a popup with a size
        // (a sign-in window) keeps its opener and opens as asked.
        .on_new_window(move |u, features| {
            if features.size().is_some() {
                return tauri::webview::NewWindowResponse::Allow;
            }
            if current(&opener_app, &opener, generation) && allowed(&u, own_origin(&opener_app).as_ref()) {
                let _ = opener_app.emit(NEW_TAB_EVENT, NewTab { label: opener.clone(), url: u.to_string() });
            }
            tauri::webview::NewWindowResponse::Deny
        })
        .on_document_title_changed(move |view, title| {
            if current(view.app_handle(), &titles, generation) {
                let title: String = title.chars().take(300).collect();
                emit(view.app_handle(), &titles, |s| s.title = title);
            }
        })
        .on_page_load(move |view, payload| {
            if !current(view.app_handle(), &loads, generation) {
                return;
            }
            let done = matches!(payload.event(), tauri::webview::PageLoadEvent::Finished);
            let url = payload.url().to_string();
            emit(view.app_handle(), &loads, |s| {
                s.url = url;
                s.loading = !done;
            });
            sync_history(&view, generation);
        });

    // A tab that is not to be seen yet is made off screen, so it never
    // flashes over the suite before the browser window hides it.
    let at = if visible { bounds } else { Bounds { x: -20000.0, y: -20000.0, ..bounds } };
    let view = webview
        .window()
        .add_child(builder, tauri::LogicalPosition::new(at.x, at.y), tauri::LogicalSize::new(at.width.max(100.0), at.height.max(100.0)))
        .map_err(|e| format!("the browser tab could not open: {e}"))?;
    let _ = view.set_zoom(zoom);
    if !visible {
        let _ = view.hide();
    }
    watch_history(&view, generation);
    #[cfg(target_os = "windows")]
    hook_fullscreen(&app, &view);

    let status = lock(&app.state::<BrowserTabs>().status).get(&label).cloned().unwrap_or_default();
    Ok(status)
}

/// Follow the browser window: where the live page goes, its zoom, or that it
/// steps aside for its picture.
#[tauri::command(async)]
pub fn browser_tab_place(app: AppHandle, webview: Webview, label: String, bounds: Bounds, zoom: f64, visible: bool) -> Result<(), String> {
    guard(&webview)?;
    check_label(&label)?;
    let Some(view) = app.get_webview(&label) else {
        return Ok(());
    };
    if visible {
        place(&view, bounds)?;
        let _ = view.set_zoom(zoom_of(zoom));
        view.show().map_err(|e| e.to_string())
    } else {
        view.hide().map_err(|e| e.to_string())
    }
}

/// Close a tab: its page, its media and its process go. Returns once the
/// webview is gone, so a browser window remounting at once makes a fresh one
/// instead of reconnecting to the dying one.
#[tauri::command(async)]
pub async fn browser_tab_close(app: AppHandle, webview: Webview, label: String) -> Result<(), String> {
    guard(&webview)?;
    check_label(&label)?;
    {
        let state = app.state::<BrowserTabs>();
        lock(&state.generation).remove(&label);
        lock(&state.status).remove(&label);
    }
    let Some(view) = app.get_webview(&label) else {
        return Ok(());
    };
    #[cfg(target_os = "windows")]
    lock(&FULLSCREEN_HOOKED).remove(&label);
    view.close().map_err(|e| e.to_string())?;
    let started = std::time::Instant::now();
    while app.get_webview(&label).is_some() && started.elapsed() < Duration::from_secs(3) {
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    Ok(())
}

#[tauri::command(async)]
pub fn browser_tab_navigate(app: AppHandle, webview: Webview, label: String, url: String) -> Result<(), String> {
    guard(&webview)?;
    check_label(&label)?;
    let view = tab(&app, &label)?;
    let target = parse(&url, &app)?;
    let text = target.to_string();
    emit(&app, &label, |s| {
        s.url = text;
        s.loading = true;
    });
    view.navigate(target).map_err(|e| e.to_string())
}

/// back · forward · reload · stop, through the page's real history.
#[tauri::command(async)]
pub fn browser_tab_action(app: AppHandle, webview: Webview, label: String, action: String) -> Result<(), String> {
    guard(&webview)?;
    check_label(&label)?;
    if !matches!(action.as_str(), "back" | "forward" | "reload" | "stop") {
        return Err(format!("unknown browser action '{action}'"));
    }
    let view = tab(&app, &label)?;
    let stop = action == "stop";
    #[cfg(target_os = "windows")]
    {
        view.with_webview(move |platform| unsafe {
            if let Ok(core) = platform.controller().CoreWebView2() {
                let _ = match action.as_str() {
                    "back" => core.GoBack(),
                    "forward" => core.GoForward(),
                    "reload" => core.Reload(),
                    _ => core.Stop(),
                };
            }
        })
        .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "windows"))]
    view.eval(match action.as_str() {
        "back" => "history.back()",
        "forward" => "history.forward()",
        "reload" => "location.reload()",
        _ => "window.stop()",
    })
    .map_err(|e| e.to_string())?;
    if stop {
        emit(&app, &label, |s| s.loading = false);
    }
    Ok(())
}

/// The visible page as JPEG — the picture that stands in for the live page
/// while something lies on top of it or the canvas moves.
#[cfg(target_os = "windows")]
#[tauri::command(async)]
pub async fn browser_tab_capture(app: AppHandle, webview: Webview, label: String) -> Result<tauri::ipc::Response, String> {
    use std::{cell::RefCell, rc::Rc};
    use webview2_com::{CapturePreviewCompletedHandler, Microsoft::Web::WebView2::Win32::COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_JPEG};
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::Com::{StructuredStorage::CreateStreamOnHGlobal, STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET};
    guard(&webview)?;
    check_label(&label)?;
    let view = tab(&app, &label)?;
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
                        return Err("no picture of the page".into());
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
            platform.controller().CoreWebView2()?.CapturePreview(COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_JPEG, &stream, &handler)
        })();
        if let Err(e) = started {
            if let Some(tx) = tx.borrow_mut().take() {
                let _ = tx.send(Err(e.to_string()));
            }
        }
    })
    .map_err(|e| e.to_string())?;
    let bytes = tokio::time::timeout(Duration::from_secs(5), rx)
        .await
        .map_err(|_| "the picture of the page timed out".to_string())?
        .map_err(|_| "the tab closed".to_string())??;
    Ok(tauri::ipc::Response::new(bytes))
}

#[cfg(not(target_os = "windows"))]
#[tauri::command(async)]
pub async fn browser_tab_capture(app: AppHandle, webview: Webview, label: String) -> Result<tauri::ipc::Response, String> {
    guard(&webview)?;
    check_label(&label)?;
    let _ = tab(&app, &label)?;
    Err("pictures of a page need WebView2 (Windows)".into())
}

/// Find in page: `window.find` in the tab (the find bar's next / previous).
#[tauri::command(async)]
pub fn find_in_browser(app: AppHandle, webview: Webview, label: String, query: String, forward: bool) -> Result<(), String> {
    guard(&webview)?;
    check_label(&label)?;
    let view = tab(&app, &label)?;
    let js = if query.is_empty() {
        "window.getSelection().removeAllRanges();".to_string()
    } else {
        format!(
            "window.find({}, false, {}, true, false, true, false);",
            serde_json::to_string(&query).map_err(|e| e.to_string())?,
            if forward { "false" } else { "true" }
        )
    };
    view.eval(&js).map_err(|e| e.to_string())
}

// ---- Window region: higher canvas windows cut out of a live page ----

#[derive(Deserialize)]
// Keep the same IPC payload on every platform; only Win32 clipping reads it.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub struct ClipRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Clip the tab's HWND to its visible region by subtracting obscuring
/// rectangles (higher canvas windows, the minimap, what lies outside the
/// canvas).
#[tauri::command(async)]
pub fn set_browser_clip_region(
    app: AppHandle,
    webview: Webview,
    label: String,
    full_width: i32,
    full_height: i32,
    obscuring_rects: Vec<ClipRect>,
) -> Result<(), String> {
    guard(&webview)?;
    check_label(&label)?;
    let view = tab(&app, &label)?;

    #[cfg(not(target_os = "windows"))]
    let _ = (&view, full_width, full_height, &obscuring_rects);

    #[cfg(target_os = "windows")]
    view.with_webview(move |wv| {
        unsafe { apply_window_region(wv, full_width, full_height, &obscuring_rects) };
    })
    .map_err(|e| format!("with_webview: {}", e))?;

    Ok(())
}

#[cfg(target_os = "windows")]
unsafe fn apply_window_region(wv: tauri::webview::PlatformWebview, full_width: i32, full_height: i32, obscuring_rects: &[ClipRect]) {
    let controller = wv.controller();
    let mut hwnd: isize = 0;
    let _ = controller.ParentWindow(&mut hwnd as *mut isize as *mut _);
    if hwnd == 0 {
        return;
    }

    if obscuring_rects.is_empty() {
        SetWindowRgn(hwnd, 0, 1);
    } else {
        let full_rgn = CreateRectRgn(0, 0, full_width, full_height);
        for r in obscuring_rects {
            let obs_rgn = CreateRectRgn(r.x, r.y, r.x + r.w, r.y + r.h);
            CombineRgn(full_rgn, full_rgn, obs_rgn, RGN_DIFF);
            DeleteObject(obs_rgn);
        }
        SetWindowRgn(hwnd, full_rgn, 1);
    }
}

#[cfg(target_os = "windows")]
const RGN_DIFF: i32 = 4;

#[cfg(target_os = "windows")]
#[link(name = "gdi32")]
extern "system" {
    fn CreateRectRgn(x1: i32, y1: i32, x2: i32, y2: i32) -> isize;
    fn CombineRgn(hrgnDst: isize, hrgnSrc1: isize, hrgnSrc2: isize, iMode: i32) -> i32;
    fn DeleteObject(ho: isize) -> i32;
}

#[cfg(target_os = "windows")]
#[link(name = "user32")]
extern "system" {
    fn SetWindowRgn(hWnd: isize, hRgn: isize, bRedraw: i32) -> i32;
}

// ---- Fullscreen video: inside the suite window, never the OS fullscreen ----

/// Tabs whose fullscreen handler is installed, label -> host HWND. The
/// handler dies with its webview; a rebuilt tab with the same label has
/// another HWND and gets its own.
#[cfg(target_os = "windows")]
static FULLSCREEN_HOOKED: Mutex<std::collections::BTreeMap<String, isize>> = Mutex::new(std::collections::BTreeMap::new());

/// WebView2's ContainsFullScreenElementChanged: the suite window stays as it
/// is and the browser window grows its page over the whole window instead.
#[cfg(target_os = "windows")]
fn hook_fullscreen(app: &AppHandle, view: &Webview) {
    let window = view.window();
    let label = view.label().to_string();
    let app = app.clone();
    lock(&FULLSCREEN_HOOKED).retain(|hooked, _| app.get_webview(hooked).is_some() || *hooked == label);
    let _ = view.with_webview(move |wv| unsafe {
        use webview2_com::ContainsFullScreenElementChangedEventHandler;
        let controller = wv.controller();
        let Ok(core) = controller.CoreWebView2() else {
            return;
        };
        let mut hwnd: isize = 0;
        let _ = controller.ParentWindow(&mut hwnd as *mut isize as *mut _);
        if lock(&FULLSCREEN_HOOKED).insert(label.clone(), hwnd) == Some(hwnd) {
            return;
        }
        let handler = ContainsFullScreenElementChangedEventHandler::create(Box::new(move |inner, _args| {
            let _ = window.set_fullscreen(false);
            let mut is_fullscreen = false;
            if let Some(ref wv) = inner {
                let mut val = windows::core::BOOL(0);
                if wv.ContainsFullScreenElement(&mut val).is_ok() {
                    is_fullscreen = val.as_bool();
                }
            }
            let _ = app.emit(FULLSCREEN_EVENT, serde_json::json!({ "label": label, "isFullscreen": is_fullscreen }));
            // A built-in handler may still fire after this one.
            let w = window.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(50));
                let _ = w.set_fullscreen(false);
            });
            Ok(())
        }));
        let mut token = 0i64;
        let _ = core.add_ContainsFullScreenElementChanged(&handler, &mut token);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_show_web_pages_never_the_suite_or_files() {
        let own: Url = "http://tauri.localhost/canvas".parse().unwrap();
        let ok = |u: &str| allowed(&u.parse().unwrap(), Some(&own));
        assert!(ok("https://example.com/a?b=1"));
        assert!(ok("http://localhost:3000/"));
        assert!(ok("about:blank"));
        assert!(ok("data:text/html,hi"));
        assert!(!ok("http://tauri.localhost/settings"));
        assert!(!ok("file:///C:/Windows/win.ini"));
        assert!(!ok("tauri://localhost/"));
        assert!(!ok("javascript:alert(1)"));
        assert!(!ok("ipc://localhost/"));
    }

    #[test]
    fn only_browser_labels_are_tabs() {
        assert!(check_label("browser-win_1-tab-17").is_ok());
        assert!(check_label("main").is_err());
        assert!(check_label("pilot-browser").is_err());
        assert!(check_label("browser-a b").is_err());
    }

    #[test]
    fn zoom_stays_in_range() {
        assert_eq!(zoom_of(0.5), 0.5);
        assert_eq!(zoom_of(0.01), 0.1);
        assert_eq!(zoom_of(f64::NAN), 1.0);
        assert_eq!(zoom_of(9.0), 5.0);
    }
}
