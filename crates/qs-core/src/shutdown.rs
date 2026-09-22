//! The one real exit path (ARCHITECTURE.md §10).
//!
//! Quit is deliberate and ordered: stop taking work, flush databases, then
//! exit. Modules register teardown hooks here and never call
//! `std::process::exit` themselves — doing so skips every step below and loses
//! data.
//!
//! Stopping child processes is part of step 1, not a step of its own: since
//! the supervisor became a register (see `processes.rs`), nothing but the
//! owning module holds a `Child`, so a module that starts a process registers
//! the hook that kills it. `systems` is the worked example.

use crate::{db::Db, window};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, Runtime};

type Hook = Box<dyn Fn() + Send + Sync + 'static>;

#[derive(Default)]
pub struct Teardown(Mutex<Vec<(String, Hook)>>);

impl Teardown {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Register a module teardown hook. This is where a module kills the processes
/// it owns — nothing else can, and nothing the suite started may outlive it.
pub fn on_shutdown<R: Runtime>(app: &AppHandle<R>, name: impl Into<String>, hook: Hook) {
    if let Some(t) = app.try_state::<Teardown>() {
        if let Ok(mut hooks) = t.0.lock() {
            hooks.push((name.into(), hook));
        }
    }
}

/// Ordered teardown, then exit. Safe to call from any thread.
pub fn quit<R: Runtime>(app: &AppHandle<R>) {
    if window::is_quitting() {
        return; // already shutting down; ignore a second Quit
    }
    window::set_quitting();
    log::info!("shutdown: starting");

    // 1. Module teardown hooks — including every child process, which only
    //    its owning module can stop.
    if let Some(t) = app.try_state::<Teardown>() {
        if let Ok(hooks) = t.0.lock() {
            for (name, hook) in hooks.iter() {
                log::info!("shutdown: teardown hook '{name}'");
                hook();
            }
        }
    }

    // 2. Flush the database. WAL checkpoint so core.db is complete on disk.
    if let Some(dbs) = app.try_state::<Db>() {
        if let Ok(conn) = dbs.0.lock() {
            let _ = conn.pragma_update(None, "wal_checkpoint", "TRUNCATE");
        }
    }

    log::info!("shutdown: complete");
    app.exit(0);
}
