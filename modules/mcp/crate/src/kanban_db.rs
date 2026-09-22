use std::str::FromStr;
use rusqlite::{params, Connection, Result as SqlResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Sentinel board id (PLAN-KANBAN-UNIFY): the suite-wide General board.
/// Not a registry entry — no folder, so the worktree lifecycle
/// (claim/complete/approve/reject) refuses cards on it.
pub const GENERAL_BOARD_ID: &str = "general";

// ── core.db read mirror (PLAN-KANBAN-UNIFY) ──────────────────────────────
//
// kanban.db is the single source of truth for every board (per-workspace
// and General). Each mutation below mirrors its card as a core.db entity
// (`core / kanban.card`) so Ctrl+K search and the links table keep seeing
// cards — surfaces render from the plugin commands, never from the mirror.
// The bus events the mirror emits double as the change signal the drawer
// and the workspaces page listen on.
// Best-effort: a mirror failure (unit tests, app not up) must never fail
// the kanban operation itself.

/// Entity id of a card's core.db mirror.
pub fn mirror_entity_id(card_id: &str) -> String {
    format!("core:kanban.card:mcp-{card_id}")
}

fn mirror_card(card: &KanbanCard) {
    let gid = mirror_entity_id(&card.id);
    // A cancelled or archived card leaves the mirror rather than cluttering
    // search — the archive is the board's history, listed on request
    // (`list_archived_cards`), not a Ctrl+K hit.
    if card.status == CardStatus::Cancelled || card.archived_at.is_some() {
        let _ = qs_core::runtime::delete_entity(&gid);
        return;
    }
    let _ = qs_core::runtime::upsert_entity(qs_core::db::Entity {
        id: gid,
        module: "core".into(),
        kind: "kanban.card".into(),
        title: card.title.clone(),
        subtitle: Some(card.column.as_str().into()),
        route: "/mcp".into(),
        icon: None,
        // kanban timestamps are seconds; the data plane uses millis.
        updated_at: card.updated_at as i64 * 1000,
        payload: Some(serde_json::json!({
            "column": card.column.as_str(),
            "status": card.status.as_str(),
            "description": card.description,
            "order": card.order,
            "createdAt": card.created_at * 1000,
            "updatedAt": card.updated_at * 1000,
            "workspace": card.workspace_id,
            "agent": card.agent_id,
        })),
    });
}

fn mirror_gone(card_id: &str) {
    let _ = qs_core::runtime::delete_entity(&mirror_entity_id(card_id));
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum KanbanColumn {
    Plan,
    Work,
    Review,
    Done,
}

impl KanbanColumn {
    pub fn as_str(&self) -> &'static str {
        match self {
            KanbanColumn::Plan => "plan",
            KanbanColumn::Work => "work",
            KanbanColumn::Review => "review",
            KanbanColumn::Done => "done",
        }
    }

}

impl std::str::FromStr for KanbanColumn {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "plan" | "backlog" => Ok(KanbanColumn::Plan),
            "work" | "in_progress" => Ok(KanbanColumn::Work),
            "review" | "awaiting_review" => Ok(KanbanColumn::Review),
            "done" | "merged" | "approved" => Ok(KanbanColumn::Done),
            _ => Err(format!(
                "Invalid column '{}'. Use: plan, work, review, or done.",
                s
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CardPriority {
    Low,
    Medium,
    High,
    Critical,
}

impl CardPriority {
    pub fn as_str(&self) -> &'static str {
        match self {
            CardPriority::Low => "low",
            CardPriority::Medium => "medium",
            CardPriority::High => "high",
            CardPriority::Critical => "critical",
        }
    }

}

impl std::str::FromStr for CardPriority {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "low" => Ok(CardPriority::Low),
            "medium" => Ok(CardPriority::Medium),
            "high" => Ok(CardPriority::High),
            "critical" => Ok(CardPriority::Critical),
            _ => Err(format!(
                "Invalid priority '{}'. Use: low, medium, high, or critical.",
                s
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CardStatus {
    Backlog,
    InProgress,
    AwaitingReview,
    Approved,
    Merged,
    Rejected,
    Cancelled,
}

impl CardStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CardStatus::Backlog => "backlog",
            CardStatus::InProgress => "in_progress",
            CardStatus::AwaitingReview => "awaiting_review",
            CardStatus::Approved => "approved",
            CardStatus::Merged => "merged",
            CardStatus::Rejected => "rejected",
            CardStatus::Cancelled => "cancelled",
        }
    }

}

impl std::str::FromStr for CardStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "backlog" => Ok(CardStatus::Backlog),
            "in_progress" => Ok(CardStatus::InProgress),
            "awaiting_review" => Ok(CardStatus::AwaitingReview),
            "approved" => Ok(CardStatus::Approved),
            "merged" => Ok(CardStatus::Merged),
            "rejected" => Ok(CardStatus::Rejected),
            "cancelled" => Ok(CardStatus::Cancelled),
            _ => Err(format!("Invalid status '{}'.", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanCard {
    pub id: String,
    /// The core.db workspace entity id (`core:workspace:<b36>`) this card
    /// belongs to. Pre-unify this was a projects.json uuid.
    pub workspace_id: String,
    pub title: String,
    pub description: String,
    pub column: KanbanColumn,
    pub order: u32,
    pub priority: CardPriority,
    pub status: CardStatus,
    #[serde(default)]
    pub blocked_by: Vec<String>,
    pub created_at: u64,
    pub updated_at: u64,
    /// Human approval of the current plan. Editing it or changing boards resets this.
    #[serde(default)]
    pub start_approved_at: Option<u64>,
    #[serde(default)]
    pub claimed_at: Option<u64>,
    #[serde(default)]
    pub completed_at: Option<u64>,
    #[serde(default)]
    pub merged_at: Option<u64>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub worktree_path: Option<String>,
    /// Set when the operator cleared the card off the board into the
    /// archive ("Archive all" on the Done column). An archived card keeps
    /// its column and status; `list_cards` skips it, `list_archived_cards`
    /// lists it, and a move or `unarchive_card` brings it back.
    #[serde(default)]
    pub archived_at: Option<u64>,
    /// Derived, never stored: the one-liners (PowerShell / cmd / bash) that
    /// run the app from the card's worktree
    /// (`worktree::test_commands_from_path`) — the card editor shows them
    /// with a copy button, the tools quote them.
    #[serde(default)]
    pub test_commands: Option<crate::worktree::TestCommands>,
}

impl KanbanCard {
    pub fn ensure_claimable(&self, require_start_approval: bool) -> Result<(), String> {
        if self.archived_at.is_some() {
            return Err(format!("Card '{}' is archived — it is finished work.", self.title));
        }
        if self.agent_id.is_some() {
            return Err(format!("Card '{}' is already claimed by '{}'", self.title,
                self.agent_id.as_deref().unwrap_or("unknown")));
        }
        if !matches!(self.status, CardStatus::Backlog | CardStatus::Rejected)
            || matches!(self.column, KanbanColumn::Review | KanbanColumn::Done)
        {
            return Err(format!("Card '{}' is not available for work.", self.title));
        }
        if require_start_approval && self.start_approved_at.is_none() {
            return Err("Start approval required: present the Plan card and wait. The user must click 'Approve work' in the card dialog before you claim or edit code.".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityLogEntry {
    pub id: i64,
    pub card_id: String,
    pub agent_id: Option<String>,
    pub action: String,
    pub message: String,
    pub timestamp: u64,
}

// ---------------------------------------------------------------------------
// Database wrapper
// ---------------------------------------------------------------------------

pub struct KanbanDb {
    conn: Mutex<Connection>,
}

/// Schema v2: cards are keyed by the core.db workspace entity id
/// (PLAN-WORKSPACE-UNIFY). `kanban_meta` marks the version — a DB without it
/// is the pre-unify workspace_id schema and gets the fresh-start rename below.
const SCHEMA_VERSION: u32 = 2;

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS kanban_meta (
    key     TEXT PRIMARY KEY,
    value   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS kanban_cards (
    id              TEXT PRIMARY KEY,
    workspace_id    TEXT NOT NULL,
    title           TEXT NOT NULL,
    description     TEXT NOT NULL DEFAULT '',
    column_name     TEXT NOT NULL DEFAULT 'plan',
    card_order      INTEGER NOT NULL DEFAULT 0,
    priority        TEXT NOT NULL DEFAULT 'medium',
    status          TEXT NOT NULL DEFAULT 'backlog',
    blocked_by      TEXT NOT NULL DEFAULT '[]',
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    claimed_at      INTEGER,
    completed_at    INTEGER,
    merged_at       INTEGER,
    agent_id        TEXT,
    branch          TEXT,
    worktree_path   TEXT,
    archived_at     INTEGER
);

CREATE TABLE IF NOT EXISTS activity_log (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    card_id     TEXT NOT NULL,
    agent_id    TEXT,
    action      TEXT NOT NULL,
    message     TEXT NOT NULL DEFAULT '',
    timestamp   INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_cards_workspace ON kanban_cards(workspace_id);
CREATE INDEX IF NOT EXISTS idx_cards_status ON kanban_cards(status);
CREATE INDEX IF NOT EXISTS idx_activity_card ON activity_log(card_id);
"#;

impl KanbanDb {
    /// Open (or create) the kanban database at the given path.
    ///
    /// A pre-unify DB (has `kanban_cards`, lacks `kanban_meta`) is renamed to
    /// `kanban.legacy.db` and a fresh v2 DB is created — the fresh-start
    /// decision from PLAN-WORKSPACE-UNIFY: projects.json uuids cannot be
    /// mapped onto workspace ids, and the old data is expendable but kept as
    /// bytes.
    pub fn open(db_path: &Path) -> Result<Self, String> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create DB directory: {}", e))?;
        }
        Self::retire_legacy_db(db_path)?;
        let conn =
            Connection::open(db_path).map_err(|e| format!("Failed to open kanban DB: {}", e))?;
        // WAL mode for concurrent reads
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .map_err(|e| format!("PRAGMA error: {}", e))?;
        conn.execute_batch(SCHEMA_SQL)
            .map_err(|e| format!("Schema init error: {}", e))?;
        conn.execute(
            "INSERT OR IGNORE INTO kanban_meta (key, value) VALUES ('version', ?1)",
            params![SCHEMA_VERSION.to_string()],
        )
        .map_err(|e| format!("Schema meta error: {}", e))?;
        Self::migrate_archived_at(&conn)?;
        let has_start_approval = conn.prepare("PRAGMA table_info(kanban_cards)")
            .and_then(|mut stmt| {
                let names = stmt.query_map([], |row| row.get::<_, String>(1))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(names.iter().any(|name| name == "start_approved_at"))
            }).map_err(|e| format!("Probe start approval: {e}"))?;
        if !has_start_approval {
            conn.execute_batch("ALTER TABLE kanban_cards ADD COLUMN start_approved_at INTEGER")
                .map_err(|e| format!("Migrate start approval: {e}"))?;
        }
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// The archive (2026-09-07) added `archived_at` to a schema that is
    /// still v2 — `CREATE TABLE IF NOT EXISTS` never touches an existing
    /// table, so a DB from before gets the column here. Idempotent.
    fn migrate_archived_at(conn: &Connection) -> Result<(), String> {
        let has_column = conn
            .prepare("PRAGMA table_info(kanban_cards)")
            .and_then(|mut stmt| {
                let names = stmt
                    .query_map([], |row| row.get::<_, String>(1))?
                    .filter_map(|r| r.ok())
                    .collect::<Vec<_>>();
                Ok(names.iter().any(|n| n == "archived_at"))
            })
            .map_err(|e| format!("Schema probe error: {}", e))?;
        if !has_column {
            conn.execute_batch("ALTER TABLE kanban_cards ADD COLUMN archived_at INTEGER")
                .map_err(|e| format!("Schema migration error (archived_at): {}", e))?;
        }
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_cards_archived ON kanban_cards(workspace_id, archived_at)",
        )
        .map_err(|e| format!("Schema index error (archived_at): {}", e))?;
        Ok(())
    }

    fn retire_legacy_db(db_path: &Path) -> Result<(), String> {
        if !db_path.exists() {
            return Ok(());
        }
        let is_legacy = {
            let conn = Connection::open(db_path)
                .map_err(|e| format!("Failed to probe kanban DB: {}", e))?;
            let has = |name: &str| -> bool {
                conn.query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    params![name],
                    |r| r.get::<_, i64>(0),
                )
                .map(|n| n > 0)
                .unwrap_or(false)
            };
            has("kanban_cards") && !has("kanban_meta")
        };
        if !is_legacy {
            return Ok(());
        }
        eprintln!(
            "mcp: pre-workspace kanban.db detected — renaming to kanban.legacy.db (fresh start)"
        );
        for suffix in ["", "-wal", "-shm"] {
            let src = PathBuf::from(format!("{}{}", db_path.display(), suffix));
            if src.exists() {
                let dst = PathBuf::from(format!(
                    "{}{}",
                    db_path.with_extension("legacy.db").display(),
                    suffix
                ));
                std::fs::rename(&src, &dst)
                    .map_err(|e| format!("Failed to retire legacy kanban DB: {}", e))?;
            }
        }
        Ok(())
    }

    // -- helpers --

    fn row_to_card(row: &rusqlite::Row) -> SqlResult<KanbanCard> {
        let col_str: String = row.get("column_name")?;
        let pri_str: String = row.get("priority")?;
        let sta_str: String = row.get("status")?;
        let blocked_json: String = row.get("blocked_by")?;
        let blocked: Vec<String> = serde_json::from_str(&blocked_json).unwrap_or_default();
        let worktree_path: Option<String> = row.get("worktree_path")?;
        let test_commands = worktree_path.as_deref().and_then(crate::worktree::test_commands_from_path);

        Ok(KanbanCard {
            id: row.get("id")?,
            workspace_id: row.get("workspace_id")?,
            title: row.get("title")?,
            description: row.get("description")?,
            column: KanbanColumn::from_str(&col_str).unwrap_or(KanbanColumn::Plan),
            order: row.get::<_, u32>("card_order")?,
            priority: CardPriority::from_str(&pri_str).unwrap_or(CardPriority::Medium),
            status: CardStatus::from_str(&sta_str).unwrap_or(CardStatus::Backlog),
            blocked_by: blocked,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
            claimed_at: row.get("claimed_at")?,
            start_approved_at: row.get("start_approved_at")?,
            completed_at: row.get("completed_at")?,
            merged_at: row.get("merged_at")?,
            agent_id: row.get("agent_id")?,
            branch: row.get("branch")?,
            worktree_path,
            archived_at: row.get("archived_at")?,
            test_commands,
        })
    }

    fn log_activity(
        conn: &Connection,
        card_id: &str,
        agent_id: Option<&str>,
        action: &str,
        message: &str,
    ) -> Result<(), String> {
        conn.execute(
            "INSERT INTO activity_log (card_id, agent_id, action, message, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![card_id, agent_id, action, message, now_ts()],
        ).map_err(|e| format!("Failed to log activity: {}", e))?;
        Ok(())
    }

    // -----------------------------------------------------------------
    // Card CRUD
    // -----------------------------------------------------------------
    //
    // Every mutation below is one transaction (`unchecked_transaction`, the
    // workspace idiom — see notes' `sync_search_index`): the card row, the
    // siblings a reorder renumbers and the activity_log row land together or
    // not at all, and a 40-card reorder is one WAL commit instead of one per
    // row. The core.db mirror runs after the commit so it never announces
    // state that is then rolled back — and after `drop(conn)`, with the
    // connection lock released: `mirror_card` takes the core.db mutex and
    // emits to every webview, and neither belongs inside this critical
    // section, where it would stall every reader and other agent's call.

    /// The board: every card of a workspace that is not archived.
    pub fn list_cards(&self, workspace_id: &str) -> Result<Vec<KanbanCard>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock: {}", e))?;
        let mut stmt = conn
            .prepare("SELECT * FROM kanban_cards WHERE workspace_id = ?1 AND archived_at IS NULL ORDER BY card_order ASC")
            .map_err(|e| format!("Query error: {}", e))?;
        let cards = stmt
            .query_map(params![workspace_id], Self::row_to_card)
            .map_err(|e| format!("Query error: {}", e))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(cards)
    }

    /// The archive: a workspace's archived cards, newest archive first.
    pub fn list_archived_cards(&self, workspace_id: &str) -> Result<Vec<KanbanCard>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock: {}", e))?;
        let mut stmt = conn
            .prepare("SELECT * FROM kanban_cards WHERE workspace_id = ?1 AND archived_at IS NOT NULL ORDER BY archived_at DESC, card_order ASC")
            .map_err(|e| format!("Query error: {}", e))?;
        let cards = stmt
            .query_map(params![workspace_id], Self::row_to_card)
            .map_err(|e| format!("Query error: {}", e))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(cards)
    }

    pub fn list_cards_by_status(
        &self,
        workspace_id: &str,
        status: &CardStatus,
    ) -> Result<Vec<KanbanCard>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock: {}", e))?;
        let mut stmt = conn
            .prepare(
                "SELECT * FROM kanban_cards WHERE workspace_id = ?1 AND status = ?2 AND archived_at IS NULL ORDER BY card_order ASC",
            )
            .map_err(|e| format!("Query error: {}", e))?;
        let cards = stmt
            .query_map(params![workspace_id, status.as_str()], Self::row_to_card)
            .map_err(|e| format!("Query error: {}", e))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(cards)
    }

    pub fn get_card(&self, card_id: &str) -> Result<Option<KanbanCard>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock: {}", e))?;
        let mut stmt = conn
            .prepare("SELECT * FROM kanban_cards WHERE id = ?1")
            .map_err(|e| format!("Query error: {}", e))?;
        let card = stmt.query_row(params![card_id], Self::row_to_card).ok();
        Ok(card)
    }

    pub fn add_card(
        &self,
        id: &str,
        workspace_id: &str,
        title: &str,
        description: &str,
        column: &KanbanColumn,
        priority: &CardPriority,
        blocked_by: &[String],
    ) -> Result<KanbanCard, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock: {}", e))?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| format!("Begin add tx: {e}"))?;

        // Compute next order in column (the archive is off the board and
        // does not count)
        let max_order: u32 = tx
            .query_row(
                "SELECT COALESCE(MAX(card_order), 0) FROM kanban_cards WHERE workspace_id = ?1 AND column_name = ?2 AND archived_at IS NULL",
                params![workspace_id, column.as_str()],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let now = now_ts();
        let blocked_json = serde_json::to_string(blocked_by).unwrap_or_else(|_| "[]".into());
        let status = CardStatus::Backlog;

        tx.execute(
            "INSERT INTO kanban_cards (id, workspace_id, title, description, column_name, card_order, priority, status, blocked_by, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![id, workspace_id, title, description, column.as_str(), max_order + 1, priority.as_str(), status.as_str(), blocked_json, now, now],
        ).map_err(|e| format!("Insert error: {}", e))?;

        Self::log_activity(
            &tx,
            id,
            None,
            "created",
            &format!("Card '{}' created", title),
        )?;

        let card = self.get_card_inner(&tx, id)?;
        tx.commit().map_err(|e| format!("Commit add: {e}"))?;
        drop(conn); // the mirror runs unlocked — see the section note
        mirror_card(&card);
        Ok(card)
    }

    fn get_card_inner(&self, conn: &Connection, card_id: &str) -> Result<KanbanCard, String> {
        let mut stmt = conn
            .prepare("SELECT * FROM kanban_cards WHERE id = ?1")
            .map_err(|e| format!("Query error: {}", e))?;
        stmt.query_row(params![card_id], Self::row_to_card)
            .map_err(|e| format!("Card '{}' not found: {}", card_id, e))
    }

    pub fn update_card(
        &self,
        card_id: &str,
        title: &str,
        description: &str,
        priority: Option<&CardPriority>,
        blocked_by: Option<&[String]>,
    ) -> Result<KanbanCard, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock: {}", e))?;
        let now = now_ts();
        // Three UPDATEs, one commit: a card never shows its new title with
        // the old priority.
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| format!("Begin update tx: {e}"))?;

        let previous = self.get_card_inner(&tx, card_id)?;
        if previous.agent_id.is_none()
            && (previous.title != title || previous.description != description
                || priority.is_some_and(|p| *p != previous.priority)
                || blocked_by.is_some_and(|deps| deps != previous.blocked_by))
        {
            tx.execute("UPDATE kanban_cards SET start_approved_at = NULL WHERE id = ?1", [card_id])
                .map_err(|e| format!("Reset start approval: {e}"))?;
        }

        // Base update
        tx.execute(
            "UPDATE kanban_cards SET title = ?1, description = ?2, updated_at = ?3 WHERE id = ?4",
            params![title, description, now, card_id],
        )
        .map_err(|e| format!("Update error: {}", e))?;

        if let Some(pri) = priority {
            tx.execute(
                "UPDATE kanban_cards SET priority = ?1 WHERE id = ?2",
                params![pri.as_str(), card_id],
            )
            .map_err(|e| format!("Update priority error: {}", e))?;
        }

        if let Some(deps) = blocked_by {
            let json = serde_json::to_string(deps).unwrap_or_else(|_| "[]".into());
            tx.execute(
                "UPDATE kanban_cards SET blocked_by = ?1 WHERE id = ?2",
                params![json, card_id],
            )
            .map_err(|e| format!("Update blocked_by error: {}", e))?;
        }

        let card = self.get_card_inner(&tx, card_id)?;
        tx.commit().map_err(|e| format!("Commit update: {e}"))?;
        drop(conn);
        mirror_card(&card);
        Ok(card)
    }

    pub fn move_card(
        &self,
        card_id: &str,
        column: &KanbanColumn,
        before_id: Option<&str>,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock: {}", e))?;
        let now = now_ts();
        // The whole reorder is one transaction: a 40-card column is one
        // commit instead of ~80, and an error midway rolls back instead of
        // leaving two cards on the same card_order or one in both columns.
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| format!("Begin move tx: {e}"))?;

        let card = self.get_card_inner(&tx, card_id)?;
        let workspace_id = &card.workspace_id;
        let source_column = &card.column;

        // Ordered IDs in the target column (excluding the moving card), with
        // the card inserted at the right position.
        let mut target_ids = Self::column_ids_except(&tx, workspace_id, column, card_id)?;
        let insert_idx = before_id
            .and_then(|bid| target_ids.iter().position(|id| id == bid))
            .unwrap_or(target_ids.len());
        target_ids.insert(insert_idx, card_id.to_string());

        // A cross-column move closes the gap the card left in its source.
        let source_ids = if source_column != column {
            Self::column_ids_except(&tx, workspace_id, source_column, card_id)?
        } else {
            Vec::new()
        };

        {
            // One prepared statement for every row of both columns; scoped
            // so its borrow of `tx` ends before the commit. A moved card is
            // back on the board, so the statement also clears `archived_at`
            // — the siblings it renumbers were never archived (see
            // `column_ids_except`), only the moving card can be.
            let mut stmt = tx
                .prepare("UPDATE kanban_cards SET column_name = ?1, card_order = ?2, updated_at = ?3, archived_at = NULL WHERE id = ?4")
                .map_err(|e| format!("Prepare reorder: {e}"))?;
            for (idx, id) in target_ids.iter().enumerate() {
                stmt.execute(params![column.as_str(), (idx + 1) as u32, now, id])
                    .map_err(|e| format!("Reorder error: {}", e))?;
            }
            for (idx, id) in source_ids.iter().enumerate() {
                stmt.execute(params![source_column.as_str(), (idx + 1) as u32, now, id])
                    .map_err(|e| format!("Reorder error: {}", e))?;
            }
        }

        // Every row above got a new card_order / updated_at, so every one
        // needs its mirror refreshed — not only the moved card, or the
        // others keep a stale `order` in their core.db payload.
        let touched = target_ids
            .iter()
            .chain(source_ids.iter())
            .map(|id| self.get_card_inner(&tx, id))
            .collect::<Result<Vec<_>, _>>()?;
        tx.commit().map_err(|e| format!("Commit move: {e}"))?;
        drop(conn);
        for card in &touched {
            mirror_card(card);
        }
        Ok(())
    }

    /// The ids of one column in board order, without `except_id` — the rows
    /// a reorder renumbers. Archived cards are off the board and stay out
    /// of the numbering.
    fn column_ids_except(
        conn: &Connection,
        workspace_id: &str,
        column: &KanbanColumn,
        except_id: &str,
    ) -> Result<Vec<String>, String> {
        let mut stmt = conn
            .prepare(
                "SELECT id FROM kanban_cards WHERE workspace_id = ?1 AND column_name = ?2 AND id != ?3 AND archived_at IS NULL ORDER BY card_order ASC",
            )
            .map_err(|e| format!("Query: {}", e))?;
        let rows = stmt
            .query_map(params![workspace_id, column.as_str(), except_id], |row| row.get(0))
            .map_err(|e| format!("Query: {}", e))?;
        let ids: Vec<String> = rows.filter_map(|r| r.ok()).collect();
        Ok(ids)
    }

    /// Every card on every board (archived ones excluded — they have no
    /// mirror) — the fresh-start re-mirror walks this.
    pub fn all_cards(&self) -> Result<Vec<KanbanCard>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock: {}", e))?;
        let mut stmt = conn
            .prepare("SELECT * FROM kanban_cards WHERE archived_at IS NULL ORDER BY card_order ASC")
            .map_err(|e| format!("Query error: {}", e))?;
        let cards = stmt
            .query_map([], Self::row_to_card)
            .map_err(|e| format!("Query error: {}", e))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(cards)
    }

    /// Rebuild every card's core.db mirror entity — the one-time
    /// kanban-unify cleanup calls this after clearing the old entities.
    pub fn remirror_all(&self) {
        if let Ok(cards) = self.all_cards() {
            for card in &cards {
                mirror_card(card);
            }
        }
    }

    /// Re-home a card onto another board (a workspace or General), keeping
    /// its column and appending it there. Only unclaimed cards move — an
    /// agent's worktree and branch live in the source workspace's folder.
    pub fn move_card_to_board(
        &self,
        card_id: &str,
        target_board_id: &str,
    ) -> Result<KanbanCard, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock: {}", e))?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| format!("Begin move-board tx: {e}"))?;
        let card = self.get_card_inner(&tx, card_id)?;

        if card.workspace_id == target_board_id {
            return Err(format!("Card '{}' is already on that board", card.title));
        }
        if card.agent_id.is_some()
            || !matches!(card.status, CardStatus::Backlog | CardStatus::Rejected)
        {
            return Err(format!(
                "Card '{}' is being worked (status: {}) — its worktree lives in the source workspace. Cancel or finish it first.",
                card.title,
                card.status.as_str()
            ));
        }

        let max_order: u32 = tx
            .query_row(
                "SELECT COALESCE(MAX(card_order), 0) FROM kanban_cards WHERE workspace_id = ?1 AND column_name = ?2 AND archived_at IS NULL",
                params![target_board_id, card.column.as_str()],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let now = now_ts();
        tx.execute(
            "UPDATE kanban_cards SET workspace_id = ?1, card_order = ?2, updated_at = ?3, start_approved_at = NULL WHERE id = ?4",
            params![target_board_id, max_order + 1, now, card_id],
        )
        .map_err(|e| format!("Move error: {}", e))?;

        Self::log_activity(
            &tx,
            card_id,
            None,
            "moved_board",
            &format!("Card '{}' moved to board '{}'", card.title, target_board_id),
        )?;

        let card = self.get_card_inner(&tx, card_id)?;
        tx.commit().map_err(|e| format!("Commit move-board: {e}"))?;
        drop(conn);
        mirror_card(&card);
        Ok(card)
    }

    pub fn delete_card(&self, card_id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock: {}", e))?;
        // The card and its activity log go together: no stray log rows for
        // a card that is gone, no card whose log vanished under it.
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| format!("Begin delete tx: {e}"))?;
        let rows = tx
            .execute("DELETE FROM kanban_cards WHERE id = ?1", params![card_id])
            .map_err(|e| format!("Delete error: {}", e))?;
        if rows == 0 {
            return Err(format!("Card '{}' not found", card_id));
        }
        tx.execute(
            "DELETE FROM activity_log WHERE card_id = ?1",
            params![card_id],
        )
        .map_err(|e| format!("Delete log error: {e}"))?;
        tx.commit().map_err(|e| format!("Commit delete: {e}"))?;
        drop(conn);
        mirror_gone(card_id);
        Ok(())
    }

    // -----------------------------------------------------------------
    // Workflow lifecycle
    // -----------------------------------------------------------------

    /// Check if all blocked_by dependencies are merged.
    pub fn can_claim(&self, card_id: &str) -> Result<(bool, Vec<String>), String> {
        let card = self
            .get_card(card_id)?
            .ok_or_else(|| format!("Card '{}' not found", card_id))?;
        if card.blocked_by.is_empty() {
            return Ok((true, vec![]));
        }
        let conn = self.conn.lock().map_err(|e| format!("Lock: {}", e))?;
        let mut unresolved = Vec::new();
        for dep_id in &card.blocked_by {
            let status: Option<String> = conn
                .query_row(
                    "SELECT status FROM kanban_cards WHERE id = ?1",
                    params![dep_id],
                    |row| row.get(0),
                )
                .ok();
            match status.as_deref() {
                Some("merged") => {} // OK
                Some(s) => unresolved.push(format!("{} (status: {})", dep_id, s)),
                None => unresolved.push(format!("{} (not found)", dep_id)),
            }
        }
        Ok((unresolved.is_empty(), unresolved))
    }

    /// Check file conflicts with other active worktrees.
    pub fn check_file_conflicts(
        &self,
        workspace_id: &str,
        card_id: &str,
        intended_files: &[String],
    ) -> Result<Vec<(String, String, Vec<String>)>, String> {
        self.check_file_conflicts_with(workspace_id, card_id, intended_files, |wt_path| {
            crate::git_helpers::get_modified_files(wt_path).unwrap_or_default()
        })
    }

    /// `check_file_conflicts` with the git call injected. The rows come out
    /// from under the connection lock first and only then does git run: one
    /// `git diff` per in-progress card is hundreds of ms on a large checkout
    /// (seconds on a cold disk), and every `list_cards` / `get_card` and
    /// every other agent's tool call would queue behind it otherwise.
    fn check_file_conflicts_with(
        &self,
        workspace_id: &str,
        card_id: &str,
        intended_files: &[String],
        modified_files: impl Fn(&str) -> Vec<String>,
    ) -> Result<Vec<(String, String, Vec<String>)>, String> {
        // Returns: Vec<(other_card_id, other_card_title, overlapping_files)>
        if intended_files.is_empty() {
            return Ok(vec![]);
        }
        let active_cards: Vec<(String, String, String)> = {
            let conn = self.conn.lock().map_err(|e| format!("Lock: {}", e))?;
            let mut stmt = conn
                .prepare(
                    "SELECT id, title, worktree_path FROM kanban_cards WHERE workspace_id = ?1 AND status = 'in_progress' AND id != ?2 AND worktree_path IS NOT NULL",
                )
                .map_err(|e| format!("Query: {}", e))?;
            let rows = stmt
                .query_map(params![workspace_id, card_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|e| format!("Query: {}", e))?;
            rows.filter_map(|r| r.ok()).collect()
        };

        let mut conflicts = Vec::new();
        for (other_id, other_title, wt_path) in &active_cards {
            // Get modified files in the other worktree
            let modified = modified_files(wt_path);
            let overlapping: Vec<String> = intended_files
                .iter()
                .filter(|f| modified.contains(f))
                .cloned()
                .collect();
            if !overlapping.is_empty() {
                conflicts.push((other_id.clone(), other_title.clone(), overlapping));
            }
        }
        Ok(conflicts)
    }

    /// Only the native card dialog calls this; MCP agents cannot approve themselves.
    pub fn approve_start(&self, card_id: &str) -> Result<KanbanCard, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock: {e}"))?;
        let tx = conn.unchecked_transaction().map_err(|e| format!("Begin start approval: {e}"))?;
        let card = self.get_card_inner(&tx, card_id)?;
        card.ensure_claimable(false)?;
        if card.column != KanbanColumn::Plan || card.workspace_id == GENERAL_BOARD_ID {
            return Err("Only a workspace card in Plan can be approved for work.".into());
        }
        tx.execute("UPDATE kanban_cards SET start_approved_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![now_ts(), card_id]).map_err(|e| format!("Approve start: {e}"))?;
        Self::log_activity(&tx, card_id, None, "start_approved", "User approved the plan for work")?;
        let card = self.get_card_inner(&tx, card_id)?;
        tx.commit().map_err(|e| format!("Commit start approval: {e}"))?;
        drop(conn);
        mirror_card(&card);
        Ok(card)
    }

    /// Claim a card: set agent, branch, worktree, move to work/in_progress.
    pub fn claim_card(
        &self,
        card_id: &str,
        agent_id: &str,
        branch: &str,
        worktree_path: &str,
        require_start_approval: bool,
    ) -> Result<KanbanCard, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock: {}", e))?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| format!("Begin claim tx: {e}"))?;
        let card = self.get_card_inner(&tx, card_id)?;
        card.ensure_claimable(require_start_approval)?;

        let now = now_ts();
        tx.execute(
            "UPDATE kanban_cards SET agent_id = ?1, branch = ?2, worktree_path = ?3, column_name = 'work', status = 'in_progress', claimed_at = ?4, updated_at = ?4 WHERE id = ?5",
            params![agent_id, branch, worktree_path, now, card_id],
        ).map_err(|e| format!("Claim error: {}", e))?;

        Self::log_activity(
            &tx,
            card_id,
            Some(agent_id),
            "claimed",
            &format!("Card '{}' claimed by agent '{}'", card.title, agent_id),
        )?;

        // Re-read the updated card
        let card = self.get_card_inner(&tx, card_id)?;
        tx.commit().map_err(|e| format!("Commit claim: {e}"))?;
        drop(conn);
        mirror_card(&card);
        Ok(card)
    }

    /// Complete a card: move to review/awaiting_review (does NOT merge).
    pub fn complete_card(
        &self,
        card_id: &str,
        agent_id: Option<&str>,
    ) -> Result<KanbanCard, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock: {}", e))?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| format!("Begin complete tx: {e}"))?;
        let card = self.get_card_inner(&tx, card_id)?;

        if card.status != CardStatus::InProgress {
            return Err(format!(
                "Card '{}' is not in progress (current status: {})",
                card.title,
                card.status.as_str()
            ));
        }

        // Validate agent if provided
        if let Some(aid) = agent_id {
            if card.agent_id.as_deref() != Some(aid) {
                return Err(format!(
                    "Agent '{}' is not assigned to card '{}' (assigned: {})",
                    aid,
                    card.title,
                    card.agent_id.as_deref().unwrap_or("none")
                ));
            }
        }

        let now = now_ts();
        tx.execute(
            "UPDATE kanban_cards SET column_name = 'review', status = 'awaiting_review', completed_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![now, card_id],
        ).map_err(|e| format!("Complete error: {}", e))?;

        Self::log_activity(
            &tx,
            card_id,
            card.agent_id.as_deref(),
            "completed",
            &format!("Card '{}' completed, awaiting review", card.title),
        )?;

        let card = self.get_card_inner(&tx, card_id)?;
        tx.commit().map_err(|e| format!("Commit complete: {e}"))?;
        drop(conn);
        mirror_card(&card);
        Ok(card)
    }

    /// Approve a card: after merge is done externally, mark as merged and clean up DB fields.
    pub fn approve_card(&self, card_id: &str) -> Result<KanbanCard, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock: {}", e))?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| format!("Begin approve tx: {e}"))?;
        let card = self.get_card_inner(&tx, card_id)?;

        if card.status != CardStatus::AwaitingReview {
            return Err(format!(
                "Card '{}' is not awaiting review (current status: {})",
                card.title,
                card.status.as_str()
            ));
        }

        let now = now_ts();
        tx.execute(
            "UPDATE kanban_cards SET column_name = 'done', status = 'merged', merged_at = ?1, updated_at = ?1, agent_id = NULL, branch = NULL, worktree_path = NULL WHERE id = ?2",
            params![now, card_id],
        ).map_err(|e| format!("Approve error: {}", e))?;

        Self::log_activity(
            &tx,
            card_id,
            card.agent_id.as_deref(),
            "approved",
            &format!("Card '{}' approved and merged", card.title),
        )?;

        let card = self.get_card_inner(&tx, card_id)?;
        tx.commit().map_err(|e| format!("Commit approve: {e}"))?;
        drop(conn);
        mirror_card(&card);
        Ok(card)
    }

    /// Reject a card: send back to in_progress, preserve worktree.
    pub fn reject_card(&self, card_id: &str, reason: &str) -> Result<KanbanCard, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock: {}", e))?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| format!("Begin reject tx: {e}"))?;
        let card = self.get_card_inner(&tx, card_id)?;

        if card.status != CardStatus::AwaitingReview {
            return Err(format!(
                "Card '{}' is not awaiting review (current status: {})",
                card.title,
                card.status.as_str()
            ));
        }

        let now = now_ts();
        tx.execute(
            "UPDATE kanban_cards SET column_name = 'work', status = 'in_progress', completed_at = NULL, updated_at = ?1 WHERE id = ?2",
            params![now, card_id],
        ).map_err(|e| format!("Reject error: {}", e))?;

        Self::log_activity(
            &tx,
            card_id,
            card.agent_id.as_deref(),
            "rejected",
            &format!("Card '{}' rejected: {}", card.title, reason),
        )?;

        let card = self.get_card_inner(&tx, card_id)?;
        tx.commit().map_err(|e| format!("Commit reject: {e}"))?;
        drop(conn);
        mirror_card(&card);
        Ok(card)
    }

    /// Cancel a card: mark as cancelled, clear agent/branch/worktree.
    pub fn cancel_card(&self, card_id: &str) -> Result<KanbanCard, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock: {}", e))?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| format!("Begin cancel tx: {e}"))?;
        let card = self.get_card_inner(&tx, card_id)?;

        let now = now_ts();
        tx.execute(
            "UPDATE kanban_cards SET column_name = 'done', status = 'cancelled', updated_at = ?1, agent_id = NULL, branch = NULL, worktree_path = NULL WHERE id = ?2",
            params![now, card_id],
        ).map_err(|e| format!("Cancel error: {}", e))?;

        Self::log_activity(
            &tx,
            card_id,
            card.agent_id.as_deref(),
            "cancelled",
            &format!("Card '{}' cancelled", card.title),
        )?;

        let card = self.get_card_inner(&tx, card_id)?;
        tx.commit().map_err(|e| format!("Commit cancel: {e}"))?;
        drop(conn);
        mirror_card(&card);
        Ok(card)
    }

    // -----------------------------------------------------------------
    // Archive
    // -----------------------------------------------------------------
    //
    // "Archive all" on the Done column: the cards leave the board but not
    // the database, so an agent's `list_kanban_cards archived=true` still
    // shows what was done earlier. Column, status and timestamps stay as
    // they were; only `archived_at` is set.

    /// Archive one card, only while it is on the live board's Done column.
    pub fn archive_card(&self, card_id: &str) -> Result<KanbanCard, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock: {}", e))?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| format!("Begin archive tx: {e}"))?;
        let card = self.get_card_inner(&tx, card_id)?;
        if card.column != KanbanColumn::Done {
            return Err("Only cards in Done can be archived".to_string());
        }
        if card.archived_at.is_some() {
            return Err(format!("Card '{}' is already archived", card.title));
        }

        tx.execute(
            "UPDATE kanban_cards SET archived_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![now_ts(), card_id],
        )
        .map_err(|e| format!("Archive error: {}", e))?;
        Self::log_activity(
            &tx,
            card_id,
            None,
            "archived",
            &format!("Card '{}' archived from Done", card.title),
        )?;

        let card = self.get_card_inner(&tx, card_id)?;
        tx.commit().map_err(|e| format!("Commit archive: {e}"))?;
        drop(conn);
        mirror_card(&card);
        Ok(card)
    }

    /// Archive every card in a board's Done column. Returns the cards that
    /// were archived (empty when Done was empty), one transaction.
    pub fn archive_done_cards(&self, workspace_id: &str) -> Result<Vec<KanbanCard>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock: {}", e))?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| format!("Begin archive tx: {e}"))?;

        let ids: Vec<String> = {
            let mut stmt = tx
                .prepare(
                    "SELECT id FROM kanban_cards WHERE workspace_id = ?1 AND column_name = 'done' AND archived_at IS NULL ORDER BY card_order ASC",
                )
                .map_err(|e| format!("Query: {}", e))?;
            let rows = stmt
                .query_map(params![workspace_id], |row| row.get(0))
                .map_err(|e| format!("Query: {}", e))?;
            rows.filter_map(|r| r.ok()).collect()
        };
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let now = now_ts();
        let mut archived = Vec::with_capacity(ids.len());
        for id in &ids {
            tx.execute(
                "UPDATE kanban_cards SET archived_at = ?1, updated_at = ?1 WHERE id = ?2",
                params![now, id],
            )
            .map_err(|e| format!("Archive error: {}", e))?;
            let card = self.get_card_inner(&tx, id)?;
            Self::log_activity(
                &tx,
                id,
                None,
                "archived",
                &format!("Card '{}' archived from Done", card.title),
            )?;
            archived.push(card);
        }

        tx.commit().map_err(|e| format!("Commit archive: {e}"))?;
        drop(conn);
        for card in &archived {
            mirror_card(card); // archived → leaves the mirror
        }
        Ok(archived)
    }

    /// Bring an archived card back onto the board, at the end of the column
    /// it was archived from (Done, unless an agent moved it meanwhile).
    pub fn unarchive_card(&self, card_id: &str) -> Result<KanbanCard, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock: {}", e))?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| format!("Begin unarchive tx: {e}"))?;
        let card = self.get_card_inner(&tx, card_id)?;
        if card.archived_at.is_none() {
            return Err(format!("Card '{}' is not archived", card.title));
        }

        let max_order: u32 = tx
            .query_row(
                "SELECT COALESCE(MAX(card_order), 0) FROM kanban_cards WHERE workspace_id = ?1 AND column_name = ?2 AND archived_at IS NULL",
                params![card.workspace_id, card.column.as_str()],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let now = now_ts();
        tx.execute(
            "UPDATE kanban_cards SET archived_at = NULL, card_order = ?1, updated_at = ?2 WHERE id = ?3",
            params![max_order + 1, now, card_id],
        )
        .map_err(|e| format!("Unarchive error: {}", e))?;

        Self::log_activity(
            &tx,
            card_id,
            None,
            "unarchived",
            &format!("Card '{}' restored from the archive", card.title),
        )?;

        let card = self.get_card_inner(&tx, card_id)?;
        tx.commit().map_err(|e| format!("Commit unarchive: {e}"))?;
        drop(conn);
        mirror_card(&card);
        Ok(card)
    }

    // -----------------------------------------------------------------
    // Activity log
    // -----------------------------------------------------------------

    pub fn get_activity_log(
        &self,
        card_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<ActivityLogEntry>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock: {}", e))?;

        let entries = if let Some(cid) = card_id {
            let mut stmt = conn
                .prepare(
                    "SELECT * FROM activity_log WHERE card_id = ?1 ORDER BY timestamp DESC LIMIT ?2",
                )
                .map_err(|e| format!("Query: {}", e))?;
            let rows = stmt
                .query_map(params![cid, limit], |row| {
                    Ok(ActivityLogEntry {
                        id: row.get("id")?,
                        card_id: row.get("card_id")?,
                        agent_id: row.get("agent_id")?,
                        action: row.get("action")?,
                        message: row.get("message")?,
                        timestamp: row.get("timestamp")?,
                    })
                })
                .map_err(|e| format!("Query: {}", e))?;
            rows.filter_map(|r| r.ok()).collect()
        } else {
            let mut stmt = conn
                .prepare("SELECT * FROM activity_log ORDER BY timestamp DESC LIMIT ?1")
                .map_err(|e| format!("Query: {}", e))?;
            let rows = stmt
                .query_map(params![limit], |row| {
                    Ok(ActivityLogEntry {
                        id: row.get("id")?,
                        card_id: row.get("card_id")?,
                        agent_id: row.get("agent_id")?,
                        action: row.get("action")?,
                        message: row.get("message")?,
                        timestamp: row.get("timestamp")?,
                    })
                })
                .map_err(|e| format!("Query: {}", e))?;
            rows.filter_map(|r| r.ok()).collect()
        };

        Ok(entries)
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh kanban.db under the temp dir: `open` wants a path (WAL, the
    /// legacy probe), so no in-memory connection. The core.db mirror is a
    /// no-op here — the runtime hooks are only installed by the running app.
    fn temp_db(name: &str) -> (KanbanDb, PathBuf) {
        let dir = std::env::temp_dir().join(format!("qs-kanban-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db = KanbanDb::open(&dir.join("kanban.db")).expect("open kanban db");
        (db, dir)
    }

    fn add(db: &KanbanDb, id: &str, column: &KanbanColumn) {
        db.add_card(id, "ws", &format!("Card {id}"), "", column, &CardPriority::Medium, &[])
            .expect("add card");
    }

    /// `id:order` for every card of a column, in board order.
    fn column_orders(db: &KanbanDb, column: &KanbanColumn) -> Vec<String> {
        db.list_cards("ws")
            .expect("list cards")
            .into_iter()
            .filter(|c| &c.column == column)
            .map(|c| format!("{}:{}", c.id, c.order))
            .collect()
    }

    #[test]
    fn approval_requires_plan_then_separate_result_approval() {
        let (db, dir) = temp_db("two-approvals");
        add(&db, "c1", &KanbanColumn::Plan);
        assert!(db.claim_card("c1", "agent", "branch", "/wt", true).unwrap_err().contains("Start approval required"));
        assert_eq!(db.get_card("c1").unwrap().unwrap().column, KanbanColumn::Plan);
        db.approve_start("c1").unwrap();
        db.claim_card("c1", "agent", "branch", "/wt", true).unwrap();
        assert!(db.approve_start("c1").is_err());
        assert!(db.approve_card("c1").is_err());
        let submitted = db.complete_card("c1", Some("agent")).unwrap();
        assert_eq!(submitted.column, KanbanColumn::Review);
        assert_eq!(submitted.worktree_path.as_deref(), Some("/wt"));
        assert!(submitted.merged_at.is_none());
        let rework = db.reject_card("c1", "Adjust the result").unwrap();
        assert_eq!(rework.status, CardStatus::InProgress);
        db.complete_card("c1", Some("agent")).unwrap();
        assert_eq!(db.approve_card("c1").unwrap().status, CardStatus::Merged);
        drop(db);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn plan_approval_survives_reopen_but_not_scope_changes() {
        let (db, dir) = temp_db("plan-approval");
        add(&db, "c1", &KanbanColumn::Plan);
        db.approve_start("c1").unwrap();
        drop(db);
        let db = KanbanDb::open(&dir.join("kanban.db")).unwrap();
        assert!(db.get_card("c1").unwrap().unwrap().start_approved_at.is_some());
        let same = db.update_card("c1", "Card c1", "", None, None).unwrap();
        assert!(same.start_approved_at.is_some());
        let edited = db.update_card("c1", "Changed scope", "", None, None).unwrap();
        assert!(edited.start_approved_at.is_none());
        assert!(db.claim_card("c1", "agent", "branch", "/wt", true).is_err());
        db.approve_start("c1").unwrap();
        let moved = db.move_card_to_board("c1", "other").unwrap();
        assert!(moved.start_approved_at.is_none());
        assert!(db.claim_card("c1", "agent", "branch", "/wt", true).is_err());
        // Auto Apply still permits immediate work without either plan approval.
        db.claim_card("c1", "agent", "branch", "/wt", false).unwrap();
        drop(db);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn existing_v2_database_gains_start_approval_without_losing_cards() {
        let (db, dir) = temp_db("migrate-start-approval");
        add(&db, "c1", &KanbanColumn::Plan);
        db.conn.lock().unwrap().execute_batch("ALTER TABLE kanban_cards DROP COLUMN start_approved_at").unwrap();
        drop(db);
        let db = KanbanDb::open(&dir.join("kanban.db")).unwrap();
        let card = db.get_card("c1").unwrap().unwrap();
        assert_eq!(card.title, "Card c1");
        assert!(card.start_approved_at.is_none());
        assert!(db.claim_card("c1", "agent", "branch", "/wt", true).is_err());
        drop(db);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Dragging a card into another column renumbers both columns 1..n
    /// with no gaps — the whole reorder is one transaction now.
    #[test]
    fn move_across_columns_renumbers_both_columns() {
        let (db, dir) = temp_db("move");
        add(&db, "c1", &KanbanColumn::Plan);
        add(&db, "c2", &KanbanColumn::Plan);
        add(&db, "c3", &KanbanColumn::Work);

        db.move_card("c1", &KanbanColumn::Work, Some("c3")).expect("move c1 before c3");
        assert_eq!(column_orders(&db, &KanbanColumn::Work), ["c1:1", "c3:2"]);
        assert_eq!(column_orders(&db, &KanbanColumn::Plan), ["c2:1"]);

        // Appending (no before_id) and a same-column move keep the invariant.
        db.move_card("c2", &KanbanColumn::Work, None).expect("append c2");
        db.move_card("c3", &KanbanColumn::Work, Some("c1")).expect("move c3 to the top");
        assert_eq!(column_orders(&db, &KanbanColumn::Work), ["c3:1", "c1:2", "c2:3"]);
        assert!(column_orders(&db, &KanbanColumn::Plan).is_empty());

        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A failure after the card statement rolls the whole mutation back:
    /// with `activity_log` gone the log row fails, and the card row must be
    /// exactly as it was — no half-claimed card, no orphaned insert, no
    /// card deleted without its log.
    #[test]
    fn failed_mutation_leaves_the_card_untouched() {
        let (db, dir) = temp_db("rollback");
        add(&db, "c1", &KanbanColumn::Plan);
        db.conn
            .lock()
            .expect("lock")
            .execute_batch("DROP TABLE activity_log")
            .expect("drop activity_log");

        assert!(db.claim_card("c1", "agent-1", "agent/agent-1-c1", "/wt/c1", false).is_err());
        let card = db.get_card("c1").expect("get").expect("card still there");
        assert_eq!(card.status, CardStatus::Backlog);
        assert_eq!(card.column, KanbanColumn::Plan);
        assert_eq!(card.agent_id, None);
        assert_eq!(card.claimed_at, None);

        assert!(db
            .add_card("c2", "ws", "Card c2", "", &KanbanColumn::Plan, &CardPriority::Low, &[])
            .is_err());
        assert!(db.get_card("c2").expect("get").is_none(), "the insert was rolled back");

        assert!(db.delete_card("c1").is_err());
        assert!(db.get_card("c1").expect("get").is_some(), "the delete was rolled back");

        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The conflict scan runs git with the connection lock released: while a
    /// (fake, blocking) git call is still in flight, a reader on another
    /// thread gets its answer instead of queueing behind the scan.
    #[test]
    fn conflict_scan_runs_git_without_the_lock() {
        use std::sync::{mpsc, Arc};
        use std::time::Duration;

        let (db, dir) = temp_db("conflicts");
        add(&db, "c1", &KanbanColumn::Plan);
        add(&db, "c2", &KanbanColumn::Plan);
        db.claim_card("c1", "agent-1", "agent/agent-1-c1", "/wt/c1", false).expect("claim c1");
        let db = Arc::new(db);

        let (git_started, git_running) = mpsc::channel::<()>();
        let (git_release, git_blocked) = mpsc::channel::<()>();
        let scan = {
            let db = Arc::clone(&db);
            std::thread::spawn(move || {
                db.check_file_conflicts_with("ws", "c2", &["a.rs".to_string()], |_wt_path| {
                    git_started.send(()).expect("test alive");
                    git_blocked.recv().expect("released by the test");
                    vec!["a.rs".to_string()]
                })
            })
        };
        git_running
            .recv_timeout(Duration::from_secs(5))
            .expect("the scan reached its git call");

        // git is "running" now; a reader must still get through.
        let (done, read) = mpsc::channel();
        let reader = {
            let db = Arc::clone(&db);
            std::thread::spawn(move || {
                let _ = done.send(db.list_cards("ws").map(|cards| cards.len()));
            })
        };
        let seen = read
            .recv_timeout(Duration::from_secs(5))
            .expect("list_cards answered while git was still running");
        assert_eq!(seen, Ok(2));

        git_release.send(()).expect("scan alive");
        let conflicts = scan.join().expect("scan thread").expect("scan result");
        assert_eq!(
            conflicts,
            vec![("c1".to_string(), "Card c1".to_string(), vec!["a.rs".to_string()])]
        );

        reader.join().expect("reader thread");
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn archive_one_card_requires_done_and_leaves_other_cards_untouched() {
        let (db, dir) = temp_db("archive-one");
        for (id, column) in [
            ("p1", KanbanColumn::Plan),
            ("w1", KanbanColumn::Work),
            ("r1", KanbanColumn::Review),
            ("d1", KanbanColumn::Done),
            ("d2", KanbanColumn::Done),
        ] {
            add(&db, id, &column);
        }
        for id in ["p1", "w1", "r1"] {
            let err = db.archive_card(id).expect_err("unfinished card rejected");
            assert!(err.contains("Only cards in Done"), "{err}");
            assert_eq!(db.get_card(id).unwrap().unwrap().archived_at, None);
            assert_eq!(db.get_activity_log(Some(id), 10).unwrap().len(), 1);
        }
        assert!(db.archive_card("missing").is_err());

        let before = db.get_card("d1").unwrap().unwrap();
        let archived = db.archive_card("d1").expect("archive only d1");
        assert!(archived.archived_at.is_some());
        assert_eq!(archived.column, before.column);
        assert_eq!(archived.status, before.status);
        assert_eq!(archived.description, before.description);
        assert_eq!(archived.created_at, before.created_at);
        assert_eq!(archived.completed_at, before.completed_at);
        assert_eq!(db.list_cards("ws").unwrap().len(), 4);
        assert_eq!(column_orders(&db, &KanbanColumn::Done), ["d2:2"]);
        let archive = db.list_archived_cards("ws").unwrap();
        assert_eq!(archive.len(), 1);
        assert_eq!(archive[0].id, "d1");
        assert!(db.archive_card("d1").is_err(), "cannot archive twice");
        let log = db.get_activity_log(Some("d1"), 10).unwrap();
        assert_eq!(log.iter().filter(|entry| entry.action == "archived").count(), 1);

        let restored = db.unarchive_card("d1").expect("restore individually archived card");
        assert_eq!(restored.archived_at, None);
        assert_eq!(column_orders(&db, &KanbanColumn::Done), ["d2:2", "d1:3"]);

        // A card moved out of Done before the request arrives must be rejected.
        db.move_card("d1", &KanbanColumn::Work, None).unwrap();
        assert!(db.archive_card("d1").is_err());
        assert!(db.list_archived_cards("ws").unwrap().is_empty());

        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn archive_one_card_rolls_back_if_activity_logging_fails() {
        let (db, dir) = temp_db("archive-one-rollback");
        add(&db, "d1", &KanbanColumn::Done);
        db.conn.lock().unwrap().execute_batch("DROP TABLE activity_log").unwrap();

        assert!(db.archive_card("d1").is_err());
        assert_eq!(db.get_card("d1").unwrap().unwrap().archived_at, None);
        assert_eq!(db.list_cards("ws").unwrap().len(), 1);

        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// "Archive all" takes every Done card off the board and nothing else;
    /// the archive lists them; restore appends the card back to Done; a move
    /// (an agent's move_kanban_card on an archived id) restores it too.
    #[test]
    fn archive_hides_done_cards_and_restore_brings_them_back() {
        let (db, dir) = temp_db("archive");
        add(&db, "p1", &KanbanColumn::Plan);
        add(&db, "d1", &KanbanColumn::Done);
        add(&db, "d2", &KanbanColumn::Done);

        let archived = db.archive_done_cards("ws").expect("archive");
        assert_eq!(archived.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["d1", "d2"]);
        assert!(archived.iter().all(|c| c.archived_at.is_some()));

        assert_eq!(column_orders(&db, &KanbanColumn::Done), Vec::<String>::new());
        assert_eq!(column_orders(&db, &KanbanColumn::Plan), ["p1:1"]);
        let listed = db.list_archived_cards("ws").expect("list archive");
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().all(|c| c.column == KanbanColumn::Done));
        // Done is empty now: a second run archives nothing.
        assert!(db.archive_done_cards("ws").expect("archive again").is_empty());
        // A new Done card gets a fresh number, unaware of the archive.
        add(&db, "d3", &KanbanColumn::Done);

        let back = db.unarchive_card("d1").expect("restore d1");
        assert_eq!(back.archived_at, None);
        assert_eq!(column_orders(&db, &KanbanColumn::Done), ["d3:1", "d1:2"]);
        assert!(db.unarchive_card("d1").is_err(), "restoring twice is refused");

        db.move_card("d2", &KanbanColumn::Plan, None).expect("move archived d2");
        assert_eq!(db.get_card("d2").expect("get").expect("card").archived_at, None);
        assert_eq!(column_orders(&db, &KanbanColumn::Plan), ["p1:1", "d2:2"]);
        assert!(db.list_archived_cards("ws").expect("list archive").is_empty());

        // All three land within the same second, so the DESC-by-timestamp
        // order is not stable — compare as a set.
        let log = db.get_activity_log(Some("d1"), 10).expect("log");
        let mut actions: Vec<&str> = log.iter().map(|e| e.action.as_str()).collect();
        actions.sort_unstable();
        assert_eq!(actions, ["archived", "created", "unarchived"]);

        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An archived card cannot be claimed — it is finished work.
    #[test]
    fn archived_card_refuses_claim() {
        let (db, dir) = temp_db("archive-claim");
        add(&db, "d1", &KanbanColumn::Done);
        db.archive_done_cards("ws").expect("archive");
        let err = db.claim_card("d1", "agent-1", "agent/agent-1-d1", "/wt/d1", false).unwrap_err();
        assert!(err.contains("archived"), "{err}");
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A kanban.db from before the archive (v2 schema without `archived_at`)
    /// opens, gets the column, and its cards list as unarchived.
    #[test]
    fn opening_a_pre_archive_db_adds_the_column() {
        let dir = std::env::temp_dir().join(format!("qs-kanban-migrate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("kanban.db");
        {
            let conn = Connection::open(&path).expect("raw open");
            // The v2 schema as it was: same tables, no archived_at column.
            let old_schema = SCHEMA_SQL.replace(",\n    archived_at     INTEGER\n", "\n");
            assert!(!old_schema.contains("archived_at"), "old schema built");
            conn.execute_batch(&old_schema).expect("old schema");
            conn.execute(
                "INSERT INTO kanban_meta (key, value) VALUES ('version', '2')",
                [],
            )
            .expect("meta");
            conn.execute(
                "INSERT INTO kanban_cards (id, workspace_id, title, description, column_name, card_order, priority, status, blocked_by, created_at, updated_at) VALUES ('old1', 'ws', 'Old card', '', 'done', 1, 'medium', 'merged', '[]', 1, 1)",
                [],
            )
            .expect("old row");
        }
        let db = KanbanDb::open(&path).expect("open migrates");
        let cards = db.list_cards("ws").expect("list");
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].archived_at, None);
        assert_eq!(db.archive_done_cards("ws").expect("archive").len(), 1);
        assert!(db.list_cards("ws").expect("list").is_empty());
        // Re-opening runs the migration again — a no-op.
        drop(db);
        let db = KanbanDb::open(&path).expect("reopen");
        assert_eq!(db.list_archived_cards("ws").expect("archive list").len(), 1);
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
