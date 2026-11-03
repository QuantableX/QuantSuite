//! Scheduled prompts (round 3, card 03): a prompt that runs on its own in a
//! project — once, every day, on weekdays, on chosen days or every N hours —
//! while the suite runs. One task sleeps until the next slot (at most a
//! minute, so a wall-clock jump after sleep or hibernation is noticed); a
//! change to any schedule wakes it.
//!
//! Times are local wall-clock times. A time the clocks skip when daylight
//! saving starts runs one hour later (02:30 → 03:30); a time they pass twice
//! when it ends runs once, at the first. "Every N hours" counts real hours
//! from its start, so it keeps its spacing across a change.
//!
//! A slot runs at most once: its run is recorded before it starts, and
//! (schedule, slot) is unique. A slot found more than two minutes late — the
//! suite was closed or the machine asleep — follows the schedule's missed-run
//! policy: `skip` records it as missed, `once` runs it once now. However many
//! slots passed, only the latest counts. A run is skipped while the previous
//! run's thread is still working.

use crate::events::ItemKind;
use crate::threads::{self, CreateThread};
use crate::{now, PilotState};
use chrono::{DateTime, Datelike, Duration, Local, LocalResult, NaiveDateTime, NaiveTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State};

/// A slot found later than this follows the missed-run policy.
pub const GRACE_MS: i64 = 2 * 60_000;
/// The engine looks again at least this often (wall-clock jumps).
const MAX_WAIT: std::time::Duration = std::time::Duration::from_secs(60);
const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 24 * HOUR_MS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Recurrence {
    /// Once, at a local date and time (`2026-11-04T08:00`).
    Once { at: String },
    /// Every day at a local time (`08:00`).
    Daily { time: String },
    /// Monday to Friday.
    Weekdays { time: String },
    /// Chosen days, 1 = Monday … 7 = Sunday.
    Days { days: Vec<u8>, time: String },
    /// Every `every` hours from a local start.
    Hours { every: u32, from: String },
}

impl Default for Recurrence {
    fn default() -> Self {
        Recurrence::Daily { time: "09:00".into() }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Schedule {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub context_id: String,
    pub provider: String,
    pub model: String,
    pub effort: String,
    pub mode: String,
    /// The agent account (round 3, card 02); "" = the default one.
    pub instance: String,
    pub recurrence: Recurrence,
    pub enabled: bool,
    /// `skip` | `once` — what a slot found late does.
    pub missed: String,
    /// `new` (a thread per run) | `same` (always the same thread).
    pub target: String,
    /// The `same` target's thread, once it exists.
    pub thread_id: Option<String>,
    /// Slots up to here are done; new and re-enabled schedules start from now.
    pub last_slot: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    /// For the UI: when it runs next (none: paused or nothing left).
    pub next_due: Option<i64>,
    pub last_run: Option<ScheduleRun>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ScheduleRun {
    pub id: String,
    pub schedule_id: String,
    /// The slot it ran for (a manual run: when it was asked for).
    pub slot: i64,
    pub at: i64,
    /// `starting` | `started` | `skipped` | `missed` | `failed`.
    pub status: String,
    pub thread_id: Option<String>,
    pub note: String,
    /// Run now, not a slot.
    pub manual: bool,
}

// ── Time ──

fn parse_time(s: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(s.trim(), "%H:%M").ok()
}

fn parse_local(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%dT%H:%M").ok()
}

/// A local wall-clock time as an instant (Unix ms), with the DST rules above.
pub fn local_instant<Tz: TimeZone>(tz: &Tz, t: NaiveDateTime) -> Option<i64> {
    match tz.from_local_datetime(&t) {
        LocalResult::Single(d) => Some(d.timestamp_millis()),
        LocalResult::Ambiguous(first, _) => Some(first.timestamp_millis()),
        // Skipped: one hour later, else the first valid minute after the gap.
        LocalResult::None => tz
            .from_local_datetime(&(t + Duration::hours(1)))
            .earliest()
            .or_else(|| (1..=180).find_map(|m| tz.from_local_datetime(&(t + Duration::minutes(m))).earliest()))
            .map(|d| d.timestamp_millis()),
    }
}

/// Monday-first weekdays a rule runs on.
fn day_set(rec: &Recurrence) -> [bool; 7] {
    match rec {
        Recurrence::Daily { .. } => [true; 7],
        Recurrence::Weekdays { .. } => [true, true, true, true, true, false, false],
        Recurrence::Days { days, .. } => {
            let mut set = [false; 7];
            for d in days.iter().filter(|d| (1..=7).contains(*d)) {
                set[usize::from(*d - 1)] = true;
            }
            set
        }
        _ => [false; 7],
    }
}

/// The first slot strictly after `after`.
pub fn next_after<Tz: TimeZone>(rec: &Recurrence, after: i64, tz: &Tz) -> Option<i64> {
    match rec {
        Recurrence::Once { at } => local_instant(tz, parse_local(at)?).filter(|s| *s > after),
        Recurrence::Hours { every, from } => {
            let start = local_instant(tz, parse_local(from)?)?;
            let step = i64::from((*every).clamp(1, 168)) * HOUR_MS;
            if after < start {
                return Some(start);
            }
            Some(start + ((after - start) / step + 1) * step)
        }
        Recurrence::Daily { time } | Recurrence::Weekdays { time } | Recurrence::Days { time, .. } => {
            let t = parse_time(time)?;
            let days = day_set(rec);
            if !days.contains(&true) {
                return None;
            }
            let first = DateTime::<Utc>::from_timestamp_millis(after)?.with_timezone(tz).date_naive() - Duration::days(1);
            (0..10).filter_map(|i| {
                let d = first + Duration::days(i);
                if !days[d.weekday().num_days_from_monday() as usize] {
                    return None;
                }
                local_instant(tz, d.and_time(t)).filter(|s| *s > after)
            })
            .next()
        }
    }
}

/// The latest slot in (`after`, `upto`] — the one a late engine acts on.
pub fn last_due<Tz: TimeZone>(rec: &Recurrence, after: i64, upto: i64, tz: &Tz) -> Option<i64> {
    if upto <= after {
        return None;
    }
    if let Recurrence::Hours { every, from } = rec {
        let start = local_instant(tz, parse_local(from)?)?;
        if upto < start {
            return None;
        }
        let step = i64::from((*every).clamp(1, 168)) * HOUR_MS;
        let slot = start + (upto - start) / step * step;
        return (slot > after).then_some(slot);
    }
    // Day rules have a slot at least weekly: the last eight days hold the latest.
    let mut cur = after.max(upto - 8 * DAY_MS);
    let mut last = None;
    while let Some(s) = next_after(rec, cur, tz).filter(|s| *s <= upto) {
        last = Some(s);
        cur = s;
    }
    last
}

/// Why a rule cannot be saved.
pub fn validate(rec: &Recurrence) -> Result<(), String> {
    let time = |t: &str| parse_time(t).map(|_| ()).ok_or_else(|| format!("\"{t}\" is not a time (HH:MM)"));
    let local = |t: &str| parse_local(t).map(|_| ()).ok_or_else(|| format!("\"{t}\" is not a date and time"));
    match rec {
        Recurrence::Once { at } => local(at),
        Recurrence::Daily { time: t } | Recurrence::Weekdays { time: t } => time(t),
        Recurrence::Days { days, time: t } => {
            if !days.iter().any(|d| (1..=7).contains(d)) {
                return Err("Pick at least one day.".into());
            }
            time(t)
        }
        Recurrence::Hours { every, from } => {
            if !(1..=168).contains(every) {
                return Err("Every 1 to 168 hours.".into());
            }
            local(from)
        }
    }
}

// ── The engine ──

/// Runs due slots, then sleeps until the next one or a change.
pub async fn scheduler(app: AppHandle) {
    loop {
        let Some(wake) = app.try_state::<PilotState>().map(|s| s.schedule_wake.clone()) else {
            return;
        };
        let handle = app.clone();
        let next = tauri::async_runtime::spawn_blocking(move || tick(&handle)).await.ok().flatten();
        let wait = next.map(|t| std::time::Duration::from_millis((t - now()).max(0) as u64)).unwrap_or(MAX_WAIT).min(MAX_WAIT);
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = wake.notified() => {}
        }
    }
}

/// One look at every schedule: what is due runs (or is recorded as missed);
/// returns the next slot.
fn tick(app: &AppHandle) -> Option<i64> {
    let state = app.try_state::<PilotState>()?;
    let list = state.db.schedules().ok()?;
    let mut changed = false;
    let mut next: Option<i64> = None;
    for s in list.iter().filter(|s| s.enabled) {
        let t = now();
        let since = s.last_slot.unwrap_or(s.created_at);
        if let Some(slot) = last_due(&s.recurrence, since, t, &Local) {
            changed = true;
            let late = t - slot > GRACE_MS;
            if late && s.missed != "once" {
                record_missed(&state, s, slot);
            } else {
                let _ = fire(app, &state, s, slot, false, late);
            }
        }
        if let Some(n) = next_after(&s.recurrence, t.max(since), &Local) {
            next = Some(next.map_or(n, |m: i64| m.min(n)));
        }
    }
    if changed {
        let _ = app.emit("pilot-schedules-changed", ());
    }
    next
}

fn record_missed(state: &PilotState, s: &Schedule, slot: i64) {
    let run = ScheduleRun {
        id: uuid::Uuid::new_v4().to_string(),
        schedule_id: s.id.clone(),
        slot,
        at: now(),
        status: "missed".into(),
        thread_id: None,
        note: format!("Due {} while QuantSuite was not running; skipped.", clock(slot)),
        manual: false,
    };
    if state.db.claim_schedule_run(&run).unwrap_or(false) {
        let _ = state.db.set_schedule_slot(&s.id, slot);
    }
}

/// "Nov 4 08:00" in local time.
fn clock(ms: i64) -> String {
    Local.timestamp_millis_opt(ms).single().map(|d| d.format("%b %-d %H:%M").to_string()).unwrap_or_default()
}

/// What a run did.
enum Outcome {
    Started(String),
    Skipped(String),
    Failed(Option<String>, String),
}

/// Runs a slot (or a manual run): recorded first, so it happens once.
pub(crate) fn fire(app: &AppHandle, state: &PilotState, s: &Schedule, slot: i64, manual: bool, late: bool) -> Result<ScheduleRun, String> {
    let at = now();
    let mut run = ScheduleRun {
        id: uuid::Uuid::new_v4().to_string(),
        schedule_id: s.id.clone(),
        slot: if manual { at } else { slot },
        at,
        status: "starting".into(),
        thread_id: None,
        note: String::new(),
        manual,
    };
    if !state.db.claim_schedule_run(&run).map_err(|e| e.to_string())? {
        return Err("This slot already ran.".into());
    }
    if !manual {
        let _ = state.db.set_schedule_slot(&s.id, slot);
    }
    let late_note = if late { format!("Due {} while QuantSuite was not running; ran once after it started.", clock(slot)) } else { String::new() };
    match start(app, state, s, manual) {
        Outcome::Started(id) => {
            run.status = "started".into();
            run.thread_id = Some(id);
            run.note = late_note;
        }
        Outcome::Skipped(why) => {
            run.status = "skipped".into();
            run.note = why;
        }
        Outcome::Failed(id, e) => {
            run.status = "failed".into();
            run.thread_id = id;
            run.note = e;
        }
    }
    let _ = state.db.update_schedule_run(&run);
    Ok(run)
}

fn busy(state: &PilotState, thread_id: &str) -> bool {
    state.runtime.active_turn(thread_id).is_some()
}

fn start(app: &AppHandle, state: &PilotState, s: &Schedule, manual: bool) -> Outcome {
    // Overlap: never a second run while the previous one still works.
    if let Some(prev) = state.db.last_started_schedule_run(&s.id).ok().flatten().and_then(|r| r.thread_id) {
        if busy(state, &prev) {
            return Outcome::Skipped("The previous run was still going.".into());
        }
    }
    let by = if manual { format!("Run now from the schedule \u{201c}{}\u{201d}.", s.name) } else { format!("Started by the schedule \u{201c}{}\u{201d}.", s.name) };
    if s.target == "same" {
        if let Some(id) = s.thread_id.as_deref().filter(|id| state.db.get_session(id).ok().flatten().is_some()) {
            return match threads::start_turn(app, state, id, s.prompt.clone(), Vec::new()) {
                Ok(turn) => {
                    state.runtime.append_item(id, &turn, ItemKind::Notice, json!({ "text": by }));
                    Outcome::Started(id.to_string())
                }
                Err(e) => Outcome::Failed(Some(id.to_string()), e),
            };
        }
    }
    let title = if s.target == "same" { s.name.clone() } else { format!("{} · {}", s.name, clock(now())) };
    let some = |v: &str| Some(v.trim().to_string()).filter(|v| !v.is_empty());
    let meta = match threads::create(
        app,
        state,
        CreateThread {
            context_id: Some(s.context_id.clone()),
            provider: some(&s.provider),
            model: some(&s.model),
            effort: some(&s.effort),
            mode: some(&s.mode),
            title: Some(title),
            instance: some(&s.instance),
            ..Default::default()
        },
    ) {
        Ok(m) => m,
        Err(e) => return Outcome::Failed(None, e),
    };
    let _ = app.emit("pilot-thread-created", &meta);
    if s.target == "same" {
        let _ = state.db.set_schedule_thread(&s.id, &meta.id);
    }
    match threads::start_turn(app, state, &meta.id, s.prompt.clone(), Vec::new()) {
        Ok(turn) => {
            state.runtime.append_item(&meta.id, &turn, ItemKind::Notice, json!({ "text": by }));
            Outcome::Started(meta.id)
        }
        Err(e) => Outcome::Failed(Some(meta.id), e),
    }
}

// ── Commands ──

fn with_status(state: &PilotState, mut s: Schedule) -> Schedule {
    let t = now();
    s.next_due = if s.enabled { next_after(&s.recurrence, t.max(s.last_slot.unwrap_or(s.created_at)), &Local) } else { None };
    s.last_run = state.db.schedule_runs(&s.id, 1).ok().and_then(|r| r.into_iter().next());
    s
}

fn changed(app: &AppHandle, state: &PilotState) {
    state.schedule_wake.notify_one();
    let _ = app.emit("pilot-schedules-changed", ());
}

#[tauri::command(async)]
pub fn pilot_schedules(state: State<'_, PilotState>) -> Result<Vec<Schedule>, String> {
    let list = state.db.schedules().map_err(|e| e.to_string())?;
    Ok(list.into_iter().map(|s| with_status(&state, s)).collect())
}

/// The name a schedule without one gets: its prompt's first line.
fn name_from(prompt: &str) -> String {
    let line = prompt.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("Scheduled prompt");
    let mut name: String = line.chars().take(48).collect();
    if line.chars().count() > 48 {
        name.push('…');
    }
    name
}

/// Creates (no id) or updates a schedule. A new or changed rule, and a
/// schedule switched on, start counting from now: nothing in the past runs.
pub(crate) fn save(state: &PilotState, mut s: Schedule) -> Result<Schedule, String> {
    s.prompt = s.prompt.trim().to_string();
    if s.prompt.is_empty() {
        return Err("The prompt is empty.".into());
    }
    validate(&s.recurrence)?;
    s.name = s.name.trim().to_string();
    if s.name.is_empty() {
        s.name = name_from(&s.prompt);
    }
    if s.context_id.trim().is_empty() {
        s.context_id = "general".into();
    }
    if !matches!(s.missed.as_str(), "skip" | "once") {
        s.missed = "skip".into();
    }
    if !matches!(s.target.as_str(), "new" | "same") {
        s.target = "new".into();
    }
    let t = now();
    let before = if s.id.is_empty() { None } else { state.db.get_schedule(&s.id).map_err(|e| e.to_string())? };
    match &before {
        None => {
            if s.id.is_empty() {
                s.id = uuid::Uuid::new_v4().to_string();
            }
            s.created_at = t;
            s.last_slot = Some(t);
            s.thread_id = None;
        }
        Some(b) => {
            s.created_at = b.created_at;
            s.thread_id = if s.target == "same" && b.context_id == s.context_id { b.thread_id.clone() } else { None };
            s.last_slot = if b.recurrence != s.recurrence || (s.enabled && !b.enabled) { Some(t) } else { b.last_slot };
        }
    }
    if s.enabled && next_after(&s.recurrence, t, &Local).is_none() {
        return Err("That time has passed — pick one in the future.".into());
    }
    s.updated_at = t;
    state.db.save_schedule(&s).map_err(|e| e.to_string())?;
    Ok(with_status(state, s))
}

#[tauri::command(async)]
pub fn pilot_schedule_save(app: AppHandle, state: State<'_, PilotState>, schedule: Schedule) -> Result<Schedule, String> {
    let saved = save(&state, schedule)?;
    changed(&app, &state);
    Ok(saved)
}

#[tauri::command(async)]
pub fn pilot_schedule_set_enabled(app: AppHandle, state: State<'_, PilotState>, id: String, enabled: bool) -> Result<Schedule, String> {
    let s = state.db.get_schedule(&id).map_err(|e| e.to_string())?.ok_or("That schedule no longer exists.")?;
    let saved = save(&state, Schedule { enabled, ..s })?;
    changed(&app, &state);
    Ok(saved)
}

#[tauri::command(async)]
pub fn pilot_schedule_delete(app: AppHandle, state: State<'_, PilotState>, id: String) -> Result<(), String> {
    state.db.delete_schedule(&id).map_err(|e| e.to_string())?;
    changed(&app, &state);
    Ok(())
}

/// Run now: a manual run, outside the slots (the next slot still runs).
#[tauri::command(async)]
pub fn pilot_schedule_run_now(app: AppHandle, state: State<'_, PilotState>, id: String) -> Result<ScheduleRun, String> {
    let s = state.db.get_schedule(&id).map_err(|e| e.to_string())?.ok_or("That schedule no longer exists.")?;
    let run = fire(&app, &state, &s, 0, true, false)?;
    let _ = app.emit("pilot-schedules-changed", ());
    Ok(run)
}

#[tauri::command(async)]
pub fn pilot_schedule_runs(state: State<'_, PilotState>, id: String, limit: Option<i64>) -> Result<Vec<ScheduleRun>, String> {
    state.db.schedule_runs(&id, limit.unwrap_or(50).clamp(1, 500)).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::Europe::Berlin;

    /// Berlin wall-clock time as Unix ms.
    fn at(s: &str) -> i64 {
        local_instant(&Berlin, parse_local(s).unwrap()).unwrap()
    }
    fn show(ms: i64) -> String {
        DateTime::<Utc>::from_timestamp_millis(ms).unwrap().with_timezone(&Berlin).format("%a %Y-%m-%dT%H:%M %Z").to_string()
    }
    fn next(rec: &Recurrence, after: &str) -> String {
        show(next_after(rec, at(after), &Berlin).unwrap())
    }

    #[test]
    fn day_rules_find_their_next_slot() {
        let daily = Recurrence::Daily { time: "08:00".into() };
        assert_eq!(next(&daily, "2026-11-04T07:59"), "Wed 2026-11-04T08:00 CET");
        assert_eq!(next(&daily, "2026-11-04T08:00"), "Thu 2026-11-05T08:00 CET", "strictly after");
        let weekdays = Recurrence::Weekdays { time: "08:00".into() };
        assert_eq!(next(&weekdays, "2026-11-06T09:00"), "Mon 2026-11-09T08:00 CET", "Friday after eight → Monday");
        let days = Recurrence::Days { days: vec![3, 7], time: "18:30".into() };
        assert_eq!(next(&days, "2026-11-04T19:00"), "Sun 2026-11-08T18:30 CET");
        assert_eq!(next(&days, "2026-11-08T19:00"), "Wed 2026-11-11T18:30 CET");
        assert_eq!(next_after(&Recurrence::Days { days: vec![], time: "08:00".into() }, 0, &Berlin), None);
        let once = Recurrence::Once { at: "2026-11-04T08:00".into() };
        assert_eq!(next(&once, "2026-11-04T07:00"), "Wed 2026-11-04T08:00 CET");
        assert_eq!(next_after(&once, at("2026-11-04T08:00"), &Berlin), None, "a one-off runs once");
    }

    #[test]
    fn every_n_hours_counts_from_its_start() {
        let rec = Recurrence::Hours { every: 6, from: "2026-11-04T08:00".into() };
        assert_eq!(next(&rec, "2026-11-01T00:00"), "Wed 2026-11-04T08:00 CET", "before the start: the start");
        assert_eq!(next(&rec, "2026-11-04T08:00"), "Wed 2026-11-04T14:00 CET");
        assert_eq!(next(&rec, "2026-11-04T23:00"), "Thu 2026-11-05T02:00 CET");
        // Real hours across the autumn change: 18:00 CEST + 12 h = 05:00 CET.
        let dst = Recurrence::Hours { every: 12, from: "2026-10-24T18:00".into() };
        assert_eq!(next(&dst, "2026-10-24T19:00"), "Sun 2026-10-25T05:00 CET");
    }

    #[test]
    fn daylight_saving_skips_and_doubles_are_handled() {
        // 29 March 2026: 02:00 → 03:00. 02:30 does not exist and runs at 03:30.
        let spring = Recurrence::Daily { time: "02:30".into() };
        assert_eq!(next(&spring, "2026-03-29T01:00"), "Sun 2026-03-29T03:30 CEST");
        assert_eq!(next(&spring, "2026-03-29T04:00"), "Mon 2026-03-30T02:30 CEST");
        // 25 October 2026: 03:00 → 02:00. 02:30 happens twice and runs once, at the first.
        let autumn = Recurrence::Daily { time: "02:30".into() };
        let first = next_after(&autumn, at("2026-10-25T01:00"), &Berlin).unwrap();
        assert_eq!(show(first), "Sun 2026-10-25T02:30 CEST");
        assert_eq!(show(next_after(&autumn, first, &Berlin).unwrap()), "Mon 2026-10-26T02:30 CET", "not again an hour later");
        // Wall-clock times stay put across the change.
        let daily = Recurrence::Daily { time: "08:00".into() };
        assert_eq!(next(&daily, "2026-10-24T09:00"), "Sun 2026-10-25T08:00 CET");
    }

    #[test]
    fn a_late_engine_acts_on_the_latest_slot_only() {
        let daily = Recurrence::Daily { time: "08:00".into() };
        // Closed from Monday evening to Thursday noon: only Thursday 08:00 counts.
        let last = last_due(&daily, at("2026-11-02T20:00"), at("2026-11-05T12:00"), &Berlin).unwrap();
        assert_eq!(show(last), "Thu 2026-11-05T08:00 CET");
        assert_eq!(last_due(&daily, at("2026-11-05T08:00"), at("2026-11-05T12:00"), &Berlin), None, "a done slot is done");
        // Months away: still the latest, without walking every day.
        let long = last_due(&daily, at("2025-01-01T00:00"), at("2026-11-05T07:00"), &Berlin).unwrap();
        assert_eq!(show(long), "Wed 2026-11-04T08:00 CET");
        let hours = Recurrence::Hours { every: 4, from: "2026-11-04T00:00".into() };
        assert_eq!(show(last_due(&hours, at("2026-11-04T01:00"), at("2026-11-04T13:59"), &Berlin).unwrap()), "Wed 2026-11-04T12:00 CET");
        assert_eq!(last_due(&hours, at("2026-11-04T12:00"), at("2026-11-04T13:59"), &Berlin), None);
    }

    #[test]
    fn rules_are_checked_before_saving() {
        assert!(validate(&Recurrence::Daily { time: "8 am".into() }).is_err());
        assert!(validate(&Recurrence::Days { days: vec![0, 8], time: "08:00".into() }).is_err());
        assert!(validate(&Recurrence::Hours { every: 0, from: "2026-11-04T08:00".into() }).is_err());
        assert!(validate(&Recurrence::Once { at: "tomorrow".into() }).is_err());
        assert!(validate(&Recurrence::Weekdays { time: "07:45".into() }).is_ok());
        assert_eq!(name_from("\n  Review the open PRs and summarize them for me please, with links\nmore"), "Review the open PRs and summarize them for me pl…");
        let json = serde_json::to_string(&Recurrence::Days { days: vec![1, 3], time: "08:00".into() }).unwrap();
        assert_eq!(json, r#"{"kind":"days","days":[1,3],"time":"08:00"}"#);
    }

    #[test]
    fn a_slot_is_claimed_once() {
        let db = crate::db::Database::open_in_memory().unwrap();
        let s = Schedule { id: "s1".into(), name: "n".into(), prompt: "p".into(), context_id: "general".into(), missed: "skip".into(), target: "new".into(), enabled: true, created_at: 1, updated_at: 1, ..Default::default() };
        db.save_schedule(&s).unwrap();
        let run = |id: &str, slot: i64, manual: bool| ScheduleRun { id: id.into(), schedule_id: "s1".into(), slot, at: slot, status: "starting".into(), manual, ..Default::default() };
        assert!(db.claim_schedule_run(&run("r1", 100, false)).unwrap());
        assert!(!db.claim_schedule_run(&run("r2", 100, false)).unwrap(), "the same slot never runs twice");
        assert!(db.claim_schedule_run(&run("r3", 100, true)).unwrap(), "run now is not a slot");
        assert!(db.claim_schedule_run(&run("r4", 200, false)).unwrap());
        db.set_schedule_slot("s1", 200).unwrap();
        db.set_schedule_slot("s1", 150).unwrap();
        assert_eq!(db.get_schedule("s1").unwrap().unwrap().last_slot, Some(200), "the done mark never moves back");
        let mut started = run("r4", 200, false);
        started.status = "started".into();
        started.thread_id = Some("t9".into());
        db.update_schedule_run(&started).unwrap();
        assert_eq!(db.last_started_schedule_run("s1").unwrap().unwrap().thread_id.as_deref(), Some("t9"));
        assert_eq!(db.schedule_runs("s1", 10).unwrap().len(), 3);
        db.delete_schedule("s1").unwrap();
        assert!(db.schedule_runs("s1", 10).unwrap().is_empty());
    }
}
