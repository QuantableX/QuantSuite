//! Deploy history (round 4, card 4): every preview and production deploy of a
//! project, with its commit, URL, result and log. Schema 28. A submodule of
//! `db` so it shares the one connection.

use super::Database;
use crate::deploy::DeployRun;
use rusqlite::{params, Row};

pub(crate) const DEPLOY_TABLES: &str = "
    CREATE TABLE IF NOT EXISTS deploy_runs (
        id TEXT PRIMARY KEY,
        project TEXT NOT NULL,
        target_id TEXT NOT NULL,
        target_name TEXT NOT NULL,
        stage TEXT NOT NULL,
        commit_id TEXT NOT NULL DEFAULT '',
        dirty INTEGER NOT NULL DEFAULT 0,
        status TEXT NOT NULL,
        url TEXT NOT NULL DEFAULT '',
        log TEXT NOT NULL DEFAULT '',
        started_at INTEGER NOT NULL,
        finished_at INTEGER
    );
    CREATE INDEX IF NOT EXISTS deploy_runs_project ON deploy_runs(project, started_at);
";

const COLS: &str = "id, project, target_id, target_name, stage, commit_id, dirty, status, url, log, started_at, finished_at";

fn row(r: &Row<'_>) -> rusqlite::Result<DeployRun> {
    Ok(DeployRun {
        id: r.get(0)?,
        project: r.get(1)?,
        target_id: r.get(2)?,
        target_name: r.get(3)?,
        stage: r.get(4)?,
        commit_id: r.get(5)?,
        dirty: r.get::<_, i64>(6)? != 0,
        status: r.get(7)?,
        url: r.get(8)?,
        log: r.get(9)?,
        started_at: r.get(10)?,
        finished_at: r.get(11)?,
    })
}

impl Database {
    pub fn deploy_save(&self, run: &DeployRun) -> rusqlite::Result<()> {
        self.lock().execute(
            &format!(
                "INSERT INTO deploy_runs ({COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT(id) DO UPDATE SET status = ?8, url = ?9, log = ?10, finished_at = ?12"
            ),
            params![run.id, run.project, run.target_id, run.target_name, run.stage, run.commit_id, run.dirty as i64, run.status, run.url, run.log, run.started_at, run.finished_at],
        )?;
        Ok(())
    }

    /// Newest first.
    pub fn deploy_runs(&self, project: &str, limit: i64) -> rusqlite::Result<Vec<DeployRun>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM deploy_runs WHERE project = ?1 ORDER BY started_at DESC, rowid DESC LIMIT ?2"))?;
        let rows = stmt.query_map(params![project, limit], row)?;
        rows.collect()
    }

    /// A run that was going when QuantSuite closed did not finish as far as we
    /// know: it must not count as a good preview.
    pub fn deploy_mark_interrupted(&self, at: i64) -> rusqlite::Result<()> {
        self.lock().execute(
            "UPDATE deploy_runs SET status = 'failed', log = log || '\nQuantSuite closed while this was running; its result is unknown. Check the host.', finished_at = ?1 WHERE status = 'running'",
            params![at],
        )?;
        Ok(())
    }
}
