//! Schedule storage (round 3, card 03): scheduled prompts and their runs.
//! Schema 22. A submodule of `db` so it shares the one connection. A run's
//! (schedule, slot) is unique for timed runs — the dedupe that makes a slot
//! run at most once, however often the engine wakes.

use super::Database;
use crate::schedules::{Recurrence, Schedule, ScheduleRun};
use rusqlite::{params, OptionalExtension, Row};

pub(crate) const SCHEDULE_TABLES: &str = "
    CREATE TABLE IF NOT EXISTS schedules (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        prompt TEXT NOT NULL,
        context_id TEXT NOT NULL,
        provider TEXT NOT NULL DEFAULT '',
        model TEXT NOT NULL DEFAULT '',
        effort TEXT NOT NULL DEFAULT '',
        mode TEXT NOT NULL DEFAULT '',
        instance TEXT NOT NULL DEFAULT '',
        recurrence TEXT NOT NULL,
        enabled INTEGER NOT NULL DEFAULT 1,
        missed TEXT NOT NULL DEFAULT 'skip',
        target TEXT NOT NULL DEFAULT 'new',
        thread_id TEXT,
        last_slot INTEGER,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS schedule_runs (
        id TEXT PRIMARY KEY,
        schedule_id TEXT NOT NULL,
        slot INTEGER NOT NULL,
        at INTEGER NOT NULL,
        status TEXT NOT NULL,
        thread_id TEXT,
        note TEXT NOT NULL DEFAULT '',
        manual INTEGER NOT NULL DEFAULT 0
    );
    CREATE UNIQUE INDEX IF NOT EXISTS schedule_runs_slot ON schedule_runs(schedule_id, slot) WHERE manual = 0;
    CREATE INDEX IF NOT EXISTS schedule_runs_at ON schedule_runs(schedule_id, at);
";

const SCHEDULE_COLS: &str = "id, name, prompt, context_id, provider, model, effort, mode, instance, recurrence, enabled, missed, target, thread_id, last_slot, created_at, updated_at";
const RUN_COLS: &str = "id, schedule_id, slot, at, status, thread_id, note, manual";

fn schedule_row(r: &Row<'_>) -> rusqlite::Result<Schedule> {
    let recurrence: String = r.get(9)?;
    Ok(Schedule {
        id: r.get(0)?,
        name: r.get(1)?,
        prompt: r.get(2)?,
        context_id: r.get(3)?,
        provider: r.get(4)?,
        model: r.get(5)?,
        effort: r.get(6)?,
        mode: r.get(7)?,
        instance: r.get(8)?,
        // An unreadable rule (a newer build's) never runs: it reads as a past one-off.
        recurrence: serde_json::from_str(&recurrence).unwrap_or(Recurrence::Once { at: String::new() }),
        enabled: r.get::<_, i64>(10)? != 0,
        missed: r.get(11)?,
        target: r.get(12)?,
        thread_id: r.get(13)?,
        last_slot: r.get(14)?,
        created_at: r.get(15)?,
        updated_at: r.get(16)?,
        next_due: None,
        last_run: None,
    })
}

fn run_row(r: &Row<'_>) -> rusqlite::Result<ScheduleRun> {
    Ok(ScheduleRun {
        id: r.get(0)?,
        schedule_id: r.get(1)?,
        slot: r.get(2)?,
        at: r.get(3)?,
        status: r.get(4)?,
        thread_id: r.get(5)?,
        note: r.get(6)?,
        manual: r.get::<_, i64>(7)? != 0,
    })
}

impl Database {
    pub fn schedules(&self) -> rusqlite::Result<Vec<Schedule>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!("SELECT {SCHEDULE_COLS} FROM schedules ORDER BY created_at"))?;
        let rows = stmt.query_map([], schedule_row)?;
        rows.collect()
    }

    pub fn get_schedule(&self, id: &str) -> rusqlite::Result<Option<Schedule>> {
        let conn = self.lock();
        conn.query_row(&format!("SELECT {SCHEDULE_COLS} FROM schedules WHERE id = ?1"), params![id], schedule_row).optional()
    }

    pub fn save_schedule(&self, s: &Schedule) -> rusqlite::Result<()> {
        let recurrence = serde_json::to_string(&s.recurrence).unwrap_or_default();
        self.lock().execute(
            &format!(
                "INSERT INTO schedules ({SCHEDULE_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
                 ON CONFLICT(id) DO UPDATE SET name = ?2, prompt = ?3, context_id = ?4, provider = ?5, model = ?6, effort = ?7, mode = ?8,
                   instance = ?9, recurrence = ?10, enabled = ?11, missed = ?12, target = ?13, thread_id = ?14, last_slot = ?15, updated_at = ?17"
            ),
            params![
                s.id,
                s.name,
                s.prompt,
                s.context_id,
                s.provider,
                s.model,
                s.effort,
                s.mode,
                s.instance,
                recurrence,
                s.enabled as i64,
                s.missed,
                s.target,
                s.thread_id,
                s.last_slot,
                s.created_at,
                s.updated_at
            ],
        )?;
        Ok(())
    }

    /// A schedule and its history go; the threads its runs started stay.
    pub fn delete_schedule(&self, id: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute("DELETE FROM schedule_runs WHERE schedule_id = ?1", params![id])?;
        conn.execute("DELETE FROM schedules WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Slots up to `slot` are done (run, skipped or missed).
    pub fn set_schedule_slot(&self, id: &str, slot: i64) -> rusqlite::Result<()> {
        self.lock().execute("UPDATE schedules SET last_slot = max(coalesce(last_slot, 0), ?2) WHERE id = ?1", params![id, slot])?;
        Ok(())
    }

    pub fn set_schedule_thread(&self, id: &str, thread_id: &str) -> rusqlite::Result<()> {
        self.lock().execute("UPDATE schedules SET thread_id = ?2 WHERE id = ?1", params![id, thread_id])?;
        Ok(())
    }

    /// Records a run before it starts. False when its slot was already taken
    /// — the run must not happen twice.
    pub fn claim_schedule_run(&self, run: &ScheduleRun) -> rusqlite::Result<bool> {
        let n = self.lock().execute(
            &format!("INSERT OR IGNORE INTO schedule_runs ({RUN_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"),
            params![run.id, run.schedule_id, run.slot, run.at, run.status, run.thread_id, run.note, run.manual as i64],
        )?;
        Ok(n == 1)
    }

    pub fn update_schedule_run(&self, run: &ScheduleRun) -> rusqlite::Result<()> {
        self.lock().execute(
            "UPDATE schedule_runs SET status = ?2, thread_id = ?3, note = ?4 WHERE id = ?1",
            params![run.id, run.status, run.thread_id, run.note],
        )?;
        Ok(())
    }

    /// Newest first.
    pub fn schedule_runs(&self, schedule_id: &str, limit: i64) -> rusqlite::Result<Vec<ScheduleRun>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!("SELECT {RUN_COLS} FROM schedule_runs WHERE schedule_id = ?1 ORDER BY at DESC, rowid DESC LIMIT ?2"))?;
        let rows = stmt.query_map(params![schedule_id, limit], run_row)?;
        rows.collect()
    }

    /// The last run that started a thread — the one an overlap check looks at.
    pub fn last_started_schedule_run(&self, schedule_id: &str) -> rusqlite::Result<Option<ScheduleRun>> {
        let conn = self.lock();
        conn.query_row(
            &format!("SELECT {RUN_COLS} FROM schedule_runs WHERE schedule_id = ?1 AND status = 'started' AND thread_id IS NOT NULL ORDER BY at DESC, rowid DESC LIMIT 1"),
            params![schedule_id],
            run_row,
        )
        .optional()
    }
}
