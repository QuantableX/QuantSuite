//! Self-healing for the overlay windows.
//!
//! Three ways the edge buttons have "disappeared" in the field, none of which
//! the occlusion flag in `init` covers:
//!
//! 1. **Minimised from outside.** An RDP reconnect with a changed resolution,
//!    Win+D, Explorer's "show desktop", Aero Shake — a minimised always-on-top
//!    window is simply gone, and nothing in this module ever restored one
//!    (diagnosed 2026-09-07: `QuantHUD` sat at -32000/-32000 with
//!    `WS_MINIMIZE` while `dual-right` was fine).
//! 2. **Display geometry changed under the window** — RDP resolution, DPI,
//!    a monitor added or removed. The pane kept its old edge coordinates.
//! 3. **The WebView2 browser process exited.** Every overlay goes blank at
//!    once, and `reload()` cannot revive a webview whose browser is gone; the
//!    log showed one of these per day, each followed by a manual restart.
//!
//! [`guard`] covers 1 and 2 per window, [`rebuild_overlays`] covers 3.

#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{Emitter, WebviewWindow, WindowEvent};
#[cfg(target_os = "windows")]
use tauri::Manager;

/// Event the page listens for; it re-applies its current tucked/expanded
/// geometry (`pages/hud/index.vue`, `reapplyGeometry`).
const RECOVER_EVENT: &str = "hud:recover";

/// Every window this module builds; destroyed together on a rebuild.
#[cfg(target_os = "windows")]
const OVERLAY_LABELS: [&str; 5] = [
    super::HUD_WINDOW,
    "dual-right",
    "region-selector",
    "color-picker-overlay",
    "screenshot-preview",
];

/// Attach the minimise guard and, on Windows, the display/session hooks.
pub(crate) fn guard(window: &WebviewWindow) {
    // Belt: a minimise that got through anyway (ShowWindow(SW_MINIMIZE) from
    // another process bypasses WM_SYSCOMMAND) shows up as a resize.
    let w = window.clone();
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Resized(_)) && w.is_minimized().unwrap_or(false) {
            recover(w.clone(), "minimised");
        }
        if matches!(event, WindowEvent::ScaleFactorChanged { .. }) {
            recover(w.clone(), "DPI changed");
        }
    });
    // Braces: refuse SC_MINIMIZE, and re-place on WM_DISPLAYCHANGE and
    // session (re)connect.
    #[cfg(target_os = "windows")]
    win32::subclass(window);
}

/// Un-minimise if needed, then ask the page to put the window back on its
/// edge. Spawned: callers are window-event and Win32 message handlers on the
/// main thread, which must not block or re-enter the window.
pub(crate) fn recover(window: WebviewWindow, why: &'static str) {
    tauri::async_runtime::spawn(async move {
        let label = window.label().to_string();
        if window.is_minimized().unwrap_or(false) {
            log::warn!("hud: '{label}' was minimised ({why}) — restoring it");
            if let Err(e) = window.unminimize() {
                log::error!("hud: unminimize '{label}' failed: {e}");
            }
        }
        log::info!("hud: '{label}' re-applying its geometry ({why})");
        if let Err(e) = window.emit_to(label.as_str(), RECOVER_EVENT, why) {
            log::error!("hud: could not tell '{label}' to recover: {e}");
        }
    });
}

/// Set while a rebuild is in flight: the browser process serves every overlay,
/// so its exit is reported once per window and must rebuild once.
#[cfg(target_os = "windows")]
pub(crate) struct RebuildState(AtomicBool);

#[cfg(target_os = "windows")]
impl Default for RebuildState {
    fn default() -> Self {
        Self(AtomicBool::new(false))
    }
}

/// Tear down every overlay window and build the primary one again; its page
/// recreates the dual pane. Off the main thread because `build()` deadlocks
/// there (see the note on `open_hud`).
#[cfg(target_os = "windows")]
pub(crate) fn rebuild_overlays(app: tauri::AppHandle, reason: String) {
    let Some(state) = app.try_state::<RebuildState>() else {
        return;
    };
    if state
        .0
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        log::info!("hud: rebuild already in progress ({reason})");
        return;
    }
    std::thread::spawn(move || {
        log::warn!("hud: rebuilding the overlay windows — {reason}");
        for label in OVERLAY_LABELS {
            if let Some(w) = app.get_webview_window(label) {
                if let Err(e) = w.destroy() {
                    log::warn!("hud: destroying '{label}' failed: {e}");
                }
            }
        }
        for _ in 0..60 {
            if OVERLAY_LABELS.iter().all(|l| app.get_webview_window(l).is_none()) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        // Give WebView2 a moment to finish tearing down the dead environment
        // before a fresh browser process is started for the new window.
        std::thread::sleep(std::time::Duration::from_millis(500));
        match super::create_hud_window(&app) {
            Ok(()) => log::info!("hud: overlay rebuilt — its page recreates the dual pane"),
            Err(e) => log::error!("hud: overlay rebuild failed: {e}"),
        }
        if let Some(state) = app.try_state::<RebuildState>() {
            state.0.store(false, Ordering::SeqCst);
        }
    });
}

#[cfg(target_os = "windows")]
mod win32 {
    use super::recover;
    use tauri::{Manager, WebviewWindow};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::RemoteDesktop::{
        WTSRegisterSessionNotification, WTSUnRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION,
    };
    use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::{
        SC_MINIMIZE, WM_DISPLAYCHANGE, WM_NCDESTROY, WM_SYSCOMMAND, WM_WTSSESSION_CHANGE,
    };

    /// "QHUD" — distinguishes this subclass from tao's own.
    const SUBCLASS_ID: usize = 0x5148_5544;

    // WM_WTSSESSION_CHANGE reason codes (wtsapi32.h).
    const WTS_CONSOLE_CONNECT: usize = 1;
    const WTS_REMOTE_CONNECT: usize = 3;
    const WTS_SESSION_LOGON: usize = 5;
    const WTS_SESSION_UNLOCK: usize = 8;

    /// Subclass the window's HWND. `SetWindowSubclass` only works from the
    /// thread that owns the window, so the call is marshalled to the main
    /// thread; the `WebviewWindow` handle travels as the subclass ref-data
    /// and is freed on `WM_NCDESTROY`.
    pub(super) fn subclass(window: &WebviewWindow) {
        let hwnd = match window.hwnd() {
            // Tauri's `windows` crate may differ from ours; the raw pointer
            // is the same either way.
            Ok(h) => h.0 as usize,
            Err(e) => {
                log::warn!("hud: no HWND for '{}': {e}", window.label());
                return;
            }
        };
        let label = window.label().to_string();
        let handle = window.clone();
        let queued = window.app_handle().run_on_main_thread(move || {
            let hwnd = HWND(hwnd as *mut std::ffi::c_void);
            let data = Box::into_raw(Box::new(handle)) as usize;
            // SAFETY: the HWND belongs to this thread; `data` is a valid
            // Box<WebviewWindow> that only `proc` reads and only
            // WM_NCDESTROY frees.
            let attached = unsafe { SetWindowSubclass(hwnd, Some(proc), SUBCLASS_ID, data) };
            if !attached.as_bool() {
                // SAFETY: the box was just leaked above and never handed out.
                drop(unsafe { Box::from_raw(data as *mut WebviewWindow) });
                log::warn!("hud: SetWindowSubclass failed for '{label}'");
                return;
            }
            // SAFETY: plain Win32 call on a window this thread owns.
            if let Err(e) = unsafe { WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) } {
                log::warn!("hud: session notifications unavailable for '{label}': {e}");
            }
        });
        if let Err(e) = queued {
            log::warn!("hud: could not subclass '{}': {e}", window.label());
        }
    }

    unsafe extern "system" fn proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        data: usize,
    ) -> LRESULT {
        if msg == WM_NCDESTROY {
            let _ = WTSUnRegisterSessionNotification(hwnd);
            let _ = RemoveWindowSubclass(hwnd, Some(proc), SUBCLASS_ID);
            drop(Box::from_raw(data as *mut WebviewWindow));
            return DefSubclassProc(hwnd, msg, wparam, lparam);
        }
        let window = &*(data as *const WebviewWindow);
        match msg {
            WM_SYSCOMMAND if (wparam.0 & 0xFFF0) == SC_MINIMIZE as usize => {
                log::info!("hud: '{}' refused a minimise request", window.label());
                return LRESULT(0);
            }
            WM_DISPLAYCHANGE => recover(window.clone(), "display changed"),
            WM_WTSSESSION_CHANGE => match wparam.0 {
                WTS_CONSOLE_CONNECT | WTS_REMOTE_CONNECT | WTS_SESSION_LOGON | WTS_SESSION_UNLOCK => {
                    recover(window.clone(), "session connected")
                }
                _ => {}
            },
            _ => {}
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }
}
