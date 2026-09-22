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
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();
}
