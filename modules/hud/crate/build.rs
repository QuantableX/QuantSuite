const COMMANDS: &[&str] = &[
    "capture_screen",
    "get_cursor_position",
    "get_available_monitors",
    "load_config",
    "save_config",
    "tuck_window",
    "show_window",
    "is_window_tucked",
    "setup_window_size",
    "set_window_position",
    "open_region_selector",
    "set_selected_region",
    "get_selected_region",
    "pick_screen_color",
    "pick_folder",
    "pick_file",
    "launch_app",
    "get_app_icon",
    "get_default_screenshots_folder",
    "list_os_screenshots",
    "read_screenshot_file",
    "read_screenshot_thumbnail",
    "open_screenshots_folder",
    "copy_screenshot_to_clipboard",
    "open_color_picker_overlay",
    "set_picked_color",
    "get_picked_color",
    "open_screenshot_preview",
    "get_screenshot_preview_path",
    "close_screenshot_preview",
    "create_dual_window",
    "close_dual_window",
    "show_notification_popup",
    "speech_status",
    "speech_setup",
    "speech_start",
    "speech_stop",
    "speech_cancel",
    "speech_devices",
    "speech_apply_settings",
    "speech_engine_start",
    "speech_engine_stop",
    "speech_engine_logs",
    "analyze_chart",
    "save_temp_image",
    "open_hud",
    "close_hud",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();
    // Native tests link Wry's TaskDialogIndirect, which needs Common Controls
    // v6. The suite binary gets this manifest from tauri-build; lib tests don't.
    if cfg!(windows) && std::env::var_os("CARGO_FEATURE_NATIVE_TESTS").is_some() {
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'");
    }
}
