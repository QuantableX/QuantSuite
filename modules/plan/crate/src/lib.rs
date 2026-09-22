//! QuantPlan — a local-first calendar.
//!
//! One database, `~/.quantsuite/modules/plan/plan.db`, holding calendars,
//! events, per-occurrence overrides and reminders. No workspaces, no sync: the
//! module owns its storage the way every other suite module does
//! (ARCHITECTURE.md §5).
//!
//! Two shapes of event, deliberately stored differently:
//!
//!   * **Timed** — `starts_at` / `ends_at` as UTC RFC 3339, plus `tz`, the IANA
//!     zone the user created it in.
//!   * **All-day** — `starts_on` / `ends_on` as bare `YYYY-MM-DD`, `ends_on`
//!     **inclusive**. A whole-day event is not a timestamp: stored as UTC
//!     midnight it shows on the previous day for everyone west of Greenwich.
//!
//! `list_occurrences` expands recurrence here rather than in the frontend, so a
//! month view is one call and the override rules exist exactly once. See
//! `recur.rs` for why the expansion works in local dates.

mod recur;

use chrono::{DateTime, Duration, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Emitter, Manager, State, Wry,
};
use uuid::Uuid;

// ─── Types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Calendar {
    pub id: String,
    pub name: String,
    /// A key into `--qp-cal-*`, never a hex value — a hex would not survive a
    /// theme change.
    pub color: String,
    pub is_visible: bool,
    pub is_default: bool,
    pub sort_index: f64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: String,
    pub calendar_id: String,
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub starts_at: Option<String>,
    pub ends_at: Option<String>,
    pub starts_on: Option<String>,
    pub ends_on: Option<String>,
    pub is_all_day: bool,
    pub tz: String,
    pub rrule: Option<String>,
    pub color: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventInput {
    pub calendar_id: String,
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub starts_at: Option<String>,
    pub ends_at: Option<String>,
    pub starts_on: Option<String>,
    pub ends_on: Option<String>,
    pub is_all_day: bool,
    pub tz: Option<String>,
    pub rrule: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventPatch {
    pub calendar_id: Option<String>,
    pub title: Option<String>,
    pub description: Option<Option<String>>,
    pub location: Option<Option<String>>,
    pub starts_at: Option<Option<String>>,
    pub ends_at: Option<Option<String>>,
    pub starts_on: Option<Option<String>>,
    pub ends_on: Option<Option<String>>,
    pub is_all_day: Option<bool>,
    pub tz: Option<String>,
    pub rrule: Option<Option<String>>,
    pub color: Option<Option<String>>,
}

/// One concrete instance of an event, after recurrence and overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Occurrence {
    pub event_id: String,
    /// The instance's *original* start — the key an override is stored under.
    /// A moved instance keeps this and changes `starts_at`.
    pub occurrence_start: String,
    pub starts_at: Option<String>,
    pub ends_at: Option<String>,
    pub starts_on: Option<String>,
    pub ends_on: Option<String>,
    pub is_all_day: bool,
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub calendar_id: String,
    pub color: String,
    pub tz: String,
    pub rrule: Option<String>,
    pub is_recurring: bool,
    pub is_override: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reminder {
    pub id: String,
    pub event_id: String,
    pub minutes_before: i64,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FreeSlot {
    pub starts_at: String,
    pub ends_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub default_view: String,
    /// 0 = Sunday … 1 = Monday. Only these two are offered.
    pub week_starts_on: i64,
    pub slot_minutes: i64,
    pub time_format: String,
    pub timezone: String,
    pub show_weekends: bool,
    pub sidebar_left_open: bool,
    pub sidebar_right_open: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            default_view: "week".into(),
            week_starts_on: 1,
            slot_minutes: 15,
            time_format: "24h".into(),
            timezone: "local".into(),
            show_weekends: true,
            sidebar_left_open: true,
            sidebar_right_open: true,
        }
    }
}

/// Parsed RRULEs by event id, each with the text it was parsed from. A view
/// touches every visible series and the parse is the same work every time;
/// the stored text is the guard — a rule that changed under its id misses.
type RuleCache = HashMap<String, (String, recur::Rule)>;

pub struct AppState {
    data_dir: PathBuf,
    db: Arc<Mutex<Connection>>,
    app_settings: Mutex<AppSettings>,
    rules: Mutex<RuleCache>,
}

// ─── Helpers ──────────────────────────────────────────────────────────────

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn new_id() -> String {
    Uuid::new_v4().to_string()
}

fn data_dir() -> PathBuf {
    qs_core::paths::module_dir("plan")
}

fn db_path(base: &Path) -> PathBuf {
    base.join("plan.db")
}

fn settings_path(base: &Path) -> PathBuf {
    base.join("config.json")
}

fn ensure_dirs(base: &Path) -> Result<(), String> {
    for sub in ["", "backups"] {
        let path = base.join(sub);
        fs::create_dir_all(&path).map_err(|e| format!("Create {}: {e}", path.display()))?;
    }
    Ok(())
}

fn emit(app: &AppHandle, event: &str, payload: Value) {
    let _ = app.emit(event, payload);
}

fn db(state: &State<'_, AppState>) -> Arc<Mutex<Connection>> {
    state.db.clone()
}

/// Drop an event's cached rule once its row changed or went. Changed text
/// would miss on its own; this keeps rules for vanished events from piling up.
fn forget_rule(state: &State<'_, AppState>, id: &str) -> Result<(), String> {
    state.rules.lock().map_err(|e| format!("Lock rule cache: {e}"))?.remove(id);
    Ok(())
}

fn parse_utc(raw: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| format!("Bad timestamp {raw}: {e}"))
}

fn parse_date(raw: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(raw, "%Y-%m-%d").map_err(|e| format!("Bad date {raw}: {e}"))
}

fn zone(name: &str) -> Tz {
    Tz::from_str(name).unwrap_or(chrono_tz::UTC)
}

/// Attach a local wall-clock time in `tz` and return the UTC instant.
///
/// Daylight saving makes this a partial function, and both edges have to be
/// decided rather than unwrapped:
///
///   * **Ambiguous** (the hour that repeats in autumn) — take the first, which
///     is what every calendar does and what the user meant when they typed it.
///   * **Nonexistent** (the hour skipped in spring; 02:30 simply has no
///     instant) — step forward an hour rather than dropping the occurrence.
///     A weekly 02:30 meeting must not silently miss one week a year.
fn local_to_utc(date: NaiveDate, time: NaiveTime, tz: Tz) -> DateTime<Utc> {
    let naive = date.and_time(time);
    match tz.from_local_datetime(&naive) {
        chrono::LocalResult::Single(dt) => dt.with_timezone(&Utc),
        chrono::LocalResult::Ambiguous(earlier, _) => earlier.with_timezone(&Utc),
        chrono::LocalResult::None => {
            let shifted = naive + Duration::hours(1);
            tz.from_local_datetime(&shifted)
                .earliest()
                .map(|dt| dt.with_timezone(&Utc))
                // Two skipped hours in a row does not occur in the IANA
                // database; UTC is the honest fallback if it ever did.
                .unwrap_or_else(|| Utc.from_utc_datetime(&naive))
        }
    }
}

// ─── Schema ───────────────────────────────────────────────────────────────

fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS calendars (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            color TEXT NOT NULL,
            is_visible INTEGER NOT NULL DEFAULT 1,
            is_default INTEGER NOT NULL DEFAULT 0,
            sort_index REAL NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        -- Timed events carry starts_at/ends_at (UTC) and tz; all-day events
        -- carry starts_on/ends_on as bare dates, ends_on INCLUSIVE.
        CREATE TABLE IF NOT EXISTS events (
            id TEXT PRIMARY KEY,
            calendar_id TEXT NOT NULL REFERENCES calendars(id) ON DELETE CASCADE,
            title TEXT NOT NULL,
            description TEXT,
            location TEXT,
            starts_at TEXT,
            ends_at TEXT,
            starts_on TEXT,
            ends_on TEXT,
            is_all_day INTEGER NOT NULL DEFAULT 0,
            tz TEXT NOT NULL DEFAULT 'UTC',
            rrule TEXT,
            -- The rule's UNTIL as YYYY-MM-DD, kept beside the text so a view
            -- can skip series that ended before its window without parsing.
            rrule_until TEXT,
            color TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_events_calendar ON events(calendar_id);
        CREATE INDEX IF NOT EXISTS idx_events_start ON events(starts_at);
        CREATE INDEX IF NOT EXISTS idx_events_start_on ON events(starts_on);

        -- 'This event only': cancel, move or edit ONE instance of a series.
        -- occurrence_start is the instance's original start, never the moved one.
        CREATE TABLE IF NOT EXISTS event_overrides (
            id TEXT PRIMARY KEY,
            event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
            occurrence_start TEXT NOT NULL,
            kind TEXT NOT NULL,
            patch_json TEXT NOT NULL DEFAULT '{}',
            UNIQUE (event_id, occurrence_start)
        );

        CREATE TABLE IF NOT EXISTS reminders (
            id TEXT PRIMARY KEY,
            event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
            minutes_before INTEGER NOT NULL,
            kind TEXT NOT NULL DEFAULT 'notification'
        );

        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value_json TEXT NOT NULL
        );
        ",
    )
    .map_err(|e| format!("Init schema: {e}"))?;
    migrate_rrule_until(conn)
}

/// Databases written before `rrule_until` existed lack the column;
/// `CREATE TABLE IF NOT EXISTS` will not add it, so probe, alter, and fill it
/// from the rules already stored — once; the column's presence is the marker.
fn migrate_rrule_until(conn: &Connection) -> Result<(), String> {
    let has_column = {
        let mut stmt = conn
            .prepare("PRAGMA table_info(events)")
            .map_err(|e| format!("Probe events schema: {e}"))?;
        let names = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| format!("Query events schema: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Read events schema: {e}"))?;
        names.iter().any(|n| n == "rrule_until")
    };
    if has_column {
        return Ok(());
    }
    conn.execute("ALTER TABLE events ADD COLUMN rrule_until TEXT", [])
        .map_err(|e| format!("Add rrule_until: {e}"))?;

    let mut stmt = conn
        .prepare("SELECT id, rrule FROM events WHERE rrule IS NOT NULL")
        .map_err(|e| format!("Prepare series: {e}"))?;
    let series = stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(|e| format!("Query series: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Read series: {e}"))?;
    for (id, raw) in series {
        // A rule the parser rejects keeps NULL: it stays in every window and
        // fails there, exactly as it did before the column existed.
        if let Some(until) = recur::parse(&raw).ok().as_ref().and_then(until_text) {
            conn.execute("UPDATE events SET rrule_until = ?1 WHERE id = ?2", params![until, id])
                .map_err(|e| format!("Fill rrule_until: {e}"))?;
        }
    }
    Ok(())
}

/// The `UNTIL` of a rule as `YYYY-MM-DD` — the `rrule_until` column, comparable
/// as text with `starts_on` and with the dates `select_events` binds.
fn until_text(rule: &recur::Rule) -> Option<String> {
    rule.until.map(|date| date.format("%Y-%m-%d").to_string())
}

const SEED_KEY: &str = "schema.seeded";

/// A calendar with no calendars cannot hold an event, so a fresh install gets
/// one. Marked, so deleting it does not bring it back on the next launch.
fn seed_schema(conn: &Connection) -> Result<(), String> {
    let seeded: Option<String> = conn
        .query_row("SELECT value_json FROM settings WHERE key = ?1", params![SEED_KEY], |r| r.get(0))
        .optional()
        .map_err(|e| format!("Read seed marker: {e}"))?;
    if seeded.is_some() {
        return Ok(());
    }

    let now = now_iso();
    for (i, (name, color)) in [("Personal", "blue"), ("Work", "purple")].iter().enumerate() {
        conn.execute(
            "INSERT INTO calendars (id, name, color, is_visible, is_default, sort_index, created_at, updated_at)
             VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6, ?6)",
            params![new_id(), name, color, i64::from(i == 0), i as f64, now],
        )
        .map_err(|e| format!("Seed calendar: {e}"))?;
    }

    conn.execute(
        "INSERT INTO settings (key, value_json) VALUES (?1, 'true')",
        params![SEED_KEY],
    )
    .map_err(|e| format!("Write seed marker: {e}"))?;
    Ok(())
}

// ─── Row readers ──────────────────────────────────────────────────────────

const CAL_COLUMNS: &str = "id, name, color, is_visible, is_default, sort_index, created_at, updated_at";

fn read_calendar(row: &rusqlite::Row<'_>) -> rusqlite::Result<Calendar> {
    Ok(Calendar {
        id: row.get(0)?,
        name: row.get(1)?,
        color: row.get(2)?,
        is_visible: row.get::<_, i64>(3)? != 0,
        is_default: row.get::<_, i64>(4)? != 0,
        sort_index: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

const EVENT_COLUMNS: &str = "id, calendar_id, title, description, location, starts_at, ends_at, \
                             starts_on, ends_on, is_all_day, tz, rrule, color, created_at, updated_at";

/// The same columns qualified for the join in `list_occurrences`. Both tables
/// have a `color`, so an unqualified list there is an ambiguous-column error at
/// runtime — the join has to name the side it means for every column, not just
/// the ones that happen to collide today.
const EVENT_COLUMNS_E: &str = "e.id, e.calendar_id, e.title, e.description, e.location, \
                               e.starts_at, e.ends_at, e.starts_on, e.ends_on, e.is_all_day, \
                               e.tz, e.rrule, e.color, e.created_at, e.updated_at";

fn read_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<Event> {
    Ok(Event {
        id: row.get(0)?,
        calendar_id: row.get(1)?,
        title: row.get(2)?,
        description: row.get(3)?,
        location: row.get(4)?,
        starts_at: row.get(5)?,
        ends_at: row.get(6)?,
        starts_on: row.get(7)?,
        ends_on: row.get(8)?,
        is_all_day: row.get::<_, i64>(9)? != 0,
        tz: row.get(10)?,
        rrule: row.get(11)?,
        color: row.get(12)?,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
    })
}

// ─── Calendars ────────────────────────────────────────────────────────────

#[tauri::command(async)]
fn list_calendars(state: State<'_, AppState>) -> Result<Vec<Calendar>, String> {
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    let sql = format!("SELECT {CAL_COLUMNS} FROM calendars ORDER BY sort_index, name");
    let mut stmt = conn.prepare(&sql).map_err(|e| format!("Prepare calendars: {e}"))?;
    let rows = stmt
        .query_map([], read_calendar)
        .map_err(|e| format!("Query calendars: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Read calendars: {e}"))?;
    Ok(rows)
}

#[tauri::command(async)]
fn create_calendar(
    name: String,
    color: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Calendar, String> {
    let calendar = {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let now = now_iso();
        let id = new_id();
        let next: f64 = conn
            .query_row("SELECT COALESCE(MAX(sort_index), -1) + 1 FROM calendars", [], |r| r.get(0))
            .map_err(|e| format!("Next calendar index: {e}"))?;
        conn.execute(
            "INSERT INTO calendars (id, name, color, is_visible, is_default, sort_index, created_at, updated_at)
             VALUES (?1, ?2, ?3, 1, 0, ?4, ?5, ?5)",
            params![id, name, color, next, now],
        )
        .map_err(|e| format!("Create calendar: {e}"))?;
        Calendar {
            id,
            name,
            color,
            is_visible: true,
            is_default: false,
            sort_index: next,
            created_at: now.clone(),
            updated_at: now,
        }
    };
    emit(&app, "plan:calendars-changed", json!({}));
    Ok(calendar)
}

#[tauri::command(async)]
fn update_calendar(
    id: String,
    name: Option<String>,
    color: Option<String>,
    is_visible: Option<bool>,
    sort_index: Option<f64>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Calendar, String> {
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let now = now_iso();
        if let Some(name) = name {
            conn.execute("UPDATE calendars SET name = ?1, updated_at = ?2 WHERE id = ?3", params![name, now, id])
                .map_err(|e| format!("Rename calendar: {e}"))?;
        }
        if let Some(color) = color {
            conn.execute("UPDATE calendars SET color = ?1, updated_at = ?2 WHERE id = ?3", params![color, now, id])
                .map_err(|e| format!("Recolor calendar: {e}"))?;
        }
        if let Some(visible) = is_visible {
            conn.execute(
                "UPDATE calendars SET is_visible = ?1, updated_at = ?2 WHERE id = ?3",
                params![visible as i64, now, id],
            )
            .map_err(|e| format!("Toggle calendar: {e}"))?;
        }
        if let Some(sort) = sort_index {
            conn.execute(
                "UPDATE calendars SET sort_index = ?1, updated_at = ?2 WHERE id = ?3",
                params![sort, now, id],
            )
            .map_err(|e| format!("Reorder calendar: {e}"))?;
        }
    }
    emit(&app, "plan:calendars-changed", json!({}));

    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    let sql = format!("SELECT {CAL_COLUMNS} FROM calendars WHERE id = ?1");
    conn.query_row(&sql, params![id], read_calendar).map_err(|e| format!("Get calendar: {e}"))
}

/// Deleting a calendar deletes its events with it (the cascade). The last one
/// cannot go — an event needs somewhere to live.
#[tauri::command(async)]
fn delete_calendar(id: String, state: State<'_, AppState>, app: AppHandle) -> Result<bool, String> {
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM calendars", [], |r| r.get(0))
            .map_err(|e| format!("Count calendars: {e}"))?;
        if count <= 1 {
            return Err("There has to be at least one calendar".into());
        }
        conn.execute("DELETE FROM calendars WHERE id = ?1", params![id])
            .map_err(|e| format!("Delete calendar: {e}"))?;
    }
    // The cascade took the calendar's events with it, their ids unknown here.
    state.rules.lock().map_err(|e| format!("Lock rule cache: {e}"))?.clear();
    emit(&app, "plan:calendars-changed", json!({}));
    Ok(true)
}

// ─── Events ───────────────────────────────────────────────────────────────

#[tauri::command(async)]
fn get_event(id: String, state: State<'_, AppState>) -> Result<Event, String> {
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    let sql = format!("SELECT {EVENT_COLUMNS} FROM events WHERE id = ?1");
    conn.query_row(&sql, params![id], read_event).map_err(|e| format!("Get event: {e}"))
}

/// Reject the shapes that would render as nothing later, at the door rather
/// than three views down.
fn validate(input_all_day: bool, starts_at: &Option<String>, starts_on: &Option<String>) -> Result<(), String> {
    if input_all_day {
        if starts_on.is_none() {
            return Err("An all-day event needs startsOn".into());
        }
    } else if starts_at.is_none() {
        return Err("A timed event needs startsAt".into());
    }
    Ok(())
}

#[tauri::command(async)]
fn create_event(event: EventInput, state: State<'_, AppState>, app: AppHandle) -> Result<Event, String> {
    validate(event.is_all_day, &event.starts_at, &event.starts_on)?;
    let rule = event.rrule.as_deref().map(recur::parse).transpose()?;

    let id = new_id();
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        insert_event(&conn, &id, &event, rule.as_ref().and_then(until_text))?;
    }
    emit(&app, "plan:events-changed", json!({ "id": id.clone() }));
    get_event(id, state)
}

/// The INSERT behind `create_event`, the rule already validated and its UNTIL
/// extracted by the caller.
fn insert_event(conn: &Connection, id: &str, event: &EventInput, rrule_until: Option<String>) -> Result<(), String> {
    conn.execute(
        "INSERT INTO events (id, calendar_id, title, description, location, starts_at, ends_at,
                             starts_on, ends_on, is_all_day, tz, rrule, rrule_until, color, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15)",
        params![
            id,
            event.calendar_id,
            event.title,
            event.description,
            event.location,
            event.starts_at,
            event.ends_at,
            event.starts_on,
            event.ends_on,
            event.is_all_day as i64,
            event.tz.as_deref().unwrap_or("UTC"),
            event.rrule,
            rrule_until,
            event.color,
            now_iso()
        ],
    )
    .map_err(|e| format!("Create event: {e}"))?;
    Ok(())
}

#[tauri::command(async)]
fn update_event(
    id: String,
    patch: EventPatch,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Event, String> {
    let rule = match &patch.rrule {
        Some(Some(raw)) => Some(recur::parse(raw)?),
        _ => None,
    };
    // A patched rule rewrites rrule_until with it — to NULL when the rule goes
    // or has no UNTIL — so the prefilter never keeps an end the rule lost.
    let rrule_until = patch.rrule.as_ref().map(|_| rule.as_ref().and_then(until_text));
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let now = now_iso();

        // Read before the macro below consumes the fields.
        let series_moved =
            patch.starts_at.is_some() || patch.rrule.is_some() || patch.starts_on.is_some();

        macro_rules! set {
            ($field:expr, $sql:literal) => {
                if let Some(value) = $field {
                    conn.execute(
                        concat!("UPDATE events SET ", $sql, " = ?1, updated_at = ?2 WHERE id = ?3"),
                        params![value, now, id],
                    )
                    .map_err(|e| format!(concat!("Update ", $sql, ": {}"), e))?;
                }
            };
        }

        set!(patch.calendar_id, "calendar_id");
        set!(patch.title, "title");
        set!(patch.description, "description");
        set!(patch.location, "location");
        set!(patch.starts_at, "starts_at");
        set!(patch.ends_at, "ends_at");
        set!(patch.starts_on, "starts_on");
        set!(patch.ends_on, "ends_on");
        set!(patch.tz, "tz");
        set!(patch.rrule, "rrule");
        set!(rrule_until, "rrule_until");
        set!(patch.color, "color");
        if let Some(all_day) = patch.is_all_day {
            conn.execute(
                "UPDATE events SET is_all_day = ?1, updated_at = ?2 WHERE id = ?3",
                params![all_day as i64, now, id],
            )
            .map_err(|e| format!("Update is_all_day: {e}"))?;
        }

        // Editing the series itself invalidates per-instance overrides: they
        // are keyed by an original start that may no longer be produced, and a
        // stale one would resurrect a cancelled instance at a random date.
        if series_moved {
            conn.execute("DELETE FROM event_overrides WHERE event_id = ?1", params![id])
                .map_err(|e| format!("Clear overrides: {e}"))?;
        }
    }
    forget_rule(&state, &id)?;
    emit(&app, "plan:events-changed", json!({ "id": id.clone() }));
    get_event(id, state)
}

#[tauri::command(async)]
fn delete_event(id: String, state: State<'_, AppState>, app: AppHandle) -> Result<bool, String> {
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        conn.execute("DELETE FROM events WHERE id = ?1", params![id])
            .map_err(|e| format!("Delete event: {e}"))?;
    }
    forget_rule(&state, &id)?;
    emit(&app, "plan:events-changed", json!({ "id": id }));
    Ok(true)
}

// ─── Occurrences ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Override {
    kind: String,
    patch: Value,
}

/// Overrides by event id, each keyed by the instance's original start.
type OverrideMap = HashMap<String, Vec<(String, Override)>>;

/// SQLite caps the parameters of one statement — 999 on older builds — so an
/// id list is bound in chunks comfortably under that.
const OVERRIDE_CHUNK: usize = 500;

/// Every override of the given events, one query per chunk of ids rather than
/// one per event — the N+1 that made a month view crawl.
fn load_overrides(conn: &Connection, event_ids: &[&str]) -> Result<OverrideMap, String> {
    let mut map = OverrideMap::new();
    for chunk in event_ids.chunks(OVERRIDE_CHUNK) {
        let sql = format!(
            "SELECT event_id, occurrence_start, kind, patch_json FROM event_overrides WHERE event_id IN ({})",
            vec!["?"; chunk.len()].join(", ")
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| format!("Prepare overrides: {e}"))?;
        let rows = stmt
            .query_map(params_from_iter(chunk.iter().copied()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    Override {
                        kind: row.get(2)?,
                        patch: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_else(|_| json!({})),
                    },
                ))
            })
            .map_err(|e| format!("Query overrides: {e}"))?;
        for row in rows {
            let (event_id, start, over) = row.map_err(|e| format!("Read overrides: {e}"))?;
            map.entry(event_id).or_default().push((start, over));
        }
    }
    Ok(map)
}

fn patch_str(patch: &Value, key: &str) -> Option<String> {
    patch.get(key).and_then(Value::as_str).map(str::to_string)
}

/// Expand one event into the instances that fall inside the window. `rule` is
/// the event's RRULE already parsed — by `expand_events`, through the cache —
/// so a view does not parse every series it touches on every navigation.
fn expand_event(
    event: &Event,
    rule: Option<&recur::Rule>,
    overrides: &[(String, Override)],
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    default_color: &str,
) -> Result<Vec<Occurrence>, String> {
    let tz = zone(&event.tz);
    let color = event.color.clone().unwrap_or_else(|| default_color.to_string());

    let mut out = Vec::new();

    // Widen the window by a day on each side before dropping to local dates:
    // an event late on the last local day can still start before `to` in UTC.
    let win_from = from.with_timezone(&tz).date_naive() - Duration::days(1);
    let win_to = to.with_timezone(&tz).date_naive() + Duration::days(1);

    if event.is_all_day {
        let Some(start_on) = &event.starts_on else {
            return Ok(out);
        };
        let start_date = parse_date(start_on)?;
        let end_date = event.ends_on.as_deref().map(parse_date).transpose()?.unwrap_or(start_date);
        // ends_on is INCLUSIVE, so a one-day event has span 0.
        let span = (end_date - start_date).num_days().max(0);

        let dates = match rule {
            Some(rule) => recur::expand_dates(start_date, rule, win_from, win_to),
            None => {
                if end_date >= win_from && start_date <= win_to {
                    vec![start_date]
                } else {
                    vec![]
                }
            }
        };

        for date in dates {
            let key = date.format("%Y-%m-%d").to_string();
            let over = overrides.iter().find(|(k, _)| *k == key).map(|(_, o)| o);
            if over.map(|o| o.kind == "cancelled").unwrap_or(false) {
                continue;
            }
            let starts_on = over.and_then(|o| patch_str(&o.patch, "startsOn")).unwrap_or_else(|| key.clone());
            let ends_on = over.and_then(|o| patch_str(&o.patch, "endsOn")).unwrap_or_else(|| {
                (parse_date(&starts_on).unwrap_or(date) + Duration::days(span))
                    .format("%Y-%m-%d")
                    .to_string()
            });
            out.push(Occurrence {
                event_id: event.id.clone(),
                occurrence_start: key,
                starts_at: None,
                ends_at: None,
                starts_on: Some(starts_on),
                ends_on: Some(ends_on),
                is_all_day: true,
                title: over.and_then(|o| patch_str(&o.patch, "title")).unwrap_or_else(|| event.title.clone()),
                description: event.description.clone(),
                location: event.location.clone(),
                calendar_id: event.calendar_id.clone(),
                color: color.clone(),
                tz: event.tz.clone(),
                rrule: event.rrule.clone(),
                is_recurring: rule.is_some(),
                is_override: over.is_some(),
            });
        }
        return Ok(out);
    }

    // Timed. The local time-of-day is what stays constant across the series —
    // see the module docs on recur.rs.
    let Some(starts_at) = &event.starts_at else {
        return Ok(out);
    };
    let start_utc = parse_utc(starts_at)?;
    let end_utc = event.ends_at.as_deref().map(parse_utc).transpose()?.unwrap_or(start_utc + Duration::hours(1));
    let duration = end_utc - start_utc;
    let start_local = start_utc.with_timezone(&tz);
    let time_of_day = start_local.time();

    let dates = match rule {
        Some(rule) => recur::expand_dates(start_local.date_naive(), rule, win_from, win_to),
        None => vec![start_local.date_naive()],
    };

    for date in dates {
        let instant = local_to_utc(date, time_of_day, tz);
        let key = instant.to_rfc3339();
        let over = overrides.iter().find(|(k, _)| *k == key).map(|(_, o)| o);
        if over.map(|o| o.kind == "cancelled").unwrap_or(false) {
            continue;
        }
        let starts = over.and_then(|o| patch_str(&o.patch, "startsAt")).unwrap_or_else(|| key.clone());
        let ends = over.and_then(|o| patch_str(&o.patch, "endsAt")).unwrap_or_else(|| {
            parse_utc(&starts).map(|s| (s + duration).to_rfc3339()).unwrap_or_else(|_| (instant + duration).to_rfc3339())
        });

        // Overlap, not containment: a meeting that started before the window
        // and runs into it belongs on the day the user is looking at.
        let occ_start = parse_utc(&starts)?;
        let occ_end = parse_utc(&ends)?;
        if occ_end < from || occ_start > to {
            continue;
        }

        out.push(Occurrence {
            event_id: event.id.clone(),
            occurrence_start: key,
            starts_at: Some(starts),
            ends_at: Some(ends),
            starts_on: None,
            ends_on: None,
            is_all_day: false,
            title: over.and_then(|o| patch_str(&o.patch, "title")).unwrap_or_else(|| event.title.clone()),
            description: event.description.clone(),
            location: event.location.clone(),
            calendar_id: event.calendar_id.clone(),
            color: color.clone(),
            tz: event.tz.clone(),
            rrule: event.rrule.clone(),
            is_recurring: rule.is_some(),
            is_override: over.is_some(),
        });
    }
    Ok(out)
}

/// Events with the colour of their calendar, for `expand_events`.
fn query_events(conn: &Connection, sql: &str, params: impl rusqlite::Params) -> Result<Vec<(Event, String)>, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| format!("Prepare events: {e}"))?;
    let rows = stmt
        .query_map(params, |row| Ok((read_event(row)?, row.get::<_, String>(15)?)))
        .map_err(|e| format!("Query events: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Read events: {e}"))?;
    Ok(rows)
}

/// Days the SQL window is widened by on each side: one for the local-date
/// widening `expand_event` applies, one for the offset between UTC and the
/// event's zone, one so that comparing RFC 3339 *text* stays right whatever
/// offset a stored timestamp carries. A few extra rows cost nothing; the exact
/// cut is `expand_event`'s.
const WINDOW_MARGIN_DAYS: i64 = 3;

/// The events a window can draw an occurrence from, in one statement, instead
/// of the whole table on every navigation. A single event has to overlap the
/// widened window; a series has to start before its end and — when the rule
/// carries `UNTIL`, kept in `rrule_until` — not end before its start. A
/// `COUNT`-bounded series has no stored end and stays in; `expand_event` walks
/// it a handful of steps and stops.
fn select_events(
    conn: &Connection,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    visible_only: bool,
) -> Result<Vec<(Event, String)>, String> {
    let lo = from - Duration::days(WINDOW_MARGIN_DAYS);
    let hi = to + Duration::days(WINDOW_MARGIN_DAYS);
    let sql = format!(
        "SELECT {EVENT_COLUMNS_E}, c.color FROM events e JOIN calendars c ON c.id = e.calendar_id
          WHERE {}(
                (e.rrule IS NULL AND (
                     (e.is_all_day = 0 AND e.starts_at < ?2 AND COALESCE(e.ends_at, e.starts_at) > ?1)
                  OR (e.is_all_day = 1 AND e.starts_on <= ?4 AND COALESCE(e.ends_on, e.starts_on) >= ?3)))
             OR (e.rrule IS NOT NULL AND (e.rrule_until IS NULL OR e.rrule_until >= ?3) AND (
                     (e.is_all_day = 0 AND e.starts_at < ?2)
                  OR (e.is_all_day = 1 AND e.starts_on <= ?4))))",
        if visible_only { "c.is_visible = 1 AND " } else { "" }
    );
    query_events(
        conn,
        &sql,
        params![
            lo.to_rfc3339(),
            hi.to_rfc3339(),
            lo.format("%Y-%m-%d").to_string(),
            hi.format("%Y-%m-%d").to_string()
        ],
    )
}

/// The parsed rule for `event` — from the cache when the text is unchanged,
/// parsed and cached otherwise. `None` for a single event.
fn resolve_rule(cache: &mut RuleCache, event: &Event) -> Result<Option<recur::Rule>, String> {
    let Some(raw) = event.rrule.as_deref().filter(|raw| !raw.trim().is_empty()) else {
        return Ok(None);
    };
    if let Some((text, rule)) = cache.get(&event.id) {
        if text == raw {
            return Ok(Some(rule.clone()));
        }
    }
    let rule = recur::parse(raw)?;
    cache.insert(event.id.clone(), (raw.to_string(), rule.clone()));
    Ok(Some(rule))
}

/// Expand the given events into the window: overrides in one query, rules from
/// the cache, the result in display order.
fn expand_events(
    conn: &Connection,
    rules: &Mutex<RuleCache>,
    events: &[(Event, String)],
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<Occurrence>, String> {
    let ids: Vec<&str> = events.iter().map(|(event, _)| event.id.as_str()).collect();
    let overrides = load_overrides(conn, &ids)?;

    let mut out = Vec::new();
    {
        let mut cache = rules.lock().map_err(|e| format!("Lock rule cache: {e}"))?;
        for (event, cal_color) in events {
            let rule = resolve_rule(&mut cache, event)?;
            let event_overrides = overrides.get(&event.id).map(Vec::as_slice).unwrap_or(&[]);
            out.extend(expand_event(event, rule.as_ref(), event_overrides, from, to, cal_color)?);
        }
    }

    out.sort_by(|a, b| {
        let ka = a.starts_at.clone().or_else(|| a.starts_on.clone()).unwrap_or_default();
        let kb = b.starts_at.clone().or_else(|| b.starts_on.clone()).unwrap_or_default();
        // All-day first within a day, the way every calendar stacks them.
        b.is_all_day.cmp(&a.is_all_day).then(ka.cmp(&kb))
    });
    Ok(out)
}

/// Every instance between `from` and `to`, recurrence expanded and overrides
/// applied. One call per rendered range — the frontend never expands a series.
#[tauri::command(async)]
fn list_occurrences(
    from: String,
    to: String,
    include_hidden: Option<bool>,
    state: State<'_, AppState>,
) -> Result<Vec<Occurrence>, String> {
    let from_utc = parse_utc(&from)?;
    let to_utc = parse_utc(&to)?;
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    let events = select_events(&conn, from_utc, to_utc, !include_hidden.unwrap_or(false))?;
    expand_events(&conn, &state.rules, &events, from_utc, to_utc)
}

/// Cancel, move or edit ONE instance. `kind` is `cancelled` | `moved` | `edited`.
#[tauri::command(async)]
fn override_occurrence(
    event_id: String,
    occurrence_start: String,
    kind: String,
    patch: Option<Value>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<bool, String> {
    if !["cancelled", "moved", "edited"].contains(&kind.as_str()) {
        return Err(format!("Unknown override kind: {kind}"));
    }
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        conn.execute(
            "INSERT INTO event_overrides (id, event_id, occurrence_start, kind, patch_json)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(event_id, occurrence_start) DO UPDATE SET
                 kind = excluded.kind, patch_json = excluded.patch_json",
            params![
                new_id(),
                event_id,
                occurrence_start,
                kind,
                serde_json::to_string(&patch.unwrap_or_else(|| json!({})))
                    .map_err(|e| format!("Serialize patch: {e}"))?
            ],
        )
        .map_err(|e| format!("Write override: {e}"))?;
    }
    emit(&app, "plan:events-changed", json!({ "id": event_id }));
    Ok(true)
}

/// End the series the day before `occurrence_start` — 'this and all following'.
/// The caller creates the new series for the changed instances; splitting the
/// old one is the half that has to happen under the same lock.
#[tauri::command(async)]
fn truncate_series(
    event_id: String,
    occurrence_start: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<bool, String> {
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let (rrule, tz): (Option<String>, String) = conn
            .query_row("SELECT rrule, tz FROM events WHERE id = ?1", params![event_id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .map_err(|e| format!("Read series: {e}"))?;
        let Some(raw) = rrule else {
            return Err("That event is not a series".into());
        };

        let cutoff = if occurrence_start.len() == 10 {
            parse_date(&occurrence_start)?
        } else {
            parse_utc(&occurrence_start)?.with_timezone(&zone(&tz)).date_naive()
        } - Duration::days(1);

        let mut rule = recur::parse(&raw)?;
        rule.until = Some(cutoff);
        // COUNT and UNTIL are mutually exclusive in RFC 5545, and a surviving
        // COUNT would keep producing occurrences past the cut.
        rule.count = None;

        conn.execute(
            "UPDATE events SET rrule = ?1, rrule_until = ?2, updated_at = ?3 WHERE id = ?4",
            params![recur::to_string(&rule), until_text(&rule), now_iso(), event_id],
        )
        .map_err(|e| format!("Truncate series: {e}"))?;
        conn.execute(
            "DELETE FROM event_overrides WHERE event_id = ?1 AND occurrence_start >= ?2",
            params![event_id, occurrence_start],
        )
        .map_err(|e| format!("Drop trailing overrides: {e}"))?;
    }
    forget_rule(&state, &event_id)?;
    emit(&app, "plan:events-changed", json!({ "id": event_id }));
    Ok(true)
}

// ─── Free slots ───────────────────────────────────────────────────────────

/// Gaps of at least `duration_minutes` between `from` and `to`, inside the
/// given local hours. The command that makes the module useful to an agent:
/// "find me 90 minutes this week".
#[tauri::command(async)]
fn find_free_slots(
    from: String,
    to: String,
    duration_minutes: i64,
    day_start_hour: Option<u32>,
    day_end_hour: Option<u32>,
    state: State<'_, AppState>,
) -> Result<Vec<FreeSlot>, String> {
    let tz_name = {
        let guard = state.app_settings.lock().map_err(|e| format!("Lock settings: {e}"))?;
        guard.timezone.clone()
    };
    let tz = if tz_name == "local" { chrono_tz::UTC } else { zone(&tz_name) };

    let busy: Vec<(DateTime<Utc>, DateTime<Utc>)> = list_occurrences(from.clone(), to.clone(), None, state)?
        .into_iter()
        .filter(|o| !o.is_all_day)
        .filter_map(|o| {
            let s = parse_utc(o.starts_at.as_deref()?).ok()?;
            let e = parse_utc(o.ends_at.as_deref()?).ok()?;
            Some((s, e))
        })
        .collect();

    // Merge overlapping busy blocks, or a gap between two meetings that
    // overlap each other would look free.
    let mut merged: Vec<(DateTime<Utc>, DateTime<Utc>)> = Vec::new();
    let mut sorted = busy;
    sorted.sort_by_key(|(s, _)| *s);
    for (start, end) in sorted {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }

    let day_start = day_start_hour.unwrap_or(9);
    let day_end = day_end_hour.unwrap_or(18);
    let want = Duration::minutes(duration_minutes.max(1));
    let from_utc = parse_utc(&from)?;
    let to_utc = parse_utc(&to)?;

    let mut slots = Vec::new();
    let mut day = from_utc.with_timezone(&tz).date_naive();
    let last_day = to_utc.with_timezone(&tz).date_naive();

    while day <= last_day {
        let open = local_to_utc(day, NaiveTime::from_hms_opt(day_start, 0, 0).unwrap(), tz).max(from_utc);
        let close = local_to_utc(day, NaiveTime::from_hms_opt(day_end, 0, 0).unwrap(), tz).min(to_utc);
        let mut cursor = open;
        for (start, end) in &merged {
            if *end <= cursor || *start >= close {
                continue;
            }
            if *start > cursor && *start - cursor >= want {
                slots.push(FreeSlot { starts_at: cursor.to_rfc3339(), ends_at: start.to_rfc3339() });
            }
            cursor = cursor.max(*end);
        }
        if close > cursor && close - cursor >= want {
            slots.push(FreeSlot { starts_at: cursor.to_rfc3339(), ends_at: close.to_rfc3339() });
        }
        day += Duration::days(1);
    }
    Ok(slots)
}

// ─── Reminders ────────────────────────────────────────────────────────────

#[tauri::command(async)]
fn list_reminders(event_id: String, state: State<'_, AppState>) -> Result<Vec<Reminder>, String> {
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    let mut stmt = conn
        .prepare("SELECT id, event_id, minutes_before, kind FROM reminders WHERE event_id = ?1 ORDER BY minutes_before DESC")
        .map_err(|e| format!("Prepare reminders: {e}"))?;
    let rows = stmt
        .query_map(params![event_id], |row| {
            Ok(Reminder { id: row.get(0)?, event_id: row.get(1)?, minutes_before: row.get(2)?, kind: row.get(3)? })
        })
        .map_err(|e| format!("Query reminders: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Read reminders: {e}"))?;
    Ok(rows)
}

#[tauri::command(async)]
fn set_reminders(
    event_id: String,
    minutes: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<Reminder>, String> {
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        conn.execute("DELETE FROM reminders WHERE event_id = ?1", params![event_id])
            .map_err(|e| format!("Clear reminders: {e}"))?;
        for m in &minutes {
            conn.execute(
                "INSERT INTO reminders (id, event_id, minutes_before, kind) VALUES (?1, ?2, ?3, 'notification')",
                params![new_id(), event_id, m],
            )
            .map_err(|e| format!("Add reminder: {e}"))?;
        }
    }
    list_reminders(event_id, state)
}

// ─── Search ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub event_id: String,
    pub title: String,
    pub starts_at: Option<String>,
    pub starts_on: Option<String>,
    pub calendar_id: String,
}

#[tauri::command(async)]
fn search(query: String, state: State<'_, AppState>) -> Result<Vec<SearchHit>, String> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Ok(Vec::new());
    }
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    let pattern = format!("%{needle}%");
    let mut stmt = conn
        .prepare(
            "SELECT id, title, starts_at, starts_on, calendar_id FROM events
              WHERE lower(title) LIKE ?1 OR lower(COALESCE(description,'')) LIKE ?1
                 OR lower(COALESCE(location,'')) LIKE ?1
              ORDER BY COALESCE(starts_at, starts_on) DESC LIMIT 50",
        )
        .map_err(|e| format!("Prepare search: {e}"))?;
    let rows = stmt
        .query_map(params![pattern], |row| {
            Ok(SearchHit {
                event_id: row.get(0)?,
                title: row.get(1)?,
                starts_at: row.get(2)?,
                starts_on: row.get(3)?,
                calendar_id: row.get(4)?,
            })
        })
        .map_err(|e| format!("Query search: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Read search hits: {e}"))?;
    Ok(rows)
}

// ─── Settings ─────────────────────────────────────────────────────────────

fn load_app_settings(base: &Path) -> AppSettings {
    fs::read_to_string(settings_path(base))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_app_settings(base: &Path, settings: &AppSettings) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(settings).map_err(|e| format!("Serialize settings: {e}"))?;
    qs_core::paths::write_atomic(&settings_path(base), raw.as_bytes()).map_err(|e| format!("Write settings: {e}"))
}

#[tauri::command(async)]
fn get_app_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    let guard = state.app_settings.lock().map_err(|e| format!("Lock settings: {e}"))?;
    Ok(guard.clone())
}

/// Async because it writes `config.json`: a sync command runs inline on the
/// main thread and a slow disk freezes the window (ARCHITECTURE.md §1).
#[tauri::command]
async fn update_app_settings(
    settings: AppSettings,
    state: State<'_, AppState>,
) -> Result<AppSettings, String> {
    let saved = {
        let mut guard = state.app_settings.lock().map_err(|e| format!("Lock settings: {e}"))?;
        *guard = settings;
        save_app_settings(&state.data_dir, &guard)?;
        guard.clone()
    };
    Ok(saved)
}

#[tauri::command(async)]
fn get_setting(key: String, state: State<'_, AppState>) -> Result<Value, String> {
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    let raw: Option<String> = conn
        .query_row("SELECT value_json FROM settings WHERE key = ?1", params![key], |r| r.get(0))
        .optional()
        .map_err(|e| format!("Get setting: {e}"))?;
    Ok(raw.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null))
}

#[tauri::command(async)]
fn set_setting(key: String, value: Value, state: State<'_, AppState>) -> Result<bool, String> {
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    conn.execute(
        "INSERT INTO settings (key, value_json) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
        params![key, serde_json::to_string(&value).map_err(|e| format!("Serialize: {e}"))?],
    )
    .map_err(|e| format!("Set setting: {e}"))?;
    Ok(true)
}

/// Async: copying the whole database is unbounded work, and on the main thread
/// it stalls the window for as long as the copy takes.
#[tauri::command]
async fn backup_database(state: State<'_, AppState>) -> Result<String, String> {
    let target = state
        .data_dir
        .join("backups")
        .join(format!("plan-{}.db", Utc::now().format("%Y%m%d-%H%M%S")));
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let mut dst = Connection::open(&target).map_err(|e| format!("Open backup target: {e}"))?;
        let backup =
            rusqlite::backup::Backup::new(&conn, &mut dst).map_err(|e| format!("Start backup: {e}"))?;
        backup
            .run_to_completion(5, std::time::Duration::from_millis(50), None)
            .map_err(|e| format!("Run backup: {e}"))?;
    }
    Ok(target.to_string_lossy().to_string())
}

// ─── Entry point ──────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Build the `plan` plugin. Pinned to `Wry` for the same reason as `systems`:
/// bare `tauri::AppHandle` already means `AppHandle<Wry>`.
pub fn init() -> TauriPlugin<Wry> {
    Builder::<Wry>::new("plan")
        .setup(|app, _api| {
            let dir = data_dir();
            ensure_dirs(&dir).map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

            let conn = Connection::open(db_path(&dir))?;
            // Foreign keys are per-connection and off by default; every cascade
            // in the schema depends on them.
            conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
            init_schema(&conn).map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            seed_schema(&conn).map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

            let settings = load_app_settings(&dir);
            app.manage(AppState {
                data_dir: dir,
                db: Arc::new(Mutex::new(conn)),
                app_settings: Mutex::new(settings),
                rules: Mutex::new(RuleCache::new()),
            });

            qs_core::tray::register_entry(
                app,
                qs_core::tray::TrayEntry {
                    module: "flow".into(),
                    label: "QuantFlow".into(),
                    route: "/flow".into(),
                    toggle_window: None,
                },
            );

            Ok(())
        })
        .invoke_handler(qs_core::diagnostics::traced(tauri::generate_handler![
            list_calendars,
            create_calendar,
            update_calendar,
            delete_calendar,
            get_event,
            create_event,
            update_event,
            delete_event,
            list_occurrences,
            override_occurrence,
            truncate_series,
            find_free_slots,
            list_reminders,
            set_reminders,
            search,
            get_setting,
            set_setting,
            get_app_settings,
            update_app_settings,
            backup_database,
        ]))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    fn timed(starts_at: &str, ends_at: &str, tz: &str, rrule: Option<&str>) -> Event {
        Event {
            id: "e1".into(),
            calendar_id: "c1".into(),
            title: "Standup".into(),
            description: None,
            location: None,
            starts_at: Some(starts_at.into()),
            ends_at: Some(ends_at.into()),
            starts_on: None,
            ends_on: None,
            is_all_day: false,
            tz: tz.into(),
            rrule: rrule.map(str::to_string),
            color: None,
            created_at: "x".into(),
            updated_at: "x".into(),
        }
    }

    fn utc(raw: &str) -> DateTime<Utc> {
        parse_utc(raw).unwrap()
    }

    /// Parse the fixture's rule and expand it, as `expand_events` does per row.
    fn expand(
        event: &Event,
        overrides: &[(String, Override)],
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Vec<Occurrence> {
        let rule = event.rrule.as_deref().map(|raw| recur::parse(raw).unwrap());
        expand_event(event, rule.as_ref(), overrides, from, to, "blue").unwrap()
    }

    /// The reason the expansion works in local dates: 09:00 Berlin is 08:00Z in
    /// winter and 07:00Z in summer. A series expanded in UTC would shift.
    #[test]
    fn weekly_event_keeps_its_local_time_across_a_dst_change() {
        // 2026-03-02 is a Monday; Europe/Berlin springs forward on 2026-03-29.
        let event = timed("2026-03-02T08:00:00+00:00", "2026-03-02T08:30:00+00:00", "Europe/Berlin", Some("FREQ=WEEKLY"));
        let out = expand(&event, &[], utc("2026-03-01T00:00:00Z"), utc("2026-04-15T00:00:00Z"));

        let tz = zone("Europe/Berlin");
        for occ in &out {
            let local = parse_utc(occ.starts_at.as_ref().unwrap()).unwrap().with_timezone(&tz);
            assert_eq!(local.hour(), 9, "occurrence {} drifted off 09:00 local", occ.occurrence_start);
        }
        // Before the change it is 08:00Z, after it 07:00Z — the proof the
        // instant really moved while the wall clock did not.
        assert!(out.first().unwrap().starts_at.as_ref().unwrap().contains("T08:00"));
        assert!(out.last().unwrap().starts_at.as_ref().unwrap().contains("T07:00"));
    }

    #[test]
    fn a_cancelled_occurrence_disappears_from_the_middle_of_a_series() {
        let event = timed("2026-06-01T08:00:00+00:00", "2026-06-01T09:00:00+00:00", "UTC", Some("FREQ=DAILY"));
        let all = expand(&event, &[], utc("2026-06-01T00:00:00Z"), utc("2026-06-05T00:00:00Z"));
        assert_eq!(all.len(), 4);

        let key = all[1].occurrence_start.clone();
        let overrides = vec![(key.clone(), Override { kind: "cancelled".into(), patch: json!({}) })];
        let left = expand(&event, &overrides, utc("2026-06-01T00:00:00Z"), utc("2026-06-05T00:00:00Z"));
        assert_eq!(left.len(), 3);
        assert!(left.iter().all(|o| o.occurrence_start != key));
    }

    #[test]
    fn a_moved_occurrence_keeps_its_original_key() {
        let event = timed("2026-06-01T08:00:00+00:00", "2026-06-01T09:00:00+00:00", "UTC", Some("FREQ=DAILY"));
        let all = expand(&event, &[], utc("2026-06-01T00:00:00Z"), utc("2026-06-03T00:00:00Z"));
        let key = all[1].occurrence_start.clone();
        let overrides = vec![(
            key.clone(),
            Override { kind: "moved".into(), patch: json!({ "startsAt": "2026-06-02T14:00:00+00:00" }) },
        )];
        let out = expand(&event, &overrides, utc("2026-06-01T00:00:00Z"), utc("2026-06-03T00:00:00Z"));
        let moved = out.iter().find(|o| o.occurrence_start == key).unwrap();
        assert!(moved.starts_at.as_ref().unwrap().contains("T14:00"));
        assert!(moved.is_override);
        // The duration travels with the move: 08:00–09:00 becomes 14:00–15:00.
        assert!(moved.ends_at.as_ref().unwrap().contains("T15:00"));
    }

    /// An all-day event on 1 September is on 1 September in Auckland too.
    #[test]
    fn an_all_day_event_does_not_slide_a_day() {
        let event = Event {
            is_all_day: true,
            starts_at: None,
            ends_at: None,
            starts_on: Some("2026-09-01".into()),
            ends_on: Some("2026-09-03".into()),
            tz: "Pacific/Auckland".into(),
            rrule: None,
            ..timed("2026-09-01T00:00:00Z", "2026-09-01T00:00:00Z", "Pacific/Auckland", None)
        };
        let out = expand(&event, &[], utc("2026-08-25T00:00:00Z"), utc("2026-09-10T00:00:00Z"));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].starts_on.as_deref(), Some("2026-09-01"));
        // ends_on is inclusive: 1st through 3rd is three days, not four.
        assert_eq!(out[0].ends_on.as_deref(), Some("2026-09-03"));
    }

    #[test]
    fn an_event_running_into_the_window_is_included() {
        // Starts before the window opens, ends inside it.
        let event = timed("2026-06-01T22:00:00+00:00", "2026-06-02T02:00:00+00:00", "UTC", None);
        let out = expand(&event, &[], utc("2026-06-02T00:00:00Z"), utc("2026-06-03T00:00:00Z"));
        assert_eq!(out.len(), 1, "an overlapping event must not be filtered out");
    }

    #[test]
    fn a_nonexistent_local_time_steps_forward_instead_of_vanishing() {
        // 2026-03-29 02:30 Europe/Berlin does not exist — the clock jumps 02:00→03:00.
        let tz = zone("Europe/Berlin");
        let date = NaiveDate::from_ymd_opt(2026, 3, 29).unwrap();
        let time = NaiveTime::from_hms_opt(2, 30, 0).unwrap();
        let instant = local_to_utc(date, time, tz);
        assert_eq!(instant.with_timezone(&tz).hour(), 3);
    }

    // ─── The window prefilter ───────────────────────────────────────────

    use std::sync::atomic::{AtomicUsize, Ordering};

    /// `Connection::trace` takes a plain fn, so the counter is a static; only
    /// one test uses it.
    static STATEMENTS: AtomicUsize = AtomicUsize::new(0);

    fn count_statement(_sql: &str) {
        STATEMENTS.fetch_add(1, Ordering::SeqCst);
    }

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// A fresh in-memory schema with its two seeded calendars; their ids.
    fn fresh_db() -> (Connection, Vec<String>) {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        seed_schema(&conn).unwrap();
        let ids = {
            let mut stmt = conn.prepare("SELECT id FROM calendars ORDER BY sort_index").unwrap();
            let ids = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
            ids.collect::<Result<Vec<_>, _>>().unwrap()
        };
        (conn, ids)
    }

    /// Five thousand events spread over three years through the production
    /// INSERT: timed and all-day singles, weekly and monthly series with UNTIL,
    /// daily series with COUNT, in three zones and both calendars.
    fn seed_events(conn: &Connection, calendars: &[String]) {
        let base = day(2025, 1, 1);
        for i in 0..5_000usize {
            let date = base + Duration::days(((i * 7919) % 1096) as i64);
            let start = Utc.from_utc_datetime(&date.and_hms_opt(6 + (i % 12) as u32, 0, 0).unwrap());
            let all_day = matches!(i % 10, 6 | 9);
            let rrule = match i % 10 {
                7 => Some(format!("FREQ=WEEKLY;UNTIL={}", (date + Duration::days(56)).format("%Y%m%d"))),
                8 => Some("FREQ=DAILY;COUNT=5".to_string()),
                9 => Some(format!("FREQ=MONTHLY;UNTIL={}", (date + Duration::days(180)).format("%Y%m%d"))),
                _ => None,
            };
            let rule = rrule.as_deref().map(|raw| recur::parse(raw).unwrap());
            let input = EventInput {
                calendar_id: calendars[i % calendars.len()].clone(),
                title: format!("Event {i}"),
                starts_at: (!all_day).then(|| start.to_rfc3339()),
                ends_at: (!all_day).then(|| (start + Duration::minutes(30 + (i % 4) as i64 * 30)).to_rfc3339()),
                starts_on: all_day.then(|| date.format("%Y-%m-%d").to_string()),
                ends_on: all_day.then(|| (date + Duration::days((i % 3) as i64)).format("%Y-%m-%d").to_string()),
                is_all_day: all_day,
                tz: Some(["UTC", "Europe/Berlin", "America/New_York"][i % 3].into()),
                rrule,
                ..EventInput::default()
            };
            insert_event(conn, &format!("ev-{i}"), &input, rule.as_ref().and_then(until_text)).unwrap();
        }
    }

    /// Occurrences as JSON in a fixed order: the two paths read rows in
    /// different orders, and the display sort keeps ties in row order.
    fn sorted_json(occurrences: &[Occurrence]) -> Vec<Value> {
        let mut out: Vec<&Occurrence> = occurrences.iter().collect();
        out.sort_by(|a, b| (&a.event_id, &a.occurrence_start).cmp(&(&b.event_id, &b.occurrence_start)));
        out.into_iter().map(|o| serde_json::to_value(o).unwrap()).collect()
    }

    /// A week must not load the whole table, ask for overrides event by event
    /// or parse every rule — and must still show exactly what expanding every
    /// row shows.
    #[test]
    fn a_week_over_five_thousand_events_is_few_statements_and_the_same_set() {
        let (mut conn, calendars) = fresh_db();
        seed_events(&conn, &calendars);
        let rules = Mutex::new(RuleCache::new());
        let everything = format!("SELECT {EVENT_COLUMNS_E}, c.color FROM events e JOIN calendars c ON c.id = e.calendar_id");
        let from = utc("2025-09-01T00:00:00Z");
        let to = utc("2025-09-08T00:00:00Z");

        // Cancel, move and rename one instance each of series in the week, so
        // the batched override query has something to carry.
        let all = query_events(&conn, &everything, []).unwrap();
        let week = expand_events(&conn, &rules, &all, from, to).unwrap();
        let picks: Vec<&Occurrence> = week.iter().filter(|o| o.is_recurring && !o.is_all_day).take(3).collect();
        assert_eq!(picks.len(), 3, "the seed must put timed series into the test week");
        for (occ, kind, patch) in [
            (picks[0], "cancelled", json!({})),
            (picks[1], "moved", json!({ "startsAt": "2025-09-04T20:00:00+00:00" })),
            (picks[2], "edited", json!({ "title": "Renamed" })),
        ] {
            conn.execute(
                "INSERT INTO event_overrides (id, event_id, occurrence_start, kind, patch_json) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![new_id(), occ.event_id, occ.occurrence_start, kind, patch.to_string()],
            )
            .unwrap();
        }
        let reference = expand_events(&conn, &rules, &query_events(&conn, &everything, []).unwrap(), from, to).unwrap();

        STATEMENTS.store(0, Ordering::SeqCst);
        conn.trace(Some(count_statement));
        let selected = select_events(&conn, from, to, false).unwrap();
        let filtered = expand_events(&conn, &rules, &selected, from, to).unwrap();
        conn.trace(None);
        let statements = STATEMENTS.load(Ordering::SeqCst);

        assert!(statements <= 3, "a week view issued {statements} statements");
        assert!(selected.len() < all.len() / 4, "the prefilter kept {} of {} rows", selected.len(), all.len());
        assert!(!filtered.is_empty());
        assert_eq!(filtered.iter().filter(|o| o.is_override).count(), 2, "moved and edited instances survive, the cancelled one is gone");
        assert_eq!(sorted_json(&filtered), sorted_json(&reference));
    }

    /// A database from before `rrule_until` gets the column filled from the
    /// rules it already holds, and the filter then uses it.
    #[test]
    fn an_older_database_gets_rrule_until_filled_from_its_rules() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE calendars (id TEXT PRIMARY KEY, name TEXT NOT NULL, color TEXT NOT NULL,
                 is_visible INTEGER NOT NULL DEFAULT 1, is_default INTEGER NOT NULL DEFAULT 0,
                 sort_index REAL NOT NULL DEFAULT 0, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
             CREATE TABLE events (id TEXT PRIMARY KEY, calendar_id TEXT NOT NULL, title TEXT NOT NULL,
                 description TEXT, location TEXT, starts_at TEXT, ends_at TEXT, starts_on TEXT, ends_on TEXT,
                 is_all_day INTEGER NOT NULL DEFAULT 0, tz TEXT NOT NULL DEFAULT 'UTC', rrule TEXT, color TEXT,
                 created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
             INSERT INTO calendars VALUES ('c1', 'Personal', 'blue', 1, 1, 0, 'x', 'x');
             INSERT INTO events (id, calendar_id, title, starts_at, rrule, created_at, updated_at) VALUES
                 ('ended', 'c1', 'Ended', '2025-01-06T08:00:00+00:00', 'FREQ=WEEKLY;UNTIL=20250301T000000Z', 'x', 'x'),
                 ('open', 'c1', 'Open', '2025-01-06T08:00:00+00:00', 'FREQ=WEEKLY', 'x', 'x'),
                 ('single', 'c1', 'Single', '2025-01-06T08:00:00+00:00', NULL, 'x', 'x');",
        )
        .unwrap();
        init_schema(&conn).unwrap();
        // Running again on the migrated database must be a no-op.
        init_schema(&conn).unwrap();

        let until = |id: &str| -> Option<String> {
            conn.query_row("SELECT rrule_until FROM events WHERE id = ?1", params![id], |r| r.get(0)).unwrap()
        };
        assert_eq!(until("ended").as_deref(), Some("2025-03-01"));
        assert_eq!(until("open"), None);
        assert_eq!(until("single"), None);

        let rows = select_events(&conn, utc("2025-06-02T00:00:00Z"), utc("2025-06-09T00:00:00Z"), true).unwrap();
        let ids: Vec<&str> = rows.iter().map(|(event, _)| event.id.as_str()).collect();
        assert_eq!(ids, vec!["open"], "the ended series is skipped by its UNTIL, the single one by its dates");
    }
}
