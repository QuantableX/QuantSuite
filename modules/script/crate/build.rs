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
    "collection_sources",
    "collection_source_save",
    "collection_source_delete",
    "collection_token_set",
    "collection_token_clear",
    "collection_catalog",
    "collection_item",
    "collection_plan",
    "collection_install",
    "collection_remove",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();
}
