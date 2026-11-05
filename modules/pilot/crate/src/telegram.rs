//! The Telegram bot (round 4, card 2, docs/PLAN-QUANTPILOT.md §53): talk to an
//! agent from Telegram.
//!
//! Off by default. The user creates a bot with BotFather and pastes its token,
//! which goes to the vault. The bot polls outward (`getUpdates`), so there is no
//! public port and no webhook. It answers one private chat, linked with a
//! one-time code; every other chat is ignored without an answer.
//!
//! Threads are ordinary QuantPilot threads, through the same `Backend` the phone
//! page uses. Anything that makes an agent act needs the PIN (`/unlock`, 10
//! minutes, the bot deletes that message), the same PIN as phone access. Messages
//! pass through Telegram's servers and are not end-to-end encrypted: Settings
//! says so.

use crate::db::{Database, PhoneAudit};
use crate::driver::Answer;
use crate::events::ThreadEvent;
use crate::phone::{self, AppBackend, Backend, PhoneItem};
use crate::PilotState;
use qs_gateway::{pairing, pairing::Pairing, stepup, stepup::StepUps};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State};
use tokio::sync::mpsc;

const API: &str = "https://api.telegram.org";
/// The vault entry holding the bot's token.
const TOKEN_SECRET: &str = "pilot.telegram-token";
/// Telegram's limit for one message.
const MESSAGE_LIMIT: usize = 4096;
const POLL_SECONDS: u64 = 30;
/// The StepUps key: there is one linked chat.
const DEVICE: &str = "telegram";
/// Buttons kept for presses; older ones say they expired.
const BUTTONS_KEPT: usize = 200;
const BACKOFF: [u64; 5] = [1, 2, 5, 10, 30];

/// What is saved in the settings; the commands below are the only way to change it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TelegramSettings {
    pub enabled: bool,
}

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

// ── Telegram's API ──────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub enum ApiError {
    /// The token is wrong or revoked: polling cannot help.
    Unauthorized,
    /// Another program is reading this bot.
    Conflict,
    /// Too many messages: wait this long.
    RateLimited(u64),
    Other(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthorized => write!(f, "Telegram refused this token."),
            Self::Conflict => write!(f, "Another program is reading messages for this bot."),
            Self::RateLimited(s) => write!(f, "Telegram asks to wait {s} seconds."),
            Self::Other(e) => write!(f, "{e}"),
        }
    }
}

/// The token is in every URL, so no error may carry the URL.
pub struct Api {
    http: reqwest::Client,
    base: String,
    token: zeroize::Zeroizing<String>,
}

impl Api {
    pub fn new(base: &str, token: &str) -> Self {
        Self { http: reqwest::Client::new(), base: base.trim_end_matches('/').to_string(), token: zeroize::Zeroizing::new(token.to_string()) }
    }

    pub async fn call(&self, method: &str, body: Value, timeout: Duration) -> Result<Value, ApiError> {
        let url = format!("{}/bot{}/{method}", self.base, self.token.as_str());
        let response = self
            .http
            .post(url)
            .json(&body)
            .timeout(timeout)
            .send()
            .await
            .map_err(|e| ApiError::Other(format!("Telegram cannot be reached: {}", e.without_url())))?;
        let status = response.status().as_u16();
        let reply: Value = response.json().await.map_err(|e| ApiError::Other(format!("Telegram's answer: {}", e.without_url())))?;
        if reply["ok"].as_bool() == Some(true) {
            return Ok(reply["result"].clone());
        }
        match status {
            401 | 404 => Err(ApiError::Unauthorized),
            409 => Err(ApiError::Conflict),
            429 => Err(ApiError::RateLimited(reply["parameters"]["retry_after"].as_u64().unwrap_or(5))),
            _ => Err(ApiError::Other(format!("Telegram: {}", reply["description"].as_str().unwrap_or("refused")))),
        }
    }

    pub async fn send(&self, method: &str, body: Value) -> Result<Value, ApiError> {
        match self.call(method, body.clone(), Duration::from_secs(20)).await {
            Err(ApiError::RateLimited(s)) => {
                tokio::time::sleep(Duration::from_secs(s.min(60))).await;
                self.call(method, body, Duration::from_secs(20)).await
            }
            other => other,
        }
    }
}

/// A tool's input for reading: a command as itself, its description in words.
pub fn readable_input(input: &str, title: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(input) else {
        return if input.is_empty() || input == title { String::new() } else { format!("\n\n{input}") };
    };
    let Some(command) = value["command"].as_str() else {
        return format!("\n\n{input}");
    };
    let mut out = String::new();
    if let Some(description) = value["description"].as_str() {
        out.push_str(&format!("\n{description}"));
    }
    if command != title {
        out.push_str(&format!("\n\n{command}"));
    }
    out
}

/// A text in pieces Telegram takes, cut at line ends where it can.
pub fn split_message(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut rest = text.trim();
    while rest.chars().count() > MESSAGE_LIMIT {
        let cut = rest.char_indices().nth(MESSAGE_LIMIT).map(|(i, _)| i).unwrap_or(rest.len());
        let head = &rest[..cut];
        // Back to a line end, unless that would leave a very short piece.
        let at = head.rfind('\n').filter(|i| *i > cut / 2).unwrap_or(cut);
        parts.push(rest[..at].trim_end().to_string());
        rest = rest[at..].trim_start();
    }
    if !rest.is_empty() {
        parts.push(rest.to_string());
    }
    parts
}

// ── The bot ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Button {
    thread: String,
    request: String,
    answer: Answer,
}

#[derive(Default)]
struct Session {
    /// `/threads`' numbers, for `/use`.
    numbered: Vec<String>,
    buttons: HashMap<String, Button>,
    button_order: Vec<String>,
    next_button: u64,
    /// Requests already shown, so a thread waiting again does not repeat them.
    announced: HashSet<String>,
    /// What the chat said while locked; sent after `/unlock`.
    held: Option<String>,
}

pub struct Bot {
    api: Api,
    backend: Arc<dyn Backend>,
    db: Arc<Database>,
    pairing: Arc<Mutex<Pairing>>,
    stepups: Arc<Mutex<StepUps>>,
    session: tokio::sync::Mutex<Session>,
}

const HELP: &str = "Message me and I pass it to your agent.\n\n/threads lists your threads\n/use 2 talks to one of them\n/new a task starts a new thread in General\n/new @project a task starts one in a project\n/projects lists your projects\n/stop stops the agent\n/unlock 123456 enters your PIN, valid for 10 minutes\n/help shows this";

impl Bot {
    pub fn new(api: Api, backend: Arc<dyn Backend>, db: Arc<Database>, pairing: Arc<Mutex<Pairing>>, stepups: Arc<Mutex<StepUps>>) -> Self {
        Self { api, backend, db, pairing, stepups, session: tokio::sync::Mutex::new(Session::default()) }
    }

    fn audit(&self, action: &str, thread: Option<&str>, ok: bool, note: &str) {
        self.db.phone_audit_add(&PhoneAudit {
            at: now(),
            device: "Telegram".into(),
            action: action.into(),
            thread_id: thread.map(str::to_string),
            ok,
            note: note.chars().take(200).collect(),
        });
    }

    fn unlocked(&self) -> bool {
        self.stepups.lock().map(|s| s.granted(DEVICE, Instant::now())).unwrap_or(false)
    }

    async fn say(&self, chat: i64, text: &str) {
        self.say_with(chat, text, None).await;
    }

    /// The buttons go on the last piece, next to what they answer.
    async fn say_with(&self, chat: i64, text: &str, buttons: Option<Value>) {
        let parts = split_message(text);
        let last = parts.len().saturating_sub(1);
        for (i, part) in parts.iter().enumerate() {
            let mut body = json!({ "chat_id": chat, "text": part, "disable_web_page_preview": true });
            if let (true, Some(b)) = (i == last, buttons.as_ref()) {
                body["reply_markup"] = json!({ "inline_keyboard": b });
            }
            if let Err(e) = self.api.send("sendMessage", body).await {
                eprintln!("pilot: telegram: could not send a message: {e}");
            }
        }
    }

    async fn blocking<T: Send + 'static>(&self, call: impl FnOnce(&dyn Backend) -> Result<T, String> + Send + 'static) -> Result<T, String> {
        let backend = self.backend.clone();
        tokio::task::spawn_blocking(move || call(backend.as_ref())).await.map_err(|e| e.to_string())?
    }

    /// One update from Telegram. Only ever errors in what it says back.
    pub async fn on_update(&self, update: &Value) {
        if let Some(message) = update.get("message") {
            self.on_message(message).await;
        } else if let Some(press) = update.get("callback_query") {
            self.on_press(press).await;
        }
    }

    async fn on_message(&self, message: &Value) {
        if message["chat"]["type"].as_str() != Some("private") {
            return;
        }
        let Some(chat) = message["chat"]["id"].as_i64() else { return };
        let text = message["text"].as_str().unwrap_or_default().trim().to_string();
        let linked = self.db.telegram_state().chat_id;
        match linked {
            None => return self.try_link(chat, message, &text).await,
            Some(id) if id != chat => return,
            Some(_) => {}
        }
        if text.is_empty() {
            return self.say(chat, "I can only pass on text for now.").await;
        }
        let (command, rest) = match text.strip_prefix('/') {
            Some(body) => {
                let (head, rest) = body.split_once(char::is_whitespace).unwrap_or((body, ""));
                // Telegram may add the bot's name: /threads@my_bot.
                (head.split('@').next().unwrap_or(head).to_lowercase(), rest.trim().to_string())
            }
            None => (String::new(), String::new()),
        };
        match command.as_str() {
            "" => self.on_text(chat, text).await,
            "start" | "help" => self.say(chat, HELP).await,
            "unlock" => self.unlock(chat, message, &rest).await,
            "threads" => self.list(chat).await,
            "projects" => self.projects(chat).await,
            "use" => self.use_thread(chat, &rest).await,
            "new" => self.new_thread(chat, &rest).await,
            "stop" => self.stop(chat).await,
            _ => self.say(chat, "I do not know that command. /help lists them.").await,
        }
    }

    async fn try_link(&self, chat: i64, message: &Value, text: &str) {
        let Some(code) = text.strip_prefix("/start").map(str::trim).filter(|c| !c.is_empty()) else { return };
        let checked = match self.pairing.lock() {
            Ok(mut p) => {
                // Nobody learns there is a bot here unless a code is open.
                if p.open(Instant::now()).is_none() && p.locked_for(Instant::now()).is_none() {
                    return;
                }
                p.check(code, Instant::now())
            }
            Err(_) => return,
        };
        match checked {
            Ok(()) => {
                let from = &message["chat"];
                let name = [from["first_name"].as_str(), from["username"].as_str()].into_iter().flatten().next().unwrap_or("Telegram").to_string();
                if self.db.telegram_link(Some(chat), &name).is_ok() {
                    self.audit("link", None, true, &name);
                    self.say(chat, &format!("Linked. Only this chat can talk to your agents now.\n\n{HELP}")).await;
                }
            }
            Err(pairing::CodeError::Wrong { attempts_left }) => self.say(chat, &format!("Wrong code ({attempts_left} attempts left).")).await,
            Err(pairing::CodeError::Locked { .. }) => self.say(chat, "Too many wrong codes. Try again later.").await,
            Err(_) => {}
        }
    }

    async fn unlock(&self, chat: i64, message: &Value, pin: &str) {
        // The PIN leaves the chat first, whatever happens next.
        if let Some(id) = message["message_id"].as_i64() {
            let _ = self.api.send("deleteMessage", json!({ "chat_id": chat, "message_id": id })).await;
        }
        let locked = self.stepups.lock().ok().map(|s| s.check_locked(DEVICE, Instant::now()));
        if let Some(Err(e)) = locked {
            return self.say(chat, &e.to_string()).await;
        }
        let attempt = pin.to_string();
        let right = self
            .blocking(move |b| match b.pin_hash()? {
                Some(hash) => Ok(stepup::verify_pin(&hash, &attempt)),
                None => Err("No PIN is set. Set one in QuantSuite on the PC.".to_string()),
            })
            .await;
        let right = match right {
            Ok(r) => r,
            Err(e) => return self.say(chat, &e).await,
        };
        let recorded = self.stepups.lock().ok().map(|mut s| s.record(DEVICE, right, Instant::now()));
        match recorded {
            Some(Ok(_)) => {
                self.audit("pin", None, true, "");
                self.say(chat, "Unlocked for 10 minutes.").await;
                let held = self.session.lock().await.held.take();
                if let Some(text) = held {
                    self.on_text(chat, text).await;
                }
            }
            Some(Err(e)) => {
                self.audit("pin", None, false, "wrong PIN");
                self.say(chat, &e.to_string()).await;
            }
            None => {}
        }
    }

    /// Plain text: the answer to a waiting question, else a message to the
    /// current thread, else a new thread.
    async fn on_text(&self, chat: i64, text: String) {
        if !self.unlocked() {
            self.session.lock().await.held = Some(text);
            return self.say(chat, "Locked. Send /unlock and your PIN, and I will pass this on.").await;
        }
        let current = self.db.telegram_state().current_thread;
        let Some(thread) = current else {
            return self.start_thread(chat, "general", text).await;
        };
        // A question the agent asked takes the text as its answer.
        let id = thread.clone();
        if let Ok(view) = self.blocking(move |b| b.thread(&id)).await {
            let open: Vec<&PhoneItem> = view.items.iter().filter(|i| i.kind == "question" && i.status == "pending" && !i.on_pc).collect();
            if let Some(item) = open.first() {
                let questions = item.questions.as_array().cloned().unwrap_or_default();
                if questions.len() != 1 {
                    return self.say(chat, "This question has several parts. Answer it on the phone page or the PC.").await;
                }
                let mut answers = serde_json::Map::new();
                answers.insert(questions[0]["question"].as_str().unwrap_or_default().to_string(), Value::String(text));
                let request = item.request_id.clone().unwrap_or_default();
                return self.answer(chat, thread, request, Answer::Answers { answers, files: Default::default() }).await;
            }
        }
        let id = thread.clone();
        match self.blocking(move |b| b.send(&id, text)).await {
            Ok(()) => {
                let _ = self.db.telegram_follow(&thread);
                self.audit("message", Some(&thread), true, "");
            }
            Err(e) => {
                self.audit("message", Some(&thread), false, &e);
                self.say(chat, &e).await;
            }
        }
    }

    async fn start_thread(&self, chat: i64, project: &str, text: String) {
        let project_id = project.to_string();
        match self.blocking(move |b| b.create(&project_id, text)).await {
            Ok(id) => {
                let _ = self.db.telegram_set_current(Some(&id));
                let _ = self.db.telegram_follow(&id);
                self.audit("new thread", Some(&id), true, "");
                self.say(chat, "Started a new thread. I will send the answer here.").await;
            }
            Err(e) => {
                self.audit("new thread", None, false, &e);
                self.say(chat, &e).await;
            }
        }
    }

    async fn new_thread(&self, chat: i64, rest: &str) {
        if rest.is_empty() {
            return self.say(chat, "What should the agent do? For example: /new fix the failing test").await;
        }
        if !self.unlocked() {
            return self.say(chat, "Locked. Send /unlock and your PIN first.").await;
        }
        let (project, text) = match rest.strip_prefix('@') {
            Some(body) => {
                let (name, text) = body.split_once(char::is_whitespace).unwrap_or((body, ""));
                let wanted = name.replace(['-', '_'], " ").to_lowercase();
                let projects = self.backend.projects();
                match projects.iter().find(|p| p.name.to_lowercase() == wanted || p.name.to_lowercase().starts_with(&wanted)) {
                    Some(p) => (p.id.clone(), text.trim().to_string()),
                    None => return self.say(chat, &format!("I do not know the project \"{name}\". /projects lists them.")).await,
                }
            }
            None => ("general".to_string(), rest.to_string()),
        };
        if text.is_empty() {
            return self.say(chat, "What should the agent do?").await;
        }
        self.start_thread(chat, &project, text).await;
    }

    async fn list(&self, chat: i64) {
        let rows = match self.blocking(|b| b.threads()).await {
            Ok(rows) => rows,
            Err(e) => return self.say(chat, &e).await,
        };
        if rows.is_empty() {
            return self.say(chat, "No threads yet. Message me to start one.").await;
        }
        let current = self.db.telegram_state().current_thread;
        let mut lines = Vec::new();
        for (i, r) in rows.iter().take(15).enumerate() {
            let state = match (&r.needs, r.running) {
                (Some(n), _) => format!(" · needs you: {n}"),
                (None, true) => " · working".to_string(),
                _ => String::new(),
            };
            let mark = if current.as_deref() == Some(r.id.as_str()) { " (current)" } else { "" };
            lines.push(format!("{}. {}{mark} · {}{state}", i + 1, r.title, r.project));
        }
        self.session.lock().await.numbered = rows.iter().take(15).map(|r| r.id.clone()).collect();
        lines.push("\n/use and a number talks to one.".into());
        self.say(chat, &lines.join("\n")).await;
    }

    async fn projects(&self, chat: i64) {
        let names: Vec<String> = self.backend.projects().into_iter().map(|p| format!("@{}", p.name.replace(' ', "-"))).collect();
        self.say(chat, &format!("{}\n\n/new @project and a task starts a thread there.", names.join("\n"))).await;
    }

    async fn use_thread(&self, chat: i64, rest: &str) {
        let number = rest.parse::<usize>().ok().filter(|n| *n > 0);
        let id = {
            let s = self.session.lock().await;
            number.and_then(|n| s.numbered.get(n - 1).cloned())
        };
        let Some(id) = id else { return self.say(chat, "Send /threads first, then /use and a number.").await };
        let _ = self.db.telegram_set_current(Some(&id));
        let _ = self.db.telegram_follow(&id);
        let shown = id.clone();
        match self.blocking(move |b| b.thread(&shown)).await {
            Ok(view) => {
                self.say(chat, &format!("Now talking to \"{}\" in {}.", view.title, view.project)).await;
                self.announce_requests(chat, &view.id, &view.title, &view.items).await;
            }
            Err(e) => self.say(chat, &e).await,
        }
    }

    async fn stop(&self, chat: i64) {
        if !self.unlocked() {
            return self.say(chat, "Locked. Send /unlock and your PIN first.").await;
        }
        let Some(thread) = self.db.telegram_state().current_thread else {
            return self.say(chat, "No current thread. /threads lists them.").await;
        };
        let id = thread.clone();
        match self.blocking(move |b| b.stop(&id)).await {
            Ok(()) => {
                self.audit("stop", Some(&thread), true, "");
                self.say(chat, "Stopped.").await;
            }
            Err(e) => {
                self.audit("stop", Some(&thread), false, &e);
                self.say(chat, &e).await;
            }
        }
    }

    async fn answer(&self, chat: i64, thread: String, request: String, answer: Answer) {
        let action = match answer {
            Answer::Allow => "allow",
            Answer::AllowSession => "allow for the session",
            Answer::Deny { .. } => "deny",
            _ => "answer",
        };
        let id = thread.clone();
        match self.blocking(move |b| b.answer(&id, &request, answer)).await {
            Ok(()) => self.audit(action, Some(&thread), true, ""),
            Err(e) => {
                self.audit(action, Some(&thread), false, &e);
                self.say(chat, &e).await;
            }
        }
    }

    async fn on_press(&self, press: &Value) {
        let chat = press["message"]["chat"]["id"].as_i64();
        let linked = self.db.telegram_state().chat_id;
        if chat.is_none() || chat != linked {
            return;
        }
        let chat = chat.unwrap_or_default();
        let id = press["id"].as_str().unwrap_or_default().to_string();
        let say = |text: &str, alert: bool| json!({ "callback_query_id": id, "text": text, "show_alert": alert });
        if !self.unlocked() {
            let _ = self.api.send("answerCallbackQuery", say("Locked. Send /unlock and your PIN first, then press again.", true)).await;
            return;
        }
        let key = press["data"].as_str().unwrap_or_default();
        let button = self.session.lock().await.buttons.remove(key);
        let Some(button) = button else {
            let _ = self.api.send("answerCallbackQuery", say("This has expired. /threads shows what is waiting.", true)).await;
            return;
        };
        let _ = self.api.send("answerCallbackQuery", say("Sent.", false)).await;
        if let Some(message) = press["message"]["message_id"].as_i64() {
            let _ = self.api.send("editMessageReplyMarkup", json!({ "chat_id": chat, "message_id": message, "reply_markup": { "inline_keyboard": [] } })).await;
        }
        self.answer(chat, button.thread, button.request, button.answer).await;
    }

    async fn button(&self, label: &str, thread: &str, request: &str, answer: Answer) -> Value {
        let mut s = self.session.lock().await;
        s.next_button += 1;
        let key = format!("b{}", s.next_button);
        s.buttons.insert(key.clone(), Button { thread: thread.into(), request: request.into(), answer });
        s.button_order.push(key.clone());
        if s.button_order.len() > BUTTONS_KEPT {
            let old = s.button_order.remove(0);
            s.buttons.remove(&old);
        }
        json!({ "text": label, "callback_data": key })
    }

    /// Show what a thread asks, once per request.
    async fn announce_requests(&self, chat: i64, thread: &str, title: &str, items: &[PhoneItem]) {
        for item in items.iter().filter(|i| i.status == "pending" && i.request_id.is_some()) {
            let request = item.request_id.clone().unwrap_or_default();
            if !self.session.lock().await.announced.insert(request.clone()) {
                continue;
            }
            match item.kind.as_str() {
                "approval" => {
                    let detail = readable_input(&item.text, &item.title);
                    let mut row = vec![
                        self.button("Deny", thread, &request, Answer::Deny { message: None }).await,
                        self.button("Allow", thread, &request, Answer::Allow).await,
                    ];
                    if item.allow_session {
                        row.insert(1, self.button("For this session", thread, &request, Answer::AllowSession).await);
                    }
                    self.say_with(chat, &format!("\"{title}\" asks:\n{}{detail}", item.title), Some(json!([row]))).await;
                }
                "plan" => {
                    let row = vec![
                        self.button("Keep planning", thread, &request, Answer::Deny { message: None }).await,
                        self.button("Go ahead", thread, &request, Answer::Allow).await,
                    ];
                    self.say_with(chat, &format!("\"{title}\" proposes a plan:\n\n{}", item.text), Some(json!([row]))).await;
                }
                "question" if item.on_pc => {
                    self.say(chat, &format!("\"{title}\" asks for a form from a connected tool. This one has to be answered on the PC.")).await;
                }
                "question" => {
                    let questions = item.questions.as_array().cloned().unwrap_or_default();
                    if questions.len() != 1 {
                        self.say(chat, &format!("\"{title}\" asks several questions. Answer them on the phone page or the PC.")).await;
                        continue;
                    }
                    let q = &questions[0];
                    let question = q["question"].as_str().unwrap_or_default();
                    let mut rows = Vec::new();
                    if !q["multiSelect"].as_bool().unwrap_or(false) {
                        for option in q["options"].as_array().cloned().unwrap_or_default() {
                            let label = option["label"].as_str().or(option.as_str()).unwrap_or_default().to_string();
                            let mut answers = serde_json::Map::new();
                            answers.insert(question.to_string(), Value::String(label.clone()));
                            rows.push(vec![self.button(&label, thread, &request, Answer::Answers { answers, files: Default::default() }).await]);
                        }
                    }
                    // A secret must not go into a chat that keeps its history.
                    if q["isSecret"].as_bool().unwrap_or(false) {
                        self.say(chat, &format!("\"{title}\" asks for a secret. Answer it on the PC, not here.")).await;
                        continue;
                    }
                    let _ = self.db.telegram_set_current(Some(thread));
                    let hint = if rows.is_empty() { "\n\nReply with your answer." } else { "\n\nPress one, or reply with your own answer." };
                    let buttons = (!rows.is_empty()).then(|| json!(rows));
                    self.say_with(chat, &format!("\"{title}\" asks:\n{question}{hint}"), buttons).await;
                }
                _ => {}
            }
        }
    }

    /// A thread started waiting for the user. Every thread counts here: this is
    /// the alert a closed phone page cannot give.
    pub async fn on_waiting(&self, thread: &str) {
        let Some(chat) = self.db.telegram_state().chat_id else { return };
        let id = thread.to_string();
        let Ok(view) = self.blocking(move |b| b.thread(&id)).await else { return };
        self.announce_requests(chat, thread, &view.title, &view.items).await;
    }

    /// A turn ended: the chat hears how, for the threads it follows.
    pub async fn on_turn_end(&self, thread: &str, status: &str) {
        let state = self.db.telegram_state();
        let Some(chat) = state.chat_id else { return };
        if !state.followed.iter().any(|t| t == thread) {
            return;
        }
        let id = thread.to_string();
        let Ok(view) = self.blocking(move |b| b.thread(&id)).await else { return };
        let answer = view.items.iter().rev().find(|i| i.kind == "assistant").map(|i| i.text.clone());
        let text = match status {
            "completed" => answer.unwrap_or_else(|| "Finished.".into()),
            "failed" => view.items.iter().rev().find(|i| i.kind == "error").map(|i| i.text.clone()).unwrap_or_else(|| "It stopped with an error.".into()),
            "limited" => "It hit a usage limit.".to_string(),
            _ => return,
        };
        self.say(chat, &text).await;
    }

    /// Reads Telegram until told to stop. A wrong token ends it; everything
    /// else waits and tries again. The offset is saved before an update is
    /// acted on, so nothing runs twice after a restart.
    pub async fn poll(self: Arc<Self>, stop: Arc<AtomicBool>, error: Arc<Mutex<Option<String>>>) {
        let mut failures = 0usize;
        // Older than this is from before the bot was on: never acted on.
        let started = chrono::Utc::now().timestamp() - 120;
        while !stop.load(Ordering::SeqCst) {
            let offset = self.db.telegram_state().update_offset;
            let body = json!({ "offset": offset, "timeout": POLL_SECONDS, "allowed_updates": ["message", "callback_query"] });
            match self.api.call("getUpdates", body, Duration::from_secs(POLL_SECONDS + 10)).await {
                Ok(updates) => {
                    failures = 0;
                    set(&error, None);
                    for update in updates.as_array().cloned().unwrap_or_default() {
                        let Some(id) = update["update_id"].as_i64() else { continue };
                        if self.db.telegram_set_offset(id + 1).is_err() {
                            // Without the offset it could run twice: not acted on.
                            continue;
                        }
                        let sent = update["message"]["date"].as_i64().unwrap_or(started);
                        if sent < started || stop.load(Ordering::SeqCst) {
                            continue;
                        }
                        self.on_update(&update).await;
                    }
                }
                Err(ApiError::Unauthorized) => {
                    set(&error, Some(ApiError::Unauthorized.to_string()));
                    return;
                }
                Err(e) => {
                    set(&error, Some(e.to_string()));
                    let wait = if let ApiError::RateLimited(s) = e { s.min(60) } else { BACKOFF[failures.min(BACKOFF.len() - 1)] };
                    failures += 1;
                    tokio::time::sleep(Duration::from_secs(wait)).await;
                }
            }
        }
    }
}

fn set(error: &Mutex<Option<String>>, value: Option<String>) {
    if let Ok(mut e) = error.lock() {
        *e = value;
    }
}

// ── In the app ──────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum BotEvent {
    Waiting(String),
    TurnEnd(String, String),
}

struct Running {
    stop: Arc<AtomicBool>,
    poller: tauri::async_runtime::JoinHandle<()>,
    events: tauri::async_runtime::JoinHandle<()>,
    sender: mpsc::UnboundedSender<BotEvent>,
    name: String,
}

/// Managed by the app.
pub struct TelegramAppState {
    pairing: Arc<Mutex<Pairing>>,
    stepups: Arc<Mutex<StepUps>>,
    running: Mutex<Option<Running>>,
    error: Arc<Mutex<Option<String>>>,
}

impl Default for TelegramAppState {
    fn default() -> Self {
        Self {
            pairing: Arc::new(Mutex::new(Pairing::new())),
            stepups: Arc::new(Mutex::new(StepUps::default())),
            running: Mutex::new(None),
            error: Arc::new(Mutex::new(None)),
        }
    }
}

/// A build for testing can talk to a stand-in; a release always talks to Telegram.
fn api_base() -> String {
    if cfg!(debug_assertions) {
        if let Ok(base) = std::env::var("QS_TELEGRAM_API") {
            return base;
        }
    }
    API.to_string()
}

fn read_token() -> Result<Option<zeroize::Zeroizing<String>>, String> {
    qs_core::vault::with_vault(|v| v.get_string(TOKEN_SECRET))
}

fn token_format_ok(token: &str) -> bool {
    match token.split_once(':') {
        Some((id, secret)) => {
            !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) && secret.len() >= 20 && secret.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        }
        None => false,
    }
}

impl TelegramAppState {
    /// From the thread events: only what the chat may hear of.
    pub fn on_thread_event(&self, thread_id: &str, event: &ThreadEvent) {
        let forward = match event {
            ThreadEvent::State { state, .. } if state == "waiting" => BotEvent::Waiting(thread_id.to_string()),
            ThreadEvent::Turn { status, .. } if status != "started" => BotEvent::TurnEnd(thread_id.to_string(), status.clone()),
            _ => return,
        };
        if let Ok(running) = self.running.lock() {
            if let Some(r) = running.as_ref() {
                let _ = r.sender.send(forward);
            }
        }
    }

    fn stop(&self) {
        if let Ok(mut running) = self.running.lock() {
            if let Some(r) = running.take() {
                r.stop.store(true, Ordering::SeqCst);
                r.poller.abort();
                r.events.abort();
            }
        }
    }

    /// `name`: the bot's @name, from Telegram.
    fn start(&self, app: &AppHandle, db: Arc<Database>, name: String) -> Result<(), String> {
        self.stop();
        let token = read_token()?.ok_or("Paste the bot's token first.")?;
        if phone::read_pin_hash()?.is_none() {
            return Err("Set a PIN first.".into());
        }
        let bot = Arc::new(Bot::new(
            Api::new(&api_base(), token.as_str()),
            Arc::new(AppBackend::new(app.clone())),
            db,
            self.pairing.clone(),
            self.stepups.clone(),
        ));
        let stop = Arc::new(AtomicBool::new(false));
        let (sender, mut receiver) = mpsc::unbounded_channel::<BotEvent>();
        let poller = tauri::async_runtime::spawn(bot.clone().poll(stop.clone(), self.error.clone()));
        let events = tauri::async_runtime::spawn(async move {
            while let Some(event) = receiver.recv().await {
                match event {
                    BotEvent::Waiting(thread) => bot.on_waiting(&thread).await,
                    BotEvent::TurnEnd(thread, status) => bot.on_turn_end(&thread, &status).await,
                }
            }
        });
        *self.running.lock().map_err(|_| "telegram lock")? = Some(Running { stop, poller, events, sender, name });
        Ok(())
    }
}

async fn bot_name() -> Result<String, String> {
    let token = read_token()?.ok_or("Paste the bot's token first.")?;
    let me = Api::new(&api_base(), token.as_str()).call("getMe", json!({}), Duration::from_secs(15)).await.map_err(|e| e.to_string())?;
    Ok(me["username"].as_str().unwrap_or_default().to_string())
}

/// At the app's start: on again if the user left it on.
pub fn start_saved(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let (Some(state), Some(pilot)) = (app.try_state::<TelegramAppState>(), app.try_state::<PilotState>()) else { return };
        if !pilot.settings.lock().map(|s| s.telegram.enabled).unwrap_or(false) {
            return;
        }
        // Telegram may not be reachable yet: the bot still starts and keeps trying.
        let name = bot_name().await.unwrap_or_default();
        if let Err(e) = state.start(&app, pilot.db.clone(), name) {
            set(&state.error, Some(e));
        }
    });
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingView {
    code: String,
    seconds_left: u64,
    link: Option<String>,
    qr: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelegramStatus {
    enabled: bool,
    running: bool,
    error: Option<String>,
    has_token: bool,
    has_pin: bool,
    bot_name: Option<String>,
    linked_to: Option<String>,
    pairing: Option<PairingView>,
}

fn status_of(pilot: &PilotState, state: &TelegramAppState) -> Result<TelegramStatus, String> {
    let name = state.running.lock().ok().and_then(|r| r.as_ref().map(|r| r.name.clone())).filter(|n| !n.is_empty());
    let saved = pilot.db.telegram_state();
    let pairing = {
        let open = state.pairing.lock().map_err(|_| "pairing lock")?.open(Instant::now());
        open.map(|(code, left)| {
            let link = name.as_ref().map(|n| format!("https://t.me/{n}?start={code}"));
            PairingView { qr: link.as_deref().and_then(pairing::qr_svg), link, code, seconds_left: left.as_secs() }
        })
    };
    Ok(TelegramStatus {
        enabled: pilot.settings.lock().map(|s| s.telegram.enabled).unwrap_or(false),
        running: state.running.lock().map(|r| r.is_some()).unwrap_or(false),
        error: state.error.lock().ok().and_then(|e| e.clone()),
        has_token: read_token().ok().flatten().is_some(),
        has_pin: phone::read_pin_hash().ok().flatten().is_some(),
        bot_name: name,
        linked_to: saved.chat_id.map(|_| saved.chat_name),
        pairing,
    })
}

#[tauri::command(async)]
pub fn pilot_telegram_status(pilot: State<'_, PilotState>, state: State<'_, TelegramAppState>) -> Result<TelegramStatus, String> {
    status_of(&pilot, &state)
}

/// The token goes to the vault and is never shown again.
#[tauri::command(async)]
pub fn pilot_telegram_set_token(pilot: State<'_, PilotState>, state: State<'_, TelegramAppState>, token: String) -> Result<TelegramStatus, String> {
    let token = token.trim();
    if !token_format_ok(token) {
        return Err("That does not look like a bot token. BotFather's looks like 123456789:AAE...".into());
    }
    qs_core::vault::with_vault(|v| v.set(TOKEN_SECRET, token.as_bytes()).map(|_| ()))?;
    status_of(&pilot, &state)
}

/// Turn the bot on or off. Turning it on asks Telegram who the bot is, so a
/// wrong token shows here.
#[tauri::command]
pub async fn pilot_telegram_set(app: AppHandle, enabled: bool) -> Result<TelegramStatus, String> {
    let state = app.state::<TelegramAppState>();
    let pilot = app.state::<PilotState>();
    set(&state.error, None);
    if enabled {
        let name = bot_name().await?;
        state.start(&app, pilot.db.clone(), name)?;
    } else {
        state.stop();
        if let Ok(mut p) = state.pairing.lock() {
            p.cancel();
        }
    }
    {
        let mut s = pilot.settings.lock().map_err(|_| "settings lock")?;
        s.telegram.enabled = enabled;
        crate::settings::write(&pilot.data_dir, &s)?;
    }
    status_of(&pilot, &state)
}

#[tauri::command(async)]
pub fn pilot_telegram_pair(pilot: State<'_, PilotState>, state: State<'_, TelegramAppState>) -> Result<TelegramStatus, String> {
    if !state.running.lock().map(|r| r.is_some()).unwrap_or(false) {
        return Err("Turn the bot on first.".into());
    }
    state.pairing.lock().map_err(|_| "pairing lock")?.start(Instant::now()).map_err(|e| e.to_string())?;
    status_of(&pilot, &state)
}

#[tauri::command(async)]
pub fn pilot_telegram_pair_cancel(pilot: State<'_, PilotState>, state: State<'_, TelegramAppState>) -> Result<TelegramStatus, String> {
    state.pairing.lock().map_err(|_| "pairing lock")?.cancel();
    status_of(&pilot, &state)
}

/// The chat can no longer talk to the agents, at once.
#[tauri::command(async)]
pub fn pilot_telegram_unlink(pilot: State<'_, PilotState>, state: State<'_, TelegramAppState>) -> Result<TelegramStatus, String> {
    pilot.db.telegram_link(None, "").map_err(|e| e.to_string())?;
    if let Ok(mut s) = state.stepups.lock() {
        s.forget(DEVICE);
    }
    status_of(&pilot, &state)
}

/// Turns the bot off and deletes the token from the vault.
#[tauri::command(async)]
pub fn pilot_telegram_forget_token(pilot: State<'_, PilotState>, state: State<'_, TelegramAppState>) -> Result<TelegramStatus, String> {
    state.stop();
    qs_core::vault::with_vault(|v| v.delete(TOKEN_SECRET).map(|_| ()))?;
    {
        let mut s = pilot.settings.lock().map_err(|_| "settings lock")?;
        s.telegram.enabled = false;
        crate::settings::write(&pilot.data_dir, &s)?;
    }
    status_of(&pilot, &state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phone::{Project, ThreadRow, ThreadView};
    use axum::{extract::{Path, State as AxumState}, routing::post, Json, Router};

    const TOKEN: &str = "123456:SECRETSECRETSECRETSECRET";

    /// A stand-in for Telegram: records what the bot sends.
    #[derive(Default)]
    struct StandIn {
        calls: Mutex<Vec<(String, Value)>>,
        updates: Mutex<Vec<Value>>,
        offsets: Mutex<Vec<i64>>,
        rate_limit_once: AtomicBool,
    }

    async fn telegram(AxumState(s): AxumState<Arc<StandIn>>, Path((bot, method)): Path<(String, String)>, Json(body): Json<Value>) -> axum::response::Response {
        use axum::response::IntoResponse;
        assert_eq!(bot, format!("bot{TOKEN}"));
        if method == "getUpdates" {
            let offset = body["offset"].as_i64().unwrap_or(0);
            s.offsets.lock().unwrap().push(offset);
            let updates: Vec<Value> = s.updates.lock().unwrap().iter().filter(|u| u["update_id"].as_i64().unwrap_or(0) >= offset).cloned().collect();
            if updates.is_empty() {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            return Json(json!({ "ok": true, "result": updates })).into_response();
        }
        if method == "sendMessage" && s.rate_limit_once.swap(false, Ordering::SeqCst) {
            return (axum::http::StatusCode::TOO_MANY_REQUESTS, Json(json!({ "ok": false, "parameters": { "retry_after": 0 } }))).into_response();
        }
        s.calls.lock().unwrap().push((method, body));
        Json(json!({ "ok": true, "result": {} })).into_response()
    }

    #[derive(Default)]
    struct Fake {
        sent: Mutex<Vec<(String, String)>>,
        created: Mutex<Vec<(String, String)>>,
        answers: Mutex<Vec<(String, String, Answer)>>,
        stops: Mutex<Vec<String>>,
        items: Mutex<Vec<PhoneItem>>,
    }

    impl Backend for Fake {
        fn threads(&self) -> Result<Vec<ThreadRow>, String> {
            Ok(vec![
                ThreadRow { id: "t1".into(), title: "First".into(), project: "General".into(), provider: "claude".into(), running: true, needs: None, updated_at: 2 },
                ThreadRow { id: "t2".into(), title: "Second".into(), project: "Repo".into(), provider: "claude".into(), running: false, needs: Some("Run a command".into()), updated_at: 1 },
            ])
        }
        fn thread(&self, id: &str) -> Result<ThreadView, String> {
            Ok(ThreadView { id: id.into(), title: format!("Title of {id}"), project: "General".into(), provider: "claude".into(), running: false, items: self.items.lock().unwrap().clone() })
        }
        fn projects(&self) -> Vec<Project> {
            vec![Project { id: "general".into(), name: "General".into() }, Project { id: "p1".into(), name: "hello world qa".into() }]
        }
        fn send(&self, id: &str, text: String) -> Result<(), String> {
            self.sent.lock().unwrap().push((id.into(), text));
            Ok(())
        }
        fn create(&self, project: &str, text: String) -> Result<String, String> {
            self.created.lock().unwrap().push((project.into(), text));
            Ok("new1".into())
        }
        fn answer(&self, id: &str, request_id: &str, answer: Answer) -> Result<(), String> {
            self.answers.lock().unwrap().push((id.into(), request_id.into(), answer));
            Ok(())
        }
        fn stop(&self, id: &str) -> Result<(), String> {
            self.stops.lock().unwrap().push(id.into());
            Ok(())
        }
        fn pin_hash(&self) -> Result<Option<String>, String> {
            Ok(Some(stepup::hash_pin("123456").unwrap()))
        }
    }

    struct Rig {
        bot: Arc<Bot>,
        stand_in: Arc<StandIn>,
        fake: Arc<Fake>,
        db: Arc<Database>,
        base: String,
        pairing: Arc<Mutex<Pairing>>,
    }

    const CHAT: i64 = 555;

    async fn rig() -> Rig {
        let stand_in = Arc::new(StandIn::default());
        let router = Router::new().route("/{bot}/{method}", post(telegram)).with_state(stand_in.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let fake = Arc::new(Fake::default());
        let db = Arc::new(Database::open_in_memory().unwrap());
        let pairing = Arc::new(Mutex::new(Pairing::new()));
        let bot = Arc::new(Bot::new(Api::new(&base, TOKEN), fake.clone(), db.clone(), pairing.clone(), Arc::new(Mutex::new(StepUps::default()))));
        Rig { bot, stand_in, fake, db, base, pairing }
    }

    fn msg(chat: i64, text: &str, id: i64) -> Value {
        json!({ "message": { "message_id": id, "date": 2_000_000_000, "text": text, "chat": { "id": chat, "type": "private", "first_name": "Sam" } } })
    }

    impl Rig {
        fn calls(&self, method: &str) -> Vec<Value> {
            self.stand_in.calls.lock().unwrap().iter().filter(|(m, _)| m == method).map(|(_, b)| b.clone()).collect()
        }

        fn said(&self) -> Vec<String> {
            self.calls("sendMessage").iter().map(|b| b["text"].as_str().unwrap_or_default().to_string()).collect()
        }

        fn clear(&self) {
            self.stand_in.calls.lock().unwrap().clear();
        }

        async fn linked(&self) {
            let code = self.pairing.lock().unwrap().start(Instant::now()).unwrap();
            self.bot.on_update(&msg(CHAT, &format!("/start {code}"), 1)).await;
            assert_eq!(self.db.telegram_state().chat_id, Some(CHAT));
            self.clear();
        }

        async fn unlocked(&self) {
            self.linked().await;
            self.bot.on_update(&msg(CHAT, "/unlock 123456", 2)).await;
            assert!(self.bot.unlocked());
            self.clear();
        }

        fn items(&self, items: Vec<PhoneItem>) {
            *self.fake.items.lock().unwrap() = items;
        }

        async fn press(&self, chat: i64, data: &str) {
            self.bot.on_update(&json!({ "callback_query": { "id": "cb1", "data": data, "message": { "message_id": 9, "chat": { "id": chat } } } })).await;
        }
    }

    fn approval(request: &str) -> PhoneItem {
        PhoneItem { id: "a".into(), kind: "approval".into(), title: "rm -rf build".into(), status: "pending".into(), request_id: Some(request.into()), allow_session: true, questions: Value::Null, ..Default::default() }
    }

    fn buttons(call: &Value) -> Vec<(String, String)> {
        call["reply_markup"]["inline_keyboard"].as_array().unwrap().iter().flat_map(|row| row.as_array().unwrap().clone()).map(|b| (b["text"].as_str().unwrap().to_string(), b["callback_data"].as_str().unwrap().to_string())).collect()
    }

    #[tokio::test]
    async fn only_the_chat_with_the_code_gets_linked_and_strangers_hear_nothing() {
        let r = rig().await;
        // No code open: not even an answer.
        r.bot.on_update(&msg(CHAT, "/start 12345678", 1)).await;
        r.bot.on_update(&msg(CHAT, "hello", 2)).await;
        assert!(r.stand_in.calls.lock().unwrap().is_empty());
        assert_eq!(r.db.telegram_state().chat_id, None);

        let code = r.pairing.lock().unwrap().start(Instant::now()).unwrap();
        let wrong = if code == "00000000" { "11111111" } else { "00000000" };
        r.bot.on_update(&msg(CHAT, &format!("/start {wrong}"), 3)).await;
        assert!(r.said()[0].contains("Wrong code (4 attempts left)"));
        assert_eq!(r.db.telegram_state().chat_id, None);

        r.bot.on_update(&msg(CHAT, &format!("/start {code}"), 4)).await;
        let state = r.db.telegram_state();
        assert_eq!((state.chat_id, state.chat_name.as_str()), (Some(CHAT), "Sam"));
        assert!(r.said().last().unwrap().starts_with("Linked."));

        // A stranger, even with the old code, gets silence and does nothing.
        r.clear();
        for text in ["hello", &format!("/start {code}"), "/threads", "/unlock 123456", "/stop"] {
            r.bot.on_update(&msg(999, text, 5)).await;
        }
        assert!(r.stand_in.calls.lock().unwrap().is_empty());
        assert_eq!(r.db.telegram_state().chat_id, Some(CHAT));
        assert!(!r.bot.unlocked());

        // A group is not a private chat, even with the linked id.
        let mut group = msg(CHAT, "hello", 6);
        group["message"]["chat"]["type"] = json!("group");
        r.bot.on_update(&group).await;
        assert!(r.stand_in.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn acting_needs_the_pin_and_the_pin_leaves_the_chat() {
        let r = rig().await;
        r.linked().await;
        r.bot.on_update(&msg(CHAT, "/stop", 2)).await;
        r.bot.on_update(&msg(CHAT, "/new a task", 3)).await;
        assert!(r.fake.created.lock().unwrap().is_empty());
        assert!(r.fake.stops.lock().unwrap().is_empty());

        // A message while locked waits for the PIN.
        r.bot.on_update(&msg(CHAT, "fix the test", 4)).await;
        assert!(r.fake.created.lock().unwrap().is_empty());
        assert!(r.said().last().unwrap().contains("Locked"));

        // Wrong, then right: both deleted, and the held message goes.
        r.clear();
        r.bot.on_update(&msg(CHAT, "/unlock 000000", 5)).await;
        assert_eq!(r.calls("deleteMessage")[0]["message_id"], 5);
        assert!(r.said()[0].contains("4 attempts left"));
        assert!(!r.bot.unlocked());
        r.bot.on_update(&msg(CHAT, "/unlock 123456", 6)).await;
        assert_eq!(r.calls("deleteMessage")[1]["message_id"], 6);
        assert!(r.bot.unlocked());
        assert_eq!(r.fake.created.lock().unwrap()[0], ("general".to_string(), "fix the test".to_string()));

        let audit = r.db.phone_audit_list(20).unwrap();
        assert!(audit.iter().any(|a| a.device == "Telegram" && a.action == "pin" && a.ok));
        assert!(audit.iter().all(|a| !a.note.contains("123456")));
    }

    #[tokio::test]
    async fn wrong_pins_lock_the_chat() {
        let r = rig().await;
        r.linked().await;
        for id in 0..5 {
            r.bot.on_update(&msg(CHAT, "/unlock 000000", id)).await;
        }
        assert!(r.said().last().unwrap().contains("Too many wrong PINs"));
        r.bot.on_update(&msg(CHAT, "/unlock 123456", 9)).await;
        assert!(!r.bot.unlocked(), "the right PIN waits too");
        assert_eq!(r.calls("deleteMessage").len(), 6, "every try was deleted");
    }

    #[tokio::test]
    async fn text_goes_to_the_current_thread_and_new_ones_can_start_in_a_project() {
        let r = rig().await;
        r.unlocked().await;
        r.bot.on_update(&msg(CHAT, "first message", 3)).await;
        assert_eq!(r.fake.created.lock().unwrap()[0].0, "general");
        assert_eq!(r.db.telegram_state().current_thread.as_deref(), Some("new1"));
        r.bot.on_update(&msg(CHAT, "and another", 4)).await;
        assert_eq!(r.fake.sent.lock().unwrap()[0], ("new1".to_string(), "and another".to_string()));
        assert_eq!(r.fake.created.lock().unwrap().len(), 1);

        r.bot.on_update(&msg(CHAT, "/new @hello-world-qa run the tests", 5)).await;
        assert_eq!(r.fake.created.lock().unwrap()[1], ("p1".to_string(), "run the tests".to_string()));
        r.clear();
        r.bot.on_update(&msg(CHAT, "/new @nope do it", 6)).await;
        assert!(r.said()[0].contains("do not know the project"));
        assert_eq!(r.fake.created.lock().unwrap().len(), 2);

        r.clear();
        r.bot.on_update(&msg(CHAT, "/threads", 7)).await;
        let list = r.said().join("\n");
        assert!(list.contains("1. First") && list.contains("working"), "{list}");
        assert!(list.contains("2. Second") && list.contains("needs you: Run a command"), "{list}");
        r.bot.on_update(&msg(CHAT, "/use 2", 8)).await;
        assert_eq!(r.db.telegram_state().current_thread.as_deref(), Some("t2"));
        r.bot.on_update(&msg(CHAT, "/stop", 9)).await;
        assert_eq!(r.fake.stops.lock().unwrap()[0], "t2");
        r.clear();
        r.bot.on_update(&msg(CHAT, "/use 9", 10)).await;
        assert!(r.said()[0].contains("/threads first"));
    }

    #[tokio::test]
    async fn an_approval_comes_with_buttons_and_a_press_answers_it_once() {
        let r = rig().await;
        r.unlocked().await;
        r.items(vec![approval("r1")]);
        r.bot.on_waiting("t1").await;
        let sent = r.calls("sendMessage");
        assert_eq!(sent.len(), 1);
        assert!(sent[0]["text"].as_str().unwrap().contains("rm -rf build"));
        let labels = buttons(&sent[0]);
        assert_eq!(labels.iter().map(|b| b.0.as_str()).collect::<Vec<_>>(), ["Deny", "For this session", "Allow"]);

        // Waiting again does not repeat it.
        r.bot.on_waiting("t1").await;
        assert_eq!(r.calls("sendMessage").len(), 1);

        let allow = &labels.iter().find(|b| b.0 == "Allow").unwrap().1;
        // A stranger's press does nothing.
        r.press(999, allow).await;
        assert!(r.fake.answers.lock().unwrap().is_empty());
        r.press(CHAT, allow).await;
        assert_eq!(*r.fake.answers.lock().unwrap(), [("t1".to_string(), "r1".to_string(), Answer::Allow)]);
        assert_eq!(r.calls("editMessageReplyMarkup")[0]["message_id"], 9);
        // A second press finds nothing.
        r.press(CHAT, allow).await;
        assert_eq!(r.fake.answers.lock().unwrap().len(), 1);
        assert!(r.calls("answerCallbackQuery").last().unwrap()["text"].as_str().unwrap().contains("expired"));
    }

    #[tokio::test]
    async fn a_locked_press_answers_nothing() {
        let r = rig().await;
        r.linked().await;
        r.items(vec![approval("r1")]);
        r.bot.on_waiting("t1").await;
        let allow = buttons(&r.calls("sendMessage")[0]).into_iter().find(|b| b.0 == "Allow").unwrap().1;
        r.press(CHAT, &allow).await;
        assert!(r.fake.answers.lock().unwrap().is_empty());
        assert!(r.calls("answerCallbackQuery")[0]["text"].as_str().unwrap().contains("Locked"));
        // After the PIN, the same button still works.
        r.bot.on_update(&msg(CHAT, "/unlock 123456", 5)).await;
        r.press(CHAT, &allow).await;
        assert_eq!(r.fake.answers.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn questions_are_answered_by_button_or_by_text_and_a_secret_never_here() {
        let r = rig().await;
        r.unlocked().await;
        r.db.telegram_set_current(Some("t1")).unwrap();
        let question = |request: &str, q: Value| PhoneItem { id: "q".into(), kind: "question".into(), status: "pending".into(), request_id: Some(request.into()), questions: json!([q]), ..Default::default() };
        r.items(vec![question("r1", json!({ "question": "Which color?", "options": [{ "label": "Red" }, { "label": "Blue" }] }))]);
        r.bot.on_waiting("t1").await;
        let labels = buttons(&r.calls("sendMessage")[0]);
        let blue = labels.iter().find(|b| b.0 == "Blue").unwrap().1.clone();
        r.press(CHAT, &blue).await;
        let answers = r.fake.answers.lock().unwrap().clone();
        match &answers[0].2 {
            Answer::Answers { answers, .. } => assert_eq!(answers["Which color?"], "Blue"),
            other => panic!("{other:?}"),
        }

        // Typed instead: the text answers, and is not sent as a message.
        r.bot.on_update(&msg(CHAT, "green", 7)).await;
        let answers = r.fake.answers.lock().unwrap().clone();
        match &answers[1].2 {
            Answer::Answers { answers, .. } => assert_eq!(answers["Which color?"], "green"),
            other => panic!("{other:?}"),
        }
        assert!(r.fake.sent.lock().unwrap().is_empty());

        r.clear();
        r.items(vec![question("r2", json!({ "question": "API key?", "isSecret": true }))]);
        r.bot.on_waiting("t1").await;
        let said = r.said();
        assert!(said[0].contains("Answer it on the PC"), "{said:?}");
        assert!(r.calls("sendMessage")[0].get("reply_markup").is_none());

        r.clear();
        r.items(vec![PhoneItem { kind: "question".into(), status: "pending".into(), request_id: Some("r3".into()), on_pc: true, ..Default::default() }]);
        r.bot.on_waiting("t1").await;
        assert!(r.said()[0].contains("on the PC"));
    }

    #[tokio::test]
    async fn the_answer_comes_back_for_followed_threads_only_and_in_pieces() {
        let r = rig().await;
        r.unlocked().await;
        r.bot.on_update(&msg(CHAT, "start it", 3)).await;
        r.clear();
        let long = format!("{}\n{}", "a".repeat(4000), "b".repeat(4000));
        r.items(vec![PhoneItem { kind: "assistant".into(), text: long, ..Default::default() }]);
        r.bot.on_turn_end("new1", "completed").await;
        let said = r.said();
        assert_eq!(said.len(), 2);
        assert!(said[0].chars().all(|c| c == 'a') && said[1].chars().all(|c| c == 'b'), "cut at the line end");

        r.clear();
        r.bot.on_turn_end("someone-else", "completed").await;
        assert!(r.stand_in.calls.lock().unwrap().is_empty(), "a thread the chat does not follow");
        r.bot.on_turn_end("new1", "interrupted").await;
        assert!(r.stand_in.calls.lock().unwrap().is_empty(), "the user stopped it themselves");
        r.items(vec![PhoneItem { kind: "error".into(), text: "It broke".into(), ..Default::default() }]);
        r.bot.on_turn_end("new1", "failed").await;
        assert_eq!(r.said(), ["It broke"]);
    }

    #[test]
    fn a_command_reads_as_itself() {
        let input = r#"{"command":"mkdir x","description":"Create x"}"#;
        assert_eq!(readable_input(input, "mkdir x"), "\nCreate x");
        assert_eq!(readable_input(input, "Run a command"), "\nCreate x\n\nmkdir x");
        assert_eq!(readable_input(r#"{"path":"a.txt"}"#, "a.txt"), "\n\n{\"path\":\"a.txt\"}");
        assert_eq!(readable_input("plain", "plain"), "");
        assert_eq!(readable_input("other", "plain"), "\n\nother");
    }

    #[test]
    fn long_text_is_split_at_lines_within_the_limit() {
        assert_eq!(split_message("short"), ["short"]);
        assert!(split_message("   ").is_empty());
        let wide = "é".repeat(MESSAGE_LIMIT + 10);
        let parts = split_message(&wide);
        assert_eq!(parts.len(), 2);
        assert!(parts.iter().all(|p| p.chars().count() <= MESSAGE_LIMIT));
        assert_eq!(parts.concat(), wide, "cut on characters, nothing lost");
    }

    #[tokio::test]
    async fn polling_never_replays_an_update_after_a_restart() {
        let r = rig().await;
        r.linked().await;
        let mut first = msg(CHAT, "/help", 3);
        first["update_id"] = json!(100);
        r.stand_in.updates.lock().unwrap().push(first);

        let run = |bot: Arc<Bot>| async move {
            let stop = Arc::new(AtomicBool::new(false));
            let error = Arc::new(Mutex::new(None));
            let task = tokio::spawn(bot.poll(stop.clone(), error));
            tokio::time::sleep(Duration::from_millis(300)).await;
            stop.store(true, Ordering::SeqCst);
            task.abort();
        };
        run(r.bot.clone()).await;
        assert_eq!(r.said().len(), 1);
        assert_eq!(r.db.telegram_state().update_offset, 101);

        // A new bot, as after a restart, on the same database.
        let again = Arc::new(Bot::new(Api::new(&r.base, TOKEN), r.fake.clone(), r.db.clone(), r.pairing.clone(), Arc::new(Mutex::new(StepUps::default()))));
        run(again).await;
        assert_eq!(r.said().len(), 1, "not handled twice");
        assert!(r.stand_in.offsets.lock().unwrap().iter().rev().take(2).all(|o| *o == 101));

        // Messages from before the bot was on are skipped.
        let mut old = msg(CHAT, "/help", 4);
        old["update_id"] = json!(101);
        old["message"]["date"] = json!(1_000);
        r.stand_in.updates.lock().unwrap().push(old);
        run(r.bot.clone()).await;
        assert_eq!(r.said().len(), 1);
        assert_eq!(r.db.telegram_state().update_offset, 102);
    }

    #[tokio::test]
    async fn the_token_never_appears_in_an_error() {
        // Nothing listens on this port.
        let api = Api::new("http://127.0.0.1:9", TOKEN);
        let err = api.call("getMe", json!({}), Duration::from_secs(2)).await.unwrap_err().to_string();
        assert!(!err.contains("SECRET") && !err.contains(TOKEN), "{err}");
        assert!(err.contains("cannot be reached"), "{err}");
    }

    #[tokio::test]
    async fn a_rate_limit_waits_and_tries_again() {
        let r = rig().await;
        r.stand_in.rate_limit_once.store(true, Ordering::SeqCst);
        r.bot.api.send("sendMessage", json!({ "chat_id": 1, "text": "x" })).await.unwrap();
        assert_eq!(r.said(), ["x"]);
    }

    #[tokio::test]
    async fn unlinking_ends_the_chat() {
        let r = rig().await;
        r.unlocked().await;
        r.db.telegram_link(None, "").unwrap();
        r.bot.on_update(&msg(CHAT, "do something", 5)).await;
        assert!(r.fake.created.lock().unwrap().is_empty());
        assert!(r.stand_in.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn tokens_are_checked_for_their_shape() {
        assert!(token_format_ok(TOKEN));
        assert!(!token_format_ok("not a token"));
        assert!(!token_format_ok("abc:SECRETSECRETSECRETSECRET"));
        assert!(!token_format_ok("123:short"));
        assert!(!token_format_ok("123:has space in it, and more than twenty"));
    }
}
