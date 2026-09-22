const COMMANDS: &[&str] = &[
    "list_scripts",
    "open_scripts_folder",
    "read_script",
    "check_script",
    "save_script",
    "list_versions",
    "read_version",
    "restore_version",
    "create_script",
    "delete_script",
    "lint_script",
    "script_python",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();
}
