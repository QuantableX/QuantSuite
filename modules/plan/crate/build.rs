const COMMANDS: &[&str] = &[
    "list_calendars",
    "create_calendar",
    "update_calendar",
    "delete_calendar",
    "get_event",
    "create_event",
    "update_event",
    "delete_event",
    "list_occurrences",
    "override_occurrence",
    "truncate_series",
    "find_free_slots",
    "list_reminders",
    "set_reminders",
    "search",
    "get_setting",
    "set_setting",
    "get_app_settings",
    "update_app_settings",
    "backup_database",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();
}
