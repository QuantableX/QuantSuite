use chrono::Utc;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use crate::{AppState, LogEntry};
use crate::settings::get_data_dir;

/// Bot log lines waiting to go to the frontend, and the interval they leave on.
///
/// The bot's stdout and stderr are drained line by line, and every line used to
/// be its own `emit`. Tauri turns each one into an `EvaluateScript` message
/// **on the main thread**, per webview — so a chatty strategy was, quite
/// literally, a UI freeze (the same shape that hung QuantCanvas: see
/// modules/canvas/crate/src/commands/terminal.rs). Batching caps the cost at
/// one event per interval no matter how much the bot prints.
///
/// The same batch goes to `bot.log`: the disk side used to open, append and
/// close the file for every single line, on the very reader threads that have
/// to keep the bot's JSON-RPC stdout drained. Now the flusher thread owns the
/// one open handle and writes each batch with a single flush.
static LOG_QUEUE: Mutex<Vec<LogEntry>> = Mutex::new(Vec::new());
const LOG_FLUSH_INTERVAL: Duration = Duration::from_millis(50);

/// Modules render in the suite's `main` window (README, "Adding a module"), so
/// the batch is addressed there rather than broadcast — QuantHUD's overlay runs
/// alongside the suite and has no use for the bot's log.
const LOG_TARGET_WINDOW: &str = "main";

/// Rotate `bot.log` at 5 MB, keeping one previous file — the same cap as the
/// suite's diagnostics log. The in-memory buffer is capped at 10 000 entries;
/// before this the file had no cap at all and grew across sessions.
const BOT_LOG_MAX_BYTES: u64 = 5 * 1024 * 1024;

/// How much of the file's end is read per step when tailing it. Entries are a
/// few hundred bytes, so the 1 000-line startup tail is a handful of steps.
const BOT_LOG_TAIL_CHUNK: u64 = 64 * 1024;

fn bot_log_path() -> PathBuf {
    get_data_dir().join("logs").join("bot.log")
}

/// The bot's on-disk log: one handle, opened lazily and kept open, rotated at
/// the cap the way `qs_core::diagnostics` rotates the suite log.
///
/// Owned by the flusher thread in [`queue_bot_log`], so there is exactly one
/// writer and no lock — the reader threads never touch the disk.
struct BotLogFile {
    path: PathBuf,
    writer: Option<BufWriter<File>>,
}

impl BotLogFile {
    fn new(path: PathBuf) -> Self {
        Self { path, writer: None }
    }

    fn open(&self) -> Option<BufWriter<File>> {
        std::fs::create_dir_all(self.path.parent()?).ok()?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .ok()?;
        Some(BufWriter::new(file))
    }

    /// Every batch is flushed, so the length the metadata reports is the
    /// whole file — nothing is waiting in the buffer.
    fn is_full(writer: &BufWriter<File>) -> bool {
        writer
            .get_ref()
            .metadata()
            .map(|m| m.len() >= BOT_LOG_MAX_BYTES)
            .unwrap_or(false)
    }

    /// Append the batch as JSON lines and flush once. Every failure is
    /// swallowed: a log that cannot be written must not take the bot down.
    fn write_batch(&mut self, entries: &[LogEntry]) {
        if self.writer.is_none() {
            self.writer = self.open();
        }

        // Rotation is checked after opening, so the first batch of a session
        // still sees an oversized file left by the last one. The handle is
        // dropped before the rename because Windows will not move an open file.
        if self.writer.as_ref().is_some_and(Self::is_full) {
            self.writer = None;
            let _ = std::fs::rename(&self.path, self.path.with_extension("log.1"));
            self.writer = self.open();
        }

        let Some(writer) = self.writer.as_mut() else {
            return;
        };
        for entry in entries {
            if let Ok(line) = serde_json::to_string(entry) {
                let _ = writeln!(writer, "{line}");
            }
        }
        let _ = writer.flush();
    }
}

fn queue_bot_log(app_handle: &AppHandle, entry: LogEntry) {
    static FLUSHER: OnceLock<()> = OnceLock::new();
    FLUSHER.get_or_init(|| {
        let app = app_handle.clone();
        std::thread::spawn(move || {
            let mut file = BotLogFile::new(bot_log_path());
            loop {
                std::thread::sleep(LOG_FLUSH_INTERVAL);
                let batch: Vec<LogEntry> = match LOG_QUEUE.lock() {
                    Ok(mut q) if !q.is_empty() => std::mem::take(&mut q),
                    _ => continue,
                };
                file.write_batch(&batch);
                let _ = app.emit_to(LOG_TARGET_WINDOW, "bot:log", batch);
            }
        });
    });

    if let Ok(mut q) = LOG_QUEUE.lock() {
        q.push(entry);
    }
}

/// A module-level line (no bot): preflight of a draft, backtest runner errors.
pub(crate) fn push_bot_log(app_handle: &AppHandle, level: &str, message: impl Into<String>) {
    push_bot_log_for(app_handle, None, level, message);
}

/// A log line, tagged with the bot it belongs to (PLAN-QUANTALGO §3.2).
pub(crate) fn push_bot_log_for(
    app_handle: &AppHandle,
    bot_id: Option<&str>,
    level: &str,
    message: impl Into<String>,
) {
    let entry = LogEntry {
        timestamp: Utc::now().to_rfc3339(),
        level: level.to_string(),
        message: message.into(),
        bot_id: bot_id.map(|s| s.to_string()),
    };

    queue_bot_log(app_handle, entry.clone());

    if let Some(state) = app_handle.try_state::<AppState>() {
        if let Ok(mut logs) = state.bot_logs.lock() {
            logs.push(entry);
            if logs.len() > 10_000 {
                logs.drain(0..1_000);
            }
        }
    }
}

/// The last `limit` entries of the log at `path`, oldest first.
///
/// Reads the file from its end in chunks until enough lines are in hand,
/// instead of pulling all of it through memory: the file grows to the rotation
/// cap on its own, a legacy one from before rotation can be far bigger, and
/// the callers only ever want the last few hundred lines. The segment before
/// the first newline may be a line cut in half by the chunk boundary, so it is
/// thrown away; a partial last line left by a crash simply fails to parse.
fn tail_bot_log(path: &Path, limit: usize) -> Vec<LogEntry> {
    let Ok(mut file) = File::open(path) else {
        return Vec::new();
    };
    let Ok(mut end) = file.metadata().map(|m| m.len()) else {
        return Vec::new();
    };

    // One newline more than `limit`, to pay for the discarded first segment.
    let mut buf: Vec<u8> = Vec::new();
    let mut newlines = 0usize;
    while end > 0 && newlines <= limit {
        let start = end.saturating_sub(BOT_LOG_TAIL_CHUNK);
        let mut chunk = vec![0u8; (end - start) as usize];
        if file.seek(SeekFrom::Start(start)).is_err() || file.read_exact(&mut chunk).is_err() {
            return Vec::new();
        }
        newlines += chunk.iter().filter(|&&b| b == b'\n').count();
        chunk.extend_from_slice(&buf);
        buf = chunk;
        end = start;
    }

    let skip = if end > 0 {
        buf.iter()
            .position(|&b| b == b'\n')
            .map_or(buf.len(), |i| i + 1)
    } else {
        0
    };

    let mut logs = String::from_utf8_lossy(&buf[skip..])
        .lines()
        .filter_map(|line| serde_json::from_str::<LogEntry>(line).ok())
        .collect::<Vec<_>>();
    if logs.len() > limit {
        logs = logs.split_off(logs.len() - limit);
    }
    logs
}

pub(crate) fn load_persisted_bot_logs(limit: usize) -> Vec<LogEntry> {
    tail_bot_log(&bot_log_path(), limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("qs-algo-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entry(i: usize) -> LogEntry {
        LogEntry {
            timestamp: format!("2026-09-04T00:00:{:02}Z", i % 60),
            level: "info".into(),
            message: format!("tick {i} — padded so a line is a realistic size"),
            bot_id: None,
        }
    }

    fn write_lines(path: &Path, n: usize) {
        let mut file = File::create(path).unwrap();
        for i in 0..n {
            writeln!(file, "{}", serde_json::to_string(&entry(i)).unwrap()).unwrap();
        }
    }

    #[test]
    fn write_batch_appends_to_one_file_across_batches() {
        let dir = fresh_dir("botlog-append");
        let mut log = BotLogFile::new(dir.join("logs").join("bot.log"));

        log.write_batch(&[entry(1), entry(2)]);
        log.write_batch(&[entry(3)]);

        let tail = tail_bot_log(&log.path, 10);
        let messages: Vec<&str> = tail.iter().map(|e| e.message.as_str()).collect();
        assert_eq!(messages.len(), 3);
        assert!(messages[0].starts_with("tick 1 "));
        assert!(messages[2].starts_with("tick 3 "));
        assert!(
            log.writer.is_some(),
            "the handle stays open between batches"
        );
    }

    /// A log that grows forever is its own outage, so the cap is enforced and
    /// exactly one previous generation is kept.
    #[test]
    fn write_batch_rotates_once_past_the_cap() {
        let dir = fresh_dir("botlog-rotate");
        let mut log = BotLogFile::new(dir.join("bot.log"));

        std::fs::write(&log.path, vec![b'x'; BOT_LOG_MAX_BYTES as usize + 1]).unwrap();
        log.write_batch(&[entry(7)]);

        let current = std::fs::read_to_string(&log.path).unwrap();
        assert!(current.contains("tick 7 "));
        assert!(
            current.len() < BOT_LOG_MAX_BYTES as usize,
            "a fresh file, not the old one"
        );
        assert_eq!(
            std::fs::metadata(dir.join("bot.log.1")).unwrap().len(),
            BOT_LOG_MAX_BYTES + 1,
            "the previous generation must be kept, not dropped"
        );
    }

    #[test]
    fn tail_returns_the_last_lines_of_a_large_file_in_order() {
        let dir = fresh_dir("botlog-tail-large");
        let path = dir.join("bot.log");
        write_lines(&path, 20_000);
        assert!(
            std::fs::metadata(&path).unwrap().len() > 4 * BOT_LOG_TAIL_CHUNK,
            "the file has to span several chunks for this to prove anything"
        );

        let tail = tail_bot_log(&path, 200);

        assert_eq!(tail.len(), 200);
        for (offset, entry) in tail.iter().enumerate() {
            let expected = format!("tick {} ", 19_800 + offset);
            assert!(
                entry.message.starts_with(&expected),
                "got {}",
                entry.message
            );
        }
    }

    /// A crash mid-write leaves a half line at the end; a chunk boundary cuts a
    /// line at the front. Neither may drop the complete lines around them.
    #[test]
    fn tail_drops_a_partial_last_line_and_keeps_the_rest() {
        let dir = fresh_dir("botlog-tail-partial");
        let path = dir.join("bot.log");
        write_lines(&path, 5);
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        write!(
            file,
            "{{\"timestamp\":\"2026-09-04T00:00:00Z\",\"level\":\"inf"
        )
        .unwrap();

        let all = tail_bot_log(&path, 10);
        assert_eq!(all.len(), 5, "the fragment is dropped, nothing else is");
        assert!(all[4].message.starts_with("tick 4 "));

        let last_three = tail_bot_log(&path, 3);
        let messages: Vec<&str> = last_three.iter().map(|e| e.message.as_str()).collect();
        assert_eq!(messages.len(), 3);
        assert!(messages[0].starts_with("tick 2 "));
        assert!(messages[2].starts_with("tick 4 "));
    }

    #[test]
    fn tail_of_a_missing_file_is_empty() {
        let dir = fresh_dir("botlog-tail-missing");
        assert!(tail_bot_log(&dir.join("bot.log"), 100).is_empty());
    }
}
