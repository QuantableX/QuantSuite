//! Two things that only matter when something has already gone wrong: a log
//! file, and a watchdog that notices the main thread has stopped answering.
//!
//! The suite hung four times in release builds before either existed. Windows
//! recorded `AppHangB1` for quantsuite.exe and that was the *entire* evidence
//! trail — `log::info!` calls throughout the workspace went nowhere, because no
//! logger was ever installed, and nothing measured the event loop. The cause
//! (blocking `#[tauri::command]`s, which Tauri runs on the main thread) had to
//! be found by reading every command in the workspace.
//!
//! Both are deliberately dependency-free: `log` is already a workspace crate,
//! and a hundred lines here beat pulling a logging framework into a binary
//! whose whole diagnostic need is "append a line to a file".

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::thread::ThreadId;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Runtime};

/// Rotate at 5 MB, keeping one previous file. Enough to cover a long session,
/// small enough that nobody has to think about it.
const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

/// How often the watchdog pings the main thread.
const PROBE_INTERVAL: Duration = Duration::from_secs(2);

/// How long a ping may take before the window counts as hung. Five seconds is
/// not an arbitrary number: it is the threshold at which Windows itself greys
/// the window out and appends "Keine Rückmeldung" to its title.
const STALL_LIMIT: Duration = Duration::from_secs(5);

/// A ping slower than this is not yet a hang, but it is the shape of one.
const SLOW_PING: Duration = Duration::from_millis(500);

/// While a hang lasts, repeat the in-flight snapshot this often. One line at
/// the five-second mark can miss the command that actually did it — but a hung
/// app must still not spend its time writing the same line every two seconds.
const STALL_REPORT_INTERVAL: Duration = Duration::from_secs(60);

// ── the log file ────────────────────────────────────────────────────────────

struct FileLogger {
    level: log::LevelFilter,
    path: PathBuf,
    file: Mutex<Option<File>>,
}

impl FileLogger {
    fn open(&self) -> Option<File> {
        OpenOptions::new().create(true).append(true).open(&self.path).ok()
    }

    fn is_full(file: &File) -> bool {
        file.metadata().map(|m| m.len() >= MAX_LOG_BYTES).unwrap_or(false)
    }
}

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "[{}] {:<5} {} — {}\n",
            chrono::Utc::now().to_rfc3339(),
            record.level(),
            record.target(),
            record.args()
        );

        // A logger that panics turns a diagnosable problem into a crash, so
        // every failure here is swallowed: no file, no log, app unaffected.
        let Ok(mut slot) = self.file.lock() else { return };
        if slot.is_none() {
            *slot = self.open();
        }

        // Rotation is checked *after* opening. Checking only an already-open
        // handle meant the very first write of a session skipped the check —
        // so a restart appended to an oversized file and never rotated again.
        // The handle is closed before the rename so the swap is unambiguous.
        if slot.as_ref().is_some_and(Self::is_full) {
            *slot = None;
            let _ = std::fs::rename(&self.path, self.path.with_extension("log.1"));
            *slot = self.open();
        }

        if let Some(file) = slot.as_mut() {
            let _ = file.write_all(line.as_bytes());
        }
    }

    fn flush(&self) {
        if let Ok(mut slot) = self.file.lock() {
            if let Some(file) = slot.as_mut() {
                let _ = file.flush();
            }
        }
    }
}

/// Install the file logger, if it is not already installed. Level comes from
/// `QS_LOG` (`error`…`trace`), defaulting to `info`.
///
/// Public because a second launch needs it: `qs-single-instance` runs before
/// this plugin's setup, and the one line it may have to write — that the
/// running instance is hung — is the most useful line in the file.
pub fn ensure_logger() {
    let level = std::env::var("QS_LOG")
        .ok()
        .and_then(|v| v.parse::<log::LevelFilter>().ok())
        .unwrap_or(log::LevelFilter::Info);

    let logger = FileLogger {
        level,
        path: crate::paths::logs_dir().join("quantsuite.log"),
        file: Mutex::new(None),
    };

    // A second install would be a bug, not an emergency — `set_boxed_logger`
    // fails rather than panicking, and the first logger keeps working.
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(level);
    }
}

// ── what the main thread is busy with ───────────────────────────────

/// The thread [`install`] ran on — Tauri's event loop, and the thread a
/// synchronous `#[tauri::command]` executes on. Captured rather than assumed,
/// so the flag on a snapshot line means something on every platform.
static MAIN_THREAD: OnceLock<ThreadId> = OnceLock::new();

/// Commands currently executing. The watchdog reads this the moment the event
/// loop stops answering, which is the one thing the old log could never say:
/// *which* command was holding it. Keyed by call number so the map stays
/// ordered by start time without sorting on every insert.
static IN_FLIGHT: Mutex<BTreeMap<u64, InFlight>> = Mutex::new(BTreeMap::new());
static NEXT_CALL: AtomicU64 = AtomicU64::new(1);

struct InFlight {
    command: String,
    since: Instant,
    on_main: bool,
}

/// Drops the call out of [`IN_FLIGHT`] when the command returns — including
/// when it returns by unwinding.
struct CallGuard(u64);

impl Drop for CallGuard {
    fn drop(&mut self) {
        if let Ok(mut map) = IN_FLIGHT.lock() {
            map.remove(&self.0);
        }
    }
}

fn enter(command: &str) -> CallGuard {
    let id = NEXT_CALL.fetch_add(1, Ordering::Relaxed);
    let on_main = MAIN_THREAD.get() == Some(&std::thread::current().id());
    if let Ok(mut map) = IN_FLIGHT.lock() {
        map.insert(
            id,
            InFlight { command: command.to_string(), since: Instant::now(), on_main },
        );
    }
    CallGuard(id)
}

/// One line naming every command still running, oldest first.
///
/// `try_lock`, deliberately: this is called from a watchdog that has already
/// found the app wedged, and a diagnostic which can itself block is worse than
/// no diagnostic at all.
fn in_flight_report() -> String {
    let Ok(map) = IN_FLIGHT.try_lock() else {
        return "in flight: unknown (the registry was locked)".into();
    };
    if map.is_empty() {
        return "nothing in flight — the main thread is blocked outside a command".into();
    }
    let listed: Vec<String> = map
        .values()
        .map(|c| {
            let flag = if c.on_main { ", ON THE MAIN THREAD" } else { "" };
            format!("{} ({}ms{flag})", c.command, c.since.elapsed().as_millis())
        })
        .collect();
    format!("in flight: {}", listed.join(", "))
}

/// Wrap a plugin's generated invoke handler so the watchdog can see what it is
/// running. Every plugin in the suite registers through this.
///
/// What it measures depends on how the command was declared, and that
/// difference is the point. A synchronous command runs to completion *inside*
/// this call, on the main thread, so its whole duration is recorded. An `async`
/// one — including `#[tauri::command(async)]` — is only registered for the
/// moment it takes to hand the future to the runtime. So a command that appears
/// in a stall report flagged `ON THE MAIN THREAD` is, by construction, one that
/// is still declared synchronously, and the fix for it is one word long.
pub fn traced<R, F>(handler: F) -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static
where
    R: Runtime,
    F: Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static,
{
    move |invoke| {
        let _guard = enter(invoke.message.command());
        handler(invoke)
    }
}

// ── the watchdog ────────────────────────────────────────────────────────────

/// Ping the main thread forever and report when it stops answering.
///
/// The probe is a closure posted to the event loop, so what it measures is
/// exactly what Windows measures: how long a queued main-thread task waits. A
/// blocking command, a flood of `emit`s or a deadlock all show up here as one
/// line in the log, with the duration.
fn install_watchdog<R: Runtime>(app: AppHandle<R>) {
    std::thread::spawn(move || {
        let mut stalled_since: Option<Instant> = None;
        let mut last_report = Instant::now();

        loop {
            std::thread::sleep(PROBE_INTERVAL);

            let (tx, rx) = mpsc::channel();
            let sent = Instant::now();
            // Err means the app is going away — stop probing, quietly.
            if app.run_on_main_thread(move || { let _ = tx.send(()); }).is_err() {
                return;
            }

            match rx.recv_timeout(STALL_LIMIT) {
                Ok(()) => {
                    let waited = sent.elapsed();
                    if let Some(since) = stalled_since.take() {
                        log::error!(
                            "main thread responsive again after {:.1}s — the window was hung; \
                             look for a blocking command or an emit storm around this timestamp",
                            since.elapsed().as_secs_f32()
                        );
                    } else if waited > SLOW_PING {
                        log::warn!(
                            "main thread took {}ms to answer — the UI stuttered for that long",
                            waited.as_millis()
                        );
                    }
                }
                Err(_) => {
                    // The transition carries the snapshot that names the cause;
                    // after that, one line a minute for as long as it lasts. A
                    // hung app must not spend its time writing the same line
                    // every two seconds, but a hang that outlives its first
                    // report should still leave a trail.
                    if stalled_since.is_none() {
                        stalled_since = Some(sent);
                        last_report = sent;
                        log::error!(
                            "main thread has not answered for {}s — the window is hung \
                             (Windows will report \"Keine Rückmeldung\") — {}",
                            STALL_LIMIT.as_secs(),
                            in_flight_report()
                        );
                    } else if last_report.elapsed() >= STALL_REPORT_INTERVAL {
                        last_report = Instant::now();
                        log::error!(
                            "main thread still hung after {:.0}s — {}",
                            stalled_since.map(|s| s.elapsed().as_secs_f32()).unwrap_or_default(),
                            in_flight_report()
                        );
                    }
                }
            }
        }
    });
}

/// Called once from the plugin's setup, before anything else logs.
pub fn install<R: Runtime>(app: AppHandle<R>) {
    // Setup runs on Tauri's event loop thread — the same thread a synchronous
    // command later executes on. Recording it here is what lets a stall report
    // say whether the command it found is the one doing the blocking.
    let _ = MAIN_THREAD.set(std::thread::current().id());
    ensure_logger();
    install_watchdog(app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::Log;

    fn logger_at(dir: &std::path::Path, name: &str) -> FileLogger {
        std::fs::create_dir_all(dir).unwrap();
        FileLogger {
            level: log::LevelFilter::Info,
            path: dir.join(name),
            file: Mutex::new(None),
        }
    }

    fn record(logger: &FileLogger, msg: &str) {
        logger.log(
            &log::Record::builder()
                .args(format_args!("{msg}"))
                .level(log::Level::Warn)
                .target("test")
                .build(),
        );
        logger.flush();
    }

    #[test]
    fn writes_the_line_to_its_file() {
        let dir = std::env::temp_dir().join("qs-diag-write");
        let _ = std::fs::remove_dir_all(&dir);
        let logger = logger_at(&dir, "quantsuite.log");

        record(&logger, "main thread has not answered");

        let written = std::fs::read_to_string(&logger.path).unwrap();
        assert!(written.contains("main thread has not answered"));
        assert!(written.contains("WARN"), "the level has to survive: {written}");
    }

    /// A log that grows forever is its own outage, so the cap is enforced and
    /// exactly one previous generation is kept.
    #[test]
    fn rotates_once_past_the_cap() {
        let dir = std::env::temp_dir().join("qs-diag-rotate");
        let _ = std::fs::remove_dir_all(&dir);
        let logger = logger_at(&dir, "quantsuite.log");

        std::fs::write(&logger.path, vec![b'x'; MAX_LOG_BYTES as usize + 1]).unwrap();
        record(&logger, "after the cap");

        let current = std::fs::read_to_string(&logger.path).unwrap();
        assert!(current.contains("after the cap"));
        assert!(current.len() < MAX_LOG_BYTES as usize, "a fresh file, not the old one");
        assert!(
            dir.join("quantsuite.log.1").exists(),
            "the previous generation must be kept, not dropped"
        );
    }

    #[test]
    fn drops_records_below_the_level() {
        let dir = std::env::temp_dir().join("qs-diag-level");
        let _ = std::fs::remove_dir_all(&dir);
        let logger = logger_at(&dir, "quantsuite.log");

        logger.log(
            &log::Record::builder()
                .args(format_args!("chatter"))
                .level(log::Level::Debug)
                .target("test")
                .build(),
        );

        assert!(!logger.path.exists(), "a filtered record must not even open the file");
    }

    /// The snapshot is the whole point of the registry: a stall that names no
    /// command is no more useful than the bare "hung" line it replaced.
    ///
    /// Deliberately written to tolerate other tests sharing the global registry
    /// — it asserts on *this* call appearing and disappearing, never on the
    /// registry being empty.
    #[test]
    fn a_running_command_shows_up_until_it_returns() {
        let report = {
            let _call = enter("plugin:test|slow_thing");
            in_flight_report()
        };
        assert!(
            report.contains("plugin:test|slow_thing"),
            "a command in flight has to be named: {report}"
        );

        assert!(
            !in_flight_report().contains("plugin:test|slow_thing"),
            "the guard must drop the entry when the command returns"
        );
    }

    /// The `ON THE MAIN THREAD` flag is what turns a snapshot into a verdict, so
    /// it must never appear on a thread that is not the recorded one. Here
    /// `install` never ran, so nothing may be flagged.
    #[test]
    fn only_the_recorded_main_thread_is_flagged() {
        let _call = enter("plugin:test|elsewhere");
        let report = in_flight_report();
        assert!(report.contains("plugin:test|elsewhere"));
        assert!(
            !report.contains("ON THE MAIN THREAD"),
            "nothing may claim the main thread when it was never recorded: {report}"
        );
    }
}
