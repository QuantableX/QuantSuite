//! "Needs you" (docs/PLAN-QUANTPILOT.md, card 6): when a thread's agent
//! waits on an approval, a question or a plan, the taskbar flashes while
//! the suite is in the background, and — per the `notify` setting — a
//! Windows toast names the thread; clicking it brings the suite forward and
//! opens that thread (`pilot-open-thread`).
//!
//! Toasts go through `tauri-winrt-notification`, the wrapper
//! tauri-plugin-notification uses on Windows. A dev build has no registered
//! AppUserModelID, so it borrows PowerShell's (the plugin's own rule).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, UserAttentionType, Wry};
#[cfg(windows)]
use tauri::Emitter;

/// The webview event a toast click sends: `{ threadId }`.
#[cfg(windows)]
pub const OPEN_EVENT: &str = "pilot-open-thread";

/// One toast per thread in this window, however many requests arrive.
const THROTTLE: Duration = Duration::from_secs(5);

static LAST: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);

fn throttled(thread_id: &str) -> bool {
    let mut guard = LAST.lock().unwrap_or_else(|p| p.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    if map.get(thread_id).is_some_and(|t| t.elapsed() < THROTTLE) {
        return true;
    }
    map.insert(thread_id.to_string(), Instant::now());
    false
}

pub fn any_window_focused(app: &AppHandle<Wry>) -> bool {
    app.webview_windows().values().any(|w| w.is_focused().unwrap_or(false))
}

/// Whether a toast should show for this setting and focus.
pub fn should_toast(mode: &str, focused: bool) -> bool {
    match mode {
        "always" => true,
        "unfocused" => !focused,
        _ => false,
    }
}

#[cfg(windows)]
fn main_window(app: &AppHandle<Wry>) -> Option<tauri::WebviewWindow<Wry>> {
    app.get_webview_window("main").or_else(|| app.webview_windows().into_values().next())
}

/// Bring the suite forward and open the thread.
#[cfg(windows)]
pub fn open_thread(app: &AppHandle<Wry>, thread_id: &str) {
    if let Some(w) = main_window(app) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
    let _ = app.emit(OPEN_EVENT, serde_json::json!({ "threadId": thread_id }));
}

/// A thread started waiting on the user.
pub fn needs_you(app: &AppHandle<Wry>, mode: &str, thread_id: &str, title: &str, detail: &str) {
    let focused = any_window_focused(app);
    if !focused {
        for w in app.webview_windows().values() {
            let _ = w.request_user_attention(Some(UserAttentionType::Informational));
        }
    }
    if !should_toast(mode, focused) || throttled(thread_id) {
        return;
    }
    toast(app, thread_id, title, detail);
}

#[cfg(windows)]
fn toast(app: &AppHandle<Wry>, thread_id: &str, title: &str, detail: &str) {
    use tauri_winrt_notification::Toast;
    let app_id = if cfg!(debug_assertions) {
        Toast::POWERSHELL_APP_ID.to_string()
    } else {
        app.config().identifier.clone()
    };
    let heading = if title.trim().is_empty() { "QuantPilot".to_string() } else { format!("QuantPilot · {}", title.trim()) };
    let clicked = app.clone();
    let id = thread_id.to_string();
    let shown = Toast::new(&app_id)
        .title(&heading)
        .text1(detail)
        .on_activated(move |_| {
            open_thread(&clicked, &id);
            Ok(())
        })
        .show();
    if let Err(e) = shown {
        eprintln!("pilot: the toast could not be shown: {e}");
    }
}

#[cfg(not(windows))]
fn toast(_app: &AppHandle<Wry>, _thread_id: &str, _title: &str, _detail: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_setting_decides_with_focus() {
        assert!(!should_toast("off", false));
        assert!(should_toast("unfocused", false));
        assert!(!should_toast("unfocused", true));
        assert!(should_toast("always", true));
        assert!(!should_toast("nonsense", false));
    }

    #[test]
    fn one_toast_per_thread_in_the_window() {
        assert!(!throttled("t-throttle"));
        assert!(throttled("t-throttle"));
        assert!(!throttled("t-other"));
    }
}
