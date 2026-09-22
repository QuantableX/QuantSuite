//! QuantHabit — habits as columns, days as rows.
//!
//! One database, `~/.quantsuite/modules/habit/habit.db`, holding three tables:
//! **habits** (the columns), **checks** (one row per ticked habit-day) and
//! **pauses** (closed or open day intervals in which a habit is off).
//!
//! The lifecycle is deliberately small. A habit starts the day it is created —
//! there is no backdating, the first trackable day is today. Pausing opens an
//! interval starting today; days inside a pause cannot be checked, do not
//! count against the completion rate and do not break a streak, because "I
//! decided not to track this for a while" is not the same fact as "I failed".
//! Resuming closes the interval yesterday, so today is trackable again.
//! Deleting removes the habit and its whole history.
//!
//! Days are **local calendar days** (`YYYY-MM-DD` strings), never timestamps.
//! A habit is done "today" in the user's own day; storing UTC instants would
//! flip the day at 01:00 or 23:00 depending on the season.

use chrono::{Datelike, NaiveDate};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Emitter, Manager, State, Wry,
};
use uuid::Uuid;

// ─── Types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pause {
    /// First paused day, inclusive.
    pub from_on: String,
    /// Last paused day, inclusive. `None` while the habit is still paused.
    pub to_on: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Habit {
    pub id: String,
    pub name: String,
    /// The first trackable day — the local day the habit was created.
    pub started_on: String,
    /// Derived: an open pause interval exists.
    pub is_paused: bool,
    /// Which weekdays the habit is tracked on: bit 0 = Monday … bit 6 =
    /// Sunday. An unset day behaves exactly like a paused one — it cannot be
    /// checked, it does not count, and it does not break a streak.
    pub days_mask: i64,
    pub sort_index: f64,
    pub pauses: Vec<Pause>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HabitPatch {
    pub name: Option<String>,
    pub sort_index: Option<f64>,
    pub days_mask: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub habit_id: String,
    pub on_day: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HabitStats {
    pub id: String,
    pub name: String,
    pub is_paused: bool,
    pub started_on: String,
    /// Days in the requested year the habit could have been checked on.
    pub eligible_days: i64,
    /// Of those, the days it actually was.
    pub checked_days: i64,
    /// checked / eligible in percent; `None` before the first eligible day.
    pub rate: Option<i64>,
    /// Streaks are *not* year-scoped — a streak that crosses New Year is one
    /// streak, and cutting it at the page boundary would just be wrong.
    pub current_streak: i64,
    pub best_streak: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsOut {
    pub year: i32,
    pub habits: Vec<HabitStats>,
    /// All checks in the year, across every habit.
    pub total_checks: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    /// week | month — how the year page groups its day rows.
    pub grouping: String,
    pub sidebar_left_open: bool,
    pub sidebar_right_open: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            grouping: "week".into(),
            sidebar_left_open: true,
            sidebar_right_open: true,
        }
    }
}

pub struct AppState {
    data_dir: PathBuf,
    db: Arc<Mutex<Connection>>,
    app_settings: Mutex<AppSettings>,
}

// ─── Helpers ──────────────────────────────────────────────────────────────

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn new_id() -> String {
    Uuid::new_v4().to_string()
}

/// The user's calendar day, not UTC's.
fn today_local() -> NaiveDate {
    chrono::Local::now().date_naive()
}

fn day_key(day: NaiveDate) -> String {
    day.format("%Y-%m-%d").to_string()
}

fn parse_day(s: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|e| format!("Bad day \"{s}\": {e}"))
}

fn data_dir() -> PathBuf {
    qs_core::paths::module_dir("habit")
}

fn db_path(base: &Path) -> PathBuf {
    base.join("habit.db")
}

fn settings_path(base: &Path) -> PathBuf {
    base.join("config.json")
}

fn emit(app: &AppHandle, event: &str, payload: Value) {
    let _ = app.emit(event, payload);
}

fn db(state: &State<'_, AppState>) -> Arc<Mutex<Connection>> {
    state.db.clone()
}

// ─── Day logic — pure, tested ─────────────────────────────────────────────

/// A pause interval as dates; `None` end = still paused.
type PauseSpan = (NaiveDate, Option<NaiveDate>);

/// Every weekday bit set — the default tracking schedule.
const ALL_DAYS: i64 = 0b111_1111;

/// Clamp a weekday mask to its seven meaningful bits; an empty schedule is a
/// habit that can never be done, so it is rejected rather than stored.
fn normalize_mask(mask: i64) -> Result<i64, String> {
    let mask = mask & ALL_DAYS;
    if mask == 0 {
        return Err("A habit needs at least one tracked weekday".into());
    }
    Ok(mask)
}

/// Is this weekday part of the habit's schedule? Bit 0 = Monday.
fn tracked_on(day: NaiveDate, mask: i64) -> bool {
    (mask >> day.weekday().num_days_from_monday()) & 1 == 1
}

fn is_paused_on(day: NaiveDate, pauses: &[PauseSpan]) -> bool {
    // `Option::is_none_or` needs Rust 1.82; the workspace MSRV is 1.77.
    pauses.iter().any(|(from, to)| *from <= day && to.map_or(true, |t| day <= t))
}

/// A day the habit could have been checked on: on/after its start, on a
/// tracked weekday, and not inside a pause. The caller bounds the future —
/// eligibility itself does not know what "today" is.
fn eligible_on(day: NaiveDate, started: NaiveDate, mask: i64, pauses: &[PauseSpan]) -> bool {
    day >= started && tracked_on(day, mask) && !is_paused_on(day, pauses)
}

/// (current, best) streak in *eligible* days.
///
/// A paused or untracked day neither extends nor breaks a run — the run
/// continues across it. An eligible unchecked day breaks it, except today: an
/// unchecked today is still open ("not done *yet*"), so the current streak is
/// the run as of yesterday and only tomorrow's sunrise ends it.
fn streaks(
    started: NaiveDate,
    mask: i64,
    pauses: &[PauseSpan],
    checks: &HashSet<NaiveDate>,
    today: NaiveDate,
) -> (i64, i64) {
    let mut best = 0_i64;
    let mut run = 0_i64;
    let mut run_before_today = 0_i64;
    let mut day = started;
    while day <= today {
        if day == today {
            run_before_today = run;
        }
        if eligible_on(day, started, mask, pauses) {
            if checks.contains(&day) {
                run += 1;
                best = best.max(run);
            } else {
                run = 0;
            }
        }
        match day.succ_opt() {
            Some(next) => day = next,
            None => break,
        }
    }
    let current = if eligible_on(today, started, mask, pauses) && !checks.contains(&today) {
        run_before_today
    } else {
        run
    };
    (current, best)
}

/// Eligible and checked days inside `[from, to]`, both inclusive.
fn year_counts(
    started: NaiveDate,
    mask: i64,
    pauses: &[PauseSpan],
    checks: &HashSet<NaiveDate>,
    from: NaiveDate,
    to: NaiveDate,
) -> (i64, i64) {
    let mut eligible = 0_i64;
    let mut checked = 0_i64;
    let mut day = from.max(started);
    while day <= to {
        if eligible_on(day, started, mask, pauses) {
            eligible += 1;
            if checks.contains(&day) {
                checked += 1;
            }
        }
        match day.succ_opt() {
            Some(next) => day = next,
            None => break,
        }
    }
    (eligible, checked)
}

// ─── Schema ───────────────────────────────────────────────────────────────

fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        -- One habit — a column on the year page. `started_on` is the local
        -- day it was created; earlier days are simply not part of its life.
        CREATE TABLE IF NOT EXISTS habits (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            started_on TEXT NOT NULL,
            days_mask INTEGER NOT NULL DEFAULT 127,
            sort_index REAL NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        -- A pause: a closed or open day interval in which the habit is off.
        -- Open = to_on IS NULL. Days inside cannot be checked and are
        -- excluded from every statistic.
        CREATE TABLE IF NOT EXISTS pauses (
            id TEXT PRIMARY KEY,
            habit_id TEXT NOT NULL REFERENCES habits(id) ON DELETE CASCADE,
            from_on TEXT NOT NULL,
            to_on TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_pauses_habit ON pauses(habit_id);

        -- A tick. Presence IS the fact — there is no 'unchecked' row.
        CREATE TABLE IF NOT EXISTS checks (
            habit_id TEXT NOT NULL REFERENCES habits(id) ON DELETE CASCADE,
            on_day TEXT NOT NULL,
            created_at TEXT NOT NULL,
            PRIMARY KEY (habit_id, on_day)
        );
        CREATE INDEX IF NOT EXISTS idx_checks_day ON checks(on_day);
        ",
    )
    .map_err(|e| format!("Init schema: {e}"))?;

    // Databases created before the weekday schedule existed lack the column;
    // `CREATE TABLE IF NOT EXISTS` will not add it, so probe and alter.
    let has_days_mask = {
        let mut stmt = conn
            .prepare("PRAGMA table_info(habits)")
            .map_err(|e| format!("Probe habits schema: {e}"))?;
        let names = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| format!("Query habits schema: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Read habits schema: {e}"))?;
        names.iter().any(|n| n == "days_mask")
    };
    if !has_days_mask {
        conn.execute("ALTER TABLE habits ADD COLUMN days_mask INTEGER NOT NULL DEFAULT 127", [])
            .map_err(|e| format!("Add days_mask: {e}"))?;
    }
    Ok(())
}

// ─── Reading habits ───────────────────────────────────────────────────────

fn load_pauses(conn: &Connection, habit_id: &str) -> Result<Vec<Pause>, String> {
    let mut stmt = conn
        .prepare("SELECT from_on, to_on FROM pauses WHERE habit_id = ?1 ORDER BY from_on")
        .map_err(|e| format!("Prepare pauses: {e}"))?;
    let rows = stmt
        .query_map(params![habit_id], |row| {
            Ok(Pause { from_on: row.get(0)?, to_on: row.get(1)? })
        })
        .map_err(|e| format!("Query pauses: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Read pauses: {e}"))?;
    Ok(rows)
}

fn pause_spans(pauses: &[Pause]) -> Result<Vec<PauseSpan>, String> {
    pauses
        .iter()
        .map(|p| {
            Ok((
                parse_day(&p.from_on)?,
                p.to_on.as_deref().map(parse_day).transpose()?,
            ))
        })
        .collect()
}

fn read_habit(conn: &Connection, id: &str) -> Result<Habit, String> {
    let (name, started_on, days_mask, sort_index, created_at, updated_at) = conn
        .query_row(
            "SELECT name, started_on, days_mask, sort_index, created_at, updated_at
               FROM habits WHERE id = ?1",
            params![id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .map_err(|e| format!("Get habit: {e}"))?;
    let pauses = load_pauses(conn, id)?;
    let is_paused = pauses.iter().any(|p| p.to_on.is_none());
    Ok(Habit {
        id: id.to_string(),
        name,
        started_on,
        is_paused,
        days_mask,
        sort_index,
        pauses,
        created_at,
        updated_at,
    })
}

fn read_all_habits(conn: &Connection) -> Result<Vec<Habit>, String> {
    let ids: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT id FROM habits ORDER BY sort_index, created_at")
            .map_err(|e| format!("Prepare habits: {e}"))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| format!("Query habits: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Read habits: {e}"))?;
        rows
    };
    ids.iter().map(|id| read_habit(conn, id)).collect()
}

// ─── Commands ─────────────────────────────────────────────────────────────

#[tauri::command(async)]
fn list_habits(state: State<'_, AppState>) -> Result<Vec<Habit>, String> {
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    read_all_habits(&conn)
}

#[tauri::command(async)]
fn create_habit(
    name: String,
    days_mask: Option<i64>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Habit, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("A habit needs a name".into());
    }
    let mask = normalize_mask(days_mask.unwrap_or(ALL_DAYS))?;
    let id = new_id();
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let now = now_iso();
        let next: f64 = conn
            .query_row("SELECT COALESCE(MAX(sort_index), -1) + 1 FROM habits", [], |r| r.get(0))
            .map_err(|e| format!("Next habit index: {e}"))?;
        conn.execute(
            "INSERT INTO habits (id, name, started_on, days_mask, sort_index, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![id, name, day_key(today_local()), mask, next, now],
        )
        .map_err(|e| format!("Create habit: {e}"))?;
    }
    emit(&app, "habit:changed", json!({ "id": id.clone() }));
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    read_habit(&conn, &id)
}

#[tauri::command(async)]
fn update_habit(
    id: String,
    patch: HabitPatch,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Habit, String> {
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let now = now_iso();
        if let Some(name) = &patch.name {
            let name = name.trim();
            if name.is_empty() {
                return Err("A habit needs a name".into());
            }
            conn.execute(
                "UPDATE habits SET name = ?1, updated_at = ?2 WHERE id = ?3",
                params![name, now, id],
            )
            .map_err(|e| format!("Update name: {e}"))?;
        }
        if let Some(sort_index) = patch.sort_index {
            conn.execute(
                "UPDATE habits SET sort_index = ?1, updated_at = ?2 WHERE id = ?3",
                params![sort_index, now, id],
            )
            .map_err(|e| format!("Update sort_index: {e}"))?;
        }
        if let Some(days_mask) = patch.days_mask {
            let mask = normalize_mask(days_mask)?;
            conn.execute(
                "UPDATE habits SET days_mask = ?1, updated_at = ?2 WHERE id = ?3",
                params![mask, now, id],
            )
            .map_err(|e| format!("Update days_mask: {e}"))?;
        }
    }
    emit(&app, "habit:changed", json!({ "id": id.clone() }));
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    read_habit(&conn, &id)
}

/// Open a pause starting today. Pausing an already-paused habit is a no-op —
/// there is exactly one open interval at a time.
#[tauri::command(async)]
fn pause_habit(id: String, state: State<'_, AppState>, app: AppHandle) -> Result<Habit, String> {
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let open: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pauses WHERE habit_id = ?1 AND to_on IS NULL",
                params![id],
                |r| r.get(0),
            )
            .map_err(|e| format!("Check open pause: {e}"))?;
        if open == 0 {
            conn.execute(
                "INSERT INTO pauses (id, habit_id, from_on, to_on) VALUES (?1, ?2, ?3, NULL)",
                params![new_id(), id, day_key(today_local())],
            )
            .map_err(|e| format!("Pause habit: {e}"))?;
            conn.execute(
                "UPDATE habits SET updated_at = ?1 WHERE id = ?2",
                params![now_iso(), id],
            )
            .map_err(|e| format!("Touch habit: {e}"))?;
        }
    }
    emit(&app, "habit:changed", json!({ "id": id.clone() }));
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    read_habit(&conn, &id)
}

/// Close the open pause so today is trackable again. A pause opened earlier
/// today is deleted outright — it never covered a full day.
#[tauri::command(async)]
fn resume_habit(id: String, state: State<'_, AppState>, app: AppHandle) -> Result<Habit, String> {
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let open: Option<(String, String)> = {
            let mut stmt = conn
                .prepare("SELECT id, from_on FROM pauses WHERE habit_id = ?1 AND to_on IS NULL")
                .map_err(|e| format!("Prepare open pause: {e}"))?;
            let mut rows = stmt
                .query_map(params![id], |row| Ok((row.get(0)?, row.get(1)?)))
                .map_err(|e| format!("Query open pause: {e}"))?;
            rows.next().transpose().map_err(|e| format!("Read open pause: {e}"))?
        };
        if let Some((pause_id, from_on)) = open {
            let today = today_local();
            if parse_day(&from_on)? >= today {
                conn.execute("DELETE FROM pauses WHERE id = ?1", params![pause_id])
                    .map_err(|e| format!("Drop same-day pause: {e}"))?;
            } else {
                let yesterday = today.pred_opt().ok_or("No yesterday for this date")?;
                conn.execute(
                    "UPDATE pauses SET to_on = ?1 WHERE id = ?2",
                    params![day_key(yesterday), pause_id],
                )
                .map_err(|e| format!("Close pause: {e}"))?;
            }
            conn.execute(
                "UPDATE habits SET updated_at = ?1 WHERE id = ?2",
                params![now_iso(), id],
            )
            .map_err(|e| format!("Touch habit: {e}"))?;
        }
    }
    emit(&app, "habit:changed", json!({ "id": id.clone() }));
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    read_habit(&conn, &id)
}

#[tauri::command(async)]
fn delete_habit(id: String, state: State<'_, AppState>, app: AppHandle) -> Result<bool, String> {
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        conn.execute("DELETE FROM habits WHERE id = ?1", params![id])
            .map_err(|e| format!("Delete habit: {e}"))?;
    }
    emit(&app, "habit:changed", json!({ "id": id }));
    Ok(true)
}

/// Tick or untick one habit-day. Rejected for days the habit cannot own:
/// before its start, in the future, or inside a pause — the statistics stay
/// honest because dishonest rows cannot exist.
#[tauri::command(async)]
fn set_check(
    id: String,
    day: String,
    checked: bool,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<bool, String> {
    let on = parse_day(&day)?;
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let habit = read_habit(&conn, &id)?;
        let started = parse_day(&habit.started_on)?;
        let spans = pause_spans(&habit.pauses)?;
        if on > today_local() {
            return Err("Cannot check a day that has not happened yet".into());
        }
        if !eligible_on(on, started, habit.days_mask, &spans) {
            return Err(format!("{day} is not a trackable day for this habit"));
        }
        if checked {
            conn.execute(
                "INSERT OR IGNORE INTO checks (habit_id, on_day, created_at) VALUES (?1, ?2, ?3)",
                params![id, day, now_iso()],
            )
            .map_err(|e| format!("Set check: {e}"))?;
        } else {
            conn.execute(
                "DELETE FROM checks WHERE habit_id = ?1 AND on_day = ?2",
                params![id, day],
            )
            .map_err(|e| format!("Clear check: {e}"))?;
        }
    }
    emit(&app, "habit:changed", json!({ "id": id, "day": day }));
    Ok(true)
}

/// Every check in one year, across all habits — the year page's one data load.
#[tauri::command(async)]
fn year_checks(year: i32, state: State<'_, AppState>) -> Result<Vec<Check>, String> {
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    let mut stmt = conn
        .prepare("SELECT habit_id, on_day FROM checks WHERE on_day BETWEEN ?1 AND ?2 ORDER BY on_day")
        .map_err(|e| format!("Prepare checks: {e}"))?;
    let rows = stmt
        .query_map(params![format!("{year:04}-01-01"), format!("{year:04}-12-31")], |row| {
            Ok(Check { habit_id: row.get(0)?, on_day: row.get(1)? })
        })
        .map_err(|e| format!("Query checks: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Read checks: {e}"))?;
    Ok(rows)
}

#[tauri::command(async)]
fn stats(year: i32, state: State<'_, AppState>) -> Result<StatsOut, String> {
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    let habits = read_all_habits(&conn)?;
    let today = today_local();
    let jan1 = NaiveDate::from_ymd_opt(year, 1, 1).ok_or("Bad year")?;
    let dec31 = NaiveDate::from_ymd_opt(year, 12, 31).ok_or("Bad year")?;
    let to = today.min(dec31);

    let mut out = Vec::with_capacity(habits.len());
    let mut total_checks = 0_i64;
    for habit in habits {
        let started = parse_day(&habit.started_on)?;
        let spans = pause_spans(&habit.pauses)?;
        let checks: HashSet<NaiveDate> = {
            let mut stmt = conn
                .prepare("SELECT on_day FROM checks WHERE habit_id = ?1")
                .map_err(|e| format!("Prepare habit checks: {e}"))?;
            let days = stmt
                .query_map(params![habit.id], |row| row.get::<_, String>(0))
                .map_err(|e| format!("Query habit checks: {e}"))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("Read habit checks: {e}"))?;
            days.iter().map(|d| parse_day(d)).collect::<Result<_, _>>()?
        };

        let (eligible_days, checked_days) =
            year_counts(started, habit.days_mask, &spans, &checks, jan1, to);
        let (current_streak, best_streak) =
            streaks(started, habit.days_mask, &spans, &checks, today);
        total_checks += checked_days;
        out.push(HabitStats {
            id: habit.id,
            name: habit.name,
            is_paused: habit.is_paused,
            started_on: habit.started_on,
            eligible_days,
            checked_days,
            rate: (eligible_days > 0)
                .then(|| (checked_days as f64 / eligible_days as f64 * 100.0).round() as i64),
            current_streak,
            best_streak,
        });
    }

    Ok(StatsOut { year, habits: out, total_checks })
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
/// main thread and a slow disk freezes the window (ARCHITECTURE.md §11).
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

// ─── Entry point ──────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Build the `habit` plugin. Pinned to `Wry` for the same reason as
/// `finance`: bare `tauri::AppHandle` already means `AppHandle<Wry>`.
pub fn init() -> TauriPlugin<Wry> {
    Builder::<Wry>::new("habit")
        .setup(|app, _api| {
            let dir = data_dir();
            fs::create_dir_all(&dir)?;

            let conn = Connection::open(db_path(&dir))?;
            conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
            init_schema(&conn).map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

            let settings = load_app_settings(&dir);
            app.manage(AppState {
                data_dir: dir,
                db: Arc::new(Mutex::new(conn)),
                app_settings: Mutex::new(settings),
            });

            Ok(())
        })
        .invoke_handler(qs_core::diagnostics::traced(tauri::generate_handler![
            list_habits,
            create_habit,
            update_habit,
            pause_habit,
            resume_habit,
            delete_habit,
            set_check,
            year_checks,
            stats,
            get_app_settings,
            update_app_settings,
        ]))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        parse_day(s).unwrap()
    }

    fn set(days: &[&str]) -> HashSet<NaiveDate> {
        days.iter().map(|s| d(s)).collect()
    }

    #[test]
    fn a_pause_covers_its_interval_inclusively() {
        let pauses = vec![(d("2026-09-03"), Some(d("2026-09-05")))];
        assert!(!is_paused_on(d("2026-09-02"), &pauses));
        assert!(is_paused_on(d("2026-09-03"), &pauses));
        assert!(is_paused_on(d("2026-09-05"), &pauses));
        assert!(!is_paused_on(d("2026-09-06"), &pauses));
    }

    #[test]
    fn an_open_pause_covers_everything_from_its_start() {
        let pauses = vec![(d("2026-09-03"), None)];
        assert!(!is_paused_on(d("2026-09-02"), &pauses));
        assert!(is_paused_on(d("2026-12-31"), &pauses));
    }

    #[test]
    fn days_before_the_start_are_never_eligible() {
        assert!(!eligible_on(d("2026-08-31"), d("2026-09-01"), ALL_DAYS, &[]));
        assert!(eligible_on(d("2026-09-01"), d("2026-09-01"), ALL_DAYS, &[]));
    }

    /// Bit 0 = Monday. 2026-08-31 is a Monday.
    #[test]
    fn the_weekday_mask_gates_eligibility() {
        let weekdays_only = 0b001_1111; // Mo–Fr
        assert!(tracked_on(d("2026-08-31"), weekdays_only)); // Monday
        assert!(tracked_on(d("2026-09-04"), weekdays_only)); // Friday
        assert!(!tracked_on(d("2026-09-05"), weekdays_only)); // Saturday
        assert!(!tracked_on(d("2026-09-06"), weekdays_only)); // Sunday
        assert!(!eligible_on(d("2026-09-05"), d("2026-08-31"), weekdays_only, &[]));
    }

    #[test]
    fn an_empty_schedule_is_rejected_and_extra_bits_are_clamped() {
        assert!(normalize_mask(0).is_err());
        assert!(normalize_mask(1 << 8).is_err()); // only ghost bits
        assert_eq!(normalize_mask(ALL_DAYS | (1 << 9)).unwrap(), ALL_DAYS);
        assert_eq!(normalize_mask(0b101).unwrap(), 0b101);
    }

    /// A paused day does not break a run — "not tracked" is not "failed".
    #[test]
    fn a_streak_continues_across_a_pause() {
        let started = d("2026-09-01");
        let pauses = vec![(d("2026-09-03"), Some(d("2026-09-04")))];
        let checks = set(&["2026-09-01", "2026-09-02", "2026-09-05", "2026-09-06"]);
        let (current, best) = streaks(started, ALL_DAYS, &pauses, &checks, d("2026-09-06"));
        assert_eq!(current, 4);
        assert_eq!(best, 4);
    }

    /// A Mo/We/Fr habit checked Mo, We, Fr has a 3-streak — the untracked
    /// days in between are skipped, not failed.
    #[test]
    fn a_streak_continues_across_untracked_weekdays() {
        let mo_we_fr = 0b001_0101;
        let started = d("2026-08-31"); // Monday
        let checks = set(&["2026-08-31", "2026-09-02", "2026-09-04"]);
        let (current, best) = streaks(started, mo_we_fr, &[], &checks, d("2026-09-04"));
        assert_eq!(current, 3);
        assert_eq!(best, 3);
    }

    #[test]
    fn an_unchecked_eligible_day_breaks_the_streak() {
        let started = d("2026-09-01");
        let checks = set(&["2026-09-01", "2026-09-02", "2026-09-04", "2026-09-05"]);
        let (current, best) = streaks(started, ALL_DAYS, &[], &checks, d("2026-09-05"));
        assert_eq!(current, 2);
        assert_eq!(best, 2);
    }

    /// Today unchecked is "not done YET", not a failure — the run as of
    /// yesterday stands until tomorrow.
    #[test]
    fn an_unchecked_today_does_not_end_the_current_streak() {
        let started = d("2026-09-01");
        let checks = set(&["2026-09-01", "2026-09-02", "2026-09-03"]);
        let (current, best) = streaks(started, ALL_DAYS, &[], &checks, d("2026-09-04"));
        assert_eq!(current, 3);
        assert_eq!(best, 3);
    }

    #[test]
    fn a_checked_today_counts_immediately() {
        let started = d("2026-09-01");
        let checks = set(&["2026-09-03", "2026-09-04"]);
        let (current, best) = streaks(started, ALL_DAYS, &[], &checks, d("2026-09-04"));
        assert_eq!(current, 2);
        assert_eq!(best, 2);
    }

    /// Paused today: the streak is whatever it was on the last eligible day.
    #[test]
    fn a_paused_today_freezes_the_streak() {
        let started = d("2026-09-01");
        let pauses = vec![(d("2026-09-04"), None)];
        let checks = set(&["2026-09-01", "2026-09-02", "2026-09-03"]);
        let (current, best) = streaks(started, ALL_DAYS, &pauses, &checks, d("2026-09-10"));
        assert_eq!(current, 3);
        assert_eq!(best, 3);
    }

    #[test]
    fn year_counts_exclude_paused_days_from_both_sides() {
        let started = d("2026-09-01");
        let pauses = vec![(d("2026-09-03"), Some(d("2026-09-04")))];
        let checks = set(&["2026-09-01", "2026-09-02", "2026-09-05"]);
        let (eligible, checked) =
            year_counts(started, ALL_DAYS, &pauses, &checks, d("2026-01-01"), d("2026-09-06"));
        // 01, 02, 05, 06 are eligible; 03 and 04 are paused.
        assert_eq!(eligible, 4);
        assert_eq!(checked, 3);
    }

    #[test]
    fn year_counts_skip_untracked_weekdays() {
        let weekdays_only = 0b001_1111; // Mo–Fr
        let started = d("2026-08-31"); // Monday
        let checks = set(&["2026-08-31", "2026-09-05"]); // Sa check could not exist, but be safe
        let (eligible, checked) =
            year_counts(started, weekdays_only, &[], &checks, d("2026-01-01"), d("2026-09-06"));
        // Mo–Fr of that week; Sa/Su fall out.
        assert_eq!(eligible, 5);
        assert_eq!(checked, 1);
    }

    #[test]
    fn year_counts_start_no_earlier_than_the_habit() {
        let (eligible, checked) = year_counts(
            d("2026-12-30"),
            ALL_DAYS,
            &[],
            &set(&["2026-12-30"]),
            d("2026-01-01"),
            d("2026-12-31"),
        );
        assert_eq!(eligible, 2);
        assert_eq!(checked, 1);
    }
}
