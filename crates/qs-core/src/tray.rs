//! One tray icon for the whole suite (ARCHITECTURE.md §10).
//!
//! Modules contribute entries through [`register_entry`] instead of building
//! their own tray — two tray icons for one application is exactly what this
//! prevents. The menu itself stays deliberately small: it only renders entries
//! that own a window of their own (`toggle_window`), as Show/Hide items next to
//! the suite window's. Route-only entries are registered but not shown — the
//! tray is for window visibility, not module navigation.

use crate::{bus, shutdown, window};
use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Runtime,
};

pub const TRAY_ID: &str = "qs-tray";

/// A dev instance runs beside the installed suite with an identical icon —
/// tooltip and menu labels are the only way to tell the two trays apart.
const APP_LABEL: &str = if cfg!(debug_assertions) {
    "QuantSuite Dev"
} else {
    "QuantSuite"
};

#[derive(Debug, Clone)]
pub struct TrayEntry {
    pub module: String,
    pub label: String,
    /// Route to open when clicked, e.g. `/algo`.
    pub route: String,
    /// Label of a window this module owns (e.g. QuantHUD's overlay). When set,
    /// the tray renders a "Show / Hide <label>" item for it; clicking emits
    /// `core.tray.toggle` with this window label, and the owning module
    /// subscribes and toggles (or creates) the window. Entries without it are
    /// not rendered in the menu.
    pub toggle_window: Option<String>,
}

#[derive(Default)]
pub struct TrayRegistry(pub Mutex<Vec<TrayEntry>>);

/// Called by a module plugin during its `setup`.
pub fn register_entry<R: Runtime>(app: &AppHandle<R>, entry: TrayEntry) {
    if let Some(reg) = app.try_state::<TrayRegistry>() {
        if let Ok(mut entries) = reg.0.lock() {
            entries.retain(|e| e.module != entry.module);
            entries.push(entry);
            entries.sort_by(|a, b| a.label.cmp(&b.label));
        }
    }
    let _ = rebuild(app);
}

fn build_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let show = MenuItem::with_id(app, "show", format!("Show / Hide {APP_LABEL}"), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", format!("Quit {APP_LABEL}"), true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;

    let entries: Vec<TrayEntry> = app
        .try_state::<TrayRegistry>()
        .and_then(|r| r.0.lock().ok().map(|e| e.clone()))
        .unwrap_or_default();

    let mut items: Vec<Box<dyn tauri::menu::IsMenuItem<R>>> = vec![Box::new(show)];

    // Only entries that own a window get a menu item (see TrayEntry docs).
    for e in &entries {
        if let Some(win) = &e.toggle_window {
            items.push(Box::new(MenuItem::with_id(
                app,
                format!("toggle:{win}"),
                format!("Show / Hide {}", e.label),
                true,
                None::<&str>,
            )?));
        }
    }

    items.push(Box::new(sep));
    items.push(Box::new(quit));

    let refs: Vec<&dyn tauri::menu::IsMenuItem<R>> = items.iter().map(|i| i.as_ref()).collect();
    Menu::with_items(app, &refs)
}

/// Rebuild the menu in place. Safe to call before the tray exists.
pub fn rebuild<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let menu = build_menu(app)?;
        tray.set_menu(Some(menu))?;
    }
    Ok(())
}

pub fn init<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let menu = build_menu(app)?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(APP_LABEL)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => window::toggle(app),
            "quit" => shutdown::quit(app),
            id if id.starts_with("toggle:") => {
                // The owning module subscribes to this and toggles its window;
                // qs-core cannot create module windows itself (it does not know
                // their routes or geometry).
                let label = id.trim_start_matches("toggle:").to_string();
                let _ = bus::emit(
                    app,
                    "core.tray.toggle",
                    serde_json::json!({ "window": label }),
                );
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // Left click toggles the window; the menu is on right click.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                window::toggle(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    Ok(())
}
