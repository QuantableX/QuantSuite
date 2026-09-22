//! The shared PTY layer for QuantSuite (docs/PLAN-CONSOLE.md §4, phase P0).
//!
//! This started life as `modules/canvas/crate/src/commands/terminal.rs` and was
//! lifted here so QuantConsole and QuantCanvas run **one** PTY implementation.
//! Every non-obvious detail below was paid for once already; the comments say
//! which failure each one prevents, because none of them look necessary until
//! they are missing.
//!
//! The crate owns three things and nothing else:
//!
//!   - spawning a shell on a real PTY (ConPTY on Windows) — [`PtySession::spawn`]
//!   - getting its output out at a rate the UI survives — the flusher thread
//!   - surviving a frontend that comes and goes — [`PtyRegistry::detach`] /
//!     [`PtyRegistry::attach_with`]
//!
//! It does **not** know about Tauri, webviews or events. The caller hands in a
//! [`PtySink`] and decides where the bytes go. That is what makes the same
//! session usable by an xterm pane (bytes straight to the webview) and, from
//! phase P1 on, by QuantConsole's VT parser (bytes into a block model).

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(windows)]
mod windows_runtime;

/// Max output retained for a detached session, and the cap on the staging
/// buffer while a child outruns the flusher (256 KB).
pub const MAX_OUTPUT_BUFFER: usize = 256 * 1024;

/// How far past a cut point a line boundary is looked for before the buffer is
/// cut mid-stream anyway.
///
/// A stream with no newline in 8 KB is not line-oriented — a binary on stdout —
/// and scanning the whole buffer on every 4 KB read would turn a flood into
/// quadratic work.
const RESYNC_WINDOW: usize = 8 * 1024;

/// How often staged output is handed to the sink.
///
/// Load-bearing, and the reason the reader thread never calls the sink itself:
/// in the Tauri case every sink call becomes one `EvaluateScript` message **on
/// the main thread**, so the sink rate *is* UI thread load. A build or an
/// `npm install` writes faster than 4 KB per read, which used to mean thousands
/// of events per second — the event loop stopped pumping window messages and
/// Windows logged `AppHangB1` for quantsuite.exe ("Keine Rückmeldung"). One
/// frame's worth of coalescing caps it at ~60 calls/s per session, no matter
/// how loud the child is.
pub const FLUSH_INTERVAL: Duration = Duration::from_millis(16);

/// What a session hands to its sink.
///
/// `Output` is always valid UTF-8 (see [`drain_text`]). `Eof` arrives exactly
/// once, after the last `Output`, when the child has exited and the stream has
/// drained — consumers use it to mark the session dead.
pub enum PtyEvent {
    Output(String),
    /// `bytes` were dropped from the front of the stream before they could be
    /// delivered: the child outran the flusher, or the detach buffer filled.
    ///
    /// It arrives before the `Output` that follows the hole. Consumers that
    /// parse the stream have to say so rather than present the remainder as
    /// continuous — a byte range is missing from the middle of a document.
    Dropped { bytes: usize },
    Eof,
}

/// Where a session's output goes. Called from the flusher thread, never from
/// the reader thread, at most once per [`FLUSH_INTERVAL`].
pub type PtySink = Box<dyn Fn(PtyEvent) + Send + 'static>;

/// How to start a shell.
pub struct PtyOptions {
    pub cwd: String,
    /// Executable to run; `None` picks [`default_shell`].
    pub shell: Option<String>,
    /// argv after the executable. This is how shell integration gets in without
    /// touching a dotfile: `--init-file <hooks>`, `-NoExit -Command . <hooks>`
    /// (docs/PLAN-CONSOLE.md §3).
    pub args: Vec<String>,
    pub cols: u16,
    pub rows: u16,
    /// Extra environment, applied *after* the UTF-8/color defaults below, so a
    /// caller can override them.
    pub env: Vec<(String, String)>,
}

impl Default for PtyOptions {
    fn default() -> Self {
        Self {
            cwd: ".".into(),
            shell: None,
            args: Vec::new(),
            cols: 80,
            rows: 24,
            env: Vec::new(),
        }
    }
}

/// A live PTY plus the two threads that move its output.
///
/// `writer` sits behind its own lock so a blocking write never holds a registry
/// lock. `master` is retained for resizing. `child` is retained so it can be
/// killed — dropping the master alone leaves the process running on Windows.
pub struct PtySession {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    connected: Arc<AtomicBool>,
    output_buffer: Arc<Mutex<VecDeque<u8>>>,
}

/// The two handles [`PtyRegistry::attach_with`] needs, cloned out of a session so
/// the replay does not run under the registry lock.
struct AttachParts {
    connected: Arc<AtomicBool>,
    output_buffer: Arc<Mutex<VecDeque<u8>>>,
}

impl PtySession {
    /// Open a PTY, spawn the shell, and start the reader and flusher threads.
    pub fn spawn(opts: &PtyOptions, sink: PtySink) -> Result<Self, String> {
        #[cfg(windows)]
        windows_runtime::load()?;

        let pair = native_pty_system()
            .openpty(PtySize {
                // A zero dimension reaches the real ConPTY and reflows whatever
                // TUI is running; callers occasionally measure a hidden element.
                rows: opts.rows.max(1),
                cols: opts.cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("failed to open PTY: {e}"))?;

        let shell_path = opts
            .shell
            .clone()
            .unwrap_or_else(|| default_shell().to_string());

        let mut cmd = CommandBuilder::new(&shell_path);
        cmd.cwd(&opts.cwd);
        for arg in &opts.args {
            cmd.arg(arg);
        }

        // Tell child tools that Unicode and 256 colors are available.
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("LANG", "en_US.UTF-8");

        // On Windows, force UTF-8 for Python children — the default code page
        // turns box-drawing output into mojibake.
        #[cfg(windows)]
        {
            cmd.env("PYTHONUTF8", "1");
            cmd.env("PYTHONIOENCODING", "utf-8");
        }

        for (key, value) in &opts.env {
            cmd.env(key, value);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("failed to spawn {shell_path}: {e}"))?;

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("failed to clone PTY reader: {e}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("failed to take PTY writer: {e}"))?;

        let connected = Arc::new(AtomicBool::new(true));
        let output_buffer: Arc<Mutex<VecDeque<u8>>> = Arc::new(Mutex::new(VecDeque::new()));

        // Two threads, deliberately. The reader only ever *stages* bytes; the
        // flusher decides when they leave. Merging them back into one thread
        // that calls the sink per read is the bug this module was hanging on —
        // a blocking `read()` gives no place to rate-limit.
        let staged: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let eof = Arc::new(AtomicBool::new(false));
        // Counted rather than logged, because the consumer is the one that has
        // to render the gap; the flusher reports it on the next tick.
        let dropped = Arc::new(AtomicUsize::new(0));

        let reader_staged = staged.clone();
        let reader_eof = eof.clone();
        let reader_dropped = dropped.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break, // EOF — shell exited
                    Ok(n) => {
                        if let Ok(mut s) = reader_staged.lock() {
                            s.extend_from_slice(&buf[..n]);
                            // A child that outruns the flusher must not grow
                            // this without bound.
                            if s.len() > MAX_OUTPUT_BUFFER {
                                let cut = line_cut(s.iter(), s.len() - MAX_OUTPUT_BUFFER);
                                s.drain(0..cut);
                                reader_dropped.fetch_add(cut, Ordering::Relaxed);
                            }
                        }
                    }
                    Err(e) => {
                        // On Windows, ERROR_BROKEN_PIPE (109) is the normal way
                        // a child exit surfaces. Treat it as EOF, not an error.
                        let is_broken_pipe = e.raw_os_error() == Some(109)
                            || e.kind() == std::io::ErrorKind::BrokenPipe;
                        if !is_broken_pipe {
                            eprintln!("qs-pty: reader error: {e}");
                        }
                        break;
                    }
                }
            }
            reader_eof.store(true, Ordering::Relaxed);
        });

        let flush_connected = connected.clone();
        let flush_buffer = output_buffer.clone();
        let flush_dropped = dropped.clone();
        std::thread::spawn(move || {
            let mut pending: Vec<u8> = Vec::new();
            loop {
                std::thread::sleep(FLUSH_INTERVAL);

                if let Ok(mut s) = staged.lock() {
                    pending.append(&mut s);
                }

                let at_eof = eof.load(Ordering::Relaxed);

                // Where this tick's bytes go is decided **under the buffer
                // lock**, and so is the drain in `PtyRegistry::attach_with`.
                // That is what stops a chunk from being delivered ahead of the
                // older bytes an attach is still replaying, and what stops one
                // from landing in a buffer that was just emptied and will never
                // be read again.
                let live = match flush_buffer.lock() {
                    Ok(mut b) => {
                        let live = flush_connected.load(Ordering::Relaxed);
                        if !live && !pending.is_empty() {
                            // Nobody listening (module switch, closed tab) —
                            // buffer for replay on attach, byte order intact.
                            b.extend(pending.drain(..));
                            if b.len() > MAX_OUTPUT_BUFFER {
                                let cut = line_cut(b.iter(), b.len() - MAX_OUTPUT_BUFFER);
                                b.drain(0..cut);
                                flush_dropped.fetch_add(cut, Ordering::Relaxed);
                            }
                        }
                        live
                    }
                    Err(_) => true,
                };

                if live {
                    // The hole is announced before the bytes that follow it, so
                    // the consumer marks the right part of its document.
                    let lost = flush_dropped.swap(0, Ordering::Relaxed);
                    if lost > 0 {
                        sink(PtyEvent::Dropped { bytes: lost });
                    }
                    if !pending.is_empty() {
                        // At EOF the held-back tail will never be completed, so
                        // it is flushed lossily — otherwise `pending` never
                        // empties and this loop spins forever without ever
                        // signalling EOF.
                        let text = if at_eof {
                            String::from_utf8_lossy(&std::mem::take(&mut pending)).into_owned()
                        } else {
                            drain_text(&mut pending)
                        };
                        if !text.is_empty() {
                            sink(PtyEvent::Output(text));
                        }
                    }
                }

                if at_eof && pending.is_empty() {
                    // EOF is only honoured once `pending` has drained, so it can
                    // never overtake the last real chunk.
                    sink(PtyEvent::Eof);
                    break;
                }
            }
        });

        Ok(Self {
            writer: Arc::new(Mutex::new(writer)),
            master: pair.master,
            child,
            connected,
            output_buffer,
        })
    }

    /// A cheap clone of the writer, for callers that want to drop the registry
    /// lock before a potentially blocking write.
    pub fn writer(&self) -> Arc<Mutex<Box<dyn Write + Send>>> {
        self.writer.clone()
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        self.master
            .resize(PtySize {
                rows: rows.max(1),
                cols: cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("failed to resize PTY: {e}"))
    }

    /// Stop calling the sink and buffer output instead. The child keeps running.
    pub fn detach(&self) {
        self.connected.store(false, Ordering::Relaxed);
    }

    fn attach_parts(&self) -> AttachParts {
        AttachParts {
            connected: self.connected.clone(),
            output_buffer: self.output_buffer.clone(),
        }
    }

    /// Kill the child and reap it. Dropping the session afterwards closes the
    /// master, which unblocks the reader thread.
    pub fn kill(&mut self) -> Result<(), String> {
        self.child.kill().map_err(|e| format!("failed to kill child: {e}"))?;
        let _ = self.child.wait();
        Ok(())
    }
}

/// The id → session map every consumer needs, with the locking discipline that
/// keeps one hung child from blocking every other session.
#[derive(Default)]
pub struct PtyRegistry {
    sessions: Mutex<HashMap<String, PtySession>>,
}

impl PtyRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, id: impl Into<String>, session: PtySession) -> Result<(), String> {
        self.sessions
            .lock()
            .map_err(|e| format!("lock error: {e}"))?
            .insert(id.into(), session);
        Ok(())
    }

    /// Write to a session, holding the map lock only long enough to clone the
    /// writer. A PTY whose child stopped reading would otherwise block spawn,
    /// resize, close and attach for every *other* session.
    pub fn write(&self, id: &str, data: &[u8]) -> Result<(), String> {
        let writer = {
            let sessions = self.sessions.lock().map_err(|e| format!("lock error: {e}"))?;
            sessions
                .get(id)
                .ok_or_else(|| format!("session not found: {id}"))?
                .writer()
        };

        let mut writer = writer.lock().map_err(|e| format!("lock error: {e}"))?;
        writer
            .write_all(data)
            .map_err(|e| format!("failed to write to PTY: {e}"))?;
        writer.flush().map_err(|e| format!("failed to flush PTY: {e}"))
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<(), String> {
        let sessions = self.sessions.lock().map_err(|e| format!("lock error: {e}"))?;
        sessions
            .get(id)
            .ok_or_else(|| format!("session not found: {id}"))?
            .resize(cols, rows)
    }

    pub fn detach(&self, id: &str) -> Result<(), String> {
        let sessions = self.sessions.lock().map_err(|e| format!("lock error: {e}"))?;
        if let Some(session) = sessions.get(id) {
            session.detach();
        }
        Ok(())
    }

    /// Resume streaming and replay what was buffered while detached.
    ///
    /// The drain, the connect flip and `replay` are one critical section, and
    /// that is the whole point. The flusher takes the same buffer lock to decide
    /// whether a chunk goes to the sink or into the buffer, so a chunk arriving
    /// mid-replay waits for it instead of overtaking it. Reconnecting first and
    /// replaying afterwards — the obvious way to write this — lets the flusher
    /// hand the consumer newer bytes while older buffered ones are still on
    /// their way in, and a parser fed out of order emits deltas describing a
    /// document that never existed.
    ///
    /// The registry lock is released first, so a replay that blocks (a parser,
    /// a database write) cannot stall every other session's spawn or close.
    pub fn attach_with<R>(&self, id: &str, replay: impl FnOnce(&str) -> R) -> Result<R, String> {
        let parts = {
            let sessions = self.sessions.lock().map_err(|e| format!("lock error: {e}"))?;
            sessions
                .get(id)
                .ok_or_else(|| format!("session not found: {id}"))?
                .attach_parts()
        };

        replay_and_connect(&parts, replay)
    }

    pub fn contains(&self, id: &str) -> bool {
        self.sessions
            .lock()
            .map(|s| s.contains_key(id))
            .unwrap_or(false)
    }

    pub fn ids(&self) -> Vec<String> {
        self.sessions
            .lock()
            .map(|s| s.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Kill one session and forget it. Unknown ids are not an error — a
    /// frontend closing a pane whose child already exited is normal.
    pub fn close(&self, id: &str) -> Result<(), String> {
        let mut sessions = self.sessions.lock().map_err(|e| format!("lock error: {e}"))?;
        if let Some(mut session) = sessions.remove(id) {
            let _ = session.kill();
        }
        Ok(())
    }

    /// Kill everything. Registered as a shutdown hook by each consumer: in a
    /// fused binary the window closes to tray with shells still alive, so they
    /// are torn down on Quit — a leaked shell would outlive the app entirely.
    pub fn close_all(&self, owner: &str) {
        let Ok(mut sessions) = self.sessions.lock() else {
            return;
        };
        for (id, session) in sessions.iter_mut() {
            if let Err(e) = session.kill() {
                eprintln!("{owner}: failed to kill session {id}: {e}");
            }
        }
        sessions.clear();
    }
}

/// The critical section behind [`PtyRegistry::attach_with`], split out so the
/// ordering it guarantees can be tested without a real child process.
fn replay_and_connect<R>(
    parts: &AttachParts,
    replay: impl FnOnce(&str) -> R,
) -> Result<R, String> {
    let mut buffer = parts
        .output_buffer
        .lock()
        .map_err(|e| format!("lock error: {e}"))?;
    let bytes: Vec<u8> = buffer.drain(..).collect();
    parts.connected.store(true, Ordering::Relaxed);
    Ok(replay(&String::from_utf8_lossy(&bytes)))
}

/// How many bytes to drop from the front of a buffer that is `overflow` bytes
/// too long, so that what survives starts on a line boundary.
///
/// Dropping raw bytes cuts the stream mid-sequence, and both caps here feed a VT
/// parser: what follows half an `OSC 133;D` is a command whose exit code never
/// arrives, and half a `CSI ?1049h` is an alternate screen that never turns on
/// — so a TUI's redraws land in the block list as garbage. A newline is the one
/// byte that appears in no escape sequence, so cutting just after one leaves the
/// parser somewhere it can make sense of, and cannot split a UTF-8 character
/// either. A stream with no newline within [`RESYNC_WINDOW`] has no such point
/// and is cut where it must be.
fn line_cut<'a>(bytes: impl Iterator<Item = &'a u8>, overflow: usize) -> usize {
    let found = bytes
        .skip(overflow)
        .take(RESYNC_WINDOW)
        .position(|b| *b == b'\n');
    match found {
        Some(offset) => overflow + offset + 1,
        None => overflow,
    }
}

/// Take everything that decodes as UTF-8, leaving a truncated tail behind.
///
/// The PTY hands out arbitrary byte boundaries, so a multi-byte character can
/// straddle two reads — and, with coalescing, two flushes. Holding the tail back
/// until its remaining bytes arrive is what keeps box-drawing characters and
/// emoji from turning into replacement glyphs mid-stream.
pub fn drain_text(bytes: &mut Vec<u8>) -> String {
    let valid = match std::str::from_utf8(bytes) {
        Ok(_) => bytes.len(),
        // `error_len() == None` is a truncated sequence at the very end — wait
        // for the rest. A genuinely invalid byte would otherwise wedge the
        // buffer forever, so that case is flushed lossily.
        Err(e) if e.error_len().is_none() => e.valid_up_to(),
        Err(_) => bytes.len(),
    };
    let tail = bytes.split_off(valid);
    let text = String::from_utf8_lossy(bytes).into_owned();
    *bytes = tail;
    text
}

/// The platform's interactive shell, used when a caller names none.
#[cfg(windows)]
pub fn default_shell() -> &'static str {
    if std::path::Path::new("C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe")
        .exists()
    {
        "powershell.exe"
    } else {
        "cmd.exe"
    }
}

/// The platform's interactive shell, used when a caller names none.
#[cfg(not(windows))]
pub fn default_shell() -> &'static str {
    if std::path::Path::new("/bin/zsh").exists() {
        "/bin/zsh"
    } else if std::path::Path::new("/bin/bash").exists() {
        "/bin/bash"
    } else {
        "/bin/sh"
    }
}

#[cfg(test)]
mod tests {
    use super::{
        default_shell, drain_text, line_cut, replay_and_connect, AttachParts, PtyOptions,
        PtyRegistry, RESYNC_WINDOW,
    };
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    /// The coalescing flusher cuts the stream at arbitrary points, so the
    /// boundary case is the whole point of `drain_text`: a character that
    /// straddles two flushes must survive intact rather than become U+FFFD.
    #[test]
    fn holds_back_a_truncated_tail_until_it_completes() {
        // "ä" is 0xC3 0xA4 — split it across two flushes.
        let mut buf = b"caf\xC3".to_vec();
        assert_eq!(drain_text(&mut buf), "caf");
        assert_eq!(buf, b"\xC3", "the lead byte must be kept for the next flush");

        buf.push(0xA4);
        assert_eq!(drain_text(&mut buf), "ä");
        assert!(buf.is_empty());
    }

    #[test]
    fn passes_complete_input_through_untouched() {
        let mut buf = "├── src ✓\n".as_bytes().to_vec();
        assert_eq!(drain_text(&mut buf), "├── src ✓\n");
        assert!(buf.is_empty());
    }

    /// A genuinely invalid byte is not a truncation and will never complete.
    /// Holding it back would wedge the buffer and stall the session.
    #[test]
    fn flushes_invalid_bytes_instead_of_stalling() {
        let mut buf = b"ok\xFFmore".to_vec();
        let out = drain_text(&mut buf);
        assert!(out.starts_with("ok"));
        assert!(out.ends_with("more"));
        assert!(buf.is_empty(), "an invalid byte must not be retained");
    }

    /// The bug this pins down: the flusher winning the race between "the
    /// session is connected again" and "the buffered bytes have been replayed"
    /// delivers newer output ahead of older, and a VT parser fed out of order
    /// describes a document that never existed.
    #[test]
    fn a_replay_makes_the_flusher_wait_instead_of_racing_it() {
        let parts = AttachParts {
            connected: Arc::new(AtomicBool::new(false)),
            output_buffer: Arc::new(Mutex::new(VecDeque::from(b"older\r\n".to_vec()))),
        };
        let order: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));
        let (started, replay_running) = std::sync::mpsc::channel();

        let flusher_buffer = parts.output_buffer.clone();
        let flusher_order = order.clone();
        let flusher = std::thread::spawn(move || {
            replay_running.recv().expect("replay started");
            // What the flusher does every tick before it decides where a chunk
            // goes.
            let _guard = flusher_buffer.lock().expect("buffer");
            flusher_order.lock().expect("order").push("newer");
        });

        let replay_order = order.clone();
        replay_and_connect(&parts, |buffered| {
            assert_eq!(buffered, "older\r\n", "the replay gets what was buffered");
            started.send(()).expect("signal");
            std::thread::sleep(std::time::Duration::from_millis(20));
            replay_order.lock().expect("order").push("older");
        })
        .expect("attach");

        flusher.join().expect("flusher");
        assert_eq!(*order.lock().expect("order"), vec!["older", "newer"]);
        assert!(parts.connected.load(Ordering::Relaxed), "attach reconnects");
        assert!(
            parts.output_buffer.lock().expect("buffer").is_empty(),
            "a replayed buffer must not be replayed twice"
        );
    }

    /// An overflow cut in the middle of an escape sequence hands the parser the
    /// tail of a `CSI`, and the sequence it was in the middle of is lost — an
    /// alternate screen that never turns on, an exit code that never arrives.
    #[test]
    fn an_overflow_is_cut_after_a_newline_not_inside_a_sequence() {
        let buf = b"one\r\n\x1b[?1049h two\r\nthree\r\n".to_vec();
        // Four bytes too long: the raw cut would land inside `\x1b[?1049h`.
        let cut = line_cut(buf.iter(), 4);
        assert_eq!(&buf[cut..], b"\x1b[?1049h two\r\nthree\r\n");
    }

    /// A binary on stdout has no line boundary to resync on. Searching the whole
    /// buffer for one on every read would make the flood quadratic, so past the
    /// window the cut happens where it must.
    #[test]
    fn a_stream_without_newlines_is_cut_where_it_has_to_be() {
        let buf = vec![b'x'; RESYNC_WINDOW * 2];
        assert_eq!(line_cut(buf.iter(), 10), 10);

        // A newline just past the window is out of reach as well.
        let mut late = vec![b'x'; RESYNC_WINDOW + 32];
        late.push(b'\n');
        assert_eq!(line_cut(late.iter(), 8), 8);
    }

    /// Closing an id nobody spawned is the normal case when a pane outlives its
    /// child — it must not be an error, and must not poison the map.
    #[test]
    fn closing_an_unknown_session_is_not_an_error() {
        let registry = PtyRegistry::new();
        assert!(!registry.contains("nope"));
        assert!(registry.close("nope").is_ok());
        assert!(registry.ids().is_empty());
    }

    /// Writing to an unknown id fails loudly instead of silently succeeding —
    /// a keystroke that goes nowhere is a bug the frontend must see.
    #[test]
    fn writing_to_an_unknown_session_reports_the_id() {
        let registry = PtyRegistry::new();
        let err = registry.write("ghost", b"ls\r").unwrap_err();
        assert!(err.contains("ghost"), "error should name the session: {err}");
    }

    #[test]
    fn defaults_are_a_usable_80x24_shell() {
        let opts = PtyOptions::default();
        assert_eq!((opts.cols, opts.rows), (80, 24));
        assert!(opts.shell.is_none());
        assert!(opts.args.is_empty(), "a bare shell takes no arguments");
        assert!(!default_shell().is_empty());
    }
}
