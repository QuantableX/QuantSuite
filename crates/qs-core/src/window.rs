//! Window lifecycle. QuantSuite is tray-resident: closing `main` hides it,
//! the process keeps running, and only Quit exits (ARCHITECTURE.md §10).
//!
//! `main` stays resident in the tray; additional `suite-*` windows share its
//! backend and data but have independent navigation. Closing an additional
//! window destroys that view without stopping the application.
//! QuantHUD also has windows — an always-on-top overlay and its
//! transparent helpers, which by their nature cannot live inside another
//! window. Those manage their own lifecycle in `tauri-plugin-hud`.
//!
//! The tray-hide pattern is lifted from QuantMCP, which already did this.

use crate::bus;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager, Runtime, WebviewWindowBuilder, WindowEvent};

/// The resident suite window, restored by the tray and second launches.
pub const MAIN: &str = "main";

/// Open another view in this application, reusing the main window's config.
/// Called from an async command: WebView2 creation must not block the UI thread.
pub fn open<R: Runtime>(app: &AppHandle<R>) -> Result<String, String> {
    let mut config = app
        .config()
        .app
        .windows
        .iter()
        .find(|config| config.label == MAIN)
        .cloned()
        .ok_or("Missing main window configuration")?;
    config.label = format!("suite-{}", uuid::Uuid::new_v4());
    config.visible = true;
    config.maximized = false;
    config.focus = true;
    let label = config.label.clone();
    WebviewWindowBuilder::from_config(app, &config)
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?;
    Ok(label)
}

/// Set only by [`crate::shutdown::quit`]. While false, a close request hides the
/// window instead of terminating the process.
static QUITTING: AtomicBool = AtomicBool::new(false);

pub fn is_quitting() -> bool {
    QUITTING.load(Ordering::SeqCst)
}

pub(crate) fn set_quitting() {
    QUITTING.store(true, Ordering::SeqCst);
}

/// Wire this into the Tauri builder: `.on_window_event(qs_core::window::on_event)`.
pub fn on_event<R: Runtime>(window: &tauri::Window<R>, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        if is_quitting() {
            return; // a real exit is in progress — let it close
        }

        // Closing the suite window hides it to the tray. It does not stop
        // bots, agents or sidecars: only Quit does that (ARCHITECTURE.md §10).
        //
        // Additional suite windows close normally. HUD overlays also own
        // their lifecycle and are left alone.
        if window.label() == MAIN {
            api.prevent_close();
            let _ = window.hide();
            let _ = bus::emit(
                &window.app_handle().clone(),
                "core.window.hidden",
                serde_json::json!({ "label": MAIN }),
            );
        }
    }
}

pub fn show<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        let _ = bus::emit(app, "core.window.shown", serde_json::json!({ "label": MAIN }));
    }
}

pub fn hide<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.hide();
        let _ = bus::emit(app, "core.window.hidden", serde_json::json!({ "label": MAIN }));
    }
}

pub fn toggle<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window(MAIN) {
        match w.is_visible() {
            Ok(true) => hide(app),
            _ => show(app),
        }
    }
}

// ── Window shape ────────────────────────────────────────────────────────

/// Clip the suite window to a circle, or restore it to a rectangle.
///
/// CSS `border-radius` only rounds what the page *paints*. The window is still
/// a rectangle: the corners outside the circle stay part of it, swallow clicks
/// that should reach the desktop, and — with `transparent: true` — read as an
/// invisible pane you cannot click through. QuantHUD hit this too and solved it
/// the same way, with a Win32 region on its notification popup.
///
/// `SetWindowRgn` changes the window's shape at the OS level. Outside the
/// region the window does not exist as far as Windows is concerned, so clicks
/// land on whatever is behind it.
///
/// Details that matter:
///
/// - `WS_THICKFRAME` and `WS_CAPTION` are stripped first. They give a
///   borderless window an *invisible* DWM resize border — measured at 14×8px
///   here — which sits outside the client area and would offset the circle from
///   where the page draws it. QuantHUD strips the same two styles on its
///   overlay for the same reason.
/// - The region is measured against the **window** rect, and Windows takes
///   ownership of it. Deleting it afterwards would clip the window with freed
///   memory.
/// - `diameter` is **logical** px (what the page lays out in); `GetWindowRect`
///   and regions are physical. Without the scale factor the circle is wrong on
///   every display not at 100%.
/// - The region is then inset 1px per side, so its hard edge lands on pixels
///   the disc actually painted. A regioned window loses clean alpha at the
///   region border: any strip between the painted disc and the region edge
///   composites as a white fringe — which is exactly the arc that showed along
///   the top, where the disc's drop shadow (dark, cast downwards) wasn't
///   covering it.
///
/// Everywhere except Windows this is a no-op: the page's `border-radius` is
/// then the only rounding, which is the pre-existing behaviour.
pub fn set_circular<R: Runtime>(app: &AppHandle<R>, diameter: Option<u32>) -> Result<(), String> {
    let Some(window) = app.get_webview_window(MAIN) else {
        return Err("no main window".into());
    };
    let _ = &window;
    let _ = diameter;

    #[cfg(target_os = "windows")]
    {
        use std::ffi::c_void;
        type Hwnd = *mut c_void;
        type Hrgn = *mut c_void;

        #[repr(C)]
        struct Rect {
            left: i32,
            top: i32,
            right: i32,
            bottom: i32,
        }

        extern "system" {
            fn GetWindowLongW(hwnd: Hwnd, index: i32) -> i32;
            fn SetWindowLongW(hwnd: Hwnd, index: i32, value: i32) -> i32;
            fn SetWindowPos(
                hwnd: Hwnd,
                after: Hwnd,
                x: i32,
                y: i32,
                cx: i32,
                cy: i32,
                flags: u32,
            ) -> i32;
            fn GetWindowRect(hwnd: Hwnd, rect: *mut Rect) -> i32;
            fn CreateEllipticRgn(x1: i32, y1: i32, x2: i32, y2: i32) -> Hrgn;
            fn SetWindowRgn(hwnd: Hwnd, rgn: Hrgn, redraw: i32) -> i32;
        }

        #[link(name = "dwmapi")]
        extern "system" {
            fn DwmSetWindowAttribute(
                hwnd: Hwnd,
                attribute: u32,
                value: *const c_void,
                size: u32,
            ) -> i32;
        }

        /// Per-window switch for DWM's own transition animations — the zoom
        /// Windows plays on maximize/restore. The shell morphs this window
        /// between the circle and fullscreen while driving its own iris
        /// animation; DWM zooming the rect at the same time reads as a twitch.
        /// Off for this window only; the system setting is untouched.
        const DWMWA_TRANSITIONS_FORCEDISABLED: u32 = 3;

        const GWL_STYLE: i32 = -16;
        const WS_THICKFRAME: i32 = 0x0004_0000;
        const WS_CAPTION: i32 = 0x00C0_0000;
        const SWP_NOMOVE: u32 = 0x0002;
        const SWP_NOSIZE: u32 = 0x0001;
        const SWP_NOZORDER: u32 = 0x0004;
        const SWP_NOACTIVATE: u32 = 0x0010;
        const SWP_FRAMECHANGED: u32 = 0x0020;

        let raw = window.hwnd().map_err(|e| e.to_string())?;
        let hwnd = raw.0 as Hwnd;

        unsafe {
            // Every shape change is bracketed by a maximize or unmaximize in
            // `useSuiteWindow.swap`, so this runs (idempotently) before any of
            // them can animate. Done here rather than at startup because the
            // window must already exist, and this command is the first thing
            // the shell calls on it.
            let disabled: i32 = 1; // Win32 BOOL
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_TRANSITIONS_FORCEDISABLED,
                &disabled as *const i32 as *const c_void,
                std::mem::size_of::<i32>() as u32,
            );

            let style = GetWindowLongW(hwnd, GWL_STYLE);
            let stripped = style & !WS_THICKFRAME & !WS_CAPTION;
            let restored = style | WS_THICKFRAME | WS_CAPTION;

            SetWindowLongW(
                hwnd,
                GWL_STYLE,
                if diameter.is_some() { stripped } else { restored },
            );
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );

            match diameter {
                // Centred in the window, so the circle stays put when the
                // window grows underneath it.
                Some(d) => {
                    let mut rect = Rect { left: 0, top: 0, right: 0, bottom: 0 };
                    if GetWindowRect(hwnd, &mut rect) == 0 {
                        return Err("GetWindowRect failed".into());
                    }
                    let w = rect.right - rect.left;
                    let h = rect.bottom - rect.top;
                    // Logical → physical, then 1px in from each side of the
                    // painted disc (see the doc comment: the fringe outside the
                    // paint composites white on a regioned window).
                    let scale = window.scale_factor().map_err(|e| e.to_string())?;
                    let d = ((d as f64) * scale).round() as i32 - 2;
                    let x = (w - d) / 2;
                    let y = (h - d) / 2;
                    // Windows takes ownership — do not delete this.
                    let rgn = CreateEllipticRgn(x, y, x + d, y + d);
                    SetWindowRgn(hwnd, rgn, 1);
                }
                // Null region = the window is its rectangle again.
                None => {
                    SetWindowRgn(hwnd, std::ptr::null_mut(), 1);
                }
            }
        }
    }

    Ok(())
}
