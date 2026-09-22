//! The suite's process register (ARCHITECTURE.md §6).
//!
//! # Why this owns nothing
//!
//! This file used to be `supervisor.rs`: it spawned children, piped their
//! stdout/stderr into a ring buffer and restarted them by policy. It had **no
//! caller** — `Supervisor::register` was never invoked once — and the audit
//! that asked why found the same answer in all four modules that start
//! processes:
//!
//! | module   | starts                | why an owner could not work                         |
//! |----------|-----------------------|-----------------------------------------------------|
//! | systems  | `python -m rotation_lab` | takes `stdin`/`stdout` for JSON-RPC; a log pump eats the answer channel |
//! | algo     | `quantalgo.runner`    | same: `stdin` for commands, `stdout` for the event stream |
//! | mcp      | MCP server            | `tokio::process::Child`, stdio on `null`, own manager |
//! | control  | Docker containers     | not child processes of this app at all              |
//!
//! A process whose pipes carry a protocol cannot be owned by something that
//! only knows how to tail them. So the module keeps its process — it is the
//! only party that knows the protocol — and tells this register *what* it
//! started. The register is a **map, not a tool**.
//!
//! Consequences, deliberately accepted:
//!
//! * There is no `RestartPolicy` and no reaper. A register cannot relaunch a
//!   process whose pipes belong to someone else, and a half-restart that
//!   leaves the owner holding a dead handle is worse than none.
//! * There are no captured logs here. A module that can offer a log tail
//!   points at its own command (`logs_command`); one that cannot leaves the
//!   field `None` and the process page hides the button.
//! * The state below is **reported, not observed** — see [`mark_running`].
//!
//! If you are about to add `Command::spawn` to this file: don't. That is the
//! design that was removed, and the table above is why.
//!
//! # How a module registers
//!
//! In the module plugin's `setup`, once, with the process *not* running — the
//! page must be able to list a stopped process, or it can never be started:
//!
//! ```ignore
//! qs_core::processes::announce(
//!     app,
//!     qs_core::processes::ProcessInfo::new("algo.bot", "algo", "Bot Runner")
//!         .start_with("plugin:algo|start_bot")
//!         .stop_with("plugin:algo|stop_bot")
//!         .logs_with("plugin:algo|process_bot_logs"),
//! );
//! ```
//!
//! Then, wherever the process actually starts, stops or dies:
//!
//! ```ignore
//! qs_core::processes::mark_running(&app, "algo.bot", Some(pid));
//! qs_core::processes::mark_stopped(&app, "algo.bot");
//! qs_core::processes::mark_failed(&app, "algo.bot", "spawn failed: …");
//! ```
//!
//! # How the shell learns of a change
//!
//! Because the state is reported, it changes only when one of the `mark_*`
//! calls above lands — so every real transition is announced on the bus as
//! [`CHANGED_TOPIC`] and the shell subscribes instead of polling `process_list`
//! (a suite that sits in the tray for days must not clone this map every few
//! seconds to keep one rail dot current). Only entries with a
//! `refresh_command` are still polled, slowly, by the process pages: their
//! modules learn of an exit by asking, and asking is what makes them report.

use crate::bus;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, Runtime};

/// Bus topic of a real transition: payload `{ "id": "<process id>", "state":
/// <ProcessState> }`. A `mark_*` that restates the current state emits nothing.
pub const CHANGED_TOPIC: &str = "core.process.changed";

/// What a module last reported about its process.
///
/// Serialised internally tagged, so the frontend gets a discriminated union:
/// `{ "state": "running", "pid": 1234, "since": 1755412345678 }`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum ProcessState {
    /// Announced but not started — the state every entry begins in.
    #[default]
    Stopped,
    Running {
        /// `None` for things that have no OS process id of their own: a Docker
        /// container is a process on the daemon's side, not a child of ours.
        pid: Option<u32>,
        /// Unix millis of the transition, for an uptime readout.
        since: i64,
    },
    /// A start that did not come up, or an exit the module treats as an error.
    /// The message is shown verbatim on the process page.
    Failed { message: String },
}

/// One process a module has announced.
///
/// The three command fields hold **full invoke names** (`plugin:<module>|<cmd>`),
/// because that is what the shell passes to `invoke`. They must honour one
/// uniform signature so the page can call them without knowing the module:
///
/// * `start_command` / `stop_command` — no arguments.
/// * `logs_command` — `{ limit: number }`, returns `string[]` (oldest first).
///
/// Where a module's existing command does not fit, it adds a thin wrapper
/// pointing at the real one rather than changing it — the module's own UI keeps
/// calling the original.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    /// Stable, module-prefixed key: `systems.engine`, `algo.bot`.
    pub id: String,
    /// Owning module id, as in `modules/<id>/module.json`.
    pub module: String,
    /// Human label for the process page.
    pub label: String,
    #[serde(default)]
    pub status: ProcessState,
    #[serde(default)]
    pub start_command: Option<String>,
    #[serde(default)]
    pub stop_command: Option<String>,
    #[serde(default)]
    pub logs_command: Option<String>,
    /// Only for modules that learn of an exit by asking, not by being told.
    /// `systems` and `algo` hold the child and report the moment it dies; the
    /// MCP servers and the docker stack are only ever observed by polling, so
    /// without this the page would keep claiming "running" for something that
    /// died while the user was looking at it.
    #[serde(default)]
    pub refresh_command: Option<String>,
}

impl ProcessInfo {
    /// A stopped entry with no commands yet — chain `start_with`/`stop_with`/
    /// `logs_with` for the ones the module can serve.
    pub fn new(id: impl Into<String>, module: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            module: module.into(),
            label: label.into(),
            status: ProcessState::Stopped,
            start_command: None,
            stop_command: None,
            logs_command: None,
            refresh_command: None,
        }
    }

    /// Full invoke name of a no-argument start command.
    #[must_use]
    pub fn start_with(mut self, command: impl Into<String>) -> Self {
        self.start_command = Some(command.into());
        self
    }

    /// Full invoke name of a no-argument stop command.
    #[must_use]
    pub fn stop_with(mut self, command: impl Into<String>) -> Self {
        self.stop_command = Some(command.into());
        self
    }

    /// Full invoke name of a `{ limit }` → `Vec<String>` log command.
    #[must_use]
    pub fn logs_with(mut self, command: impl Into<String>) -> Self {
        self.logs_command = Some(command.into());
        self
    }

    /// Full invoke name of a no-argument command that makes the module re-check
    /// this process and call `mark_*`. The page runs it before each poll. Its
    /// return value is discarded — the truth arrives through the register.
    #[must_use]
    pub fn refresh_with(mut self, command: impl Into<String>) -> Self {
        self.refresh_command = Some(command.into());
        self
    }
}

/// The map. Managed as Tauri state by [`crate::init`]; modules reach it
/// through the free functions below rather than by name.
#[derive(Default)]
pub struct ProcessRegistry {
    /// `BTreeMap`, not `HashMap`: the process pages repaint this list on every
    /// refresh, and rows that reshuffle between refreshes are unusable. Ids are
    /// module-prefixed, so key order groups by module for free.
    inner: Mutex<BTreeMap<String, ProcessInfo>>,
}

impl ProcessRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or update an entry.
    ///
    /// A re-announce (a module plugin set up twice, a hot reload) keeps the
    /// status that is already there: the descriptive half is what the module
    /// is restating, and overwriting a `Running` with the announcement's
    /// `Stopped` would make the page lie about a process that is up.
    pub fn announce(&self, info: ProcessInfo) {
        let Ok(mut map) = self.inner.lock() else { return };
        match map.get_mut(&info.id) {
            Some(existing) => {
                let status = existing.status.clone();
                *existing = info;
                existing.status = status;
            }
            None => {
                map.insert(info.id.clone(), info);
            }
        }
    }

    /// Record a reported transition. Unknown ids are ignored — announcing is
    /// the contract, and silently creating a half-filled entry here would hide
    /// the module's missing `announce` behind a row with no buttons.
    ///
    /// Returns whether the entry's state actually changed. A module restating
    /// what the map already holds (`mark_stopped` from both a stop command and
    /// the reader thread that notices the closed pipe — `systems` does exactly
    /// that) is not a transition, and [`set`] announces only transitions.
    pub fn set_state(&self, id: &str, state: ProcessState) -> bool {
        let Ok(mut map) = self.inner.lock() else { return false };
        match map.get_mut(id) {
            Some(entry) if entry.status == state => false,
            Some(entry) => {
                entry.status = state;
                true
            }
            None => {
                log::warn!("processes: state reported for unannounced id '{id}'");
                false
            }
        }
    }

    /// Every announced process, ordered by id.
    pub fn list(&self) -> Vec<ProcessInfo> {
        self.inner
            .lock()
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default()
    }
}

// ── The module-facing API ───────────────────────────────────────────────────
//
// Free functions over `AppHandle`, like `tray::register_entry`: every module
// crate already holds the shared handle, and none of them should have to name
// `ProcessRegistry` or worry about whether the state exists yet. All of these
// are no-ops when it does not (a module used outside the suite binary).

/// Declare a process. Call once, from the module plugin's `setup`, with the
/// status left at `Stopped` — the page lists stopped processes so they can be
/// started from there.
pub fn announce<R: Runtime>(app: &AppHandle<R>, info: ProcessInfo) {
    if let Some(reg) = app.try_state::<ProcessRegistry>() {
        reg.announce(info);
    }
}

/// Report that the process is up.
///
/// The register believes this. It holds no handle, so it cannot verify — and a
/// pid check would not help: while the owning module still holds an unreaped
/// `Child`, the pid stays valid on both Windows and Unix even after the process
/// died, so the check would confirm a corpse. Detecting an exit is the owner's
/// job, at the place where it already learns of one (a closed stdout, a
/// `try_wait`, a container event) — see `systems`' reader loop for the pattern.
pub fn mark_running<R: Runtime>(app: &AppHandle<R>, id: &str, pid: Option<u32>) {
    set(
        app,
        id,
        ProcessState::Running {
            pid,
            since: chrono::Utc::now().timestamp_millis(),
        },
    );
}

/// Report that the process is down — a deliberate stop *or* an observed exit.
pub fn mark_stopped<R: Runtime>(app: &AppHandle<R>, id: &str) {
    set(app, id, ProcessState::Stopped);
}

/// Report that the process could not start, or exited badly. `message` is
/// shown to the user unchanged.
pub fn mark_failed<R: Runtime>(app: &AppHandle<R>, id: &str, message: impl Into<String>) {
    set(
        app,
        id,
        ProcessState::Failed {
            message: message.into(),
        },
    );
}

/// Record the transition and, if it is one, announce it (see the module doc).
///
/// The emit happens after the register's lock is released: bus delivery runs
/// Rust subscribers inline, and one of those reading `list()` would deadlock.
fn set<R: Runtime>(app: &AppHandle<R>, id: &str, state: ProcessState) {
    let Some(reg) = app.try_state::<ProcessRegistry>() else { return };
    if !reg.set_state(id, state.clone()) {
        return;
    }
    if let Err(e) = bus::emit(app, CHANGED_TOPIC, serde_json::json!({ "id": id, "state": state })) {
        // The map is already right; only the push notification was lost. The
        // pages re-read the register on their next refresh regardless.
        log::warn!("processes: could not announce '{id}': {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running(pid: u32) -> ProcessState {
        ProcessState::Running { pid: Some(pid), since: 1 }
    }

    #[test]
    fn announce_keeps_the_reported_status() {
        let reg = ProcessRegistry::new();
        reg.announce(ProcessInfo::new("t.proc", "t", "Proc"));
        assert!(reg.set_state("t.proc", running(7)));

        // A re-announce (hot reload, plugin set up twice) restates the
        // descriptive half and must not reset a live process to Stopped.
        reg.announce(ProcessInfo::new("t.proc", "t", "Proc renamed").stop_with("plugin:t|stop"));
        let entry = &reg.list()[0];
        assert_eq!(entry.label, "Proc renamed");
        assert_eq!(entry.stop_command.as_deref(), Some("plugin:t|stop"));
        assert_eq!(entry.status, running(7));
    }

    #[test]
    fn set_state_reports_changed_versus_unchanged() {
        let reg = ProcessRegistry::new();
        reg.announce(ProcessInfo::new("t.proc", "t", "Proc"));

        // Every entry starts Stopped, so restating it is not a transition.
        assert!(!reg.set_state("t.proc", ProcessState::Stopped));
        assert!(reg.set_state("t.proc", running(7)));
        assert!(!reg.set_state("t.proc", running(7)));
        // A fresh `since` is a fresh start: the uptime readout must follow it.
        assert!(reg.set_state("t.proc", ProcessState::Running { pid: Some(7), since: 2 }));
        assert!(reg.set_state("t.proc", ProcessState::Failed { message: "boom".into() }));
        assert!(!reg.set_state("t.proc", ProcessState::Failed { message: "boom".into() }));
        assert!(reg.set_state("t.proc", ProcessState::Stopped));

        // Unannounced ids are ignored, not created — nothing changed.
        assert!(!reg.set_state("t.ghost", running(1)));
        assert_eq!(reg.list().len(), 1);
    }
}
