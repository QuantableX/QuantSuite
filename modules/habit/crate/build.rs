const COMMANDS: &[&str] = &[
    "list_habits",
    "create_habit",
    "update_habit",
    "pause_habit",
    "resume_habit",
    "delete_habit",
    "set_check",
    "year_checks",
    "stats",
    "get_app_settings",
    "update_app_settings",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();
}
