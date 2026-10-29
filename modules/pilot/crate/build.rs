const COMMANDS: &[&str] = &[
    "pilot_status",
    "pilot_contexts",
    "pilot_settings_get",
    "pilot_settings_set",
    "pilot_sessions_list",
    "pilot_session_create",
    "pilot_session_open",
    "pilot_session_launch",
    "pilot_session_stop",
    "pilot_session_delete",
    "pilot_session_update",
    "pilot_runtime_state",
    "pilot_thread_create",
    "pilot_thread_snapshot",
    "pilot_thread_send",
    "pilot_thread_answer",
    "pilot_thread_interrupt",
    "pilot_thread_update",
    "pilot_provider_catalog",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();
}
