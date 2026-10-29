//! The V3 runtime (docs/PLAN-QUANTPILOT.md §4.1): one provider process per
//! active thread, its stdout turned into timeline items, its stdin fed by
//! the driver.
//!
//! - **Turns.** `send` starts a turn; the driver's `TurnEnd` (or the
//!   process ending) finishes it. A second `send` while a turn runs is
//!   refused before anything is saved.
//! - **Warm processes.** After a turn the process stays up for follow-ups
//!   — at most [`WARM_RUNS`] idle ones, each for [`WARM_FOR`]; then stdin
//!   closes and the CLI exits. The next turn resumes the saved session.
//!   A changed model, effort or mode starts a fresh process.
//! - **Stop.** `interrupt` asks the CLI; after [`INTERRUPT_GRACE`] the
//!   process tree is killed. App quit kills every tree.
//! - **Bounds.** Frames up to [`MAX_FRAME`], a stderr tail of
//!   [`STDERR_TAIL`], tool previews of [`INPUT_PREVIEW`]/[`OUTPUT_PREVIEW`].
//!
//! Tauri-free on purpose: events leave through an [`EventSink`], so the
//! whole path from stdout line to persisted item is testable.

use crate::db::{Database, TurnRow};
use crate::driver::{Answer, Catalog, Driver, Emit, RequestKind, ToolInfo, TurnInput, TurnStatus};
use crate::events::{bounded, bounded_output, Item, ItemKind, ThreadEvent};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

pub const MAX_FRAME: usize = 4 * 1024 * 1024;
pub const STDERR_TAIL: usize = 16 * 1024;
pub const INPUT_PREVIEW: usize = 4 * 1024;
pub const OUTPUT_PREVIEW: usize = 8 * 1024;
pub const WARM_RUNS: usize = 2;
pub const WARM_FOR: Duration = Duration::from_secs(600);
pub const INTERRUPT_GRACE: Duration = Duration::from_secs(8);
const FLUSH_EVERY: Duration = Duration::from_millis(1000);
const DISCOVERY_DEADLINE: Duration = Duration::from_secs(15);
const DISCOVERY_MAX_BYTES: usize = 1024 * 1024;
const CATALOG_TTL: Duration = Duration::from_secs(300);

/// Where envelopes go: `(thread id, seq, event)`.
pub type EventSink = Arc<dyn Fn(&str, u64, &ThreadEvent) + Send + Sync>;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Per-thread event sequence numbers, continuing across runs.
#[derive(Default)]
pub struct Seqs(Mutex<HashMap<String, u64>>);

impl Seqs {
    fn next(&self, thread_id: &str) -> u64 {
        let mut m = lock(&self.0);
        let n = m.entry(thread_id.to_string()).or_insert(0);
        *n += 1;
        *n
    }

    pub fn current(&self, thread_id: &str) -> u64 {
        lock(&self.0).get(thread_id).copied().unwrap_or(0)
    }
}

/// How to start a provider process for a thread.
pub struct LaunchPlan {
    /// Equal keys may share a warm process (provider, session, model, effort, mode).
    pub key: String,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub env: Vec<(String, String)>,
    pub driver: Box<dyn Driver>,
}

// ── Projector: emits → timeline items ──

pub struct Projector {
    db: Arc<Database>,
    sink: EventSink,
    seqs: Arc<Seqs>,
    thread_id: String,
    turn_id: String,
    next_seq: i64,
    keys: HashMap<String, String>,
    live: HashMap<String, Item>,
    dirty: HashSet<String>,
    last_flush: Instant,
    tools: HashMap<String, String>,
    requests: HashMap<String, String>,
}

impl Projector {
    pub fn new(db: Arc<Database>, sink: EventSink, seqs: Arc<Seqs>, thread_id: &str) -> Self {
        let next_seq = db.next_seq(thread_id).unwrap_or(1);
        Self {
            db,
            sink,
            seqs,
            thread_id: thread_id.to_string(),
            turn_id: String::new(),
            next_seq,
            keys: HashMap::new(),
            live: HashMap::new(),
            dirty: HashSet::new(),
            last_flush: Instant::now(),
            tools: HashMap::new(),
            requests: HashMap::new(),
        }
    }

    fn emit(&self, event: ThreadEvent) {
        let seq = self.seqs.next(&self.thread_id);
        (self.sink)(&self.thread_id, seq, &event);
    }

    fn persist(&self, item: &Item) {
        if let Err(e) = self.db.upsert_item(item) {
            eprintln!("pilot: saving a timeline item failed: {e}");
        }
    }

    /// Persist and show an item; keep it live while it can still change.
    fn publish(&mut self, item: Item, keep_live: bool) {
        self.persist(&item);
        self.dirty.remove(&item.id);
        self.emit(ThreadEvent::Item { item: item.clone() });
        if keep_live {
            self.live.insert(item.id.clone(), item);
        } else {
            self.live.remove(&item.id);
        }
    }

    fn new_item(&mut self, kind: ItemKind, parent: Option<String>, data: Value) -> Item {
        let t = now_ms();
        let item = Item {
            id: uuid::Uuid::new_v4().to_string(),
            thread_id: self.thread_id.clone(),
            turn_id: self.turn_id.clone(),
            seq: self.next_seq,
            kind,
            parent,
            data,
            created_at: t,
            updated_at: t,
        };
        self.next_seq += 1;
        item
    }

    pub fn begin_turn(&mut self, turn_id: &str) {
        self.flush();
        self.turn_id = turn_id.to_string();
        self.keys.clear();
        self.live.clear();
        self.tools.clear();
        // Requests from an earlier turn stay answerable until the process ends.
    }

    pub fn user_message(&mut self, text: &str, attachments: Value) -> Item {
        let item = self.new_item(ItemKind::User, None, json!({ "text": text, "attachments": attachments }));
        self.publish(item.clone(), false);
        item
    }

    /// A message the user sent into the running turn: part of this turn,
    /// marked so the timeline can say it steered.
    pub fn steer_message(&mut self, text: &str) {
        let item = self.new_item(ItemKind::User, None, json!({ "text": text, "attachments": [], "steer": true }));
        self.publish(item, false);
    }

    pub fn notice(&mut self, kind: ItemKind, text: &str) {
        let item = self.new_item(kind, None, json!({ "text": text }));
        self.publish(item, false);
    }

    /// Write streaming items that changed since the last flush.
    pub fn flush(&mut self) {
        for id in self.dirty.drain().collect::<Vec<_>>() {
            if let Some(item) = self.live.get(&id) {
                if let Err(e) = self.db.upsert_item(item) {
                    eprintln!("pilot: saving a timeline item failed: {e}");
                }
            }
        }
        self.last_flush = Instant::now();
    }

    fn live_mut(&mut self, key: &str) -> Option<&mut Item> {
        let id = self.keys.get(key)?;
        self.live.get_mut(id)
    }

    fn append_text(&mut self, key: &str, text: &str, estimated: Option<i64>) {
        let snapshot = {
            let Some(item) = self.live_mut(key) else {
                return;
            };
            if let Some(t) = item.data["text"].as_str().map(|cur| format!("{cur}{text}")) {
                item.data["text"] = Value::String(t);
            }
            if let Some(n) = estimated {
                item.data["estimatedTokens"] = n.into();
            }
            item.updated_at = now_ms();
            item.clone()
        };
        self.dirty.insert(snapshot.id.clone());
        if !text.is_empty() {
            self.emit(ThreadEvent::Delta { item_id: snapshot.id, text: text.to_string() });
        } else if estimated.is_some() {
            // Only the token estimate moved (thinking text can be withheld).
            self.emit(ThreadEvent::Item { item: snapshot });
        }
        if self.last_flush.elapsed() >= FLUSH_EVERY {
            self.flush();
        }
    }

    fn finish_text(&mut self, key: &str, text: Option<&str>) {
        let Some(item) = self.live_mut(key) else {
            return;
        };
        if let Some(t) = text {
            item.data["text"] = Value::String(t.to_string());
        }
        item.data["streaming"] = false.into();
        item.updated_at = now_ms();
        let item = item.clone();
        self.publish(item, false);
    }

    fn tool_mut(&mut self, call_id: &str) -> Option<&mut Item> {
        let id = self.tools.get(call_id)?;
        self.live.get_mut(id)
    }

    fn republish_tool(&mut self, call_id: &str) {
        if let Some(item) = self.tool_mut(call_id).map(|i| {
            i.updated_at = now_ms();
            i.clone()
        }) {
            let done = item.data["status"] != "running";
            self.publish(item, !done);
        }
    }

    fn usage_totals(&self) -> ThreadEvent {
        let t = self.db.get_session(&self.thread_id).ok().flatten();
        ThreadEvent::Usage {
            context_tokens: t.as_ref().and_then(|t| t.context_tokens),
            context_window: t.as_ref().and_then(|t| t.context_window),
            cost_usd: t.as_ref().map(|t| t.cost_usd),
            input_tokens: t.as_ref().map(|t| t.input_tokens),
            output_tokens: t.as_ref().map(|t| t.output_tokens),
        }
    }

    fn set_state(&self, state: &str, detail: Option<String>) {
        let _ = self.db.set_state(&self.thread_id, state);
        self.emit(ThreadEvent::State { state: state.to_string(), detail });
    }

    /// Apply one emit. Returns the turn's end when this emit ended it.
    pub fn apply(&mut self, e: Emit, info: &dyn Fn(&str, &Value) -> ToolInfo, provider: &str) -> Option<TurnStatus> {
        match e {
            Emit::Bound { session_id, model } => {
                let _ = self.db.set_provider_session(&self.thread_id, &session_id);
                self.emit(ThreadEvent::Session { provider: provider.to_string(), provider_session_id: session_id, model });
            }
            Emit::MessageStart { key, phase, parent } => {
                let item = self.new_item(ItemKind::Assistant, parent, json!({ "text": "", "phase": phase, "streaming": true }));
                self.keys.insert(key, item.id.clone());
                self.publish(item, true);
            }
            Emit::TextDelta { key, text } => self.append_text(&key, &text, None),
            Emit::MessageDone { key, text } => {
                self.finish_text(&key, Some(&text));
                let _ = self.db.touch(&self.thread_id);
            }
            Emit::ReasoningStart { key, parent } => {
                let item = self.new_item(ItemKind::Reasoning, parent, json!({ "text": "", "streaming": true }));
                self.keys.insert(key, item.id.clone());
                self.publish(item, true);
            }
            Emit::ReasoningDelta { key, text, estimated_tokens } => self.append_text(&key, &text, estimated_tokens),
            Emit::ReasoningDone { key } => self.finish_text(&key, None),
            Emit::ToolStart { call_id, name, input, parent } => {
                let ti = info(&name, &input);
                let item = self.new_item(
                    ItemKind::Tool,
                    parent,
                    json!({
                        "callId": call_id,
                        "name": name,
                        "category": ti.category,
                        "title": ti.title,
                        "input": bounded(&pretty(&input), INPUT_PREVIEW),
                        "status": "running",
                        "isError": false,
                    }),
                );
                self.tools.insert(call_id, item.id.clone());
                self.publish(item, true);
            }
            Emit::ToolInput { call_id, input } => {
                let name = self.tool_mut(&call_id).and_then(|i| i.data["name"].as_str().map(str::to_string));
                if let Some(name) = name {
                    let ti = info(&name, &input);
                    if let Some(item) = self.tool_mut(&call_id) {
                        item.data["input"] = bounded(&pretty(&input), INPUT_PREVIEW).into();
                        item.data["title"] = ti.title.into();
                    }
                    self.republish_tool(&call_id);
                }
            }
            Emit::ToolResult { call_id, output, is_error } => {
                if let Some(item) = self.tool_mut(&call_id) {
                    item.data["output"] = bounded_output(&output, OUTPUT_PREVIEW).into();
                    item.data["isError"] = is_error.into();
                    let denied = is_error && item.data["status"] == "denied";
                    item.data["status"] = if denied { "denied" } else if is_error { "failed" } else { "completed" }.into();
                }
                self.republish_tool(&call_id);
            }
            Emit::FileChanged { path, call_id } => {
                // The tool has usually finished already, so its row is no
                // longer live: update the saved one.
                if let Some(id) = self.tools.get(&call_id).cloned() {
                    let item = self.live.get(&id).cloned().or_else(|| self.db.get_item(&id).ok().flatten());
                    if let Some(mut item) = item {
                        item.data["path"] = path.into();
                        item.updated_at = now_ms();
                        let running = item.data["status"] == "running";
                        self.publish(item, running);
                    }
                }
            }
            Emit::Request(r) => {
                let (kind, data) = match r.kind {
                    RequestKind::Approval => {
                        let ti = info(&r.tool_name, &r.input);
                        (
                            ItemKind::Approval,
                            json!({
                                "requestId": r.request_id,
                                "toolName": r.tool_name,
                                "category": ti.category,
                                "title": ti.title,
                                "input": bounded(&pretty(&r.input), INPUT_PREVIEW),
                                "callId": r.call_id,
                                "status": "pending",
                            }),
                        )
                    }
                    RequestKind::Question => (
                        ItemKind::Question,
                        json!({
                            "requestId": r.request_id,
                            "questions": r.input.get("questions").cloned().unwrap_or(json!([])),
                            "status": "pending",
                        }),
                    ),
                    RequestKind::Plan => (
                        ItemKind::Plan,
                        json!({
                            "requestId": r.request_id,
                            "plan": r.input.get("plan").and_then(Value::as_str).unwrap_or_default(),
                            "status": "pending",
                        }),
                    ),
                };
                let parent = r.parent.or_else(|| {
                    r.call_id.as_ref().and_then(|c| self.tool_mut(c)).and_then(|i| i.parent.clone())
                });
                let item = self.new_item(kind, parent, data);
                self.requests.insert(r.request_id, item.id.clone());
                self.publish(item, true);
                self.set_state("waiting", None);
            }
            Emit::Todos { todos } => {
                let key = format!("todos:{}", self.turn_id);
                if let Some(item) = self.live_mut(&key) {
                    item.data["todos"] = todos;
                    item.updated_at = now_ms();
                    let item = item.clone();
                    self.publish(item, true);
                } else {
                    let item = self.new_item(ItemKind::Todo, None, json!({ "todos": todos }));
                    self.keys.insert(key, item.id.clone());
                    self.publish(item, true);
                }
            }
            Emit::Usage { context_tokens, context_window, cost_usd, input_tokens, output_tokens } => {
                let _ = self.db.set_context(&self.thread_id, context_tokens, context_window);
                if cost_usd.is_some() || input_tokens.is_some() || output_tokens.is_some() {
                    let _ = self.db.add_usage(&self.thread_id, input_tokens, output_tokens, cost_usd);
                }
                self.emit(self.usage_totals());
            }
            Emit::RateLimit { windows } => self.emit(ThreadEvent::RateLimit { windows }),
            Emit::Notice { text } => self.notice(ItemKind::Notice, &text),
            Emit::TurnEnd { status, error, duration_ms, cost_usd } => {
                self.end_turn(status, error, duration_ms, cost_usd);
                return Some(status);
            }
            Emit::Catalog(_) | Emit::Write(_) => {}
        }
        None
    }

    pub fn end_turn(&mut self, status: TurnStatus, error: Option<String>, duration_ms: Option<i64>, cost_usd: Option<f64>) {
        // Whatever still streams or runs ends with the turn.
        let open: Vec<Item> = self.live.values().cloned().collect();
        for mut item in open {
            match item.kind {
                ItemKind::Assistant | ItemKind::Reasoning => {
                    item.data["streaming"] = false.into();
                }
                ItemKind::Tool if item.data["status"] == "running" => {
                    item.data["status"] = if status == TurnStatus::Completed { "completed" } else { "interrupted" }.into();
                }
                ItemKind::Approval | ItemKind::Question | ItemKind::Plan => continue,
                _ => {}
            }
            item.updated_at = now_ms();
            self.publish(item, false);
        }
        self.flush();
        if let Some(err) = error.as_deref().filter(|e| !e.trim().is_empty()) {
            self.notice(ItemKind::Error, err);
        } else if status == TurnStatus::Interrupted {
            self.notice(ItemKind::Notice, "You stopped the agent.");
        }
        if !self.turn_id.is_empty() {
            let _ = self.db.finish_turn(&self.turn_id, status.as_str(), duration_ms, cost_usd, error.as_deref());
        }
        let _ = self.db.set_error(&self.thread_id, error.as_deref());
        let _ = self.db.touch(&self.thread_id);
        self.emit(ThreadEvent::Turn {
            turn_id: self.turn_id.clone(),
            status: status.as_str().to_string(),
            duration_ms,
            cost_usd,
            error,
        });
        let state = match status {
            TurnStatus::Completed => "idle",
            TurnStatus::Failed => "error",
            TurnStatus::Interrupted => "interrupted",
            TurnStatus::Limited => "limited",
        };
        self.set_state(state, None);
    }

    /// The user answered a request: its item shows the decision.
    pub fn resolve(&mut self, request_id: &str, answer: &Answer) {
        let Some(id) = self.requests.remove(request_id) else {
            return;
        };
        let item = self.live.get(&id).cloned().or_else(|| self.db.get_item(&id).ok().flatten());
        let Some(mut item) = item else {
            return;
        };
        let (status, reply) = match answer {
            Answer::Allow => (if item.kind == ItemKind::Plan { "approved" } else { "allowed" }, Value::Null),
            Answer::AllowSession => ("allowedSession", Value::Null),
            Answer::Deny { message } => {
                (if item.kind == ItemKind::Plan { "rejected" } else { "denied" }, json!(message.clone().unwrap_or_default()))
            }
            Answer::Answers { answers } => ("answered", Value::Object(answers.clone())),
            Answer::Dismiss => ("dismissed", Value::Null),
        };
        item.data["status"] = status.into();
        if !reply.is_null() {
            item.data["reply"] = reply;
        }
        item.updated_at = now_ms();
        if let Some(call_id) = item.data["callId"].as_str().map(str::to_string) {
            if matches!(answer, Answer::Deny { .. }) {
                if let Some(tool) = self.tool_mut(&call_id) {
                    tool.data["status"] = "denied".into();
                }
            }
        }
        self.publish(item, false);
        if self.requests.is_empty() {
            self.set_state("running", None);
        }
    }

    pub fn expire_pending(&mut self) {
        self.requests.clear();
        if let Ok(items) = self.db.expire_pending(&self.thread_id) {
            for item in items {
                self.live.remove(&item.id);
                self.emit(ThreadEvent::Item { item });
            }
        }
    }
}

fn pretty(v: &Value) -> String {
    match v {
        Value::Object(m) if m.is_empty() => String::new(),
        _ => serde_json::to_string_pretty(v).unwrap_or_default(),
    }
}

// ── Processes ──

struct Run {
    thread_id: String,
    provider: &'static str,
    /// What the process was started with; a live switch updates it.
    key: Mutex<String>,
    pid: u32,
    child: Mutex<Option<Child>>,
    stdin: Mutex<Option<ChildStdin>>,
    driver: Mutex<Box<dyn Driver>>,
    proj: Mutex<Projector>,
    stderr: Arc<Mutex<Vec<u8>>>,
    turn: Mutex<Option<String>>,
    interrupting: AtomicBool,
    idle_since: Mutex<Option<Instant>>,
}

impl Run {
    fn write(&self, lines: &[String]) -> Result<(), String> {
        let mut stdin = lock(&self.stdin);
        let pipe = stdin.as_mut().ok_or("the agent process is closing")?;
        for line in lines {
            pipe.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
            pipe.write_all(b"\n").map_err(|e| e.to_string())?;
        }
        pipe.flush().map_err(|e| e.to_string())
    }

    fn turn_active(&self) -> bool {
        lock(&self.turn).is_some()
    }

    fn stderr_tail(&self) -> String {
        let buf = lock(&self.stderr);
        let text = String::from_utf8_lossy(&buf);
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        let start = lines.len().saturating_sub(8);
        lines[start..].join("\n")
    }

    /// Graceful: the CLI exits when its stdin closes; a kill follows if not.
    fn close(self: &Arc<Self>) {
        lock(&self.stdin).take();
        let me = Arc::downgrade(self);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(5));
            if let Some(run) = me.upgrade() {
                run.kill();
            }
        });
    }

    fn kill(&self) {
        lock(&self.stdin).take();
        kill_tree(self.pid);
        if let Some(child) = lock(&self.child).as_mut() {
            let _ = child.kill();
        }
    }
}

/// The CLI's own children (shells, MCP servers, subagents) go with it.
pub fn kill_tree(pid: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000)
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("kill").args(["-TERM", &format!("-{pid}")]).status();
        let _ = Command::new("kill").args(["-TERM", &pid.to_string()]).status();
    }
}

fn spawn_process(program: &str, args: &[String], cwd: &str, env: &[(String, String)]) -> Result<Child, String> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd.spawn().map_err(|e| format!("could not start {program}: {e}"))
}

/// Complete lines up to `max` bytes; a longer frame is skipped whole and
/// reported once as `Err(len)`.
pub fn read_bounded_line(reader: &mut impl BufRead, max: usize, buf: &mut Vec<u8>) -> std::io::Result<Option<Result<(), usize>>> {
    buf.clear();
    let mut overflow = 0usize;
    loop {
        let available = match reader.fill_buf() {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if available.is_empty() {
            if buf.is_empty() && overflow == 0 {
                return Ok(None);
            }
            return Ok(Some(if overflow > 0 { Err(overflow) } else { Ok(()) }));
        }
        let (chunk, found) = match available.iter().position(|&b| b == b'\n') {
            Some(i) => (&available[..i], Some(i + 1)),
            None => (available, None),
        };
        if overflow == 0 && buf.len() + chunk.len() <= max {
            buf.extend_from_slice(chunk);
        } else {
            overflow += buf.len() + chunk.len();
            buf.clear();
        }
        let used = found.unwrap_or(available.len());
        reader.consume(used);
        if found.is_some() {
            if buf.last() == Some(&b'\r') {
                buf.pop();
            }
            return Ok(Some(if overflow > 0 { Err(overflow) } else { Ok(()) }));
        }
    }
}

pub struct Runtime {
    db: Arc<Database>,
    sink: EventSink,
    seqs: Arc<Seqs>,
    runs: Mutex<HashMap<String, Arc<Run>>>,
    catalogs: Mutex<HashMap<String, (Instant, Catalog)>>,
    me: Mutex<Weak<Runtime>>,
    on_turn_end: Mutex<Option<TurnEndHook>>,
}

/// Called after every turn with its thread and how it ended.
pub type TurnEndHook = Arc<dyn Fn(&str, TurnStatus) + Send + Sync>;

impl Runtime {
    pub fn new(db: Arc<Database>, sink: EventSink) -> Arc<Self> {
        let rt = Arc::new(Self {
            db,
            sink,
            seqs: Arc::new(Seqs::default()),
            runs: Mutex::new(HashMap::new()),
            catalogs: Mutex::new(HashMap::new()),
            me: Mutex::new(Weak::new()),
            on_turn_end: Mutex::new(None),
        });
        *lock(&rt.me) = Arc::downgrade(&rt);
        rt
    }

    fn run(&self, thread_id: &str) -> Option<Arc<Run>> {
        lock(&self.runs).get(thread_id).cloned()
    }

    pub fn active_turn(&self, thread_id: &str) -> Option<String> {
        self.run(thread_id).and_then(|r| lock(&r.turn).clone())
    }

    /// Threads with a turn running right now.
    pub fn busy_threads(&self) -> Vec<String> {
        lock(&self.runs).values().filter(|r| r.turn_active()).map(|r| r.thread_id.clone()).collect()
    }

    pub fn process_alive(&self, thread_id: &str) -> bool {
        self.run(thread_id).is_some()
    }

    /// A page of the timeline and the event seq it reflects. While a
    /// process runs, its projector is held for the read so no event can
    /// fall between the items and the seq.
    pub fn snapshot(&self, thread_id: &str, before_seq: Option<i64>, limit: i64) -> rusqlite::Result<(Vec<Item>, u64)> {
        match self.run(thread_id) {
            Some(run) => {
                let mut proj = lock(&run.proj);
                proj.flush();
                let items = self.db.items(thread_id, before_seq, limit)?;
                Ok((items, self.seqs.current(thread_id)))
            }
            None => Ok((self.db.items(thread_id, before_seq, limit)?, self.seqs.current(thread_id))),
        }
    }

    /// A thread-level event from outside a run (a rename, an automatic title).
    pub fn emit(&self, thread_id: &str, event: ThreadEvent) {
        let seq = self.seqs.next(thread_id);
        (self.sink)(thread_id, seq, &event);
    }

    pub fn cached_catalog(&self, provider: &str) -> Option<Catalog> {
        let cats = lock(&self.catalogs);
        cats.get(provider).filter(|(at, _)| at.elapsed() < CATALOG_TTL).map(|(_, c)| c.clone())
    }

    pub fn store_catalog(&self, catalog: Catalog) {
        lock(&self.catalogs).insert(catalog.provider.clone(), (Instant::now(), catalog));
    }

    /// Start a turn. `plan` is asked for the launch whenever no matching
    /// warm process exists; it is not consulted otherwise.
    pub fn send(
        &self,
        thread_id: &str,
        input: TurnInput,
        attachments: Value,
        plan: impl FnOnce() -> Result<LaunchPlan, String>,
        key: &str,
    ) -> Result<String, String> {
        if self.run(thread_id).is_some_and(|r| r.turn_active()) {
            return Err("the agent is still working on this thread".into());
        }
        let turn_id = uuid::Uuid::new_v4().to_string();
        let reuse = self.run(thread_id).filter(|r| *lock(&r.key) == key && lock(&r.stdin).is_some());
        let (run, fresh) = match reuse {
            Some(run) => (run, false),
            None => {
                if let Some(old) = lock(&self.runs).remove(thread_id) {
                    old.kill();
                }
                match plan().and_then(|p| self.spawn(thread_id, p)) {
                    Ok(run) => (run, true),
                    Err(e) => {
                        // Nothing runs, but the message and why are visible.
                        let mut proj = Projector::new(self.db.clone(), self.sink.clone(), self.seqs.clone(), thread_id);
                        self.start_turn_rows(&mut proj, thread_id, &turn_id, &input, attachments);
                        proj.end_turn(TurnStatus::Failed, Some(e.clone()), None, None);
                        return Err(e);
                    }
                }
            }
        };
        {
            let mut proj = lock(&run.proj);
            self.start_turn_rows(&mut proj, thread_id, &turn_id, &input, attachments);
        }
        *lock(&run.turn) = Some(turn_id.clone());
        *lock(&run.idle_since) = None;
        run.interrupting.store(false, Ordering::SeqCst);
        let mut lines = Vec::new();
        {
            let mut driver = lock(&run.driver);
            if fresh {
                lines.extend(driver.handshake());
            }
            lines.extend(driver.user_message(&input));
        }
        if let Err(e) = run.write(&lines) {
            lock(&run.turn).take();
            lock(&run.proj).end_turn(TurnStatus::Failed, Some(format!("could not reach the agent: {e}")), None, None);
            run.kill();
            return Err(e);
        }
        Ok(turn_id)
    }

    fn start_turn_rows(&self, proj: &mut Projector, thread_id: &str, turn_id: &str, input: &TurnInput, attachments: Value) {
        proj.begin_turn(turn_id);
        let _ = self.db.insert_turn(&TurnRow {
            id: turn_id.to_string(),
            thread_id: thread_id.to_string(),
            status: "started".into(),
            started_at: now_ms(),
            ended_at: None,
            duration_ms: None,
            cost_usd: None,
            error: None,
        });
        proj.user_message(&input.text, attachments);
        let _ = self.db.touch(thread_id);
        let _ = self.db.resume(thread_id);
        proj.emit(ThreadEvent::Turn {
            turn_id: turn_id.to_string(),
            status: "started".into(),
            duration_ms: None,
            cost_usd: None,
            error: None,
        });
        proj.set_state("running", None);
    }

    fn spawn(&self, thread_id: &str, plan: LaunchPlan) -> Result<Arc<Run>, String> {
        let mut child = spawn_process(&plan.program, &plan.args, &plan.cwd, &plan.env)?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let stderr = child.stderr.take().ok_or("no stderr")?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let pid = child.id();
        let provider = plan.driver.provider();
        let run = Arc::new(Run {
            thread_id: thread_id.to_string(),
            provider,
            key: Mutex::new(plan.key),
            pid,
            child: Mutex::new(Some(child)),
            stdin: Mutex::new(Some(stdin)),
            driver: Mutex::new(plan.driver),
            proj: Mutex::new(Projector::new(self.db.clone(), self.sink.clone(), self.seqs.clone(), thread_id)),
            stderr: Arc::new(Mutex::new(Vec::new())),
            turn: Mutex::new(None),
            interrupting: AtomicBool::new(false),
            idle_since: Mutex::new(None),
        });
        let tail = run.stderr.clone();
        std::thread::spawn(move || {
            let mut stderr = stderr;
            let mut chunk = [0u8; 4096];
            while let Ok(n) = stderr.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                let mut buf = lock(&tail);
                buf.extend_from_slice(&chunk[..n]);
                if buf.len() > STDERR_TAIL {
                    let cut = buf.len() - STDERR_TAIL;
                    buf.drain(..cut);
                }
            }
        });
        let me = lock(&self.me).clone();
        let pumped = run.clone();
        std::thread::spawn(move || pump(me, pumped, stdout));
        lock(&self.runs).insert(thread_id.to_string(), run.clone());
        self.reap();
        Ok(run)
    }

    pub fn answer(&self, thread_id: &str, request_id: &str, answer: &Answer) -> Result<(), String> {
        let run = self.run(thread_id).ok_or("the agent process for this thread has ended")?;
        let lines = lock(&run.driver).answer(request_id, answer)?;
        run.write(&lines)?;
        lock(&run.proj).resolve(request_id, answer);
        Ok(())
    }

    /// Apply a new mode and/or model to the thread's live process, running
    /// or warm. `true` when the process took every change — it then counts
    /// as started with `key`; otherwise the next turn starts a fresh one.
    pub fn reconfigure(&self, thread_id: &str, key: &str, mode: Option<&str>, model: Option<&str>) -> bool {
        let Some(run) = self.run(thread_id) else {
            return false;
        };
        let mut lines = Vec::new();
        {
            let mut driver = lock(&run.driver);
            if let Some(m) = mode {
                match driver.set_mode(m) {
                    Some(l) => lines.push(l),
                    None => return false,
                }
            }
            if let Some(m) = model {
                match driver.set_model(m) {
                    Some(l) => lines.push(l),
                    None => return false,
                }
            }
        }
        if lines.is_empty() || run.write(&lines).is_err() {
            return false;
        }
        *lock(&run.key) = key.to_string();
        true
    }

    /// Whether a message can go into the thread's running turn right now.
    pub fn can_steer(&self, thread_id: &str) -> bool {
        self.run(thread_id).is_some_and(|r| r.turn_active() && lock(&r.driver).can_steer())
    }

    /// A message into the running turn (steering). Refused when no turn
    /// runs or the provider cannot take one mid-turn — then it is the
    /// caller's to queue or send.
    pub fn steer(&self, thread_id: &str, input: TurnInput) -> Result<(), String> {
        let run = self.run(thread_id).ok_or("nothing is running in this thread")?;
        if !run.turn_active() {
            return Err("nothing is running in this thread".into());
        }
        let lines = {
            let mut driver = lock(&run.driver);
            if !driver.can_steer() {
                return Err(format!("{} cannot take a message while it works", driver.provider()));
            }
            driver.user_message(&input)
        };
        run.write(&lines)?;
        lock(&run.proj).steer_message(&input.text);
        let _ = self.db.touch(thread_id);
        let _ = self.db.resume(thread_id);
        Ok(())
    }

    pub fn interrupt(&self, thread_id: &str) -> Result<(), String> {
        let run = self.run(thread_id).ok_or("nothing is running in this thread")?;
        let Some(turn) = lock(&run.turn).clone() else {
            return Ok(());
        };
        run.interrupting.store(true, Ordering::SeqCst);
        let lines = lock(&run.driver).interrupt();
        if run.write(&lines).is_err() {
            run.kill();
            return Ok(());
        }
        let watched = Arc::downgrade(&run);
        std::thread::spawn(move || {
            std::thread::sleep(INTERRUPT_GRACE);
            if let Some(run) = watched.upgrade() {
                if lock(&run.turn).as_deref() == Some(turn.as_str()) {
                    run.kill();
                }
            }
        });
        Ok(())
    }

    /// End the thread's process now (delete, provider switch).
    pub fn stop(&self, thread_id: &str) {
        if let Some(run) = lock(&self.runs).remove(thread_id) {
            run.interrupting.store(true, Ordering::SeqCst);
            run.kill();
        }
    }

    pub fn stop_all(&self) {
        let runs: Vec<Arc<Run>> = lock(&self.runs).drain().map(|(_, r)| r).collect();
        for run in runs {
            run.interrupting.store(true, Ordering::SeqCst);
            run.kill();
        }
    }

    /// Keep at most [`WARM_RUNS`] idle processes, none older than [`WARM_FOR`].
    fn reap(&self) {
        let mut idle: Vec<(Instant, Arc<Run>)> = lock(&self.runs)
            .values()
            .filter_map(|r| lock(&r.idle_since).map(|t| (t, r.clone())))
            .collect();
        idle.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
        for (i, (since, run)) in idle.into_iter().enumerate() {
            if i >= WARM_RUNS || since.elapsed() >= WARM_FOR {
                self.retire(&run);
            }
        }
    }

    fn retire(&self, run: &Arc<Run>) {
        let mut runs = lock(&self.runs);
        if runs.get(&run.thread_id).is_some_and(|r| Arc::ptr_eq(r, run)) {
            runs.remove(&run.thread_id);
        }
        drop(runs);
        run.close();
    }

    /// Tell the owner a turn ended (the queue moves on, or holds). Called
    /// on its own thread: the hook may start the next turn.
    fn notify_turn_end(&self, thread_id: &str, status: TurnStatus) {
        if let Some(hook) = lock(&self.on_turn_end).clone() {
            let thread_id = thread_id.to_string();
            std::thread::spawn(move || hook(&thread_id, status));
        }
    }

    pub fn set_turn_end_hook(&self, hook: TurnEndHook) {
        *lock(&self.on_turn_end) = Some(hook);
    }

    fn turn_ended(&self, run: &Arc<Run>, status: TurnStatus) {
        lock(&run.turn).take();
        run.interrupting.store(false, Ordering::SeqCst);
        self.notify_turn_end(&run.thread_id, status);
        let since = Instant::now();
        *lock(&run.idle_since) = Some(since);
        let me = lock(&self.me).clone();
        let watched = Arc::downgrade(run);
        std::thread::spawn(move || {
            std::thread::sleep(WARM_FOR);
            if let (Some(rt), Some(run)) = (me.upgrade(), watched.upgrade()) {
                if *lock(&run.idle_since) == Some(since) {
                    rt.retire(&run);
                }
            }
        });
        self.reap();
    }

    fn process_ended(&self, run: &Arc<Run>, code: Option<i32>) {
        let mut ended = None;
        {
            let mut proj = lock(&run.proj);
            if lock(&run.turn).take().is_some() {
                let interrupted = run.interrupting.load(Ordering::SeqCst);
                let tail = run.stderr_tail();
                let error = (!interrupted).then(|| {
                    let code = code.map(|c| format!(" (exit code {c})")).unwrap_or_default();
                    if tail.is_empty() {
                        format!("The agent process ended{code} before the turn finished.")
                    } else {
                        format!("The agent process ended{code} before the turn finished:\n{tail}")
                    }
                });
                let status = if interrupted { TurnStatus::Interrupted } else { TurnStatus::Failed };
                proj.end_turn(status, error, None, None);
                ended = Some(status);
            }
            proj.expire_pending();
            proj.flush();
        }
        {
            let mut runs = lock(&self.runs);
            if runs.get(&run.thread_id).is_some_and(|r| Arc::ptr_eq(r, run)) {
                runs.remove(&run.thread_id);
            }
        }
        if let Some(status) = ended {
            self.notify_turn_end(&run.thread_id, status);
        }
    }
}

fn pump(rt: Weak<Runtime>, run: Arc<Run>, stdout: std::process::ChildStdout) {
    let mut reader = BufReader::with_capacity(64 * 1024, stdout);
    let mut buf = Vec::with_capacity(64 * 1024);
    loop {
        match read_bounded_line(&mut reader, MAX_FRAME, &mut buf) {
            Ok(None) | Err(_) => break,
            Ok(Some(Err(len))) => {
                lock(&run.proj).notice(ItemKind::Notice, &format!("Skipped an oversized message from the agent ({len} bytes)."));
            }
            Ok(Some(Ok(()))) => {
                let line = String::from_utf8_lossy(&buf);
                if line.trim().is_empty() {
                    continue;
                }
                let mut ended = None;
                let mut writes = Vec::new();
                {
                    let mut driver = lock(&run.driver);
                    let emits = driver.on_line(&line);
                    if emits.is_empty() {
                        continue;
                    }
                    let mut proj = lock(&run.proj);
                    let info = |name: &str, input: &Value| driver.tool_info(name, input);
                    for e in emits {
                        match e {
                            Emit::Write(l) => writes.push(l),
                            Emit::Catalog(c) => {
                                if let Some(rt) = rt.upgrade() {
                                    rt.store_catalog(c);
                                }
                            }
                            other => {
                                if let Some(status) = proj.apply(other, &info, run.provider) {
                                    ended = Some(status);
                                }
                            }
                        }
                    }
                }
                if !writes.is_empty() {
                    let _ = run.write(&writes);
                }
                if let Some(status) = ended {
                    if let Some(rt) = rt.upgrade() {
                        rt.turn_ended(&run, status);
                    } else {
                        lock(&run.turn).take();
                    }
                }
            }
        }
    }
    let code = lock(&run.child).as_mut().and_then(|c| c.wait().ok()).and_then(|s| s.code());
    if let Some(rt) = rt.upgrade() {
        rt.process_ended(&run, code);
    }
}

/// Ask a CLI for its catalog: start it, send the driver's handshake, wait
/// for the `Catalog` emit, end it. Bounded in time and bytes.
pub fn discover(program: &str, args: &[String], cwd: &str, env: &[(String, String)], mut driver: Box<dyn Driver>) -> Result<Catalog, String> {
    let mut child = spawn_process(program, args, cwd, env)?;
    let pid = child.id();
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut buf = Vec::new();
        let mut total = 0usize;
        while let Ok(Some(Ok(()))) = read_bounded_line(&mut reader, 256 * 1024, &mut buf) {
            total += buf.len();
            if total > DISCOVERY_MAX_BYTES || tx.send(String::from_utf8_lossy(&buf).into_owned()).is_err() {
                break;
            }
        }
    });
    for line in driver.handshake() {
        let _ = stdin.write_all(line.as_bytes());
        let _ = stdin.write_all(b"\n");
    }
    let _ = stdin.flush();
    let deadline = Instant::now() + DISCOVERY_DEADLINE;
    let mut found = None;
    while found.is_none() {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        match rx.recv_timeout(left) {
            Ok(line) => {
                for e in driver.on_line(&line) {
                    match e {
                        Emit::Catalog(c) => found = Some(c),
                        Emit::Write(l) => {
                            let _ = stdin.write_all(l.as_bytes());
                            let _ = stdin.write_all(b"\n");
                        }
                        _ => {}
                    }
                }
            }
            Err(_) => break,
        }
    }
    drop(stdin);
    kill_tree(pid);
    let _ = child.kill();
    let _ = child.wait();
    found.ok_or_else(|| format!("{} did not report its models in time", driver.provider()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::ClaudeDriver;
    use crate::db::ThreadMeta;
    use std::io::Cursor;

    fn thread(db: &Database, id: &str) {
        db.insert_session(&ThreadMeta {
            id: id.into(),
            provider: "claude".into(),
            provider_session_id: None,
            title: String::new(),
            context_id: "general".into(),
            context_name: "General".into(),
            cwd: ".".into(),
            model: String::new(),
            mode: "full".into(),
            created_at: 1,
            updated_at: 1,
            cost_usd: 0.0,
            input_tokens: 0,
            output_tokens: 0,
            last_error: None,
            kind: "chat".into(),
            effort: String::new(),
            title_source: "auto".into(),
            state: "idle".into(),
            context_tokens: None,
            context_window: None,
            last_activity_at: 1,
            pinned: false,
            sort_key: 1.0,
            settled_at: None,
            snoozed_until: None,
            auto_settle: true,
        })
        .unwrap();
    }

    type Log = Arc<Mutex<Vec<(u64, ThreadEvent)>>>;

    fn sink() -> (EventSink, Log) {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let l = log.clone();
        (Arc::new(move |_t: &str, seq: u64, e: &ThreadEvent| lock(&l).push((seq, e.clone()))), log)
    }

    #[test]
    fn bounded_lines_skip_oversized_frames() {
        let data = format!("{}\nshort\r\n{}\nlast", "x".repeat(50), "y".repeat(10));
        let mut r = Cursor::new(data.into_bytes());
        let mut buf = Vec::new();
        assert_eq!(read_bounded_line(&mut r, 20, &mut buf).unwrap(), Some(Err(50)));
        assert_eq!(read_bounded_line(&mut r, 20, &mut buf).unwrap(), Some(Ok(())));
        assert_eq!(buf, b"short");
        assert_eq!(read_bounded_line(&mut r, 20, &mut buf).unwrap(), Some(Ok(())));
        assert_eq!(buf, b"yyyyyyyyyy");
        assert_eq!(read_bounded_line(&mut r, 20, &mut buf).unwrap(), Some(Ok(())));
        assert_eq!(buf, b"last");
        assert_eq!(read_bounded_line(&mut r, 20, &mut buf).unwrap(), None);
    }

    #[test]
    fn projector_turns_a_recorded_claude_turn_into_items() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        thread(&db, "t1");
        let (sink, log) = sink();
        let mut proj = Projector::new(db.clone(), sink, Arc::new(Seqs::default()), "t1");
        proj.begin_turn("u1");
        proj.user_message("hi", json!([]));
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/claude_tool_turn.jsonl");
        let mut d = ClaudeDriver::new();
        let mut status = None;
        for line in std::fs::read_to_string(path).unwrap().lines() {
            for e in d.on_line(line) {
                let info = |n: &str, i: &Value| d.tool_info(n, i);
                if let Some(s) = proj.apply(e, &info, "claude") {
                    status = Some(s);
                }
            }
        }
        assert_eq!(status, Some(TurnStatus::Completed));
        let items = db.items("t1", None, 100).unwrap();
        let kinds: Vec<&str> = items.iter().map(|i| i.kind.as_str()).collect();
        assert_eq!(kinds, vec!["user", "reasoning", "tool", "reasoning", "assistant"]);
        let tool = &items[2];
        assert_eq!(tool.data["status"], "completed");
        assert_eq!(tool.data["title"], "echo hello");
        assert_eq!(tool.data["output"], "hello");
        let answer = &items[4];
        assert_eq!(answer.data["text"], "Remembered: **KIWI-42**.\n\n`echo hello` printed: `hello`");
        assert_eq!(answer.data["streaming"], false);
        let t = db.get_session("t1").unwrap().unwrap();
        assert_eq!(t.provider_session_id.as_deref(), Some("5156b244-fca0-4214-9a7a-5a1bd39a331b"));
        assert!((t.cost_usd - 0.00899421).abs() < 1e-9);
        assert_eq!(t.context_window, Some(1_000_000));
        assert_eq!(t.state, "idle");
        let log = lock(&log);
        let seqs: Vec<u64> = log.iter().map(|(s, _)| *s).collect();
        assert!(seqs.windows(2).all(|w| w[1] == w[0] + 1), "envelope seqs are contiguous");
        let streamed: String = log
            .iter()
            .filter_map(|(_, e)| if let ThreadEvent::Delta { text, .. } = e { Some(text.as_str()) } else { None })
            .collect();
        assert_eq!(streamed, "Remembered: **KIWI-42**.\n\n`echo hello` printed: `hello`", "no duplicated text");
    }

    #[test]
    fn requests_wait_and_resolve() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        thread(&db, "t1");
        let (sink, log) = sink();
        let mut proj = Projector::new(db.clone(), sink, Arc::new(Seqs::default()), "t1");
        proj.begin_turn("u1");
        let mut d = ClaudeDriver::new();
        let info = |n: &str, i: &Value| ClaudeDriver::new().tool_info(n, i);
        for e in d.on_line(r#"{"type":"control_request","request_id":"r1","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"rm -rf build"},"tool_use_id":"t1"}}"#) {
            proj.apply(e, &info, "claude");
        }
        let item = db.items("t1", None, 10).unwrap().pop().unwrap();
        assert_eq!(item.kind, ItemKind::Approval);
        assert_eq!(item.data["title"], "rm -rf build");
        assert_eq!(db.get_session("t1").unwrap().unwrap().state, "waiting");
        proj.resolve("r1", &Answer::Deny { message: Some("no".into()) });
        let item = db.get_item(&item.id).unwrap().unwrap();
        assert_eq!(item.data["status"], "denied");
        assert_eq!(item.data["reply"], "no");
        assert_eq!(db.get_session("t1").unwrap().unwrap().state, "running");
        assert!(lock(&log).iter().any(|(_, e)| matches!(e, ThreadEvent::State { state, .. } if state == "waiting")));
    }

    #[test]
    fn a_failed_turn_leaves_a_visible_error() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        thread(&db, "t1");
        let (sink, _) = sink();
        let mut proj = Projector::new(db.clone(), sink, Arc::new(Seqs::default()), "t1");
        proj.begin_turn("u1");
        proj.end_turn(TurnStatus::Failed, Some("boom".into()), None, None);
        let items = db.items("t1", None, 10).unwrap();
        assert_eq!(items.last().unwrap().kind, ItemKind::Error);
        let t = db.get_session("t1").unwrap().unwrap();
        assert_eq!((t.state.as_str(), t.last_error.as_deref()), ("error", Some("boom")));
    }

    fn wait_for_turn(db: &Database, rt: &Runtime, thread_id: &str, deny: bool) -> crate::db::TurnRow {
        let start = Instant::now();
        loop {
            if deny {
                for item in db.items(thread_id, None, 50).unwrap() {
                    if item.kind == ItemKind::Approval && item.data["status"] == "pending" {
                        let rid = item.data["requestId"].as_str().unwrap().to_string();
                        let _ = rt.answer(thread_id, &rid, &Answer::Deny { message: None });
                    }
                }
            }
            if let Some(t) = db.last_turn(thread_id).unwrap() {
                if t.status != "started" {
                    return t;
                }
            }
            assert!(start.elapsed() < Duration::from_secs(180), "the live turn did not finish");
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// Live, opt-in: `cargo test -p tauri-plugin-pilot -- --ignored live_claude --nocapture`.
    /// A temporary git repo, one tiny Claude Code turn, a fresh process that
    /// resumes the session and must remember the token. Every tool request
    /// is denied.
    #[test]
    #[ignore = "live: two tiny Claude Code turns (costs a few cents)"]
    fn live_claude_remembers_across_resume() {
        use crate::claude::{launch_args, LaunchOptions, SessionRef};
        let claude = crate::providers::which("claude").expect("claude on PATH").to_string_lossy().into_owned();
        let dir = std::env::temp_dir().join(format!("qp-live-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let _ = Command::new("git").arg("init").arg("-q").current_dir(&dir).status();
        let db = Arc::new(Database::open_in_memory().unwrap());
        thread(&db, "t1");
        let (sink, _) = sink();
        let rt = Runtime::new(db.clone(), sink);
        let sid = uuid::Uuid::new_v4().to_string();
        let cwd = dir.to_string_lossy().into_owned();
        let plan = |session: SessionRef| {
            let mut args = launch_args(&LaunchOptions {
                session,
                model: "haiku".into(),
                effort: String::new(),
                mode: "ask".into(),
                mcp_config: None,
                append_prompt_file: None,
                add_dirs: Vec::new(),
            });
            args.extend(["--settings".into(), r#"{"permissions":{"ask":["Bash","Write","Edit"]}}"#.into()]);
            LaunchPlan {
                key: "live".into(),
                program: claude.clone(),
                args,
                cwd: cwd.clone(),
                env: vec![("PATH".into(), qs_core::system_path::current().to_string_lossy().into_owned())],
                driver: Box::new(ClaudeDriver::new()),
            }
        };
        let first = TurnInput { text: "Remember the token PAPAYA-7. Reply with OK only.".into(), images: Vec::new() };
        rt.send("t1", first, json!([]), || Ok(plan(SessionRef::New(sid.clone()))), "live").unwrap();
        let t = wait_for_turn(&db, &rt, "t1", true);
        assert_eq!(t.status, "completed", "first turn: {:?}", t.error);
        assert_eq!(db.get_session("t1").unwrap().unwrap().provider_session_id.as_deref(), Some(sid.as_str()));

        rt.stop("t1");
        let start = Instant::now();
        while rt.process_alive("t1") && start.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(50));
        }
        let second = TurnInput { text: "What token did I ask you to remember? Reply with the token only.".into(), images: Vec::new() };
        rt.send("t1", second, json!([]), || Ok(plan(SessionRef::Resume(sid.clone()))), "live").unwrap();
        let t = wait_for_turn(&db, &rt, "t1", true);
        assert_eq!(t.status, "completed", "second turn: {:?}", t.error);
        let answer = db
            .items("t1", None, 100)
            .unwrap()
            .into_iter()
            .rev()
            .find(|i| i.kind == ItemKind::Assistant)
            .expect("an answer");
        println!("answer: {}", answer.data["text"]);
        assert!(answer.data["text"].as_str().unwrap().contains("PAPAYA-7"));
        rt.stop_all();
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Live, opt-in: Supervised with a forced ask rule. An allowed Write runs;
    /// a denied one sends the reason to the agent, which acts on it.
    #[test]
    #[ignore = "live: two small Claude Code turns with approvals (costs a few cents)"]
    fn live_claude_approvals() {
        use crate::claude::{launch_args, LaunchOptions, SessionRef};
        let claude = crate::providers::which("claude").expect("claude on PATH").to_string_lossy().into_owned();
        let dir = std::env::temp_dir().join(format!("qp-live-ap-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Arc::new(Database::open_in_memory().unwrap());
        thread(&db, "t1");
        let (sink, _) = sink();
        let rt = Runtime::new(db.clone(), sink);
        let sid = uuid::Uuid::new_v4().to_string();
        let cwd = dir.to_string_lossy().into_owned();
        let plan = |session: SessionRef| {
            let mut args = launch_args(&LaunchOptions {
                session,
                model: "haiku".into(),
                effort: String::new(),
                mode: "ask".into(),
                mcp_config: None,
                append_prompt_file: None,
                add_dirs: Vec::new(),
            });
            args.extend(["--settings".into(), r#"{"permissions":{"ask":["Write","Bash"]}}"#.into()]);
            LaunchPlan {
                key: "live".into(),
                program: claude.clone(),
                args,
                cwd: cwd.clone(),
                env: vec![("PATH".into(), qs_core::system_path::current().to_string_lossy().into_owned())],
                driver: Box::new(ClaudeDriver::new()),
            }
        };
        let answer_all = |answer: Answer| {
            let start = Instant::now();
            loop {
                if let Some(t) = db.last_turn("t1").unwrap() {
                    if t.status != "started" {
                        return t;
                    }
                }
                for item in db.items("t1", None, 50).unwrap() {
                    if item.kind == ItemKind::Approval && item.data["status"] == "pending" {
                        let _ = rt.answer("t1", item.data["requestId"].as_str().unwrap(), &answer);
                    }
                }
                assert!(start.elapsed() < Duration::from_secs(180), "the live turn did not finish");
                std::thread::sleep(Duration::from_millis(200));
            }
        };
        let first = TurnInput { text: "Create a file named a.txt containing exactly HI using your Write tool. Then reply DONE.".into(), images: Vec::new() };
        rt.send("t1", first, json!([]), || Ok(plan(SessionRef::New(sid.clone()))), "live").unwrap();
        let t = answer_all(Answer::Allow);
        assert_eq!(t.status, "completed", "{:?}", t.error);
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap_or_default().trim(), "HI", "the allowed write ran");

        let second = TurnInput { text: "Create a file named c.txt containing HI using your Write tool.".into(), images: Vec::new() };
        rt.send("t1", second, json!([]), || Ok(plan(SessionRef::Resume(sid.clone()))), "live").unwrap();
        let t = answer_all(Answer::Deny { message: Some("No new files. Reply with the single word MANGO instead.".into()) });
        assert_eq!(t.status, "completed", "{:?}", t.error);
        assert!(!dir.join("c.txt").exists(), "the denied write did not run");
        let answer = db.items("t1", None, 100).unwrap().into_iter().rev().find(|i| i.kind == ItemKind::Assistant).unwrap();
        println!("answer after deny: {}", answer.data["text"]);
        assert!(answer.data["text"].as_str().unwrap().contains("MANGO"), "the agent read the reason");
        rt.stop_all();
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Live, opt-in, no inference: the installed Claude Code reports its catalog.
    #[test]
    #[ignore = "live: starts the installed Claude Code for its initialize handshake only"]
    fn live_claude_catalog() {
        let claude = crate::providers::which("claude").expect("claude on PATH").to_string_lossy().into_owned();
        let env = vec![("PATH".to_string(), qs_core::system_path::current().to_string_lossy().into_owned())];
        let cwd = std::env::temp_dir().to_string_lossy().into_owned();
        let started = Instant::now();
        let c = discover(&claude, &crate::claude::discovery_args(), &cwd, &env, Box::new(ClaudeDriver::new())).unwrap();
        println!("catalog in {:?}: {} models, {} commands, version {:?}", started.elapsed(), c.models.len(), c.commands.len(), c.version);
        assert!(!c.models.is_empty());
    }

    /// The stand-in CLI for the test below: this test binary, run again with
    /// `QP_STAND_IN` set. It reads the handshake line first — so the
    /// runtime's writes never race its exit — then prints a recorded turn.
    /// The harness's own output lines are not JSON and are ignored.
    #[test]
    #[ignore = "a helper process for a_process_turn_runs_end_to_end"]
    fn stand_in_cli() {
        let Ok(path) = std::env::var("QP_STAND_IN") else {
            return;
        };
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
        print!("{}", std::fs::read_to_string(path).unwrap());
        let _ = std::io::stdout().flush();
    }

    /// The whole path with a real process: a stand-in CLI that prints a
    /// recorded Claude turn and exits.
    #[test]
    fn a_process_turn_runs_end_to_end() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        thread(&db, "t1");
        let (sink, log) = sink();
        let rt = Runtime::new(db.clone(), sink);
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/claude_tool_turn.jsonl");
        let program = std::env::current_exe().unwrap().to_string_lossy().into_owned();
        let args: Vec<String> = ["--exact", "runtime::tests::stand_in_cli", "--ignored", "--nocapture", "--test-threads=1"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let env = vec![("QP_STAND_IN".to_string(), fixture.to_string_lossy().into_owned())];
        let ended: Arc<Mutex<Vec<(String, TurnStatus)>>> = Arc::new(Mutex::new(Vec::new()));
        {
            let ended = ended.clone();
            rt.set_turn_end_hook(Arc::new(move |t: &str, s: TurnStatus| lock(&ended).push((t.to_string(), s))));
        }
        let turn = rt
            .send(
                "t1",
                TurnInput { text: "hi".into(), images: Vec::new() },
                json!([]),
                || {
                    Ok(LaunchPlan {
                        key: "k".into(),
                        program,
                        args,
                        cwd: ".".into(),
                        env,
                        driver: Box::new(ClaudeDriver::new()),
                    })
                },
                "k",
            )
            .unwrap();
        let start = Instant::now();
        while rt.process_alive("t1") && start.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!rt.process_alive("t1"), "the stand-in exited and the run was cleaned up");
        let t = db.last_turn("t1").unwrap().unwrap();
        assert_eq!((t.id.as_str(), t.status.as_str()), (turn.as_str(), "completed"), "turn error: {:?}", t.error);
        assert!(lock(&log).iter().any(|(_, e)| matches!(e, ThreadEvent::Turn { status, .. } if status == "completed")));
        let start = Instant::now();
        while lock(&ended).is_empty() && start.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(lock(&ended).as_slice(), &[("t1".to_string(), TurnStatus::Completed)], "the owner hears about the end once");
        assert!(rt.steer("t1", TurnInput { text: "more".into(), images: Vec::new() }).is_err(), "nothing runs to steer into");
        assert!(rt.send("t1", TurnInput::default(), json!([]), || Err("no CLI here".into()), "other").is_err());
        let items = db.items("t1", None, 100).unwrap();
        assert_eq!(items.last().unwrap().data["text"], "no CLI here", "a failed start is visible");
    }
}
