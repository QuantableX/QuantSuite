//! The terminal drawer (card 13): plain shells in a thread's checkout, and
//! the project actions' Run terminals. Every PTY lives in the console's one
//! registry under its own owner (`pilot-drawer`), so the console's tab bar
//! never adopts one and QuantPilot's V2 terminals stay apart. An action's
//! command is typed into its shell, so the shell stays when it ends; Run
//! again reuses the action's live terminal (Ctrl+C, then the command).

use crate::db::ThreadMeta;
use crate::settings::{ProjectAction, Settings};
use qs_mod_console::{ConsoleState, OpenSessionRequest};
use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};

pub const OWNER: &str = "pilot-drawer";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DrawerTerm {
    pub pty_id: String,
    pub thread_id: String,
    /// `Terminal`, or the action's name.
    pub label: String,
    /// The project action this terminal runs.
    pub action: Option<String>,
    pub cwd: String,
    pub created_at: i64,
}

#[derive(Default)]
pub struct Drawer {
    terms: Mutex<Vec<DrawerTerm>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// What runs on a hook.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Hook {
    WorktreeCreate,
    Settle,
}

/// A project's actions (by its context id).
pub fn actions_of<'a>(settings: &'a Settings, project: &str) -> &'a [ProjectAction] {
    settings.actions.get(project).map(Vec::as_slice).unwrap_or_default()
}

impl Drawer {
    /// The thread's terminals that are still alive.
    pub fn list(&self, console: &ConsoleState, thread_id: &str) -> Vec<DrawerTerm> {
        let mut terms = lock(&self.terms);
        terms.retain(|t| console.ptys.contains(&t.pty_id));
        terms.iter().filter(|t| t.thread_id == thread_id).cloned().collect()
    }

    pub fn forget(&self, pty_id: &str) {
        lock(&self.terms).retain(|t| t.pty_id != pty_id);
    }

    /// A new shell in `cwd`; with `action`, its command typed in once the shell is up.
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        &self,
        app: &AppHandle,
        target: &str,
        shell: &str,
        thread_id: &str,
        cwd: &str,
        action: Option<&ProjectAction>,
        size: (Option<u16>, Option<u16>),
    ) -> Result<DrawerTerm, String> {
        let console = app.try_state::<ConsoleState>().ok_or("the console is not running")?;
        if let Some(a) = action {
            // Run again: the action's live terminal, interrupted and run anew.
            let live = self.list(&console, thread_id).into_iter().find(|t| t.action.as_deref() == Some(a.id.as_str()));
            if let Some(t) = live {
                let _ = console.ptys.write(&t.pty_id, b"\x03");
                type_later(app, &t.pty_id, &a.command, Duration::from_millis(300));
                return Ok(t);
            }
        }
        let pty_id = qs_mod_console::spawn_session(
            app,
            &console,
            target,
            OpenSessionRequest {
                cwd: cwd.to_string(),
                shell: Some(shell.to_string()).filter(|s| !s.is_empty()),
                cols: size.0,
                rows: size.1,
                owner: Some(OWNER.into()),
                args: Vec::new(),
                env: Vec::new(),
            },
        )?;
        if let Some(a) = action {
            type_later(app, &pty_id, &a.command, Duration::from_millis(500));
        }
        let term = DrawerTerm {
            pty_id,
            thread_id: thread_id.to_string(),
            label: action.map(|a| a.name.clone()).unwrap_or_else(|| "Terminal".into()),
            action: action.map(|a| a.id.clone()),
            cwd: cwd.to_string(),
            created_at: chrono::Utc::now().timestamp_millis(),
        };
        lock(&self.terms).push(term.clone());
        Ok(term)
    }

    /// The project's actions flagged for `hook`, each in its Run terminal.
    pub fn run_hook(&self, app: &AppHandle, settings: &Settings, shell: &str, thread: &ThreadMeta, hook: Hook) -> Vec<String> {
        let mut failed = Vec::new();
        for a in actions_of(settings, &thread.context_id) {
            let wanted = match hook {
                Hook::WorktreeCreate => a.run_on_worktree_create,
                Hook::Settle => a.run_on_settle,
            };
            if wanted {
                if let Err(e) = self.open(app, "main", shell, &thread.id, &thread.cwd, Some(a), (None, None)) {
                    failed.push(format!("{}: {e}", a.name));
                }
            }
        }
        failed
    }
}

/// Type a command into a shell after it had time to start (input typed
/// earlier is buffered by the console, but some shells clear it at start).
fn type_later(app: &AppHandle, pty_id: &str, command: &str, delay: Duration) {
    let (app, pty_id, line) = (app.clone(), pty_id.to_string(), format!("{}\r", command.trim()));
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        if let Some(console) = app.try_state::<ConsoleState>() {
            let _ = console.ptys.write(&pty_id, line.as_bytes());
        }
    });
}

pub fn close(app: &AppHandle, drawer: &Drawer, pty_id: &str) {
    if let Some(console) = app.try_state::<ConsoleState>() {
        let _ = console.ptys.close(pty_id);
        console.remove_meta(pty_id);
    }
    drawer.forget(pty_id);
}
