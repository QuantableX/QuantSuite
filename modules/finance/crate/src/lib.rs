//! QuantFinance: a monthly budget alongside manually tracked funds and savings plans.
//! All money is stored as integer cents. Funds supply both balances and budget projections.

mod funds;
use funds::{
    add_fund_entry, book_fund_plan, delete_fund_record, funds_overview, save_fund_account,
    save_fund_plan,
};

use chrono::{Datelike, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
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
pub struct Item {
    pub id: String,
    pub fund_id: Option<String>,
    pub fund_plan_count: usize,
    /// income | saving | expense
    pub kind: String,
    pub name: String,
    /// Cents **per occurrence**, not per month.
    pub amount_cents: i64,
    /// 1 = monthly, 3 = quarterly, 12 = yearly.
    pub every_months: i64,
    /// The same amount reduced to a month, computed once here so the frontend
    /// never has to agree with the backend about rounding.
    pub monthly_cents: i64,
    pub color: String,
    pub notes: Option<String>,
    pub is_active: bool,
    pub sort_index: f64,
    /// Saving items only: what this is for, and how much is already put aside.
    pub target_cents: Option<i64>,
    pub saved_cents: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemInput {
    pub kind: String,
    pub name: String,
    pub amount_cents: i64,
    pub every_months: Option<i64>,
    pub notes: Option<String>,
    pub target_cents: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemPatch {
    pub kind: Option<String>,
    pub name: Option<String>,
    pub amount_cents: Option<i64>,
    pub every_months: Option<i64>,
    #[serde(default, deserialize_with = "deserialize_nullable")]
    pub notes: Option<Option<String>>,
    pub is_active: Option<bool>,
    pub sort_index: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_nullable")]
    pub target_cents: Option<Option<i64>>,
    pub saved_cents: Option<i64>,
}

// Distinguish an omitted patch field from an explicitly cleared target.
fn deserialize_nullable<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// The four figures the whole module exists to produce.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub income_cents: i64,
    pub saving_cents: i64,
    pub expense_cents: i64,
    /// income − saving − expense. Negative when the plan does not add up, and
    /// that is exactly the case worth showing loudly.
    pub leftover_cents: i64,
    /// Saved plus leftover, over income, in percent. `null` without income.
    pub savings_rate: Option<i64>,
}

/// A target, from either of the two places one can come from.
///
/// **plan** — a saving line with a number to reach. Its rate is the line's own
/// monthly amount, so the money is part of the monthly plan.
///
/// **manual** — a target that stands on its own, with a rate typed by hand or
/// none at all. Nothing about it touches the plan's totals: it is for money
/// that comes out of the leftover, out of a bonus, or is simply a pot being
/// watched.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalProgress {
    pub id: String,
    /// plan | manual
    pub source: String,
    pub name: String,
    pub color: String,
    pub target_cents: i64,
    pub saved_cents: i64,
    pub monthly_cents: i64,
    pub remaining_cents: i64,
    /// Months at the planned rate, or `null` when nothing is going in.
    pub months_left: Option<i64>,
    /// `YYYY-MM` the target is reached, or `null`.
    pub reached_on: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetPatch {
    pub name: Option<String>,
    pub target_cents: Option<i64>,
    pub saved_cents: Option<i64>,
    pub monthly_cents: Option<i64>,
    pub sort_index: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SankeyNode {
    pub id: String,
    pub fund_id: Option<String>,
    pub label: String,
    /// income | hub | saving | expense | leftover
    pub kind: String,
    pub value_cents: i64,
    pub color: String,
    pub layer: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SankeyLink {
    pub source: String,
    pub target: String,
    pub value_cents: i64,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SankeyData {
    pub nodes: Vec<SankeyNode>,
    pub links: Vec<SankeyLink>,
    #[serde(flatten)]
    pub summary: Summary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub currency: String,
    pub locale: String,
    pub sidebar_left_open: bool,
    pub sidebar_right_open: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            currency: "EUR".into(),
            locale: "de-DE".into(),
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
    Utc::now().to_rfc3339()
}

fn new_id() -> String {
    Uuid::new_v4().to_string()
}

fn data_dir() -> PathBuf {
    qs_core::paths::module_dir("finance")
}

fn db_path(base: &Path) -> PathBuf {
    base.join("finance.db")
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

/// An amount reduced to one month.
///
/// Rounded, not truncated: 100 € a quarter is 33,33 € a month, and truncating
/// would quietly lose a cent off the leftover on every such line. Computed in
/// exactly one place so the chart, the totals and the list can never disagree.
fn monthly_cents(amount_cents: i64, every_months: i64) -> i64 {
    let every = every_months.max(1);
    if every == 1 {
        return amount_cents;
    }
    let half = every / 2;
    if amount_cents >= 0 {
        (amount_cents + half) / every
    } else {
        (amount_cents - half) / every
    }
}

const KINDS: [&str; 3] = ["income", "saving", "expense"];

/// The colour of a line is its *meaning*, not a preference.
///
/// Green in, yellow set aside, red gone. Saving is deliberately not red: the
/// money has not left, it has moved somewhere else that is still yours, and
/// colouring it like a cost makes a healthy plan look alarming. Derived rather
/// than stored, so an old row picks up the rule the moment it is read.
fn color_for(kind: &str) -> &'static str {
    match kind {
        "income" => "income",
        "saving" => "saving",
        _ => "expense",
    }
}

fn check_kind(kind: &str) -> Result<(), String> {
    if KINDS.contains(&kind) {
        Ok(())
    } else {
        Err(format!("Unknown item kind: {kind}"))
    }
}

// ─── Schema ───────────────────────────────────────────────────────────────

fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        -- One recurring line. `kind` is income | saving | expense; irregular
        -- spending is never entered at all and comes out of the leftover.
        CREATE TABLE IF NOT EXISTS items (
            id TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            name TEXT NOT NULL,
            amount_cents INTEGER NOT NULL,
            every_months INTEGER NOT NULL DEFAULT 1,
            color TEXT NOT NULL,
            notes TEXT,
            is_active INTEGER NOT NULL DEFAULT 1,
            sort_index REAL NOT NULL DEFAULT 0,
            target_cents INTEGER,
            saved_cents INTEGER NOT NULL DEFAULT 0,
            -- The month `saved_cents` was last true for (year * 12 + month0).
            -- Every month after it adds one monthly amount; see `roll_forward`.
            saved_as_of INTEGER,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_items_kind ON items(kind);

        -- A target that stands on its own, outside the monthly plan. Its rate
        -- is typed by hand and is deliberately NOT part of any total: this is
        -- for money coming out of the leftover or a bonus, or for a pot that
        -- is only being watched.
        CREATE TABLE IF NOT EXISTS targets (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            target_cents INTEGER NOT NULL,
            saved_cents INTEGER NOT NULL DEFAULT 0,
            monthly_cents INTEGER NOT NULL DEFAULT 0,
            saved_as_of INTEGER,
            sort_index REAL NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value_json TEXT NOT NULL
        );
        ",
    )
    .map_err(|e| format!("Init schema: {e}"))?;
    migrate_saved_as_of(conn)?;
    funds::init_schema(conn)
}

/// Databases from before the saved amount kept up by itself have no
/// `saved_as_of`. Add the column and anchor every row at the month it was
/// last edited: that is when its saved amount was last known to be right, so
/// the months since then are paid in on the next load, exactly as if the
/// column had always been there.
fn migrate_saved_as_of(conn: &Connection) -> Result<(), String> {
    for table in ["items", "targets"] {
        let has_column = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .and_then(|mut stmt| {
                stmt.query_map([], |row| row.get::<_, String>(1))?
                    .collect::<Result<Vec<_>, _>>()
            })
            .map_err(|e| format!("Inspect {table}: {e}"))?
            .iter()
            .any(|name| name == "saved_as_of");
        if has_column {
            continue;
        }
        conn.execute(
            &format!("ALTER TABLE {table} ADD COLUMN saved_as_of INTEGER"),
            [],
        )
        .map_err(|e| format!("Add saved_as_of to {table}: {e}"))?;
        // `updated_at` is RFC 3339, so the year and month sit at fixed offsets.
        conn.execute(
            &format!(
                "UPDATE {table} SET saved_as_of =
                    CAST(substr(updated_at, 1, 4) AS INTEGER) * 12
                  + CAST(substr(updated_at, 6, 2) AS INTEGER) - 1"
            ),
            [],
        )
        .map_err(|e| format!("Anchor saved_as_of in {table}: {e}"))?;
    }
    Ok(())
}

// ─── Saved amounts keep up by themselves ──────────────────────────────────

/// Months since year zero, so two months subtract to a count of months.
fn month_index(year: i32, month0: u32) -> i64 {
    year as i64 * 12 + month0 as i64
}

fn current_month() -> i64 {
    let today = Utc::now().date_naive();
    month_index(today.year(), today.month0())
}

/// The saved amount after `months` more months at `monthly`. A row that has
/// been switched off is not paying in, so it only moves its anchor.
fn accrue(saved_cents: i64, monthly_cents: i64, months: i64, active: bool) -> i64 {
    if active {
        saved_cents + monthly_cents * months.max(0)
    } else {
        saved_cents
    }
}

/// Pay every month since each row's `saved_as_of` into its saved amount and
/// move the anchor to `this_month`. Idempotent: a second call in the same
/// month changes nothing, so every read path may call it.
///
/// The result is written back rather than computed on the fly, so a later
/// change of the rate applies from now on and does not rewrite the months
/// already paid in. A row that never had an anchor (`NULL`) gets one without
/// back pay, because there is nothing to know how far back it should go.
fn roll_forward(conn: &Connection, this_month: i64) -> Result<(), String> {
    let now = now_iso();

    let items: Vec<(String, i64, i64, i64, bool, Option<i64>)> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, amount_cents, every_months, saved_cents, is_active, saved_as_of
                   FROM items
                  WHERE kind = 'saving' AND (saved_as_of IS NULL OR saved_as_of < ?1)",
            )
            .map_err(|e| format!("Prepare roll-forward: {e}"))?;
        let rows = stmt
            .query_map(params![this_month], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get::<_, i64>(4)? != 0,
                    row.get(5)?,
                ))
            })
            .map_err(|e| format!("Query roll-forward: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Read roll-forward: {e}"))?;
        rows
    };
    for (id, amount, every, saved, active, as_of) in items {
        let saved = match as_of {
            Some(from) => accrue(
                saved,
                monthly_cents(amount, every),
                this_month - from,
                active,
            ),
            None => saved,
        };
        conn.execute(
            "UPDATE items SET saved_cents = ?1, saved_as_of = ?2, updated_at = ?3 WHERE id = ?4",
            params![saved, this_month, now, id],
        )
        .map_err(|e| format!("Roll item forward: {e}"))?;
    }

    let targets: Vec<(String, i64, i64, Option<i64>)> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, saved_cents, monthly_cents, saved_as_of
                   FROM targets
                  WHERE saved_as_of IS NULL OR saved_as_of < ?1",
            )
            .map_err(|e| format!("Prepare target roll-forward: {e}"))?;
        let rows = stmt
            .query_map(params![this_month], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .map_err(|e| format!("Query target roll-forward: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Read target roll-forward: {e}"))?;
        rows
    };
    for (id, saved, monthly, as_of) in targets {
        let saved = match as_of {
            Some(from) => accrue(saved, monthly, this_month - from, true),
            None => saved,
        };
        conn.execute(
            "UPDATE targets SET saved_cents = ?1, saved_as_of = ?2, updated_at = ?3 WHERE id = ?4",
            params![saved, this_month, now, id],
        )
        .map_err(|e| format!("Roll target forward: {e}"))?;
    }
    Ok(())
}

const SEED_KEY: &str = "schema.seeded";

/// An empty plan is a blank page with no shape. A handful of typical lines at
/// zero shows what belongs where; the user overwrites the amounts and deletes
/// what does not apply. Marked, so a deleted line does not come back.
fn seed_schema(conn: &Connection) -> Result<(), String> {
    let seeded: Option<String> = conn
        .query_row(
            "SELECT value_json FROM settings WHERE key = ?1",
            params![SEED_KEY],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("Read seed marker: {e}"))?;
    if seeded.is_some() {
        return Ok(());
    }
    let now = now_iso();

    let seeds: [(&str, &str); 6] = [
        ("income", "Salary"),
        ("saving", "Savings"),
        ("saving", "Investments"),
        ("expense", "Rent"),
        ("expense", "Utilities"),
        ("expense", "Subscriptions"),
    ];
    for (i, (kind, name)) in seeds.iter().enumerate() {
        conn.execute(
            "INSERT INTO items (id, kind, name, amount_cents, every_months, color, notes,
                                is_active, sort_index, target_cents, saved_cents, saved_as_of,
                                created_at, updated_at)
             VALUES (?1, ?2, ?3, 0, 1, ?4, NULL, 1, ?5, NULL, 0, ?6, ?7, ?7)",
            params![
                new_id(),
                kind,
                name,
                color_for(kind),
                i as f64,
                current_month(),
                now
            ],
        )
        .map_err(|e| format!("Seed item: {e}"))?;
    }

    conn.execute(
        "INSERT INTO settings (key, value_json) VALUES (?1, 'true')",
        params![SEED_KEY],
    )
    .map_err(|e| format!("Write seed marker: {e}"))?;
    Ok(())
}

// ─── Items ────────────────────────────────────────────────────────────────

const ITEM_COLUMNS: &str = "id, kind, name, amount_cents, every_months, color, notes, \
                            is_active, sort_index, target_cents, saved_cents, created_at, updated_at";

fn read_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<Item> {
    let amount: i64 = row.get(3)?;
    let every: i64 = row.get(4)?;
    let kind: String = row.get(1)?;
    let color = color_for(&kind).to_string();
    Ok(Item {
        id: row.get(0)?,
        fund_id: None,
        fund_plan_count: 0,
        kind,
        name: row.get(2)?,
        amount_cents: amount,
        every_months: every,
        monthly_cents: monthly_cents(amount, every),
        color,
        notes: row.get(6)?,
        is_active: row.get::<_, i64>(7)? != 0,
        sort_index: row.get(8)?,
        target_cents: row.get(9)?,
        saved_cents: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

/// The plan, in the order it is shown: income, then saving, then fixed costs.
#[tauri::command(async)]
fn list_items(state: State<'_, AppState>) -> Result<Vec<Item>, String> {
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    roll_forward(&conn, current_month())?;
    read_budget_items(&conn)
}

fn read_budget_items(conn: &Connection) -> Result<Vec<Item>, String> {
    let sql = format!(
        "SELECT {ITEM_COLUMNS} FROM items WHERE kind != 'saving'
          ORDER BY CASE kind WHEN 'income' THEN 0 WHEN 'saving' THEN 1 ELSE 2 END,
                   sort_index, name"
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("Prepare items: {e}"))?;
    let mut rows = stmt
        .query_map([], read_item)
        .map_err(|e| format!("Query items: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Read items: {e}"))?;
    rows.extend(funds::budget_items(conn)?);
    rows.sort_by(|a, b| {
        let rank = |kind: &str| match kind {
            "income" => 0,
            "saving" => 1,
            _ => 2,
        };
        rank(&a.kind)
            .cmp(&rank(&b.kind))
            .then(a.sort_index.total_cmp(&b.sort_index))
    });
    Ok(rows)
}

#[tauri::command(async)]
fn get_item(id: String, state: State<'_, AppState>) -> Result<Item, String> {
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    if let Some(item) = funds::budget_items(&conn)?.into_iter().find(|i| i.id == id) {
        return Ok(item);
    }
    let sql = format!("SELECT {ITEM_COLUMNS} FROM items WHERE id = ?1");
    conn.query_row(&sql, params![id], read_item)
        .map_err(|e| format!("Get item: {e}"))
}

#[tauri::command(async)]
fn create_item(
    item: ItemInput,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Item, String> {
    check_kind(&item.kind)?;
    if item.kind == "saving" {
        let id = {
            let arc = db(&state);
            let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
            funds::create_budget_item(&conn, item)?
        };
        emit(&app, "finance:funds-changed", json!({}));
        emit(&app, "finance:plan-changed", json!({}));
        return get_item(id, state);
    }
    let id = new_id();
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let now = now_iso();
        let next: f64 = conn
            .query_row(
                "SELECT COALESCE(MAX(sort_index), -1) + 1 FROM items WHERE kind = ?1",
                params![item.kind],
                |r| r.get(0),
            )
            .map_err(|e| format!("Next item index: {e}"))?;
        conn.execute(
            "INSERT INTO items (id, kind, name, amount_cents, every_months, color, notes,
                                is_active, sort_index, target_cents, saved_cents, saved_as_of,
                                created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?9, 0, ?10, ?11, ?11)",
            params![
                id,
                item.kind,
                item.name,
                item.amount_cents,
                item.every_months.unwrap_or(1).max(1),
                color_for(&item.kind),
                item.notes,
                next,
                item.target_cents,
                current_month(),
                now
            ],
        )
        .map_err(|e| format!("Create item: {e}"))?;
    }
    emit(&app, "finance:plan-changed", json!({ "id": id.clone() }));
    get_item(id, state)
}

#[tauri::command(async)]
fn update_item(
    id: String,
    patch: ItemPatch,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Item, String> {
    let current = get_item(id.clone(), state.clone())?;
    if current.fund_id.is_some() {
        {
            let arc = db(&state);
            let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
            funds::update_budget_item(&conn, &id, patch)?;
        }
        emit(&app, "finance:funds-changed", json!({}));
        emit(&app, "finance:plan-changed", json!({}));
        return get_item(id, state);
    }
    if patch.kind.as_deref() == Some("saving") {
        return Err("Create a fund to add a saving line.".into());
    }
    if let Some(kind) = &patch.kind {
        check_kind(kind)?;
    }
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let now = now_iso();

        macro_rules! set {
            ($field:expr, $sql:literal) => {
                if let Some(value) = $field {
                    conn.execute(
                        concat!(
                            "UPDATE items SET ",
                            $sql,
                            " = ?1, updated_at = ?2 WHERE id = ?3"
                        ),
                        params![value, now, id],
                    )
                    .map_err(|e| format!(concat!("Update ", $sql, ": {}"), e))?;
                }
            };
        }

        if let Some(kind) = &patch.kind {
            conn.execute(
                "UPDATE items SET kind = ?1, color = ?2, updated_at = ?3 WHERE id = ?4",
                params![kind, color_for(kind), now, id],
            )
            .map_err(|e| format!("Update kind: {e}"))?;
        }
        set!(patch.name, "name");
        set!(patch.amount_cents, "amount_cents");
        set!(patch.notes, "notes");
        set!(patch.sort_index, "sort_index");
        set!(patch.target_cents, "target_cents");
        // A hand-typed saved amount is true for this month: it becomes the new
        // anchor, so the months before it are never paid in a second time.
        if let Some(saved) = patch.saved_cents {
            conn.execute(
                "UPDATE items SET saved_cents = ?1, saved_as_of = ?2, updated_at = ?3 WHERE id = ?4",
                params![saved, current_month(), now, id],
            )
            .map_err(|e| format!("Update saved_cents: {e}"))?;
        }
        if let Some(every) = patch.every_months {
            conn.execute(
                "UPDATE items SET every_months = ?1, updated_at = ?2 WHERE id = ?3",
                params![every.max(1), now, id],
            )
            .map_err(|e| format!("Update every_months: {e}"))?;
        }
        if let Some(active) = patch.is_active {
            conn.execute(
                "UPDATE items SET is_active = ?1, updated_at = ?2 WHERE id = ?3",
                params![active as i64, now, id],
            )
            .map_err(|e| format!("Update is_active: {e}"))?;
        }
    }
    emit(&app, "finance:plan-changed", json!({ "id": id.clone() }));
    get_item(id, state)
}

#[tauri::command(async)]
fn delete_item(id: String, state: State<'_, AppState>, app: AppHandle) -> Result<bool, String> {
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let is_fund: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM funds WHERE id=?1)",
                [&id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if is_fund {
            funds::delete_budget_item(&conn, &id)?;
        } else {
            conn.execute("DELETE FROM items WHERE id = ?1", params![id])
                .map_err(|e| format!("Delete item: {e}"))?;
        }
    }
    emit(&app, "finance:funds-changed", json!({}));
    emit(&app, "finance:plan-changed", json!({ "id": id }));
    Ok(true)
}

// ─── Summary ──────────────────────────────────────────────────────────────

/// Sum the active items of one kind, each reduced to a month.
///
/// The reduction happens per row and the rounded results are added — not the
/// other way round. Summing first and dividing once would make the total
/// disagree with the rows the user is reading, which is worse than being a
/// cent off the mathematically pure answer.
fn kind_total(conn: &Connection, kind: &str) -> Result<i64, String> {
    if kind == "saving" {
        return funds::budget_items(conn)?
            .into_iter()
            .try_fold(0i64, |sum, item| {
                sum.checked_add(item.monthly_cents)
                    .filter(|n| *n <= 9_007_199_254_740_991)
                    .ok_or_else(|| "Planned contributions are too large.".into())
            });
    }
    let mut stmt = conn
        .prepare("SELECT amount_cents, every_months FROM items WHERE kind = ?1 AND is_active = 1")
        .map_err(|e| format!("Prepare {kind} total: {e}"))?;
    let total: i64 = stmt
        .query_map(params![kind], |row| {
            Ok(monthly_cents(row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|e| format!("Query {kind} total: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Read {kind} total: {e}"))?
        .into_iter()
        .sum();
    Ok(total)
}

fn build_summary(conn: &Connection) -> Result<Summary, String> {
    let income = kind_total(conn, "income")?;
    let saving = kind_total(conn, "saving")?;
    let expense = kind_total(conn, "expense")?;
    let leftover = income - saving - expense;
    let savings_rate = if income > 0 {
        Some(((saving + leftover.max(0)) as f64 / income as f64 * 100.0).round() as i64)
    } else {
        None
    };
    Ok(Summary {
        income_cents: income,
        saving_cents: saving,
        expense_cents: expense,
        leftover_cents: leftover,
        savings_rate,
    })
}

#[tauri::command(async)]
fn summary(state: State<'_, AppState>) -> Result<Summary, String> {
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    build_summary(&conn)
}

// ─── Sankey ───────────────────────────────────────────────────────────────

/// The plan as a flow: income lines → one hub → saving, fixed costs, leftover.
///
/// The leftover is a node like any other, so the bands add up to the income
/// and nobody goes looking for a missing slice. When the plan does not balance
/// the leftover is negative and simply does not appear — the picture then
/// shows more going out than coming in, which is the honest rendering.
#[tauri::command(async)]
fn sankey(state: State<'_, AppState>) -> Result<SankeyData, String> {
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;

    build_sankey(&conn)
}

fn build_sankey(conn: &Connection) -> Result<SankeyData, String> {
    let items = read_budget_items(conn)?
        .into_iter()
        .filter(|i| i.is_active)
        .collect::<Vec<_>>();

    let summary = build_summary(conn)?;

    const HUB: &str = "hub:income";
    let mut nodes = vec![SankeyNode {
        id: HUB.into(),
        fund_id: None,
        label: "Income".into(),
        kind: "hub".into(),
        value_cents: summary.income_cents,
        color: "income".into(),
        layer: 1,
    }];
    let mut links = Vec::new();

    let mut incoming: Vec<&Item> = items
        .iter()
        .filter(|i| i.kind == "income" && i.monthly_cents > 0)
        .collect();
    incoming.sort_by_key(|entry| std::cmp::Reverse(entry.monthly_cents));
    for item in incoming {
        nodes.push(SankeyNode {
            id: format!("in:{}", item.id),
            fund_id: None,
            label: item.name.clone(),
            kind: "income".into(),
            value_cents: item.monthly_cents,
            color: color_for(&item.kind).into(),
            layer: 0,
        });
        links.push(SankeyLink {
            source: format!("in:{}", item.id),
            target: HUB.into(),
            value_cents: item.monthly_cents,
            color: color_for(&item.kind).into(),
        });
    }

    // Saving before fixed costs, both biggest first — the order the bands are
    // stacked in is the order the eye reads them.
    let mut outgoing: Vec<&Item> = items
        .iter()
        .filter(|i| (i.kind == "saving" || i.kind == "expense") && i.monthly_cents > 0)
        .collect();
    outgoing.sort_by(|a, b| {
        let rank = |k: &str| i32::from(k != "saving");
        rank(&a.kind)
            .cmp(&rank(&b.kind))
            .then(b.monthly_cents.cmp(&a.monthly_cents))
    });
    for item in outgoing {
        nodes.push(SankeyNode {
            id: format!("out:{}", item.id),
            fund_id: item.fund_id.clone(),
            label: item.name.clone(),
            kind: item.kind.clone(),
            value_cents: item.monthly_cents,
            color: color_for(&item.kind).into(),
            layer: 2,
        });
        links.push(SankeyLink {
            source: HUB.into(),
            target: format!("out:{}", item.id),
            value_cents: item.monthly_cents,
            color: color_for(&item.kind).into(),
        });
    }

    if summary.leftover_cents > 0 {
        nodes.push(SankeyNode {
            id: "leftover".into(),
            fund_id: None,
            label: "Leftover".into(),
            kind: "leftover".into(),
            value_cents: summary.leftover_cents,
            color: "leftover".into(),
            layer: 2,
        });
        links.push(SankeyLink {
            source: HUB.into(),
            target: "leftover".into(),
            value_cents: summary.leftover_cents,
            color: "leftover".into(),
        });
    }

    Ok(SankeyData {
        nodes,
        links,
        summary,
    })
}

// ─── Goals ────────────────────────────────────────────────────────────────

/// The projection, shared by both kinds of target.
///
/// Ceiling division: a remainder still needs a whole month. A rate of zero has
/// no answer at all — `None` rather than an invented date, because "never at
/// this rate" is the useful thing to say.
#[allow(clippy::too_many_arguments)]
fn project(
    id: String,
    source: &str,
    name: String,
    target_cents: i64,
    saved_cents: i64,
    monthly_cents: i64,
    this_month: i64,
) -> GoalProgress {
    let remaining = (target_cents - saved_cents).max(0);
    let months = if remaining == 0 {
        Some(0)
    } else if monthly_cents > 0 {
        // `div_ceil` on integers is still unstable on this toolchain.
        Some((remaining + monthly_cents - 1) / monthly_cents)
    } else {
        None
    };
    let reached_on = months.map(|m| {
        let at = this_month + m;
        format!("{:04}-{:02}", at.div_euclid(12), at.rem_euclid(12) + 1)
    });
    GoalProgress {
        id,
        source: source.into(),
        name,
        // Money set aside, whichever way it got there.
        color: "saving".into(),
        target_cents,
        saved_cents,
        monthly_cents,
        remaining_cents: remaining,
        months_left: months,
        reached_on,
    }
}

/// Every target, from both places one can come from.
///
/// **plan** — a saving line that carries a number to reach. Its rate is that
/// line's own monthly amount, so the money is part of the monthly plan and
/// there is nothing to keep in step.
///
/// **manual** — a target that stands on its own, with a rate typed by hand or
/// none at all. It touches no total: this is for money coming out of the
/// leftover or a bonus, or for a pot that is only being watched.
#[tauri::command(async)]
fn goals(state: State<'_, AppState>) -> Result<Vec<GoalProgress>, String> {
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;

    build_goals(&conn)
}

fn build_goals(conn: &Connection) -> Result<Vec<GoalProgress>, String> {
    let this_month = current_month();
    roll_forward(conn, this_month)?;
    let mut out = Vec::new();

    let items = funds::budget_items(conn)?
        .into_iter()
        .filter(|i| i.target_cents.is_some_and(|n| n > 0));
    for item in items {
        out.push(project(
            item.id,
            "plan",
            item.name,
            item.target_cents.unwrap_or(0),
            item.saved_cents,
            item.monthly_cents,
            this_month,
        ));
    }

    let standalone = {
        let mut stmt = conn
            .prepare(
                "SELECT id, name, target_cents, saved_cents, monthly_cents
                   FROM targets ORDER BY sort_index, created_at",
            )
            .map_err(|e| format!("Prepare targets: {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })
            .map_err(|e| format!("Query targets: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Read targets: {e}"))?;
        rows
    };
    for (id, name, target, saved, monthly) in standalone {
        out.push(project(
            id, "manual", name, target, saved, monthly, this_month,
        ));
    }

    Ok(out)
}

#[tauri::command(async)]
fn create_target(name: String, state: State<'_, AppState>, app: AppHandle) -> Result<bool, String> {
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let now = now_iso();
        let next: f64 = conn
            .query_row(
                "SELECT COALESCE(MAX(sort_index), -1) + 1 FROM targets",
                [],
                |r| r.get(0),
            )
            .map_err(|e| format!("Next target index: {e}"))?;
        conn.execute(
            "INSERT INTO targets (id, name, target_cents, saved_cents, monthly_cents,
                                  saved_as_of, sort_index, created_at, updated_at)
             VALUES (?1, ?2, 0, 0, 0, ?3, ?4, ?5, ?5)",
            params![new_id(), name, current_month(), next, now],
        )
        .map_err(|e| format!("Create target: {e}"))?;
    }
    emit(&app, "finance:plan-changed", json!({ "what": "targets" }));
    Ok(true)
}

#[tauri::command(async)]
fn update_target(
    id: String,
    patch: TargetPatch,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<bool, String> {
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let now = now_iso();

        macro_rules! set {
            ($field:expr, $sql:literal) => {
                if let Some(value) = $field {
                    conn.execute(
                        concat!(
                            "UPDATE targets SET ",
                            $sql,
                            " = ?1, updated_at = ?2 WHERE id = ?3"
                        ),
                        params![value, now, id],
                    )
                    .map_err(|e| format!(concat!("Update ", $sql, ": {}"), e))?;
                }
            };
        }

        set!(patch.name, "name");
        set!(patch.target_cents, "target_cents");
        if let Some(saved) = patch.saved_cents {
            conn.execute(
                "UPDATE targets SET saved_cents = ?1, saved_as_of = ?2, updated_at = ?3 WHERE id = ?4",
                params![saved, current_month(), now, id],
            )
            .map_err(|e| format!("Update saved_cents: {e}"))?;
        }
        set!(patch.monthly_cents, "monthly_cents");
        set!(patch.sort_index, "sort_index");
    }
    emit(&app, "finance:plan-changed", json!({ "what": "targets" }));
    Ok(true)
}

#[tauri::command(async)]
fn delete_target(id: String, state: State<'_, AppState>, app: AppHandle) -> Result<bool, String> {
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        conn.execute("DELETE FROM targets WHERE id = ?1", params![id])
            .map_err(|e| format!("Delete target: {e}"))?;
    }
    emit(&app, "finance:plan-changed", json!({ "what": "targets" }));
    Ok(true)
}

// ─── Settings and backup ──────────────────────────────────────────────────

fn load_app_settings(base: &Path) -> AppSettings {
    fs::read_to_string(settings_path(base))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_app_settings(base: &Path, settings: &AppSettings) -> Result<(), String> {
    let raw =
        serde_json::to_string_pretty(settings).map_err(|e| format!("Serialize settings: {e}"))?;
    qs_core::paths::write_atomic(&settings_path(base), raw.as_bytes())
        .map_err(|e| format!("Write settings: {e}"))
}

#[tauri::command(async)]
fn get_app_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    let guard = state
        .app_settings
        .lock()
        .map_err(|e| format!("Lock settings: {e}"))?;
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
        let mut guard = state
            .app_settings
            .lock()
            .map_err(|e| format!("Lock settings: {e}"))?;
        *guard = settings;
        save_app_settings(&state.data_dir, &guard)?;
        guard.clone()
    };
    Ok(saved)
}

/// Async: copying the whole database is unbounded work.
#[tauri::command]
async fn backup_database(state: State<'_, AppState>) -> Result<String, String> {
    let target = state
        .data_dir
        .join("backups")
        .join(format!("finance-{}.db", Utc::now().format("%Y%m%d-%H%M%S")));
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let mut dst = Connection::open(&target).map_err(|e| format!("Open backup target: {e}"))?;
        let backup = rusqlite::backup::Backup::new(&conn, &mut dst)
            .map_err(|e| format!("Start backup: {e}"))?;
        backup
            .run_to_completion(5, std::time::Duration::from_millis(50), None)
            .map_err(|e| format!("Run backup: {e}"))?;
    }
    Ok(target.to_string_lossy().to_string())
}

// ─── Entry point ──────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Build the `finance` plugin. Pinned to `Wry` for the same reason as
/// `systems`: bare `tauri::AppHandle` already means `AppHandle<Wry>`.
pub fn init() -> TauriPlugin<Wry> {
    Builder::<Wry>::new("finance")
        .setup(|app, _api| {
            let dir = data_dir();
            ensure_dirs(&dir).map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

            let conn = Connection::open(db_path(&dir))?;
            conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
            init_schema(&conn).map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            seed_schema(&conn).map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            funds::migrate_budget(&conn).map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

            let settings = load_app_settings(&dir);
            app.manage(AppState {
                data_dir: dir,
                db: Arc::new(Mutex::new(conn)),
                app_settings: Mutex::new(settings),
            });

            qs_core::tray::register_entry(
                app,
                qs_core::tray::TrayEntry {
                    module: "finance".into(),
                    label: "QuantFinance".into(),
                    route: "/finance".into(),
                    toggle_window: None,
                },
            );

            Ok(())
        })
        .invoke_handler(qs_core::diagnostics::traced(tauri::generate_handler![
            list_items,
            get_item,
            create_item,
            update_item,
            delete_item,
            summary,
            sankey,
            goals,
            create_target,
            update_target,
            delete_target,
            get_app_settings,
            update_app_settings,
            backup_database,
            funds_overview,
            save_fund_account,
            add_fund_entry,
            delete_fund_record,
            save_fund_plan,
            book_fund_plan,
        ]))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_db() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        init_schema(&conn).expect("schema");
        conn
    }

    fn saved(conn: &Connection, table: &str, id: &str) -> (i64, Option<i64>) {
        conn.query_row(
            &format!("SELECT saved_cents, saved_as_of FROM {table} WHERE id = ?1"),
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("row")
    }

    #[test]
    fn accrual_adds_one_rate_per_month_and_nothing_when_switched_off() {
        assert_eq!(accrue(900_000, 100_000, 1, true), 1_000_000);
        assert_eq!(accrue(900_000, 100_000, 3, true), 1_200_000);
        assert_eq!(accrue(900_000, 100_000, 0, true), 900_000);
        assert_eq!(accrue(900_000, 100_000, -2, true), 900_000);
        assert_eq!(accrue(900_000, 100_000, 3, false), 900_000);
    }

    /// 9.000 saved as of last month at 1.000 a month reads 10.000 this month,
    /// and reading it again in the same month leaves it at 10.000.
    #[test]
    fn a_plan_target_gains_its_monthly_amount_each_month_once() {
        let conn = memory_db();
        let august = month_index(2026, 7);
        conn.execute(
            "INSERT INTO items (id, kind, name, amount_cents, every_months, color, notes, is_active,
                                sort_index, target_cents, saved_cents, saved_as_of, created_at, updated_at)
             VALUES ('s', 'saving', 'Savings', 100000, 1, 'saving', NULL, 1, 0, 1000000, 900000, ?1, 't', 't')",
            params![august],
        )
        .unwrap();

        roll_forward(&conn, august + 1).unwrap();
        assert_eq!(saved(&conn, "items", "s"), (1_000_000, Some(august + 1)));
        roll_forward(&conn, august + 1).unwrap();
        assert_eq!(saved(&conn, "items", "s"), (1_000_000, Some(august + 1)));
        // Two more months, a quarterly line: 300 a quarter is 100 a month.
        conn.execute(
            "UPDATE items SET amount_cents = 30000, every_months = 3 WHERE id = 's'",
            [],
        )
        .unwrap();
        roll_forward(&conn, august + 3).unwrap();
        assert_eq!(saved(&conn, "items", "s"), (1_020_000, Some(august + 3)));
    }

    /// A line that is switched off pays nothing in but still moves its
    /// anchor, so switching it back on does not back-pay the months off.
    #[test]
    fn an_inactive_line_moves_its_anchor_without_paying_in() {
        let conn = memory_db();
        let august = month_index(2026, 7);
        conn.execute(
            "INSERT INTO items (id, kind, name, amount_cents, every_months, color, notes, is_active,
                                sort_index, target_cents, saved_cents, saved_as_of, created_at, updated_at)
             VALUES ('s', 'saving', 'Savings', 100000, 1, 'saving', NULL, 0, 0, 1000000, 900000, ?1, 't', 't')",
            params![august],
        )
        .unwrap();
        roll_forward(&conn, august + 2).unwrap();
        assert_eq!(saved(&conn, "items", "s"), (900_000, Some(august + 2)));
    }

    #[test]
    fn a_standalone_target_uses_its_own_rate() {
        let conn = memory_db();
        let august = month_index(2026, 7);
        conn.execute(
            "INSERT INTO targets (id, name, target_cents, saved_cents, monthly_cents, saved_as_of,
                                  sort_index, created_at, updated_at)
             VALUES ('e', 'Emergency', 1000000, 900000, 50000, ?1, 0, 't', 't')",
            params![august],
        )
        .unwrap();
        roll_forward(&conn, august + 2).unwrap();
        assert_eq!(saved(&conn, "targets", "e"), (1_000_000, Some(august + 2)));
    }

    /// A row without an anchor is given one, without inventing back pay.
    #[test]
    fn a_row_without_an_anchor_gets_one_and_no_back_pay() {
        let conn = memory_db();
        conn.execute(
            "INSERT INTO targets (id, name, target_cents, saved_cents, monthly_cents, saved_as_of,
                                  sort_index, created_at, updated_at)
             VALUES ('e', 'Emergency', 1000000, 900000, 50000, NULL, 0, 't', 't')",
            [],
        )
        .unwrap();
        let now = month_index(2026, 8);
        roll_forward(&conn, now).unwrap();
        assert_eq!(saved(&conn, "targets", "e"), (900_000, Some(now)));
    }

    /// An older database is anchored at each row's last edit, so the months
    /// since then are paid in on the next load.
    #[test]
    fn an_older_database_is_anchored_at_the_last_edit() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE items (id TEXT PRIMARY KEY, kind TEXT NOT NULL, name TEXT NOT NULL,
                amount_cents INTEGER NOT NULL, every_months INTEGER NOT NULL DEFAULT 1,
                color TEXT NOT NULL, notes TEXT, is_active INTEGER NOT NULL DEFAULT 1,
                sort_index REAL NOT NULL DEFAULT 0, target_cents INTEGER,
                saved_cents INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
             CREATE TABLE targets (id TEXT PRIMARY KEY, name TEXT NOT NULL, target_cents INTEGER NOT NULL,
                saved_cents INTEGER NOT NULL DEFAULT 0, monthly_cents INTEGER NOT NULL DEFAULT 0,
                sort_index REAL NOT NULL DEFAULT 0, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
             INSERT INTO items VALUES ('s', 'saving', 'Savings', 100000, 1, 'saving', NULL, 1, 0,
                1000000, 900000, '2026-08-14T09:30:00+00:00', '2026-08-14T09:30:00+00:00');
             INSERT INTO targets VALUES ('e', 'Emergency', 1000000, 900000, 100000, 0,
                '2026-07-01T00:00:00+00:00', '2026-07-01T00:00:00+00:00');",
        )
        .unwrap();

        init_schema(&conn).unwrap();
        assert_eq!(
            saved(&conn, "items", "s"),
            (900_000, Some(month_index(2026, 7)))
        );
        assert_eq!(
            saved(&conn, "targets", "e"),
            (900_000, Some(month_index(2026, 6)))
        );
        // Running the migration again is harmless.
        init_schema(&conn).unwrap();

        roll_forward(&conn, month_index(2026, 8)).unwrap();
        assert_eq!(
            saved(&conn, "items", "s"),
            (1_000_000, Some(month_index(2026, 8)))
        );
        assert_eq!(
            saved(&conn, "targets", "e"),
            (1_100_000, Some(month_index(2026, 8)))
        );
    }

    #[test]
    fn a_monthly_amount_is_itself() {
        assert_eq!(monthly_cents(150_000, 1), 150_000);
    }

    /// 100 € a quarter is 33,33 € a month — rounded, not truncated, so the
    /// leftover does not quietly gain a cent off every such line.
    #[test]
    fn a_quarterly_amount_rounds_rather_than_truncates() {
        assert_eq!(monthly_cents(10_000, 3), 3_333);
        assert_eq!(monthly_cents(10_001, 3), 3_334);
    }

    #[test]
    fn a_yearly_amount_divides_by_twelve() {
        assert_eq!(monthly_cents(120_000, 12), 10_000);
        // 1.000 € a year is 83,33 € a month.
        assert_eq!(monthly_cents(100_000, 12), 8_333);
    }

    #[test]
    fn a_zero_or_negative_cadence_cannot_divide_by_zero() {
        assert_eq!(monthly_cents(5_000, 0), 5_000);
        assert_eq!(monthly_cents(5_000, -3), 5_000);
    }

    #[test]
    fn negative_amounts_round_away_from_zero_too() {
        assert_eq!(monthly_cents(-10_000, 3), -3_333);
    }

    /// Saving is yellow, not red: the money has not left, it has moved
    /// somewhere that is still yours.
    #[test]
    fn colour_follows_the_kind() {
        assert_eq!(color_for("income"), "income");
        assert_eq!(color_for("saving"), "saving");
        assert_eq!(color_for("expense"), "expense");
        // Anything unexpected reads as money gone rather than as money kept.
        assert_eq!(color_for("nonsense"), "expense");
    }

    #[test]
    fn an_unknown_kind_is_rejected() {
        assert!(check_kind("income").is_ok());
        assert!(check_kind("saving").is_ok());
        assert!(check_kind("expense").is_ok());
        assert!(check_kind("transfer").is_err());
    }
}
