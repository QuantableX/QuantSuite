const COMMANDS: &[&str] = &[
    "list_items",
    "get_item",
    "create_item",
    "update_item",
    "delete_item",
    "summary",
    "sankey",
    "goals",
    "create_target",
    "update_target",
    "delete_target",
    "get_app_settings",
    "update_app_settings",
    "backup_database",
    "funds_overview",
    "save_fund_account",
    "add_fund_entry",
    "delete_fund_record",
    "save_fund_plan",
    "book_fund_plan",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();
}
