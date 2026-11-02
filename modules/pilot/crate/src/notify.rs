//! "Needs you" (docs/PLAN-QUANTPILOT.md, card 6): when a thread's agent
//! waits on an approval, a question or a plan, the taskbar flashes while
//! the suite is in the background, and — per the `notify` setting — a
//! Windows toast names the thread; clicking it brings the suite forward and
//! opens that thread (`pilot-open-thread`).
//!
//! Parity 20: the taskbar button carries a badge with how many threads wait
//! (Windows overlay icon, cleared at zero), and bringing the suite forward
//! after a flash opens the thread that asked — once, within 30 minutes.
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

/// The thread whose request flashed the taskbar, and when.
static ATTENTION: Mutex<Option<(String, Instant)>> = Mutex::new(None);
/// How long a flash still means "open that thread" when the suite comes forward.
const ATTENTION_FOR: Duration = Duration::from_secs(30 * 60);

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
        *ATTENTION.lock().unwrap_or_else(|p| p.into_inner()) = Some((thread_id.to_string(), Instant::now()));
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

/// The thread to open now that the suite came forward, if a flash asked for it.
pub fn take_attention() -> Option<String> {
    let pending = ATTENTION.lock().unwrap_or_else(|p| p.into_inner()).take();
    pending.filter(|(_, at)| at.elapsed() < ATTENTION_FOR).map(|(id, _)| id)
}

/// The main window got the focus (parity 20): open the thread that flashed.
pub fn activated(app: &AppHandle<Wry>) {
    if let Some(id) = take_attention() {
        use tauri::Emitter as _;
        let _ = app.emit("pilot-open-thread", serde_json::json!({ "threadId": id }));
    }
}

/// The taskbar badge: how many threads wait on the user; none at zero.
pub fn set_badge(app: &AppHandle<Wry>, count: u32) -> Result<(), String> {
    let Some(w) = app.get_webview_window("main") else { return Ok(()) };
    #[cfg(windows)]
    {
        let icon = (count > 0).then(|| {
            let (rgba, size) = badge(count);
            tauri::image::Image::new_owned(rgba, size, size)
        });
        w.set_overlay_icon(icon).map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        w.set_badge_count((count > 0).then_some(count as i64)).map_err(|e| e.to_string())
    }
}

/// 3×5 digits and a plus, row by row (three bits each).
const GLYPHS: [[u8; 5]; 11] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b010, 0b010, 0b010],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
    [0b000, 0b010, 0b111, 0b010, 0b000],
];

/// A 32×32 RGBA badge: an amber disc with the count in white ("9+" above nine).
pub fn badge(count: u32) -> (Vec<u8>, u32) {
    const S: u32 = 32;
    const SCALE: u32 = 4;
    let mut px = vec![0u8; (S * S * 4) as usize];
    let c = (S as f32 - 1.0) / 2.0;
    for y in 0..S {
        for x in 0..S {
            let d = ((x as f32 - c).powi(2) + (y as f32 - c).powi(2)).sqrt();
            let a = (15.8 - d).clamp(0.0, 1.0);
            if a > 0.0 {
                let i = ((y * S + x) * 4) as usize;
                px[i..i + 4].copy_from_slice(&[224, 150, 64, (a * 255.0) as u8]);
            }
        }
    }
    let glyphs: Vec<usize> = if count > 9 { vec![9, 10] } else { vec![count as usize] };
    let width = glyphs.len() as u32 * 3 * SCALE + (glyphs.len() as u32 - 1) * SCALE;
    let (x0, y0) = ((S - width) / 2, (S - 5 * SCALE) / 2);
    for (n, g) in glyphs.iter().enumerate() {
        let gx = x0 + n as u32 * 4 * SCALE;
        for (row, bits) in GLYPHS[*g].iter().enumerate() {
            for col in 0..3u32 {
                if bits >> (2 - col) & 1 == 1 {
                    for dy in 0..SCALE {
                        for dx in 0..SCALE {
                            let (x, y) = (gx + col * SCALE + dx, y0 + row as u32 * SCALE + dy);
                            let i = ((y * S + x) * 4) as usize;
                            px[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
                        }
                    }
                }
            }
        }
    }
    (px, S)
}

#[cfg(not(windows))]
fn toast(_app: &AppHandle<Wry>, _thread_id: &str, _title: &str, _detail: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flash_opens_its_thread_once_and_the_badge_is_drawn() {
        *ATTENTION.lock().unwrap() = Some(("t-1".into(), Instant::now()));
        assert_eq!(take_attention().as_deref(), Some("t-1"));
        assert_eq!(take_attention(), None, "only once");
        *ATTENTION.lock().unwrap() = Some(("t-2".into(), Instant::now() - ATTENTION_FOR - Duration::from_secs(1)));
        assert_eq!(take_attention(), None, "an old flash opens nothing");
        for n in [1, 7, 12] {
            let (px, s) = badge(n);
            assert_eq!((px.len(), s), (32 * 32 * 4, 32));
            let white = px.chunks(4).filter(|p| p == &[255, 255, 255, 255]).count();
            let amber = px.chunks(4).filter(|p| p[0] == 224 && p[3] > 0).count();
            assert!(white > 40 && amber > 400, "{n}: {white} white, {amber} amber");
            assert_eq!(&px[0..4], &[0, 0, 0, 0], "the corners stay transparent");
        }
        if let Ok(dir) = std::env::var("QP_BADGE_DIR") {
            for n in [1, 3, 12] {
                let (px, s) = badge(n);
                image::RgbaImage::from_raw(s, s, px).unwrap().save(format!("{dir}/badge-{n}.png")).unwrap();
            }
        }
    }

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
