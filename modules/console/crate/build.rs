/// Every command the plugin exposes. Tauri derives the permission identifiers
/// from this list; a command missing here is denied at runtime, per call, with
/// nothing at build time (ARCHITECTURE.md §2).
const COMMANDS: &[&str] = &[
    "open_session",
    "write_session",
    "resize_session",
    "close_session",
    "detach_session",
    "attach_session",
    "session_alive",
    "list_sessions",
    "list_shells",
    "save_pasted_image",
    "run_command",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();
}
