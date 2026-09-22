//! One signed update channel for the entire suite. Modules own no updater.
use serde::Serialize;
use std::time::Duration;
use tauri::{ipc::Channel, AppHandle, State, WebviewWindow};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::Mutex;

#[derive(Default)]
pub struct Updates(Mutex<Option<Update>>);

#[derive(Serialize)]
pub struct UpdateInfo {
    current_version: String,
    version: Option<String>,
    notes: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct Progress {
    downloaded: u64,
    total: Option<u64>,
    installing: bool,
}

fn require_suite(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == "main" || window.label().starts_with("suite-") {
        Ok(())
    } else {
        Err("Updates are available from the QuantSuite main menu.".into())
    }
}

#[tauri::command]
pub async fn suite_check_for_update(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, Updates>,
) -> Result<UpdateInfo, String> {
    require_suite(&window)?;
    let mut pending = state
        .0
        .try_lock()
        .map_err(|_| "An update operation is already running.")?;
    *pending = None;
    let updater = app
        .updater_builder()
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| e.to_string())?;
    let update = tokio::time::timeout(Duration::from_secs(30), updater.check())
        .await
        .map_err(|_| "The update check timed out. Please try again.")?
        .map_err(|e| e.to_string())?;
    let info = UpdateInfo {
        current_version: app.package_info().version.to_string(),
        version: update.as_ref().map(|u| u.version.clone()),
        notes: update.as_ref().and_then(|u| u.body.clone()),
    };
    *pending = update;
    Ok(info)
}

#[tauri::command]
pub async fn suite_install_update(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, Updates>,
    version: String,
    on_progress: Channel<Progress>,
) -> Result<(), String> {
    require_suite(&window)?;
    if cfg!(debug_assertions) {
        return Err("Install updates from an installed release of QuantSuite.".into());
    }
    // Keep this lock through installation: multiple suite windows share it.
    let mut pending = state
        .0
        .try_lock()
        .map_err(|_| "An update operation is already running.")?;
    let update = pending
        .take()
        .ok_or("Check for updates before installing.")?;
    if update.version != version {
        return Err("The available update changed. Please check again.".into());
    }
    let mut downloaded = 0;
    update
        .download_and_install(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = on_progress.send(Progress {
                    downloaded,
                    total,
                    installing: false,
                });
            },
            || {
                let _ = on_progress.send(Progress {
                    downloaded: 0,
                    total: None,
                    installing: true,
                });
            },
        )
        .await
        .map_err(|e| e.to_string())?;
    app.restart();
}
