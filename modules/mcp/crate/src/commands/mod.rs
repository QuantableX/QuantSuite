//! The plugin's `#[tauri::command]`s, one file per family. `lib.rs` keeps
//! `init()`, `AppState` and the handler registration; the command names
//! are what `build.rs`, the generated permissions and the frontend key on,
//! so they never change when a function moves between these files.

pub(crate) mod agentos;
pub(crate) mod clients;
pub(crate) mod kanban;
pub(crate) mod registry;
pub(crate) mod tools;
pub(crate) mod workspace;
