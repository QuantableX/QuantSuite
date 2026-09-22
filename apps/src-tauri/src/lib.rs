//! The single QuantSuite binary.
//!
//! `qs-core` is registered first â€” module plugins assume its state exists. Each
//! module added in a later phase contributes exactly one line here and one line
//! in `Cargo.toml`.

mod updates;

pub fn run() {
    tauri::Builder::default()
        // Registered before everything else, deliberately: a second launch
        // must be intercepted before any plugin initialises â€” two instances
        // would contend for core.db and the sidecars (PLAN-V2 E6). The
        // second process exits; the first brings its window to the front.
        // Not upstream's plugin: its second-instance handshake is a
        // `SendMessageW` with no timeout, so a relaunch against a hung first
        // instance blocks forever instead of exiting. `qs-single-instance` is
        // that code with the send bounded (see its module docs).
        .plugin(qs_single_instance::init(|app, _args, _cwd| {
            qs_core::window::show(app);
        }))
        // Core must come first among the suite's own plugins: bus, core.db,
        // settings, the process register, tray. Module plugins announce their
        // processes into that register during their own setup, which runs
        // after this line.
        .plugin(qs_core::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updates::Updates::default())
        .invoke_handler(tauri::generate_handler![
            updates::suite_check_for_update,
            updates::suite_install_update,
        ])
        // Shared Tauri plugins the modules rely on, registered once.
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        // Autostart is off until the user turns it on in settings; registering
        // the plugin only makes the OS entry writable. The argument is what
        // tells a boot launch apart from a double-click — it must stay in sync
        // with `qs_core::autostart::FLAG`.
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![qs_core::autostart::FLAG]),
        ))
        // Modules, one per landed migration phase.
        .plugin(qs_mod_systems::init())
        .plugin(qs_mod_notes::init())
        .plugin(qs_mod_plan::init())
        .plugin(qs_mod_habit::init())
        .plugin(qs_mod_finance::init())
        .plugin(qs_mod_algo::init())
        .plugin(qs_mod_mcp::init())
        .plugin(qs_mod_pilot::init())
        .plugin(qs_mod_canvas::init())
        .plugin(qs_mod_terminal::init())
        .plugin(qs_mod_console::init())
        .plugin(qs_mod_hud::init())
        .plugin(qs_mod_memory::init())
        .plugin(qs_mod_script::init())
        //
        // Tray residency: closing the main window hides it, it does not quit.
        // The window itself is a plain maximised undecorated rectangle â€” the
        // circular launcher died with PLAN-V2; `set_circular` remains in
        // qs-core for QuantHUD's overlay technique but the shell no longer
        // calls it.
        .on_window_event(qs_core::window::on_event)
        // `main` is declared hidden so an autostart launch never flashes a
        // window; every other launch shows it here. This cannot move into a
        // plugin's setup — those run before Tauri creates the config windows.
        .setup(|app| {
            qs_core::autostart::apply_launch_visibility(app.handle());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running QuantSuite");
}
