mod capture;
mod config;
mod layout;
mod proximity;
mod resilience;
mod speech;
mod trigger_region;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, PhysicalPosition, WebviewWindow, Wry,
};

/// Physical work area, including taskbars on the top or either side.
fn get_work_area(monitor: &tauri::Monitor) -> layout::Frame {
    let scale_factor = monitor.scale_factor();
    let screen_height = monitor.size().height as i32;

    #[cfg(target_os = "windows")]
    {
        #[repr(C)]
        struct RECT { left: i32, top: i32, right: i32, bottom: i32 }

        #[repr(C)]
        struct POINT { x: i32, y: i32 }

        #[repr(C)]
        #[allow(non_snake_case)]
        struct MONITORINFO {
            cbSize: u32,
            rcMonitor: RECT,
            rcWork: RECT,
            dwFlags: u32,
        }

        extern "system" {
            fn MonitorFromPoint(pt: POINT, dwFlags: u32) -> *mut std::ffi::c_void;
            fn GetMonitorInfoW(hMonitor: *mut std::ffi::c_void, lpmi: *mut MONITORINFO) -> i32;
        }

        const MONITOR_DEFAULTTONEAREST: u32 = 0x0002;

        // Ask the monitor the caller actually targeted — SPI_GETWORKAREA only
        // ever reports the primary display, which is wrong as soon as a second
        // monitor has a different resolution or taskbar.
        let position = monitor.position();
        let size = monitor.size();
        let center = POINT {
            x: position.x + size.width as i32 / 2,
            y: position.y + size.height as i32 / 2,
        };

        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            rcMonitor: RECT { left: 0, top: 0, right: 0, bottom: 0 },
            rcWork: RECT { left: 0, top: 0, right: 0, bottom: 0 },
            dwFlags: 0,
        };

        let ok = unsafe {
            let handle = MonitorFromPoint(center, MONITOR_DEFAULTTONEAREST);
            GetMonitorInfoW(handle, &mut info)
        };
        if ok != 0 {
            // rcWork is already in physical pixels, same as monitor.size().
            let work_h = info.rcWork.bottom - info.rcWork.top;
            // Only use it when it looks sane (positive and smaller than full screen)
            if work_h > 0 && work_h <= screen_height {
                return layout::Frame {
                    x: info.rcWork.left, y: info.rcWork.top,
                    width: (info.rcWork.right - info.rcWork.left) as u32,
                    height: work_h as u32,
                };
            }
        }
    }

    // Fallback: assume 48 logical-px taskbar
    layout::Frame {
        x: monitor.position().x, y: monitor.position().y,
        width: monitor.size().width,
        height: (screen_height - (48.0 * scale_factor) as i32).max(1) as u32,
    }
}

// ── Overlay lifecycle helpers ────────────────────────────────────────────────
//
// The overlays are transparent, always-on-top WebView2 windows. When one of
// them stops painting, or its page never reaches `tuck_window`, there is
// nothing to see — the edge buttons are simply gone — and until now nothing
// was logged either, because a command's `Err` only ever went to the webview
// console. Everything below exists to make those cases (a) recover on their
// own and (b) leave a line in the suite log.

/// The monitor an overlay command targets: the configured index, else the
/// monitor the window is on, else the primary one, else the first enumerated.
///
/// Falls back instead of failing. Right after logon the monitor list can be
/// short or empty for a moment; an error here used to abort the page's whole
/// startup sequence before it reached `show()`, and the overlay then stayed
/// hidden until the next launch.
fn pick_monitor(
    app: &tauri::AppHandle,
    window: Option<&WebviewWindow>,
    monitor_index: Option<usize>,
) -> Result<tauri::Monitor, String> {
    let monitors = app.available_monitors().unwrap_or_default();
    if let Some(idx) = monitor_index {
        if let Some(m) = monitors.get(idx) {
            return Ok(m.clone());
        }
        log::warn!(
            "hud: monitor index {idx} not available ({} enumerated) — falling back",
            monitors.len()
        );
    }
    if let Some(Ok(Some(m))) = window.map(|w| w.current_monitor()) {
        return Ok(m);
    }
    if let Ok(Some(m)) = app.primary_monitor() {
        return Ok(m);
    }
    monitors.into_iter().next().ok_or_else(|| {
        log::error!("hud: no monitor found at all");
        "No monitor found".to_string()
    })
}

/// Labels of overlay windows whose page has placed them at least
/// once (`tuck_window` / `show_window`). Cleared when a window is (re)built,
/// so [`watch_first_reveal`] can tell "still booting" from "hidden by the
/// tray toggle or proximity" — both are `is_visible() == false`.
struct RevealState(std::sync::Mutex<std::collections::HashSet<String>>);

fn mark_revealed(window: &WebviewWindow) {
    if let Some(state) = window.app_handle().try_state::<RevealState>() {
        if let Ok(mut set) = state.0.lock() {
            set.insert(window.label().to_string());
        }
    }
}

fn clear_revealed(app: &tauri::AppHandle, label: &str) {
    if let Some(state) = app.try_state::<RevealState>() {
        if let Ok(mut set) = state.0.lock() {
            set.remove(label);
        }
    }
}

fn is_revealed(app: &tauri::AppHandle, label: &str) -> bool {
    app.try_state::<RevealState>()
        .and_then(|state| state.0.lock().ok().map(|set| set.contains(label)))
        .unwrap_or(true)
}

/// Self-heal for an overlay whose page never placed itself.
///
/// Every overlay is built hidden and placed by its own page before proximity
/// can reveal it. If that page dies before then — a failed command, a
/// script error, a webview that never finished booting under logon load —
/// the window exists, is hidden, and nothing ever retries. This waits a
/// generous 30s, then reloads the page once, and logs either way. A page that
/// simply took long is unaffected: it marks itself revealed and the check is a
/// no-op.
fn watch_first_reveal(app: tauri::AppHandle, label: &'static str) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(30));
        if is_revealed(&app, label) {
            return;
        }
        let Some(window) = app.get_webview_window(label) else {
            return;
        };
        log::warn!("hud: '{label}' has not placed itself 30s after creation — reloading its page");
        if let Err(e) = window.reload() {
            log::error!("hud: reloading '{label}' failed: {e}");
            return;
        }
        std::thread::sleep(std::time::Duration::from_secs(30));
        if !is_revealed(&app, label) && app.get_webview_window(label).is_some() {
            log::error!("hud: '{label}' still not revealed after a reload — check the webview console");
        }
    });
}

/// Reload an overlay whose renderer process died, rebuild every overlay when
/// the browser process itself is gone.
///
/// WebView2 does not recover a crashed render process on its own; the page
/// just goes blank, and blank in a transparent window is invisible. Nothing in
/// Tauri surfaces the event, so this hooks `ProcessFailed` on the underlying
/// CoreWebView2 and reloads. A browser-process exit takes every webview of the
/// shared environment with it and a reload cannot bring one back, so that case
/// destroys and recreates the windows (`resilience::rebuild_overlays`).
/// Unresponsive renderers are only logged — they usually come back, and a
/// reload would throw away whatever the user was doing.
fn watch_renderer(window: &WebviewWindow) {
    #[cfg(target_os = "windows")]
    {
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            COREWEBVIEW2_PROCESS_FAILED_KIND, COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
            COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
            COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE,
        };
        use webview2_com::ProcessFailedEventHandler;

        let label = window.label().to_string();
        let handle = window.clone();
        let result = window.with_webview(move |platform| {
            let inner_label = label.clone();
            let handler = ProcessFailedEventHandler::create(Box::new(move |_sender, args| {
                let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND(-1);
                if let Some(args) = args {
                    // SAFETY: plain COM getter on the event args WebView2 handed us.
                    let _ = unsafe { args.ProcessFailedKind(&mut kind) };
                }
                if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED {
                    resilience::rebuild_overlays(
                        handle.app_handle().clone(),
                        format!("browser process of '{inner_label}' exited"),
                    );
                } else if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED {
                    log::warn!("hud: renderer of '{inner_label}' exited — reloading the page");
                    if let Err(e) = handle.reload() {
                        log::error!("hud: reloading '{inner_label}' failed: {e}");
                    }
                } else if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE {
                    log::warn!("hud: renderer of '{inner_label}' is unresponsive");
                } else {
                    log::warn!("hud: webview process failure in '{inner_label}' (kind {})", kind.0);
                }
                Ok(())
            }));
            let mut token = 0i64;
            // SAFETY: COM calls on the live controller Tauri handed to this closure,
            // on the thread that owns it.
            let attached = unsafe {
                platform
                    .controller()
                    .CoreWebView2()
                    .and_then(|core| core.add_ProcessFailed(&handler, &mut token))
            };
            if let Err(e) = attached {
                log::warn!("hud: could not hook ProcessFailed on '{label}': {e}");
            }
        });
        if let Err(e) = result {
            log::warn!("hud: with_webview failed for '{}': {e}", window.label());
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = window;
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CaptureResult {
    pub image_base64: String,
    pub width: u32,
    pub height: u32,
}

/// Capture screen and return as base64 PNG for frontend OCR processing
#[tauri::command]
async fn capture_screen(region: Option<[i32; 4]>, default_crop: Option<bool>) -> Result<CaptureResult, String> {
    let crop = default_crop.unwrap_or(true);
    // Capture, PNG encode and base64 are hundreds of ms of CPU — running them
    // inline would park an async-runtime worker for the whole time.
    let (base64_data, width, height) = tauri::async_runtime::spawn_blocking(move || {
        capture::capture_screen_base64(region, crop)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    Ok(CaptureResult {
        image_base64: base64_data,
        width,
        height,
    })
}

const CONTENT_WIDTH: i32 = 320; // Width of main content area
const TRIGGER_WIDTH: i32 = 20;  // Width of the rounded edge tab
const TOTAL_WIDTH: i32 = CONTENT_WIDTH + TRIGGER_WIDTH; // 340px
/// Tuck to the rounded tab at the selected edge.
#[tauri::command]
async fn tuck_window(window: WebviewWindow, position: String, monitor_index: Option<usize>) -> Result<(), String> {
    place_overlay(window, position, monitor_index, false)
}

/// Expand the side panel or the centered top panel.
#[tauri::command]
async fn show_window(window: WebviewWindow, position: String, monitor_index: Option<usize>) -> Result<(), String> {
    place_overlay(window, position, monitor_index, true)
}

fn place_overlay(window: WebviewWindow, position: String, monitor_index: Option<usize>, expanded: bool) -> Result<(), String> {
    let edge = layout::Edge::parse(&position)?;
    let monitor = pick_monitor(window.app_handle(), Some(&window), monitor_index)?;
    let scale = monitor.scale_factor();
    let frame = layout::frame(get_work_area(&monitor), scale, edge, expanded);
    proximity::pause(&window)?;
    // Tuck before moving; expand after moving so it stays on its monitor.
    if !expanded {
        window.set_size(tauri::PhysicalSize::new(frame.width, frame.height)).map_err(|e| e.to_string())?;
    }
    window.set_position(PhysicalPosition::new(frame.x, frame.y)).map_err(|e| e.to_string())?;
    if expanded {
        window.set_size(tauri::PhysicalSize::new(frame.width, frame.height)).map_err(|e| e.to_string())?;
    }
    trigger_region::apply(&window, scale, edge, expanded)?;
    if expanded { proximity::expanded(&window)?; }
    else { proximity::tucked(&window, &monitor)?; }
    // Correct placement also counts while proximity intentionally hides it.
    mark_revealed(&window);
    Ok(())
}

/// Check if window is tucked
#[tauri::command]
async fn is_window_tucked(window: WebviewWindow) -> Result<bool, String> {
    let monitor = window.current_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("No monitor found")?;
    let scale_factor = monitor.scale_factor();
    let size = window.outer_size().map_err(|e| e.to_string())?;
    let depth = (TRIGGER_WIDTH as f64 * scale_factor).round() as u32;
    Ok(size.width <= depth || size.height <= depth)
}

/// Setup window for full height (call on startup)
#[tauri::command]
async fn setup_window_size(window: WebviewWindow, monitor_index: Option<usize>) -> Result<(), String> {
    use tauri::PhysicalSize;

    // Get monitor work area
    let monitor = pick_monitor(window.app_handle(), Some(&window), monitor_index)?;
    proximity::pause(&window)?;

    let scale_factor = monitor.scale_factor();
    let window_height = get_work_area(&monitor).height as i32;
    let window_width = (TOTAL_WIDTH as f64 * scale_factor) as u32;

    window.set_size(PhysicalSize::new(window_width, window_height as u32))
        .map_err(|e| e.to_string())?;

    // Position window at left edge by default
    let monitor_x = monitor.position().x;
    let monitor_y = monitor.position().y;

    window.set_position(PhysicalPosition::new(monitor_x, monitor_y))
        .map_err(|e| e.to_string())?;

    // Deliberately no `show()` here: this always parks the window full-width at
    // the LEFT edge, and the page calls `tuck_window` right after — which snaps
    // it to the configured edge and enables proximity. Showing already would flash a
    // full-width panel (on the wrong side, for right-edge users).

    Ok(())
}

/// Move to another edge, preserving tucked/expanded state.
#[tauri::command]
async fn set_window_position(window: WebviewWindow, position: String, monitor_index: Option<usize>) -> Result<(), String> {
    if is_window_tucked(window.clone()).await? {
        tuck_window(window, position, monitor_index).await
    } else {
        show_window(window, position, monitor_index).await
    }
}

#[derive(Debug, Serialize)]
pub struct MonitorInfo {
    pub index: usize,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
}

/// Get list of available monitors
#[tauri::command]
async fn get_available_monitors(app: tauri::AppHandle) -> Result<Vec<MonitorInfo>, String> {
    let monitors = app.available_monitors()
        .map_err(|e| e.to_string())?;

    let monitor_list: Vec<MonitorInfo> = monitors
        .into_iter()
        .enumerate()
        .map(|(index, monitor)| {
            let size = monitor.size();
            let _raw_name = monitor.name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("Display {}", index + 1));

            // Use a simple sequential "Display N" format with resolution
            let display_name = format!("Display {} ({}×{})", index + 1, size.width, size.height);

            MonitorInfo {
                index,
                name: display_name,
                width: size.width,
                height: size.height,
                is_primary: index == 0, // First monitor is typically primary
            }
        })
        .collect();

    Ok(monitor_list)
}

#[derive(Debug, Serialize)]
pub struct CursorPosition {
    pub x: i32,
    pub y: i32,
}

#[tauri::command]
async fn get_cursor_position() -> Result<CursorPosition, String> {
    // Use Windows API to get cursor position
    #[cfg(target_os = "windows")]
    {
        use std::mem::MaybeUninit;

        #[repr(C)]
        struct POINT {
            x: i32,
            y: i32,
        }

        extern "system" {
            fn GetCursorPos(lpPoint: *mut POINT) -> i32;
        }

        let mut point = MaybeUninit::<POINT>::uninit();
        let result = unsafe { GetCursorPos(point.as_mut_ptr()) };

        if result != 0 {
            let point = unsafe { point.assume_init() };
            return Ok(CursorPosition { x: point.x, y: point.y });
        }
    }

    Err("Failed to get cursor position".to_string())
}

/// Pick the screen color at the current cursor position (Windows only).
/// Hides the overlay window first so GetPixel reads the real desktop.
#[tauri::command]
async fn pick_screen_color(app: tauri::AppHandle) -> Result<String, String> {
    // Hide the overlay from Rust so timing is deterministic
    if let Some(overlay) = app.get_webview_window("color-picker-overlay") {
        let _ = overlay.hide();
    }

    // The compositor wait and the GDI reads block — keep them off the async
    // runtime's workers.
    tauri::async_runtime::spawn_blocking(read_pixel_under_cursor)
        .await
        .map_err(|e| e.to_string())?
}

fn read_pixel_under_cursor() -> Result<String, String> {
    // Wait for the compositor to fully remove the overlay surface
    std::thread::sleep(std::time::Duration::from_millis(150));

    #[cfg(target_os = "windows")]
    {
        use std::mem::MaybeUninit;

        #[repr(C)]
        struct POINT { x: i32, y: i32 }

        type HDC = *mut std::ffi::c_void;
        type HWND = *mut std::ffi::c_void;
        type COLORREF = u32;

        extern "system" {
            fn GetCursorPos(lpPoint: *mut POINT) -> i32;
            fn GetDC(hWnd: HWND) -> HDC;
            fn ReleaseDC(hWnd: HWND, hDC: HDC) -> i32;
            fn GetPixel(hdc: HDC, x: i32, y: i32) -> COLORREF;
        }

        let mut pt = MaybeUninit::<POINT>::uninit();
        if unsafe { GetCursorPos(pt.as_mut_ptr()) } == 0 {
            return Err("Failed to get cursor position".to_string());
        }
        let pt = unsafe { pt.assume_init() };

        let hdc = unsafe { GetDC(std::ptr::null_mut()) };
        if hdc.is_null() {
            return Err("Failed to get screen DC".to_string());
        }

        let color = unsafe { GetPixel(hdc, pt.x, pt.y) };
        unsafe { ReleaseDC(std::ptr::null_mut(), hdc) };

        if color == 0xFFFFFFFF {
            return Err(format!("GetPixel failed at ({}, {})", pt.x, pt.y));
        }

        let r = color & 0xFF;
        let g = (color >> 8) & 0xFF;
        let b = (color >> 16) & 0xFF;
        Ok(format!("#{:02x}{:02x}{:02x}", r, g, b))
    }

    #[cfg(not(target_os = "windows"))]
    Err("pick_screen_color is only supported on Windows".to_string())
}

#[derive(Debug, Serialize)]
pub struct OsScreenshot {
    pub path: String,
    pub filename: String,
    pub modified: u64,
}

/// Open a native folder picker dialog and return the selected path
#[tauri::command]
async fn pick_folder(default_path: Option<String>) -> Result<Option<String>, String> {
    // The dialog blocks for as long as the user takes to answer it.
    tauri::async_runtime::spawn_blocking(move || {
        let mut dialog = rfd::FileDialog::new();
        if let Some(ref path) = default_path {
            if !path.is_empty() {
                dialog = dialog.set_directory(path);
            }
        }
        dialog
            .pick_folder()
            .map(|path| path.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())
}

/// Open a native file picker dialog and return the selected file path
#[tauri::command]
async fn pick_file(default_path: Option<String>) -> Result<Option<String>, String> {
    // The dialog blocks for as long as the user takes to answer it.
    tauri::async_runtime::spawn_blocking(move || {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Executables", &["exe", "lnk", "bat", "cmd"])
            .add_filter("All Files", &["*"]);
        if let Some(ref path) = default_path {
            if !path.is_empty() {
                dialog = dialog.set_directory(path);
            }
        }
        dialog
            .pick_file()
            .map(|path| path.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())
}

/// Launch a local application or file using the OS shell
#[tauri::command]
async fn launch_app(path: String) -> Result<(), String> {
    use std::process::Command;
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        Command::new("cmd")
            .args(["/C", "start", "", &path])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Failed to launch app: {}", e))?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to launch app: {}", e))?;
    }
    Ok(())
}

/// Extract the icon from an executable or .lnk shortcut as a base64 PNG data URL
#[tauri::command]
async fn get_app_icon(path: String) -> Result<Option<String>, String> {
    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Graphics::Gdi::{
            CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP,
            BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS,
        };
        use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL;
        use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
        use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, ICONINFO};

        let wide: Vec<u16> = OsStr::new(&path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let mut file_info: SHFILEINFOW = std::mem::zeroed();
            let result = SHGetFileInfoW(
                PCWSTR(wide.as_ptr()),
                FILE_ATTRIBUTE_NORMAL,
                Some(&mut file_info),
                std::mem::size_of::<SHFILEINFOW>() as u32,
                SHGFI_ICON | SHGFI_LARGEICON,
            );

            if result == 0 || file_info.hIcon.is_invalid() {
                return Ok(None);
            }

            let hicon = file_info.hIcon;

            let mut info = ICONINFO::default();
            if GetIconInfo(hicon, &mut info).is_err() {
                let _ = DestroyIcon(hicon);
                return Ok(None);
            }

            let hbm_color = info.hbmColor;
            let hbm_mask = info.hbmMask;

            let mut bmp = BITMAP::default();
            GetObjectW(
                hbm_color.into(),
                std::mem::size_of::<BITMAP>() as i32,
                Some(&mut bmp as *mut _ as *mut _),
            );

            let w = bmp.bmWidth as u32;
            let h = bmp.bmHeight as u32;
            if w == 0 || h == 0 {
                let _ = DeleteObject(hbm_color.into());
                let _ = DeleteObject(hbm_mask.into());
                let _ = DestroyIcon(hicon);
                return Ok(None);
            }

            let hdc = CreateCompatibleDC(None);
            let mut bi: BITMAPINFO = std::mem::zeroed();
            bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bi.bmiHeader.biWidth = w as i32;
            bi.bmiHeader.biHeight = -(h as i32);
            bi.bmiHeader.biPlanes = 1;
            bi.bmiHeader.biBitCount = 32;

            let mut pixels = vec![0u8; (w * h * 4) as usize];
            GetDIBits(
                hdc,
                hbm_color,
                0,
                h,
                Some(pixels.as_mut_ptr() as *mut _),
                &mut bi,
                DIB_RGB_COLORS,
            );

            // BGRA → RGBA
            for chunk in pixels.chunks_exact_mut(4) {
                chunk.swap(0, 2);
            }

            let _ = DeleteDC(hdc);
            let _ = DeleteObject(hbm_color.into());
            let _ = DeleteObject(hbm_mask.into());
            let _ = DestroyIcon(hicon);

            let img = image::RgbaImage::from_raw(w, h, pixels)
                .ok_or("Failed to create image from icon data")?;
            let mut png_bytes = Vec::new();
            use image::ImageEncoder;
            image::codecs::png::PngEncoder::new(&mut png_bytes)
                .write_image(&img, w, h, image::ExtendedColorType::Rgba8)
                .map_err(|e| format!("PNG encode error: {}", e))?;

            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
            Ok(Some(format!("data:image/png;base64,{}", b64)))
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
        Ok(None)
    }
}

/// Get the default screenshots folder path
#[tauri::command]
async fn get_default_screenshots_folder() -> Result<String, String> {
    let pictures = dirs::picture_dir().ok_or("Cannot find Pictures directory")?;
    let ss_dir = pictures.join("Screenshots");
    Ok(ss_dir.to_string_lossy().to_string())
}

/// List screenshots from the OS screenshots folder (or custom folder)
#[tauri::command]
async fn list_os_screenshots(custom_folder: Option<String>) -> Result<Vec<OsScreenshot>, String> {
    let ss_dir = if let Some(ref folder) = custom_folder {
        if !folder.is_empty() {
            std::path::PathBuf::from(folder)
        } else {
            let pictures = dirs::picture_dir().ok_or("Cannot find Pictures directory")?;
            pictures.join("Screenshots")
        }
    } else {
        let pictures = dirs::picture_dir().ok_or("Cannot find Pictures directory")?;
        pictures.join("Screenshots")
    };
    if !ss_dir.exists() {
        return Ok(vec![]);
    }
    let mut results = Vec::new();
    let entries = std::fs::read_dir(&ss_dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext_lower = ext.to_lowercase();
            if ext_lower == "png" || ext_lower == "jpg" || ext_lower == "jpeg" || ext_lower == "bmp" {
                let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                let modified = entry.metadata()
                    .and_then(|m| m.modified())
                    .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())
                    .unwrap_or(0);
                results.push(OsScreenshot {
                    path: path.to_string_lossy().to_string(),
                    filename,
                    modified,
                });
            }
        }
    }
    results.sort_by_key(|entry| std::cmp::Reverse(entry.modified));
    if results.len() > 50 { results.truncate(50); }
    Ok(results)
}

/// Read a screenshot file and return as base64
#[tauri::command]
async fn read_screenshot_file(path: String) -> Result<String, String> {
    let data = std::fs::read(&path).map_err(|e| format!("Failed to read file: {}", e))?;
    Ok(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &data))
}

/// Read a screenshot and return a small thumbnail as base64 PNG
#[tauri::command]
async fn read_screenshot_thumbnail(path: String, max_width: u32) -> Result<String, String> {
    // Decode + rescale + encode is CPU work, not async work.
    tauri::async_runtime::spawn_blocking(move || {
        let img = image::open(&path).map_err(|e| format!("Failed to open image: {}", e))?;
        let thumb = img.thumbnail(max_width, max_width);
        let mut buf = std::io::Cursor::new(Vec::new());
        thumb.write_to(&mut buf, image::ImageFormat::Png)
            .map_err(|e| format!("Failed to encode thumbnail: {}", e))?;
        Ok(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, buf.into_inner()))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Open the OS screenshots folder in file explorer
#[tauri::command]
async fn open_screenshots_folder(custom_folder: Option<String>) -> Result<(), String> {
    let ss_dir = if let Some(ref folder) = custom_folder {
        if !folder.is_empty() {
            std::path::PathBuf::from(folder)
        } else {
            let pictures = dirs::picture_dir().ok_or("Cannot find Pictures directory")?;
            pictures.join("Screenshots")
        }
    } else {
        let pictures = dirs::picture_dir().ok_or("Cannot find Pictures directory")?;
        pictures.join("Screenshots")
    };
    if !ss_dir.exists() {
        std::fs::create_dir_all(&ss_dir).map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(ss_dir.to_string_lossy().to_string())
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }
    Ok(())
}

/// Copy a screenshot image to the system clipboard (actual image, not text)
#[tauri::command]
#[allow(unused_variables)]
async fn copy_screenshot_to_clipboard(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let img = image::open(&path).map_err(|e| format!("Failed to open image: {}", e))?;
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());

        // Convert RGBA to BGRA for Windows DIB
        let mut bgra_pixels: Vec<u8> = Vec::with_capacity((w * h * 4) as usize);
        for pixel in rgba.pixels() {
            bgra_pixels.push(pixel[2]); // B
            bgra_pixels.push(pixel[1]); // G
            bgra_pixels.push(pixel[0]); // R
            bgra_pixels.push(pixel[3]); // A
        }

        // Windows DIB is bottom-up, flip rows
        let row_bytes = (w * 4) as usize;
        let mut flipped = vec![0u8; bgra_pixels.len()];
        for y in 0..h as usize {
            let src_start = y * row_bytes;
            let dst_start = ((h as usize) - 1 - y) * row_bytes;
            flipped[dst_start..dst_start + row_bytes]
                .copy_from_slice(&bgra_pixels[src_start..src_start + row_bytes]);
        }

        type HWND = *mut std::ffi::c_void;
        type HANDLE = *mut std::ffi::c_void;
        type UINT = u32;
        type BOOL = i32;

        #[repr(C)]
        #[allow(non_snake_case)]
        struct BITMAPINFOHEADER {
            biSize: u32,
            biWidth: i32,
            biHeight: i32,
            biPlanes: u16,
            biBitCount: u16,
            biCompression: u32,
            biSizeImage: u32,
            biXPelsPerMeter: i32,
            biYPelsPerMeter: i32,
            biClrUsed: u32,
            biClrImportant: u32,
        }

        const CF_DIB: UINT = 8;
        const GMEM_MOVEABLE: UINT = 0x0002;

        extern "system" {
            fn OpenClipboard(hWnd: HWND) -> BOOL;
            fn CloseClipboard() -> BOOL;
            fn EmptyClipboard() -> BOOL;
            fn SetClipboardData(uFormat: UINT, hMem: HANDLE) -> HANDLE;
            fn GlobalAlloc(uFlags: UINT, dwBytes: usize) -> HANDLE;
            fn GlobalLock(hMem: HANDLE) -> *mut u8;
            fn GlobalUnlock(hMem: HANDLE) -> BOOL;
        }

        let header_size = std::mem::size_of::<BITMAPINFOHEADER>();
        let data_size = flipped.len();
        let total_size = header_size + data_size;

        unsafe {
            if OpenClipboard(std::ptr::null_mut()) == 0 {
                return Err("Failed to open clipboard".into());
            }
            EmptyClipboard();

            let hmem = GlobalAlloc(GMEM_MOVEABLE, total_size);
            if hmem.is_null() {
                CloseClipboard();
                return Err("Failed to allocate global memory".into());
            }

            let ptr = GlobalLock(hmem);
            if ptr.is_null() {
                CloseClipboard();
                return Err("Failed to lock global memory".into());
            }

            let header = BITMAPINFOHEADER {
                biSize: header_size as u32,
                biWidth: w as i32,
                biHeight: h as i32, // positive = bottom-up
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0, // BI_RGB
                biSizeImage: data_size as u32,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            };

            std::ptr::copy_nonoverlapping(
                &header as *const BITMAPINFOHEADER as *const u8,
                ptr,
                header_size,
            );
            std::ptr::copy_nonoverlapping(
                flipped.as_ptr(),
                ptr.add(header_size),
                data_size,
            );

            GlobalUnlock(hmem);

            if SetClipboardData(CF_DIB, hmem).is_null() {
                CloseClipboard();
                return Err("Failed to set clipboard data".into());
            }

            CloseClipboard();
        }

        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    Err("copy_screenshot_to_clipboard is only supported on Windows".to_string())
}

#[tauri::command]
async fn load_config() -> Result<String, String> {
    config::load_config().map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_config(config: String) -> Result<(), String> {
    config::save_config(&config).map_err(|e| e.to_string())
}

use speech::lock;
use std::sync::Mutex;
use tauri::State;

// Global state for selected region
struct RegionState(Mutex<Option<[i32; 4]>>);

// Global state for picked color
struct PickedColorState(Mutex<Option<Option<String>>>);

// Global state for screenshot preview path
struct ScreenshotPreviewState(Mutex<Option<String>>);

/// Open fullscreen transparent region selector window
#[tauri::command]
async fn open_region_selector(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    // `/hud/…`, not `/…`: module pages are namespaced under the module id so
    // Nuxt layers cannot collide on merge. A bare `/region-selector` loads
    // nothing — the window opens transparent and empty, with no error.
    let window = WebviewWindowBuilder::new(&app, "region-selector", WebviewUrl::App("/hud/region-selector".into()))
        .title("Select Region")
        .fullscreen(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .transparent(true)
        .visible(false)
        .build()
        .map_err(|e| format!("Failed to build window: {}", e))?;

    // Show window after a tiny delay to ensure content is rendered
    let window_clone = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        let _ = window_clone.show();
        let _ = window_clone.set_focus();
    });

    Ok(())
}

/// Save selected region and close selector
#[tauri::command]
async fn set_selected_region(
    app: tauri::AppHandle,
    state: State<'_, RegionState>,
    region: Option<[i32; 4]>
) -> Result<(), String> {
    *lock(&state.0) = region;

    if let Some(window) = app.get_webview_window("region-selector") {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Get selected region (called by main window)
#[tauri::command]
async fn get_selected_region(state: State<'_, RegionState>) -> Result<Option<[i32; 4]>, String> {
    Ok(lock(&state.0).take())
}

/// Open transparent color picker overlay window spanning ALL monitors
#[tauri::command]
async fn open_color_picker_overlay(app: tauri::AppHandle, state: State<'_, PickedColorState>) -> Result<(), String> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    // Clear any stale result from a previous pick
    *lock(&state.0) = None;

    // Compute bounding box of the entire virtual desktop (all monitors)
    let monitors = app.available_monitors().map_err(|e| e.to_string())?;
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    // Track primary monitor position relative to virtual desktop origin
    let mut primary_left: i32 = 0;
    let mut primary_w: u32 = 0;
    if let Some(pm) = app.primary_monitor().map_err(|e| e.to_string())? {
        primary_left = pm.position().x;
        primary_w = pm.size().width;
    }
    for m in &monitors {
        let pos = m.position();
        let size = m.size();
        min_x = min_x.min(pos.x);
        min_y = min_y.min(pos.y);
        max_x = max_x.max(pos.x + size.width as i32);
        max_y = max_y.max(pos.y + size.height as i32);
    }
    let virt_w = (max_x - min_x) as u32;
    let virt_h = (max_y - min_y) as u32;

    // Pass primary monitor offset (relative to virtual desktop origin) as query
    // params so the overlay can center the instructions on the primary screen.
    let primary_offset_x = primary_left - min_x; // px from left edge of overlay
    let url = format!(
        "/hud/color-picker-overlay?pmx={}&pmw={}",
        primary_offset_x, primary_w
    );

    let window = WebviewWindowBuilder::new(&app, "color-picker-overlay", WebviewUrl::App(url.into()))
        .title("Pick Color")
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .transparent(true)
        .visible(false)
        .build()
        .map_err(|e| format!("Failed to build window: {}", e))?;

    // Strip WS_THICKFRAME and WS_CAPTION so the DWM invisible border is
    // removed, then use SetWindowPos to place the window at exact physical
    // pixel coordinates with zero deadspace.
    #[cfg(target_os = "windows")]
    {
        use std::ffi::c_void;
        type HWND = *mut c_void;

        extern "system" {
            fn GetWindowLongW(hWnd: HWND, nIndex: i32) -> i32;
            fn SetWindowLongW(hWnd: HWND, nIndex: i32, dwNewLong: i32) -> i32;
            fn SetWindowPos(
                hWnd: HWND,
                hWndInsertAfter: HWND,
                x: i32, y: i32, cx: i32, cy: i32,
                uFlags: u32,
            ) -> i32;
        }

        const GWL_STYLE: i32 = -16;
        const WS_THICKFRAME: i32 = 0x00040000;
        const WS_CAPTION: i32 = 0x00C00000;
        const HWND_TOPMOST: isize = -1;
        const SWP_FRAMECHANGED: u32 = 0x0020;
        const SWP_NOACTIVATE: u32 = 0x0010;

        if let Ok(raw) = window.hwnd() {
            let hwnd = raw.0 as HWND;
            unsafe {
                // Remove the styles that cause the invisible DWM border
                let style = GetWindowLongW(hwnd, GWL_STYLE);
                SetWindowLongW(hwnd, GWL_STYLE, style & !WS_THICKFRAME & !WS_CAPTION);

                // Now position — SWP_FRAMECHANGED forces Windows to re-apply
                // the new style so the border is truly gone.
                SetWindowPos(
                    hwnd,
                    HWND_TOPMOST as HWND,
                    min_x, min_y,
                    virt_w as i32, virt_h as i32,
                    SWP_FRAMECHANGED | SWP_NOACTIVATE,
                );
            }
        }
    }

    // Fallback for non-Windows
    #[cfg(not(target_os = "windows"))]
    {
        window.set_size(tauri::PhysicalSize::new(virt_w, virt_h)).map_err(|e| e.to_string())?;
        window.set_position(PhysicalPosition::new(min_x, min_y)).map_err(|e| e.to_string())?;
    }

    let window_clone = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        let _ = window_clone.show();
        let _ = window_clone.set_focus();
    });

    Ok(())
}

/// Save picked color and close overlay
#[tauri::command]
async fn set_picked_color(
    app: tauri::AppHandle,
    state: State<'_, PickedColorState>,
    color: Option<String>,
) -> Result<(), String> {
    *lock(&state.0) = Some(color);

    if let Some(window) = app.get_webview_window("color-picker-overlay") {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Get picked color (called by main window)
#[tauri::command]
async fn get_picked_color(state: State<'_, PickedColorState>) -> Result<Option<String>, String> {
    let val = lock(&state.0).take();
    match val {
        Some(color) => Ok(color),
        None => Ok(None),
    }
}

/// Open fullscreen screenshot preview window
#[tauri::command]
async fn open_screenshot_preview(app: tauri::AppHandle, state: State<'_, ScreenshotPreviewState>, path: String) -> Result<(), String> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    // Store the path so the preview window can read it
    *lock(&state.0) = Some(path);

    // Close existing preview window if any
    if let Some(existing) = app.get_webview_window("screenshot-preview") {
        let _ = existing.close();
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    let window = WebviewWindowBuilder::new(&app, "screenshot-preview", WebviewUrl::App("/hud/screenshot-preview".into()))
        .title("Screenshot Preview")
        .fullscreen(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .transparent(true)
        .visible(false)
        .build()
        .map_err(|e| format!("Failed to build window: {}", e))?;

    let window_clone = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        let _ = window_clone.show();
        let _ = window_clone.set_focus();
    });

    Ok(())
}

/// Get the screenshot preview path (called by preview window)
#[tauri::command]
async fn get_screenshot_preview_path(state: State<'_, ScreenshotPreviewState>) -> Result<Option<String>, String> {
    Ok(lock(&state.0).clone())
}

/// Close the screenshot preview window
#[tauri::command]
async fn close_screenshot_preview(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("screenshot-preview") {
        let _ = window.close();
    }
    Ok(())
}

/// Create a second window on the right edge for dual mode
#[tauri::command]
async fn create_dual_window(app: tauri::AppHandle, monitor_index: Option<usize>) -> Result<(), String> {
    use tauri::{WebviewUrl, WebviewWindowBuilder, PhysicalSize};

    // Close an existing dual window first, and wait until it is really gone:
    // a fixed 100ms was sometimes too short, and the rebuild then failed with
    // "a window with this label already exists".
    if let Some(existing) = app.get_webview_window("dual-right") {
        let _ = existing.close();
        for _ in 0..40 {
            if app.get_webview_window("dual-right").is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        if app.get_webview_window("dual-right").is_some() {
            log::warn!("hud: previous dual-right window did not close in 2s");
        }
    }
    clear_revealed(&app, "dual-right");

    let monitor = pick_monitor(&app, None, monitor_index)?;

    let scale_factor = monitor.scale_factor();
    let screen_width = monitor.size().width as i32;
    let window_height = get_work_area(&monitor).height as i32;
    let window_width = (TOTAL_WIDTH as f64 * scale_factor) as u32;

    let window = WebviewWindowBuilder::new(&app, "dual-right", WebviewUrl::App("/hud".into()))
        .title("QuantHUD Right")
        .inner_size(TOTAL_WIDTH as f64, 900.0)
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        // An overlay has no business being minimised — see resilience.rs.
        .minimizable(false)
        .visible(false)
        .focused(false)
        .transparent(true)
        .shadow(false)
        .disable_drag_drop_handler()
        .build()
        .map_err(|e| format!("Failed to create dual window: {}", e))?;

    proximity::register(&window)?;
    watch_renderer(&window);
    resilience::guard(&window);
    watch_first_reveal(app.clone(), "dual-right");

    // Size to full height
    window.set_size(PhysicalSize::new(window_width, window_height as u32))
        .map_err(|e| e.to_string())?;

    // Position at right edge
    let monitor_x = monitor.position().x;
    let monitor_y = monitor.position().y;
    let x = screen_width - (TRIGGER_WIDTH as f64 * scale_factor) as i32;
    window.set_position(PhysicalPosition::new(monitor_x + x, monitor_y))
        .map_err(|e| e.to_string())?;

    // Like the primary HUD, reveal only when the frontend calls tuck_window:
    // the final tab size and region must be installed before it intercepts input.

    Ok(())
}

/// Close the dual-right window if it exists
#[tauri::command]
async fn close_dual_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("dual-right") {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Show a native Win32 notification popup with OK button and chime
#[tauri::command]
async fn show_notification_popup(message: String) -> Result<(), String> {
    std::thread::spawn(move || {
        #[cfg(target_os = "windows")]
        unsafe {
            type HANDLE = *mut std::ffi::c_void;
            type HWND = *mut std::ffi::c_void;
            type HDC = *mut std::ffi::c_void;
            type HBRUSH = *mut std::ffi::c_void;
            type HFONT = *mut std::ffi::c_void;
            type HGDIOBJ = *mut std::ffi::c_void;
            type HRGN = *mut std::ffi::c_void;

            #[repr(C)]
            struct RECT { left: i32, top: i32, right: i32, bottom: i32 }

            #[repr(C)]
            struct PAINTSTRUCT {
                hdc: HDC,
                f_erase: i32,
                rc_paint: RECT,
                f_restore: i32,
                f_inc_update: i32,
                rgb_reserved: [u8; 32],
            }

            #[repr(C)]
            struct WNDCLASSW {
                style: u32,
                lpfn_wnd_proc: unsafe extern "system" fn(HWND, u32, usize, isize) -> isize,
                cb_cls_extra: i32,
                cb_wnd_extra: i32,
                h_instance: HANDLE,
                h_icon: HANDLE,
                h_cursor: HANDLE,
                hbr_background: HBRUSH,
                lpsz_menu_name: *const u16,
                lpsz_class_name: *const u16,
            }

            #[repr(C)]
            struct MSG {
                hwnd: HWND,
                message: u32,
                w_param: usize,
                l_param: isize,
                time: u32,
                pt_x: i32,
                pt_y: i32,
            }

            #[repr(C)]
            struct DRAWITEMSTRUCT {
                ctl_type: u32, ctl_id: u32, item_id: u32,
                item_action: u32, item_state: u32,
                hwnd_item: HWND, hdc: HDC, rc_item: RECT, item_data: usize,
            }

            extern "system" {
                fn GetSystemMetrics(index: i32) -> i32;
                fn RegisterClassW(wc: *const WNDCLASSW) -> u16;
                fn CreateWindowExW(
                    ex: u32, class: *const u16, title: *const u16, style: u32,
                    x: i32, y: i32, w: i32, h: i32,
                    parent: HWND, menu: HANDLE, inst: HANDLE, param: *mut std::ffi::c_void,
                ) -> HWND;
                fn ShowWindow(h: HWND, cmd: i32) -> i32;
                fn UpdateWindow(h: HWND) -> i32;
                fn DestroyWindow(h: HWND) -> i32;
                fn PostQuitMessage(code: i32);
                fn DefWindowProcW(h: HWND, msg: u32, w: usize, l: isize) -> isize;
                fn GetMessageW(msg: *mut MSG, h: HWND, min: u32, max: u32) -> i32;
                fn TranslateMessage(msg: *const MSG) -> i32;
                fn DispatchMessageW(msg: *const MSG) -> isize;
                fn BeginPaint(h: HWND, ps: *mut PAINTSTRUCT) -> HDC;
                fn EndPaint(h: HWND, ps: *const PAINTSTRUCT) -> i32;
                fn CreateSolidBrush(color: u32) -> HBRUSH;
                fn FillRect(hdc: HDC, rc: *const RECT, brush: HBRUSH) -> i32;
                fn DeleteObject(obj: HGDIOBJ) -> i32;
                fn SetTextColor(hdc: HDC, color: u32) -> u32;
                fn SetBkMode(hdc: HDC, mode: i32) -> i32;
                fn CreateFontW(
                    h: i32, w: i32, esc: i32, ori: i32, weight: i32,
                    italic: u32, underline: u32, strikeout: u32,
                    charset: u32, out_prec: u32, clip_prec: u32,
                    quality: u32, pitch: u32, face: *const u16,
                ) -> HFONT;
                fn SelectObject(hdc: HDC, obj: HGDIOBJ) -> HGDIOBJ;
                fn DrawTextW(hdc: HDC, text: *const u16, len: i32, rc: *mut RECT, fmt: u32) -> i32;
                fn CreateRoundRectRgn(x1: i32, y1: i32, x2: i32, y2: i32, cx: i32, cy: i32) -> HRGN;
                fn SetWindowRgn(h: HWND, hrgn: HRGN, redraw: i32) -> i32;
                fn FillRgn(hdc: HDC, hrgn: HRGN, hbr: HBRUSH) -> i32;
                fn MessageBeep(utype: u32) -> i32;
                fn SetWindowLongPtrW(h: HWND, index: i32, new_long: isize) -> isize;
                fn GetWindowLongPtrW(h: HWND, index: i32) -> isize;
                fn SetProcessDpiAwarenessContext(value: isize) -> i32;
            }

            const WM_PAINT: u32 = 0x000F;
            const WM_COMMAND: u32 = 0x0111;
            const WM_DESTROY: u32 = 0x0002;
            const WM_DRAWITEM: u32 = 0x002B;
            const WS_POPUP: u32 = 0x8000_0000;
            const WS_VISIBLE: u32 = 0x1000_0000;
            const WS_CHILD: u32 = 0x4000_0000;
            const BS_OWNERDRAW: u32 = 0x0000_000B;
            const WS_EX_TOPMOST: u32 = 0x0008;
            const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
            const DT_CENTER: u32 = 0x01;
            const DT_VCENTER: u32 = 0x04;
            const DT_SINGLELINE: u32 = 0x20;
            const GWLP_USERDATA: i32 = -21;
            const POPUP_W: i32 = 340;
            const POPUP_H: i32 = 100;
            const BTN_ID: usize = 1001;
            const ODS_SELECTED: u32 = 0x0001;
            const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;

            unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, w: usize, l: isize) -> isize {
                match msg {
                    WM_PAINT => {
                        let user_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const String;
                        let mut ps = std::mem::zeroed::<PAINTSTRUCT>();
                        let hdc = BeginPaint(hwnd, &mut ps);
                        let bg = CreateSolidBrush(0x00201a1a);
                        let rc = RECT { left: 0, top: 0, right: POPUP_W, bottom: POPUP_H };
                        FillRect(hdc, &rc, bg);
                        DeleteObject(bg as HGDIOBJ);
                        SetBkMode(hdc, 1);
                        SetTextColor(hdc, 0x00FFFFFF); // white
                        let face: Vec<u16> = "Segoe UI\0".encode_utf16().collect();
                        let font = CreateFontW(
                            -15, 0, 0, 0, 600, 0, 0, 0, 0, 0, 0, 5, 0, face.as_ptr(),
                        );
                        let old = SelectObject(hdc, font as HGDIOBJ);
                        let mut trc = RECT { left: 10, top: 10, right: POPUP_W - 10, bottom: 55 };
                        if !user_ptr.is_null() {
                            let s = &*user_ptr;
                            let text_w: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
                            DrawTextW(hdc, text_w.as_ptr(), -1, &mut trc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
                        }
                        SelectObject(hdc, old);
                        DeleteObject(font as HGDIOBJ);
                        EndPaint(hwnd, &ps);
                        0
                    }
                    WM_DRAWITEM => {
                        let dis = &*(l as *const DRAWITEMSTRUCT);
                        let hdc = dis.hdc;
                        let rc = &dis.rc_item;
                        // First clear entire button rect with popup bg to eliminate default border
                        let clear_br = CreateSolidBrush(0x00201a1a);
                        let clear_rc = RECT { left: rc.left, top: rc.top, right: rc.right, bottom: rc.bottom };
                        FillRect(hdc, &clear_rc, clear_br);
                        DeleteObject(clear_br as HGDIOBJ);
                        // Draw rounded button on top
                        let bg_color = if dis.item_state & ODS_SELECTED != 0 { 0x00555555 } else { 0x00444444 };
                        let bg = CreateSolidBrush(bg_color);
                        let rgn = CreateRoundRectRgn(rc.left, rc.top, rc.right, rc.bottom, 10, 10);
                        FillRgn(hdc, rgn, bg);
                        DeleteObject(rgn as HGDIOBJ);
                        DeleteObject(bg as HGDIOBJ);
                        // Button text
                        SetBkMode(hdc, 1);
                        SetTextColor(hdc, 0x00FFFFFF);
                        let face: Vec<u16> = "Segoe UI\0".encode_utf16().collect();
                        let font = CreateFontW(-13, 0, 0, 0, 600, 0, 0, 0, 0, 0, 0, 5, 0, face.as_ptr());
                        let old = SelectObject(hdc, font as HGDIOBJ);
                        let text: Vec<u16> = "OK\0".encode_utf16().collect();
                        let mut trc = RECT { left: rc.left, top: rc.top, right: rc.right, bottom: rc.bottom };
                        DrawTextW(hdc, text.as_ptr(), -1, &mut trc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
                        SelectObject(hdc, old);
                        DeleteObject(font as HGDIOBJ);
                        1
                    }
                    WM_COMMAND => {
                        if (w & 0xFFFF) == BTN_ID {
                            DestroyWindow(hwnd);
                        }
                        0
                    }
                    WM_DESTROY => {
                        // Take the message back and free it — the window owned it.
                        let user_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut String;
                        if !user_ptr.is_null() {
                            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                            drop(Box::from_raw(user_ptr));
                        }
                        PostQuitMessage(0);
                        0
                    }
                    _ => DefWindowProcW(hwnd, msg, w, l),
                }
            }

            // One class for all popups. Window-class atoms are a finite
            // per-session resource and nothing ever unregisters them, so a name
            // per invocation accumulated one atom per notification. Re-
            // registering an existing class is a no-op that fails harmlessly.
            let class_name: Vec<u16> = "QuantHUDNotif\0".encode_utf16().collect();
            let wc = WNDCLASSW {
                style: 0,
                lpfn_wnd_proc: wnd_proc,
                cb_cls_extra: 0,
                cb_wnd_extra: 0,
                h_instance: std::ptr::null_mut(),
                h_icon: std::ptr::null_mut(),
                h_cursor: std::ptr::null_mut(),
                hbr_background: std::ptr::null_mut(),
                lpsz_menu_name: std::ptr::null(),
                lpsz_class_name: class_name.as_ptr(),
            };
            RegisterClassW(&wc);
            // DPI awareness for sharp font rendering
            SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let sx = GetSystemMetrics(0); // SM_CXSCREEN
            let sy = GetSystemMetrics(1); // SM_CYSCREEN
            let title: Vec<u16> = "\0".encode_utf16().collect();
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                class_name.as_ptr(),
                title.as_ptr(),
                WS_POPUP,
                sx - POPUP_W - 16, sy - POPUP_H - 60,
                POPUP_W, POPUP_H,
                std::ptr::null_mut(), std::ptr::null_mut(),
                std::ptr::null_mut(), std::ptr::null_mut(),
            );
            // Rounded corners
            let rgn = CreateRoundRectRgn(0, 0, POPUP_W, POPUP_H, 16, 16);
            SetWindowRgn(hwnd, rgn, 1);
            // Hand the message text to the window; WM_DESTROY frees it again
            let msg_ptr = Box::into_raw(Box::new(message));
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, msg_ptr as isize);
            // Create OK button
            let btn_class: Vec<u16> = "BUTTON\0".encode_utf16().collect();
            let btn_text: Vec<u16> = "OK\0".encode_utf16().collect();
            let btn_w = 70;
            let btn_h = 28;
            let btn_x = (POPUP_W - btn_w) / 2;
            let btn_y = POPUP_H - btn_h - 12;
            CreateWindowExW(
                0,
                btn_class.as_ptr(),
                btn_text.as_ptr(),
                WS_CHILD | WS_VISIBLE | BS_OWNERDRAW,
                btn_x, btn_y, btn_w, btn_h,
                hwnd, BTN_ID as *mut std::ffi::c_void,
                std::ptr::null_mut(), std::ptr::null_mut(),
            );
            ShowWindow(hwnd, 5);
            UpdateWindow(hwnd);
            MessageBeep(0x40); // subtle chime
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    });
    Ok(())
}

// ── Chart Analyzer ──

const WYCKOFF_SYSTEM: &str = r#"You are an expert Wyckoff Method chart analyst trained in Richard D. Wyckoff's original methodology. You analyze price charts by identifying the Composite Man's footprints through supply/demand dynamics, volume-price relationships, and structural patterns. You ONLY respond with raw JSON. No explanations, no markdown, no thinking, no text before or after the JSON. Just the JSON object.

WYCKOFF METHODOLOGY REFERENCE:

THREE LAWS:
1. Supply and Demand: When demand > supply, prices rise. When supply > demand, prices fall. Assess by comparing price spread and volume on up-moves vs down-moves.
2. Cause and Effect: The horizontal trading range (cause) determines the magnitude of the subsequent trend (effect). Wider/longer trading ranges produce larger moves.
3. Effort vs Result: Volume (effort) should confirm price movement (result). Divergence signals potential trend change. High volume with little price progress = absorption. Expanding volume with expanding spread = harmony/continuation.

ACCUMULATION EVENTS (identify during bottoming/basing):
- PS (Preliminary Support): First notable buying after a prolonged downtrend. Volume increases, spread widens, but downtrend continues. Signals selling pressure may be weakening.
- SC (Selling Climax): Widening spread and heavy/panicky selling absorbed by large interests near a bottom. Price often closes well off the low. Marks potential bottom.
- AR (Automatic Rally): Sharp rally after SC as selling pressure diminishes. Fueled by short covering and institutional buying. The AR high helps define the upper boundary of the trading range.
- ST (Secondary Test): Price revisits the SC area to test supply/demand balance. Volume and spread should be significantly less than the SC. Multiple STs are common. If ST goes below SC, expect new lows or prolonged consolidation.
- Spring/Shakeout: Price drops below TR support then reverses back into the range. A bear trap. Tests remaining supply. Low-volume spring = bullish, ready for markup. Terminal shakeout = aggressive spring with wider drop. NOT required (Schematic #2 has no spring).
- Test: Large operators test for remaining supply at key levels. Successful test = higher low on lesser volume.
- SOS (Sign of Strength): Price advance on increasing spread and relatively higher volume. Often follows a spring/shakeout, validating that analysis.
- LPS (Last Point of Support): Pullback after SOS on diminished spread and volume. Former resistance becomes support. Excellent entry point for longs.
- BU (Back-Up): Short-term profit-taking and test of supply near resistance after SOS ("jump across the creek" then "back up to the creek"). Can be a simple pullback or new higher-level TR.

DISTRIBUTION EVENTS (identify during topping):
- PSY (Preliminary Supply): Large interests begin unloading after a pronounced up-move. Volume expands, spread widens. Signals potential trend change.
- BC (Buying Climax): Marked increases in volume and spread near a top. Public buying absorbed by professionals. Often coincides with good news. Marks potential top.
- AR (Automatic Reaction): Selloff after BC as buying diminishes and supply continues. Low helps define lower boundary of distribution TR.
- ST (Secondary Test): Price revisits BC area. For confirmed top, supply must outweigh demand with decreased volume and spread. May take form of UT (Upthrust).
- UT (Upthrust): Price moves above TR resistance then quickly reverses back into range. A bull trap testing remaining demand.
- SOW (Sign of Weakness): Down-move to or past lower TR boundary on increased spread and volume. Shows supply is dominant.
- LPSY (Last Point of Supply): Feeble rally on narrow spread after SOW. Shows difficulty advancing. Exhaustion of demand before markdown.
- UTAD (Upthrust After Distribution): Distributional counterpart to spring. Price breaks above TR resistance then reverses. Tests new demand. NOT required (Schematic #2 has no UTAD).

ACCUMULATION PHASES:
- Phase A: Stopping the downtrend. Identified by PS, SC, AR, ST sequence. SC and ST lows + AR high define the TR boundaries. Heavy volume on SC transitioning to lighter volume on ST.
- Phase B: Building the cause. Institutions accumulate at low prices. Multiple STs, possible upthrusts at upper TR. Wide swings early, narrowing over time as supply absorbed. Volume on downswings diminishes over time.
- Phase C: Testing remaining supply. Spring or shakeout breaks below TR support then reverses (Schematic #1). Or testing occurs at higher level within TR without spring (Schematic #2). Low-volume test = ready for markup.
- Phase D: Demand dominates. Pattern of SOSs on widening spread/increasing volume and LPSs on smaller spread/diminished volume. Price reaches at least the top of TR.
- Phase E: Markup begins. Stock leaves TR, demand in full control. Reactions are short-lived. Re-accumulation TRs ("stepping stones") may form.

DISTRIBUTION PHASES:
- Phase A: Stopping the uptrend. PSY and BC followed by AR and ST. May terminate without climactic action (exhaustion shown by decreasing spread/volume on rallies).
- Phase B: Building cause for downtrend. Institutions distribute long inventory and initiate shorts. SOWs show increased spread/volume to downside.
- Phase C: Testing remaining demand via UT or UTAD (bull trap). Or demand so weak price doesn't reach BC level. UTAD not required.
- Phase D: Supply clearly dominant. Price travels to or through TR support. Multiple weak rallies (LPSYs). Clear break of support or decline below mid-TR.
- Phase E: Markdown unfolds. Stock leaves TR, supply in control. Rallies are feeble. May lead to re-distribution TR.

SCHEMATICS:
- Accumulation #1: Has a Spring/Shakeout in Phase C (price breaks below support then reverses).
- Accumulation #2: NO spring. Testing occurs at higher levels within the TR.
- Distribution #1: Has a UTAD in Phase C (price breaks above resistance then reverses).
- Distribution #2: NO UTAD. Demand too weak to push to BC level.
- Re-Accumulation: Occurs during a longer uptrend. Phase A resembles distribution. Shorter duration, smaller amplitude than primary accumulation.
- Re-Distribution: Occurs within a larger downtrend. Phase A may resemble accumulation with climactic downside action."#;

const WYCKOFF_PROMPT: &str = r#"Analyze this price chart using the Wyckoff Method.

CRITICAL: Focus your analysis on the MOST RECENT price action — the rightmost portion of the chart (the current/live edge). The older price history visible on the left side of the chart is only CONTEXT to help you understand where price has been. Your job is to identify what is happening RIGHT NOW at the current price.

Ask yourself:
1. What is the CURRENT structure at the right edge? Is price currently in a trading range, breaking out of one, or trending?
2. What was the MOST RECENT significant event? (e.g., did price just spring below support? just rally on high volume? just fail at resistance?)
3. What Wyckoff phase is the CURRENT price action in — not what phase the entire chart history covers?
4. Based on the recent price bars, volume, and spread at the RIGHT EDGE: what is the immediate bias?

Analyze step by step:
- RECENT STRUCTURE: Identify the most recent trading range, trend, or transition at the right side of the chart.
- VOLUME AT THE EDGE: Is recent volume expanding or contracting? On which direction (up-bars vs down-bars)?
- SPREAD AT THE EDGE: Are recent price bars widening or narrowing? In which direction?
- CURRENT EVENTS: What Wyckoff events have occurred in the MOST RECENT trading range or trend? List them chronologically.
- CURRENT PHASE: Which phase (A-E) describes where price is RIGHT NOW in the current structure?
- SCHEMATIC FIT: Which schematic best matches the current/most recent structure?

Return ONLY this JSON:
{"market_phase":"Accumulation|Markup|Distribution|Markdown","schematic":"Accumulation #1|Accumulation #2|Distribution #1|Distribution #2|Re-Accumulation|Re-Distribution","wyckoff_phase":"A|B|C|D|E","events":["list Wyckoff events from the CURRENT/MOST RECENT structure only, in chronological order: PS,SC,AR,ST,Spring,Test,SOS,LPS,BU,PSY,BC,UT,SOW,LPSY,UTAD"],"current_transition":"Absorbing Supply|Testing Support|Testing Resistance|Spring Rally|Markup Beginning|Breaking Out|Pulling Back|Topping Out|Distributing|Breaking Down|Shakeout Recovery|Demand Weakening|Supply Exhaustion|Rally Fading|Trending Up|Trending Down|Range Bound","bias":"Bullish|Bearish|Neutral"}"#;

#[tauri::command]
async fn save_temp_image(image_base64: String) -> Result<String, String> {
    let temp_dir = std::env::temp_dir();
    let path = temp_dir.join("quanthud_chart_capture.png");
    let bytes = STANDARD.decode(&image_base64).map_err(|e| e.to_string())?;
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
async fn analyze_chart(
    image_base64: String,
    analysis_types: Vec<String>,
    provider: String,
    base_url: String,
    model: String,
) -> Result<String, String> {
    let mut prompt_parts: Vec<String> = Vec::new();

    for t in &analysis_types {
        if t.as_str() == "wyckoff" { prompt_parts.push(WYCKOFF_PROMPT.to_string()) }
    }

    if prompt_parts.is_empty() {
        return Err("No valid analysis types selected".into());
    }

    let prompt = prompt_parts.join("\n\n---\n\n");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let (url, body) = match provider.as_str() {
        "ollama" => {
            // Ollama /api/chat with images array + forced JSON mode
            let url = format!("{}/api/chat", base_url.trim_end_matches('/'));
            let body = serde_json::json!({
                "model": model,
                "messages": [
                    {"role": "system", "content": WYCKOFF_SYSTEM},
                    {"role": "user", "content": prompt, "images": [image_base64]}
                ],
                "stream": false,
                "format": "json",
                "options": {"temperature": 0}
            });
            (url, body)
        }
        "lmstudio" => {
            // LM Studio OpenAI-compatible /v1/chat/completions + forced JSON mode
            let url = format!("{}/v1/chat/completions", base_url.trim_end_matches('/'));
            let body = serde_json::json!({
                "model": model,
                "messages": [
                    {"role": "system", "content": WYCKOFF_SYSTEM},
                    {"role": "user", "content": [
                        {
                            "type": "image_url",
                            "image_url": {
                                "url": format!("data:image/png;base64,{}", image_base64)
                            }
                        },
                        {
                            "type": "text",
                            "text": prompt
                        }
                    ]}
                ],
                "max_tokens": 2048,
                "temperature": 0,
                "response_format": {
                    "type": "json_schema",
                    "json_schema": {
                        "name": "wyckoff_analysis",
                        "strict": true,
                        "schema": {
                            "type": "object",
                            "properties": {
                                "market_phase": {"type": "string", "enum": ["Accumulation", "Markup", "Distribution", "Markdown"]},
                                "schematic": {"type": "string"},
                                "wyckoff_phase": {"type": "string", "enum": ["A", "B", "C", "D", "E"]},
                                "events": {"type": "array", "items": {"type": "string", "enum": ["PS","SC","AR","ST","Spring","Shakeout","Test","SOS","LPS","BU","PSY","BC","UT","SOW","LPSY","UTAD"]}},
                                "current_transition": {"type": "string", "enum": ["Absorbing Supply","Testing Support","Testing Resistance","Spring Rally","Markup Beginning","Breaking Out","Pulling Back","Topping Out","Distributing","Breaking Down","Shakeout Recovery","Demand Weakening","Supply Exhaustion","Rally Fading","Trending Up","Trending Down","Range Bound"]},
                                "bias": {"type": "string", "enum": ["Bullish", "Bearish", "Neutral"]}
                            },
                            "required": ["market_phase", "schematic", "wyckoff_phase", "events", "current_transition", "bias"],
                            "additionalProperties": false
                        }
                    }
                },
                "stream": false
            });
            (url, body)
        }
        _ => return Err(format!("Unknown AI provider: {}", provider)),
    };

    let res = client
        .post(&url)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "Request timed out. The model may need more time or resources.".to_string()
            } else if e.is_connect() {
                format!("Cannot connect to {}. Is {} running?", base_url, provider)
            } else {
                format!("Request failed: {}", e)
            }
        })?;

    if !res.status().is_success() {
        let status = res.status();
        let error_body = res.text().await.unwrap_or_default();
        return Err(format!("AI error ({}): {}", status, error_body));
    }

    let data: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    // Extract text based on provider response format
    let text = match provider.as_str() {
        "ollama" => data["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        "lmstudio" => {
            // Some reasoning models (e.g. Qwen3.5) put output in reasoning_content instead of content
            let content = data["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or("")
                .to_string();
            if content.is_empty() {
                data["choices"][0]["message"]["reasoning_content"]
                    .as_str()
                    .unwrap_or("")
                    .to_string()
            } else {
                content
            }
        }
        _ => String::new(),
    };

    if text.is_empty() {
        return Err("Empty response from AI model".into());
    }

    Ok(text)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// The HUD's own window.
///
/// In the standalone app this was `main`. In the suite `main` is the shell, so
/// the overlay gets its own label and is created on demand — same geometry and
/// flags as the old `tauri.conf.json` entry (ARCHITECTURE.md §10).
pub const HUD_WINDOW: &str = "hud";

/// Show the HUD overlay, creating it on first use.
///
/// Lazy on purpose: the window is a 340×900 always-on-top strip, and a suite
/// launched to look at charts should not have it appear unasked.
// Not `pub`: `#[tauri::command]` on a public fn re-exports the macro it
// generates, which then collides with its own definition. Every other command in
// this file is private for the same reason.
/// **Must stay `async`.** A sync command runs on the main thread, where
/// `WebviewWindowBuilder::build()` deadlocks against the event loop — the app
/// freezes with no error and no panic. Every other window-creating command in
/// this file (`open_region_selector`, `open_color_picker_overlay`,
/// `open_screenshot_preview`, `create_dual_window`) is async for this reason.
#[tauri::command]
async fn open_hud(app: tauri::AppHandle) -> Result<(), String> {
    proximity::set_enabled(&app, true)?;
    if app.get_webview_window(HUD_WINDOW).is_some() {
        return Ok(());
    }

    create_hud_window(&app)
}

/// Build the overlay window. Never call this synchronously on the main thread —
/// `build()` deadlocks there (see the note on `open_hud`); callers are either
/// async commands or spawned tasks.
fn create_hud_window(app: &tauri::AppHandle) -> Result<(), String> {
    clear_revealed(app, HUD_WINDOW);
    let window = tauri::WebviewWindowBuilder::new(
        app,
        HUD_WINDOW,
        tauri::WebviewUrl::App("/hud".into()),
    )
    .title("QuantHUD")
    .inner_size(340.0, 900.0)
    .resizable(false)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    // An overlay has no business being minimised — see resilience.rs.
    .minimizable(false)
    .transparent(true)
    .shadow(false)
    // Tauri's drag-drop handler swallows HTML5 drag events in WebView2 — with
    // it enabled, the notes/todo section and card reordering is dead. The
    // standalone app disabled it (`dragDropEnabled: false`); same here and on
    // the `dual-right` pane below.
    .disable_drag_drop_handler()
    // Hidden until the page has measured the monitor and tucked the window to
    // its edge — created visible, it flashes at the OS default position
    // (centre of the screen) for as long as the webview takes to boot.
    // `tuck_window`/`show_window` hand visibility to proximity once placed.
    .visible(false)
    // Approaching the tab must not steal focus from the user's application.
    .focused(false)
    .build()
    .map_err(|e| format!("failed to create the HUD window: {e}"))?;

    proximity::register(&window)?;
    watch_renderer(&window);
    resilience::guard(&window);
    watch_first_reveal(app.clone(), HUD_WINDOW);

    Ok(())
}

/// Tray-menu toggle for the overlay (see `core.tray.toggle` in qs-core).
///
/// Toggle the user's enablement, not proximity's temporary visibility. Both
/// sides stay disabled after a manual hide, including while the mouse is near.
fn toggle_hud_visibility(app: &tauri::AppHandle) {
    let result = if app.get_webview_window(HUD_WINDOW).is_some() {
        proximity::toggle(app)
    } else {
        proximity::set_enabled(app, true).and_then(|()| create_hud_window(app))
    };
    if let Err(e) = result {
        log::warn!("hud: tray toggle failed: {e}");
    }
}

/// Hide the HUD overlay and its dual-pane companion.
#[tauri::command(async)]
fn close_hud(app: tauri::AppHandle) {
    if let Err(e) = proximity::set_enabled(&app, false) {
        log::warn!("hud: hiding overlays failed: {e}");
    }
}

/// Build the `hud` plugin. Pinned to `Wry` — see the note on `systems`.
///
/// The shell, clipboard and global-shortcut plugins the standalone app
/// registered are now registered once by the suite binary.
pub fn init() -> TauriPlugin<Wry> {
    // WebView2 launch flags. The WebView2 loader *appends* this variable to
    // the arguments wry passes, and Chromium keeps only the last value of a
    // repeated switch — so the `--disable-features` list below must carry
    // wry's own three (mini menu, PDF UI, SmartScreen) or they are lost.
    //
    // `--use-fake-ui-for-media-stream`: auto-grant the microphone (no prompt).
    //
    // `CalculateNativeWinOcclusion` off: Chromium stops compositing a window
    // it computes as fully covered, and on Windows that computation goes
    // wrong for transparent always-on-top windows around the lock screen,
    // logon transition and display sleep — the window is un-covered again
    // but never resumes painting. Blank in a transparent window is invisible,
    // which is exactly how the overlay's edge buttons "disappeared" after the
    // suite had run for a while, and why at boot the pane that loaded while
    // hidden (`hud`) came up empty more often than the one shown while
    // loading (`dual-right`). Process-wide, so the main window gets it too;
    // the cost is painting it while something covers it.
    #[cfg(target_os = "windows")]
    std::env::set_var(
        "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
        "--use-fake-ui-for-media-stream \
         --disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,CalculateNativeWinOcclusion",
    );

    Builder::<Wry>::new("hud")
        .setup(|app, _api| {
            app.manage(RegionState(Mutex::new(None)));
            app.manage(PickedColorState(Mutex::new(None)));
            app.manage(ScreenshotPreviewState(Mutex::new(None)));
            app.manage(RevealState(Default::default()));
            app.manage(resilience::RebuildState::default());
            proximity::init(app);
            config::import_legacy_config();
            // QuantVoice: the state, the process-register entry, the
            // push-to-talk hotkey, the shutdown hook (speech.rs).
            speech::init(app);
            // The standalone app built its own tray here, with Show/Hide bound
            // to its `main` window and Quit calling `app.exit(0)` directly.
            // qs-core owns the single suite tray and the ordered shutdown now,
            // so this module only contributes a menu entry that opens the
            // overlay (ARCHITECTURE.md §10).
            qs_core::tray::register_entry(
                app,
                qs_core::tray::TrayEntry {
                    module: "hud".into(),
                    label: "QuantHUD".into(),
                    route: "/hud".into(),
                    // Renders a "Show / Hide QuantHUD" tray item; the click
                    // arrives as `core.tray.toggle`, handled below.
                    toggle_window: Some(HUD_WINDOW.into()),
                },
            );

            // Tray "Show / Hide QuantHUD". Handled off the main thread because
            // the hidden→missing case builds the window, and `build()` on the
            // main thread deadlocks (see `open_hud`).
            if let Some(bus) = app.try_state::<qs_core::bus::Bus>() {
                let handle = app.clone();
                bus.subscribe(
                    "core.tray.toggle",
                    Box::new(move |ev| {
                        if ev.payload.get("window").and_then(|v| v.as_str()) != Some(HUD_WINDOW) {
                            return;
                        }
                        let handle = handle.clone();
                        tauri::async_runtime::spawn(async move {
                            toggle_hud_visibility(&handle);
                        });
                    }),
                );
            }

            // The overlay and its transparent helper windows must not survive a
            // Quit; they are always-on-top and skip the taskbar, so a leaked one
            // is very hard to get rid of.
            let handle = app.clone();
            qs_core::shutdown::on_shutdown(
                app,
                "hud:windows",
                Box::new(move || {
                    for label in [HUD_WINDOW, "dual-right", "region-selector", "color-picker-overlay", "screenshot-preview"] {
                        if let Some(w) = handle.get_webview_window(label) {
                            let _ = w.destroy();
                        }
                    }
                }),
            );

            Ok(())
        })
        .invoke_handler(qs_core::diagnostics::traced(tauri::generate_handler![
            capture_screen,
            get_cursor_position,
            get_available_monitors,
            load_config,
            save_config,
            tuck_window,
            show_window,
            is_window_tucked,
            setup_window_size,
            set_window_position,
            open_region_selector,
            set_selected_region,
            get_selected_region,
            pick_screen_color,
            pick_folder,
            pick_file,
            launch_app,
            get_app_icon,
            get_default_screenshots_folder,
            list_os_screenshots,
            read_screenshot_file,
            read_screenshot_thumbnail,
            open_screenshots_folder,
            copy_screenshot_to_clipboard,
            open_color_picker_overlay,
            set_picked_color,
            get_picked_color,
            open_screenshot_preview,
            get_screenshot_preview_path,
            close_screenshot_preview,
            create_dual_window,
            close_dual_window,
            show_notification_popup,
            speech::speech_status,
            speech::speech_setup,
            speech::speech_start,
            speech::speech_stop,
            speech::speech_cancel,
            speech::speech_devices,
            speech::speech_apply_settings,
            speech::speech_engine_start,
            speech::speech_engine_stop,
            speech::speech_engine_logs,
            analyze_chart,
            save_temp_image,
            open_hud,
            close_hud
        ]))
        .build()
}
