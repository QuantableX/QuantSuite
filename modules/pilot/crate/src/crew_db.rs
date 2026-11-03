//! Crew storage (round 2, card 07): a crew, its tasks, its members (the
//! orchestrator and every worker attempt) and its message log — the
//! "message bus". Schema 20. A submodule of `db` so it shares the one
//! connection.

use super::{now, Database};
use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

pub(crate) const CREW_TABLES: &str = "
    CREATE TABLE IF NOT EXISTS crews (
        id TEXT PRIMARY KEY,
        orchestrator_id TEXT NOT NULL,
        context_id TEXT NOT NULL,
        objective TEXT NOT NULL DEFAULT '',
        status TEXT NOT NULL DEFAULT 'running',
        max_parallel INTEGER NOT NULL DEFAULT 0,
        allow_peers INTEGER NOT NULL DEFAULT 0,
        rev INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS crews_orchestrator ON crews(orchestrator_id);
    CREATE TABLE IF NOT EXISTS crew_tasks (
        id TEXT PRIMARY KEY,
        crew_id TEXT NOT NULL,
        key TEXT NOT NULL,
        spec TEXT NOT NULL,
        deps TEXT NOT NULL DEFAULT '[]',
        card_id TEXT,
        provider TEXT NOT NULL DEFAULT '',
        model TEXT NOT NULL DEFAULT '',
        worktree INTEGER NOT NULL DEFAULT 1,
        timeout_min INTEGER NOT NULL DEFAULT 0,
        status TEXT NOT NULL DEFAULT 'waiting',
        progress INTEGER NOT NULL DEFAULT 0,
        note TEXT NOT NULL DEFAULT '',
        worker_id TEXT,
        attempts INTEGER NOT NULL DEFAULT 0,
        summary TEXT NOT NULL DEFAULT '',
        result TEXT NOT NULL DEFAULT '',
        error TEXT NOT NULL DEFAULT '',
        started_at INTEGER,
        finished_at INTEGER,
        sort INTEGER NOT NULL DEFAULT 0,
        brief TEXT NOT NULL DEFAULT '',
        UNIQUE(crew_id, key)
    );
    CREATE INDEX IF NOT EXISTS crew_tasks_worker ON crew_tasks(worker_id);
    CREATE TABLE IF NOT EXISTS crew_members (
        crew_id TEXT NOT NULL,
        thread_id TEXT NOT NULL,
        task_id TEXT,
        role TEXT NOT NULL,
        attempt INTEGER NOT NULL DEFAULT 0,
        status TEXT NOT NULL DEFAULT 'active',
        provider TEXT NOT NULL DEFAULT '',
        joined_at INTEGER NOT NULL,
        PRIMARY KEY (crew_id, thread_id)
    );
    CREATE INDEX IF NOT EXISTS crew_members_thread ON crew_members(thread_id);
    CREATE TABLE IF NOT EXISTS crew_messages (
        id TEXT PRIMARY KEY,
        crew_id TEXT NOT NULL,
        task_id TEXT,
        from_id TEXT NOT NULL,
        to_id TEXT NOT NULL,
        kind TEXT NOT NULL,
        text TEXT NOT NULL,
        reply_to TEXT,
        status TEXT NOT NULL DEFAULT '',
        at INTEGER NOT NULL,
        n INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX IF NOT EXISTS crew_messages_crew ON crew_messages(crew_id, n);
";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Crew {
    pub id: String,
    pub orchestrator_id: String,
    pub context_id: String,
    pub objective: String,
    /// `running` | `stopped` | `done`.
    pub status: String,
    /// Workers at once; 0 = the setting.
    pub max_parallel: i64,
    /// Workers may message each other (with `message_thread`), not only the orchestrator.
    pub allow_peers: bool,
    /// Bumped on every change: `crew_wait` and the UI compare it.
    pub rev: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CrewTask {
    pub id: String,
    pub crew_id: String,
    /// Short name the orchestrator gave it: `billing`, `queue`.
    pub key: String,
    pub spec: String,
    /// Keys of the tasks that must be done first.
    pub deps: Vec<String>,
    pub card_id: Option<String>,
    /// Agent id; empty = the orchestrator's.
    pub provider: String,
    pub model: String,
    /// The worker gets its own git worktree.
    pub worktree: bool,
    /// Minutes before a worker is stopped; 0 = the setting.
    pub timeout_min: i64,
    /// `waiting` | `running` | `asking` | `stalled` | `done` | `failed` | `stopped`.
    pub status: String,
    pub progress: i64,
    /// The worker's latest progress note, or why the task waits.
    pub note: String,
    pub worker_id: Option<String>,
    pub attempts: i64,
    pub summary: String,
    pub result: String,
    pub error: String,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub sort: i64,
    /// Said to the next worker of this task only: why the last attempt
    /// ended, the user's or orchestrator's note (a retry).
    pub brief: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CrewMember {
    pub crew_id: String,
    pub thread_id: String,
    pub task_id: Option<String>,
    /// `orchestrator` | `worker`.
    pub role: String,
    pub attempt: i64,
    /// `active` | `done` | `failed` | `stopped` | `replaced`.
    pub status: String,
    pub provider: String,
    pub joined_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CrewMessage {
    pub id: String,
    pub crew_id: String,
    pub task_id: Option<String>,
    /// A thread id, `user` or `quantpilot`.
    pub from_id: String,
    /// A thread id, `user`, `crew` or `quantpilot`.
    pub to_id: String,
    /// `dispatch` | `steer` | `ask` | `reply` | `progress` | `done` | `failed` |
    /// `escalation` | `gate` | `decision` | `peer` | `stop` | `retry` | `note`.
    pub kind: String,
    pub text: String,
    pub reply_to: Option<String>,
    /// A question: `open` | `answered`. A gate: `pending` | `approved` | `rejected`.
    pub status: String,
    pub at: i64,
    /// Order within the crew.
    pub n: i64,
}

const CREW_COLS: &str = "id, orchestrator_id, context_id, objective, status, max_parallel, allow_peers, rev, created_at, updated_at";
const TASK_COLS: &str = "id, crew_id, key, spec, deps, card_id, provider, model, worktree, timeout_min, status, progress, note, worker_id, attempts, summary, result, error, started_at, finished_at, sort, brief";
const MEMBER_COLS: &str = "crew_id, thread_id, task_id, role, attempt, status, provider, joined_at";
const MESSAGE_COLS: &str = "id, crew_id, task_id, from_id, to_id, kind, text, reply_to, status, at, n";

fn crew_row(r: &Row) -> rusqlite::Result<Crew> {
    Ok(Crew {
        id: r.get(0)?,
        orchestrator_id: r.get(1)?,
        context_id: r.get(2)?,
        objective: r.get(3)?,
        status: r.get(4)?,
        max_parallel: r.get(5)?,
        allow_peers: r.get::<_, i64>(6)? != 0,
        rev: r.get(7)?,
        created_at: r.get(8)?,
        updated_at: r.get(9)?,
    })
}

fn task_row(r: &Row) -> rusqlite::Result<CrewTask> {
    Ok(CrewTask {
        id: r.get(0)?,
        crew_id: r.get(1)?,
        key: r.get(2)?,
        spec: r.get(3)?,
        deps: serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or_default(),
        card_id: r.get(5)?,
        provider: r.get(6)?,
        model: r.get(7)?,
        worktree: r.get::<_, i64>(8)? != 0,
        timeout_min: r.get(9)?,
        status: r.get(10)?,
        progress: r.get(11)?,
        note: r.get(12)?,
        worker_id: r.get(13)?,
        attempts: r.get(14)?,
        summary: r.get(15)?,
        result: r.get(16)?,
        error: r.get(17)?,
        started_at: r.get(18)?,
        finished_at: r.get(19)?,
        sort: r.get(20)?,
        brief: r.get(21)?,
    })
}

fn member_row(r: &Row) -> rusqlite::Result<CrewMember> {
    Ok(CrewMember {
        crew_id: r.get(0)?,
        thread_id: r.get(1)?,
        task_id: r.get(2)?,
        role: r.get(3)?,
        attempt: r.get(4)?,
        status: r.get(5)?,
        provider: r.get(6)?,
        joined_at: r.get(7)?,
    })
}

fn message_row(r: &Row) -> rusqlite::Result<CrewMessage> {
    Ok(CrewMessage {
        id: r.get(0)?,
        crew_id: r.get(1)?,
        task_id: r.get(2)?,
        from_id: r.get(3)?,
        to_id: r.get(4)?,
        kind: r.get(5)?,
        text: r.get(6)?,
        reply_to: r.get(7)?,
        status: r.get(8)?,
        at: r.get(9)?,
        n: r.get(10)?,
    })
}

/// What changes on a task; `None` leaves a field as it is.
#[derive(Debug, Default, Clone)]
pub struct TaskPatch {
    pub status: Option<String>,
    pub progress: Option<i64>,
    pub note: Option<String>,
    pub worker_id: Option<Option<String>>,
    pub attempts: Option<i64>,
    pub summary: Option<String>,
    pub result: Option<String>,
    pub error: Option<String>,
    pub started_at: Option<Option<i64>>,
    pub finished_at: Option<Option<i64>>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub brief: Option<String>,
}

impl Database {
    pub fn crew_create(&self, orchestrator_id: &str, context_id: &str, objective: &str, max_parallel: i64, allow_peers: bool) -> rusqlite::Result<Crew> {
        let id = format!("crew-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
        let at = now();
        {
            let conn = self.lock();
            conn.execute(
                "INSERT INTO crews (id, orchestrator_id, context_id, objective, status, max_parallel, allow_peers, rev, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 'running', ?5, ?6, 1, ?7, ?7)",
                params![id, orchestrator_id, context_id, objective, max_parallel, allow_peers as i64, at],
            )?;
            conn.execute(
                "INSERT OR IGNORE INTO crew_members (crew_id, thread_id, task_id, role, attempt, status, provider, joined_at) VALUES (?1, ?2, NULL, 'orchestrator', 0, 'active', '', ?3)",
                params![id, orchestrator_id, at],
            )?;
        }
        Ok(self.crew(&id)?.expect("just inserted"))
    }

    pub fn crew(&self, id: &str) -> rusqlite::Result<Option<Crew>> {
        let conn = self.lock();
        conn.query_row(&format!("SELECT {CREW_COLS} FROM crews WHERE id = ?1"), params![id], crew_row).optional()
    }

    /// The crew a thread leads — its newest.
    pub fn crew_led_by(&self, orchestrator_id: &str) -> rusqlite::Result<Option<Crew>> {
        let conn = self.lock();
        conn.query_row(
            &format!("SELECT {CREW_COLS} FROM crews WHERE orchestrator_id = ?1 ORDER BY created_at DESC LIMIT 1"),
            params![orchestrator_id],
            crew_row,
        )
        .optional()
    }

    pub fn crews(&self) -> rusqlite::Result<Vec<Crew>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!("SELECT {CREW_COLS} FROM crews ORDER BY created_at"))?;
        let rows = stmt.query_map([], crew_row)?.collect();
        rows
    }

    pub fn crew_update(&self, id: &str, status: Option<&str>, objective: Option<&str>, max_parallel: Option<i64>, allow_peers: Option<bool>) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE crews SET status = COALESCE(?2, status), objective = COALESCE(?3, objective), max_parallel = COALESCE(?4, max_parallel),
             allow_peers = COALESCE(?5, allow_peers), rev = rev + 1, updated_at = ?6 WHERE id = ?1",
            params![id, status, objective, max_parallel, allow_peers.map(|b| b as i64), now()],
        )?;
        Ok(())
    }

    /// Something in the crew changed.
    pub fn crew_touch(&self, id: &str) -> rusqlite::Result<i64> {
        let conn = self.lock();
        conn.execute("UPDATE crews SET rev = rev + 1, updated_at = ?2 WHERE id = ?1", params![id, now()])?;
        conn.query_row("SELECT rev FROM crews WHERE id = ?1", params![id], |r| r.get(0))
    }

    /// A crew and everything in it (the orchestrator thread was deleted).
    pub fn crew_delete(&self, id: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute("DELETE FROM crew_messages WHERE crew_id = ?1", params![id])?;
        conn.execute("DELETE FROM crew_members WHERE crew_id = ?1", params![id])?;
        conn.execute("DELETE FROM crew_tasks WHERE crew_id = ?1", params![id])?;
        conn.execute("DELETE FROM crews WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn crew_task_add(&self, task: &CrewTask) -> rusqlite::Result<()> {
        let conn = self.lock();
        let sort: i64 = conn.query_row("SELECT COALESCE(MAX(sort), 0) + 1 FROM crew_tasks WHERE crew_id = ?1", params![task.crew_id], |r| r.get(0))?;
        conn.execute(
            &format!("INSERT INTO crew_tasks ({TASK_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)"),
            params![
                task.id,
                task.crew_id,
                task.key,
                task.spec,
                serde_json::to_string(&task.deps).unwrap_or_else(|_| "[]".into()),
                task.card_id,
                task.provider,
                task.model,
                task.worktree as i64,
                task.timeout_min,
                task.status,
                task.progress,
                task.note,
                task.worker_id,
                task.attempts,
                task.summary,
                task.result,
                task.error,
                task.started_at,
                task.finished_at,
                sort,
                task.brief
            ],
        )?;
        Ok(())
    }

    pub fn crew_tasks(&self, crew_id: &str) -> rusqlite::Result<Vec<CrewTask>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!("SELECT {TASK_COLS} FROM crew_tasks WHERE crew_id = ?1 ORDER BY sort"))?;
        let rows = stmt.query_map(params![crew_id], task_row)?.collect();
        rows
    }

    pub fn crew_task(&self, id: &str) -> rusqlite::Result<Option<CrewTask>> {
        let conn = self.lock();
        conn.query_row(&format!("SELECT {TASK_COLS} FROM crew_tasks WHERE id = ?1"), params![id], task_row).optional()
    }

    /// The task a thread works on as a worker — its newest.
    pub fn crew_task_of_worker(&self, thread_id: &str) -> rusqlite::Result<Option<CrewTask>> {
        let conn = self.lock();
        conn.query_row(&format!("SELECT {TASK_COLS} FROM crew_tasks WHERE worker_id = ?1 ORDER BY sort DESC LIMIT 1"), params![thread_id], task_row)
            .optional()
    }

    pub fn crew_task_update(&self, id: &str, p: &TaskPatch) -> rusqlite::Result<()> {
        let conn = self.lock();
        let set_worker = p.worker_id.is_some();
        let set_started = p.started_at.is_some();
        let set_finished = p.finished_at.is_some();
        conn.execute(
            "UPDATE crew_tasks SET
                status = COALESCE(?2, status), progress = COALESCE(?3, progress), note = COALESCE(?4, note),
                worker_id = CASE WHEN ?5 THEN ?6 ELSE worker_id END, attempts = COALESCE(?7, attempts),
                summary = COALESCE(?8, summary), result = COALESCE(?9, result), error = COALESCE(?10, error),
                started_at = CASE WHEN ?11 THEN ?12 ELSE started_at END,
                finished_at = CASE WHEN ?13 THEN ?14 ELSE finished_at END,
                provider = COALESCE(?15, provider), model = COALESCE(?16, model), brief = COALESCE(?17, brief)
             WHERE id = ?1",
            params![
                id,
                p.status,
                p.progress,
                p.note,
                set_worker,
                p.worker_id.clone().flatten(),
                p.attempts,
                p.summary,
                p.result,
                p.error,
                set_started,
                p.started_at.flatten(),
                set_finished,
                p.finished_at.flatten(),
                p.provider,
                p.model,
                p.brief
            ],
        )?;
        Ok(())
    }

    pub fn crew_member_add(&self, m: &CrewMember) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            &format!("INSERT OR REPLACE INTO crew_members ({MEMBER_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"),
            params![m.crew_id, m.thread_id, m.task_id, m.role, m.attempt, m.status, m.provider, m.joined_at],
        )?;
        Ok(())
    }

    pub fn crew_member_status(&self, crew_id: &str, thread_id: &str, status: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute("UPDATE crew_members SET status = ?3 WHERE crew_id = ?1 AND thread_id = ?2", params![crew_id, thread_id, status])?;
        Ok(())
    }

    pub fn crew_members(&self, crew_id: &str) -> rusqlite::Result<Vec<CrewMember>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!("SELECT {MEMBER_COLS} FROM crew_members WHERE crew_id = ?1 ORDER BY role = 'worker', joined_at"))?;
        let rows = stmt.query_map(params![crew_id], member_row)?.collect();
        rows
    }

    /// The crews a thread belongs to (as orchestrator or worker).
    #[cfg(test)]
    pub fn crews_of_thread(&self, thread_id: &str) -> rusqlite::Result<Vec<String>> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT DISTINCT crew_id FROM crew_members WHERE thread_id = ?1")?;
        let rows = stmt.query_map(params![thread_id], |r| r.get(0))?.collect();
        rows
    }

    #[allow(clippy::too_many_arguments)]
    pub fn crew_message_add(
        &self,
        crew_id: &str,
        task_id: Option<&str>,
        from_id: &str,
        to_id: &str,
        kind: &str,
        text: &str,
        reply_to: Option<&str>,
        status: &str,
    ) -> rusqlite::Result<CrewMessage> {
        let id = format!("m-{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
        let at = now();
        {
            let conn = self.lock();
            let n: i64 = conn.query_row("SELECT COALESCE(MAX(n), 0) + 1 FROM crew_messages WHERE crew_id = ?1", params![crew_id], |r| r.get(0))?;
            conn.execute(
                &format!("INSERT INTO crew_messages ({MESSAGE_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"),
                params![id, crew_id, task_id, from_id, to_id, kind, text, reply_to, status, at, n],
            )?;
            conn.execute("UPDATE crews SET rev = rev + 1, updated_at = ?2 WHERE id = ?1", params![crew_id, at])?;
        }
        Ok(self.crew_message(&id)?.expect("just inserted"))
    }

    pub fn crew_message(&self, id: &str) -> rusqlite::Result<Option<CrewMessage>> {
        let conn = self.lock();
        conn.query_row(&format!("SELECT {MESSAGE_COLS} FROM crew_messages WHERE id = ?1"), params![id], message_row).optional()
    }

    pub fn crew_message_status(&self, id: &str, status: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute("UPDATE crew_messages SET status = ?2 WHERE id = ?1", params![id, status])?;
        Ok(())
    }

    /// The log, oldest first; `after` = only messages after that number.
    pub fn crew_messages(&self, crew_id: &str, after: i64) -> rusqlite::Result<Vec<CrewMessage>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!("SELECT {MESSAGE_COLS} FROM crew_messages WHERE crew_id = ?1 AND n > ?2 ORDER BY n"))?;
        let rows = stmt.query_map(params![crew_id, after], message_row)?.collect();
        rows
    }

    /// Starts a waiting task when a slot is free. One statement, so two
    /// workers settling at once never start the same task twice.
    pub fn crew_task_claim(&self, id: &str, crew_id: &str, max_running: i64) -> rusqlite::Result<bool> {
        let conn = self.lock();
        let n = conn.execute(
            "UPDATE crew_tasks SET status = 'running' WHERE id = ?1 AND status = 'waiting'
             AND (SELECT COUNT(*) FROM crew_tasks WHERE crew_id = ?2 AND status = 'running') < ?3",
            params![id, crew_id, max_running],
        )?;
        Ok(n == 1)
    }

    /// Tasks of every crew whose worker is out working, asking or stalled.
    pub fn crew_tasks_active(&self) -> rusqlite::Result<Vec<CrewTask>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!("SELECT {TASK_COLS} FROM crew_tasks WHERE status IN ('running', 'asking', 'stalled') ORDER BY sort"))?;
        let rows = stmt.query_map([], task_row)?.collect();
        rows
    }

    /// Open questions and pending gates of a crew.
    pub fn crew_open_requests(&self, crew_id: &str) -> rusqlite::Result<Vec<CrewMessage>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {MESSAGE_COLS} FROM crew_messages WHERE crew_id = ?1 AND ((kind = 'ask' AND status = 'open') OR (kind = 'gate' AND status = 'pending')) ORDER BY n"
        ))?;
        let rows = stmt.query_map(params![crew_id], message_row)?.collect();
        rows
    }
}
