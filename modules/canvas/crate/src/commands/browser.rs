use tauri::{AppHandle, Emitter, Manager};

#[derive(serde::Deserialize)]
pub struct ClipRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[tauri::command]
pub async fn navigate_browser(app: AppHandle, label: String, url: String) -> Result<(), String> {
    let webview = app
        .get_webview(&label)
        .ok_or_else(|| format!("Webview '{}' not found", label))?;

    let parsed_url: url::Url = url.parse().map_err(|e: url::ParseError| e.to_string())?;
    webview
        .navigate(parsed_url)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn eval_browser(app: AppHandle, label: String, js: String) -> Result<(), String> {
    let webview = app
        .get_webview(&label)
        .ok_or_else(|| format!("Webview '{}' not found", label))?;

    webview.eval(&js).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_browser_url(app: AppHandle, label: String) -> Result<String, String> {
    let webview = app
        .get_webview(&label)
        .ok_or_else(|| format!("Webview '{}' not found", label))?;

    Ok(webview.url().map_err(|e: tauri::Error| e.to_string())?.to_string())
}

/// Clip the browser webview HWND to its visible region by subtracting
/// obscuring rectangles (higher-z canvas windows / sidebars).
#[tauri::command]
pub async fn set_browser_clip_region(
    app: AppHandle,
    label: String,
    full_width: i32,
    full_height: i32,
    obscuring_rects: Vec<ClipRect>,
) -> Result<(), String> {
    let webview = app
        .get_webview(&label)
        .ok_or_else(|| format!("Webview '{}' not found", label))?;

    #[cfg(target_os = "windows")]
    {
        webview
            .with_webview(move |wv| {
                unsafe { apply_window_region(wv, full_width, full_height, &obscuring_rects) };
            })
            .map_err(|e| format!("with_webview: {}", e))?;
    }

    Ok(())
}

// ---- Win32 region clipping (Windows only) ----

#[cfg(target_os = "windows")]
unsafe fn apply_window_region(
    wv: tauri::webview::PlatformWebview,
    full_width: i32,
    full_height: i32,
    obscuring_rects: &[ClipRect],
) {
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

#[tauri::command]
pub async fn find_in_browser(app: AppHandle, label: String, query: String, forward: bool) -> Result<(), String> {
    let webview = app
        .get_webview(&label)
        .ok_or_else(|| format!("Webview '{}' not found", label))?;

    let js = if query.is_empty() {
        "window.getSelection().removeAllRanges();".to_string()
    } else {
        format!(
            "window.find('{}', false, {}, true, false, true, false);",
            query.replace('\\', "\\\\").replace('\'', "\\'"),
            if forward { "false" } else { "true" }
        )
    };

    webview.eval(&js).map_err(|e| e.to_string())
}

/// Called from injected JS in child webviews to open a link in a new tab.
/// Uses app.emit() so the event reaches the main webview's listen().
#[tauri::command]
pub async fn browser_request_new_tab(app: AppHandle, url: String) -> Result<(), String> {
    app.emit("browser-new-tab-request", &url)
        .map_err(|e| e.to_string())
}

/// Webviews whose fullscreen handler is already installed, label -> host HWND.
/// The registration token is not kept, so the handler can never be removed —
/// hook once per webview instance. The label alone is not identity: closing a
/// tab and rebuilding it (workspace switch) reuses the very same label on a
/// fresh webview, which needs its own handler.
#[cfg(target_os = "windows")]
static FULLSCREEN_HOOKED: std::sync::Mutex<std::collections::BTreeMap<String, isize>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

/// Hook into WebView2's ContainsFullScreenElementChanged event.
/// Instead of letting the window go fullscreen, we emit events to the frontend
/// so it can expand/restore the webview within the window bounds.
#[tauri::command]
pub async fn browser_disable_fullscreen(app: AppHandle, label: String) -> Result<(), String> {
    let webview = app
        .get_webview(&label)
        .ok_or_else(|| format!("Webview '{}' not found", label))?;

    let window = webview.window();
    let label_clone = label.clone();

    #[cfg(target_os = "windows")]
    {
        // Entries of closed webviews would otherwise pile up for every tab ever
        // opened; the hook itself dies with its webview.
        {
            let mut hooked = FULLSCREEN_HOOKED
                .lock()
                .map_err(|e| format!("Lock fullscreen hooks: {}", e))?;
            hooked.retain(|hooked_label, _| app.get_webview(hooked_label).is_some());
        }

        let app_clone = app.clone();
        webview
            .with_webview(move |wv| {
                unsafe { hook_fullscreen_changed(wv, window, app_clone, label_clone) };
            })
            .map_err(|e| format!("with_webview: {}", e))?;
    }

    Ok(())
}

#[cfg(target_os = "windows")]
unsafe fn hook_fullscreen_changed(
    wv: tauri::webview::PlatformWebview,
    window: tauri::Window,
    app: AppHandle,
    label: String,
) {
    use webview2_com::ContainsFullScreenElementChangedEventHandler;
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2;

    let controller = wv.controller();

    let core_wv: Result<ICoreWebView2, _> = controller.CoreWebView2();
    let core_wv = match core_wv {
        Ok(wv) => wv,
        Err(_) => return,
    };

    // Hook once per webview instance — the host HWND is what tells a rebuilt
    // tab apart from the live one that already carries the handler.
    let mut hwnd: isize = 0;
    let _ = controller.ParentWindow(&mut hwnd as *mut isize as *mut _);
    match FULLSCREEN_HOOKED.lock() {
        Ok(mut hooked) => {
            if hooked.insert(label.clone(), hwnd) == Some(hwnd) {
                return;
            }
        }
        Err(_) => return,
    }

    let handler = ContainsFullScreenElementChangedEventHandler::create(Box::new(
        move |core_wv_inner, _args| {
            // Prevent the Tauri window from going OS-fullscreen
            let _ = window.set_fullscreen(false);

            // Check whether content is entering or leaving fullscreen
            let mut is_fullscreen = false;
            if let Some(ref wv) = core_wv_inner {
                let mut val: i32 = 0;
                if wv.ContainsFullScreenElement(&mut val as *mut i32 as *mut _).is_ok() {
                    is_fullscreen = val != 0;
                }
            }

            // Emit event to frontend so it can expand/restore the webview
            let _ = app.emit("browser-fullscreen-changed", serde_json::json!({
                "label": label,
                "isFullscreen": is_fullscreen,
            }));

            // Also set fullscreen false after a short delay to counter any
            // Tauri built-in handler that may fire after us
            let w = window.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(50));
                let _ = w.set_fullscreen(false);
            });

            Ok(())
        },
    ));

    let mut token: i64 = 0;
    let _ = core_wv.add_ContainsFullScreenElementChanged(&handler, &mut token);
}
