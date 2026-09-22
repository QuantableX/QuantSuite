const COMMANDS: &[&str] = &[
    "list_notes",
    "get_note",
    "create_note",
    "update_note_meta",
    "set_note_property",
    "save_note_content",
    "move_note",
    "archive_note",
    "restore_note",
    "delete_note",
    "empty_trash",
    "list_backlinks",
    "list_properties",
    "create_property",
    "update_property",
    "delete_property",
    "list_views",
    "create_view",
    "update_view",
    "delete_view",
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
