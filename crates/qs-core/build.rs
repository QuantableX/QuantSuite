/// Generates the Tauri 2 permission set for plugin `qs`.
///
/// Every command needs a permission or it is denied at runtime. `tauri_plugin`
/// emits one `allow-<command>` / `deny-<command>` pair per entry here, plus the
/// `default` set declared in `permissions/default.toml`.
const COMMANDS: &[&str] = &[
    "get_settings",
    "get_setting",
    "set_setting",
    "emit_event",
    "recent_events",
    "upsert_entity",
    "delete_entity",
    "list_entities",
    "count_entities",
    "search_entities",
    "link_entities",
    "unlink_entities",
    "linked_entities",
    "process_list",
    "set_circular_window",
    "window_new",
    "window_show",
    "window_hide",
    "window_toggle",
    "quit",
    "autostart_enabled",
    "set_autostart",
    "app_version",
    "suite_paths",
    "agent_tools",
    "agent_tool_decision",
    "agent_pending_calls",
    "agent_call_claim",
    "agent_call_complete",
    "vault_status",
    "vault_unlock",
    "bundle_plan",
    "bundle_export",
    "bundle_export_cancel",
    "import_propose",
    "import_bundle",
    "import_status",
    "server_status",
    "server_set_role",
    "server_set_watchdog",
    "gateway_status",
    "gateway_configure",
    "gateway_pairing_start",
    "gateway_pairing_status",
    "gateway_pairing_confirm",
    "gateway_pairing_cancel",
    "gateway_devices",
    "gateway_device_rename",
    "gateway_device_set_role",
    "gateway_device_revoke",
    "gateway_device_forget",
    "gateway_audit",
    "gateway_set_pin",
    "gateway_pair_begin",
    "gateway_pair_finish",
    "gateway_pair_current",
    "gateway_remote_call",
    "gateway_step_up",
    "gateway_link_status",
    "gateway_link_retry",
    "gateway_use_connection",
    "move_info",
    "move_export",
    "move_send",
    "move_cancel",
    "move_remote_propose",
    "move_remote_import",
    "move_finish",
    "move_propose",
    "move_import",
    "gateway_connections",
    "gateway_connection_test",
    "gateway_connection_forget",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();
    // tests/remote_dispatch_spike.rs builds a Wry app on the mock runtime,
    // which links TaskDialogIndirect from Common Controls v6. The suite binary
    // gets that manifest from tauri-build; test binaries do not (the crash
    // would be STATUS_ENTRYPOINT_NOT_FOUND). Test targets only.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-tests=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'");
    }
}
