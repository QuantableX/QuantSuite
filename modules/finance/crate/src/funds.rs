//! Funds own balances and savings plans; the budget projects those same plans.
use super::{emit, monthly_cents, new_id, AppState, Item, ItemInput, ItemPatch};
use chrono::{Datelike, Local, Months, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, State};

type Result<T> = std::result::Result<T, String>;
const MAX_CENTS: i64 = 9_007_199_254_740_991;
fn err(e: rusqlite::Error) -> String {
    e.to_string()
}
fn today() -> NaiveDate {
    Local::now().date_naive()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FundInput {
    pub name: String,
    pub opened_on: String,
    pub opening_cents: i64,
    pub opening_value_cents: i64,
    pub target_cents: Option<i64>,
    pub notes: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fund {
    pub id: String,
    #[serde(flatten)]
    pub details: FundInput,
    pub deposited_cents: i64,
    pub withdrawn_cents: i64,
    pub net_input_cents: i64,
    pub current_value_cents: i64,
    pub gain_cents: i64,
    pub value_as_of: String,
    pub entries: Vec<FundEntry>,
    pub plans: Vec<FundPlan>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryInput {
    pub fund_id: String,
    pub kind: String,
    pub amount_cents: i64,
    pub occurred_on: String,
    pub notes: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FundEntry {
    pub id: String,
    pub kind: String,
    pub amount_cents: i64,
    pub occurred_on: String,
    pub notes: String,
    pub plan_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanInput {
    pub fund_id: String,
    pub name: String,
    pub amount_cents: i64,
    pub every_months: u32,
    pub next_on: String,
    pub is_active: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FundPlan {
    pub id: String,
    #[serde(flatten)]
    pub details: PlanInput,
}

pub(super) fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS funds (
            id TEXT PRIMARY KEY, name TEXT NOT NULL, opened_on TEXT NOT NULL,
            opening_cents INTEGER NOT NULL CHECK(opening_cents >= 0),
            opening_value_cents INTEGER NOT NULL CHECK(opening_value_cents >= 0),
            target_cents INTEGER, notes TEXT NOT NULL DEFAULT ''
        );
        CREATE TABLE IF NOT EXISTS fund_plans (
            id TEXT PRIMARY KEY, fund_id TEXT NOT NULL REFERENCES funds(id) ON DELETE CASCADE,
            name TEXT NOT NULL, amount_cents INTEGER NOT NULL CHECK(amount_cents > 0),
            every_months INTEGER NOT NULL, next_on TEXT NOT NULL,
            anchor_day INTEGER NOT NULL, is_active INTEGER NOT NULL DEFAULT 1
        );
        CREATE TABLE IF NOT EXISTS fund_entries (
            id TEXT PRIMARY KEY, fund_id TEXT NOT NULL REFERENCES funds(id) ON DELETE CASCADE,
            kind TEXT NOT NULL CHECK(kind IN ('deposit', 'withdrawal', 'valuation')),
            amount_cents INTEGER NOT NULL CHECK(amount_cents >= 0),
            occurred_on TEXT NOT NULL, notes TEXT NOT NULL DEFAULT '',
            plan_id TEXT REFERENCES fund_plans(id) ON DELETE SET NULL,
            UNIQUE(plan_id, occurred_on)
        );
        CREATE INDEX IF NOT EXISTS idx_fund_entries ON fund_entries(fund_id, occurred_on);
        CREATE INDEX IF NOT EXISTS idx_fund_plans ON fund_plans(fund_id);
    ",
    )
    .map_err(err)
}

fn date(raw: &str) -> Result<NaiveDate> {
    let parsed = NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .map_err(|_| "Enter a valid date (YYYY-MM-DD).")?;
    if parsed.format("%Y-%m-%d").to_string() != raw || !(1900..=9998).contains(&parsed.year()) {
        return Err("Enter a date between 1900 and 9998.".into());
    }
    Ok(parsed)
}

fn amount(value: i64, positive: bool) -> Result<()> {
    if value < i64::from(positive) || value > MAX_CENTS {
        return Err(if positive {
            "Enter a positive amount."
        } else {
            "Enter a non-negative amount."
        }
        .into());
    }
    Ok(())
}

fn add(a: i64, b: i64) -> Result<i64> {
    let value = a.checked_add(b).ok_or("Fund total is too large.")?;
    if !(-MAX_CENTS..=MAX_CENTS).contains(&value) {
        return Err("Fund total is too large.".into());
    }
    Ok(value)
}

fn fund_start(conn: &Connection, id: &str) -> Result<String> {
    conn.query_row("SELECT opened_on FROM funds WHERE id=?1", [id], |r| {
        r.get(0)
    })
    .optional()
    .map_err(err)?
    .ok_or_else(|| "Fund no longer exists.".into())
}

fn read_funds(conn: &Connection) -> Result<Vec<Fund>> {
    let mut stmt = conn.prepare("SELECT id,name,opened_on,opening_cents,opening_value_cents,target_cents,notes FROM funds ORDER BY rowid").map_err(err)?;
    let mut funds = stmt
        .query_map([], |r| {
            Ok(Fund {
                id: r.get(0)?,
                details: FundInput {
                    name: r.get(1)?,
                    opened_on: r.get(2)?,
                    opening_cents: r.get(3)?,
                    opening_value_cents: r.get(4)?,
                    target_cents: r.get(5)?,
                    notes: r.get(6)?,
                },
                deposited_cents: 0,
                withdrawn_cents: 0,
                net_input_cents: 0,
                current_value_cents: 0,
                gain_cents: 0,
                value_as_of: String::new(),
                entries: vec![],
                plans: vec![],
            })
        })
        .map_err(err)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(err)?;
    for fund in &mut funds {
        let mut entries = conn.prepare("SELECT id,kind,amount_cents,occurred_on,notes,plan_id FROM fund_entries WHERE fund_id=?1 ORDER BY occurred_on,rowid").map_err(err)?;
        fund.entries = entries
            .query_map([&fund.id], |r| {
                Ok(FundEntry {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    amount_cents: r.get(2)?,
                    occurred_on: r.get(3)?,
                    notes: r.get(4)?,
                    plan_id: r.get(5)?,
                })
            })
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?;
        fund.deposited_cents = fund.details.opening_cents;
        fund.current_value_cents = fund.details.opening_value_cents;
        fund.value_as_of = fund.details.opened_on.clone();
        for entry in &fund.entries {
            match entry.kind.as_str() {
                "deposit" => {
                    fund.deposited_cents = add(fund.deposited_cents, entry.amount_cents)?;
                    fund.current_value_cents = add(fund.current_value_cents, entry.amount_cents)?;
                }
                "withdrawal" => {
                    fund.withdrawn_cents = add(fund.withdrawn_cents, entry.amount_cents)?;
                    fund.current_value_cents = add(fund.current_value_cents, -entry.amount_cents)?;
                }
                "valuation" => fund.current_value_cents = entry.amount_cents,
                _ => unreachable!("SQL constrains entry kinds"),
            }
            // Reject an overdraw at any point, including after deleting an earlier deposit.
            if fund.current_value_cents < 0 {
                return Err(
                    "This would overdraw the fund. Check the amount, date or earlier balance."
                        .into(),
                );
            }
            fund.value_as_of = entry.occurred_on.clone();
        }
        fund.net_input_cents = add(fund.deposited_cents, -fund.withdrawn_cents)?;
        fund.gain_cents = add(fund.current_value_cents, -fund.net_input_cents)?;
        fund.entries.reverse();
        let mut plans = conn.prepare("SELECT id,name,amount_cents,every_months,next_on,is_active FROM fund_plans WHERE fund_id=?1 ORDER BY next_on,rowid").map_err(err)?;
        fund.plans = plans
            .query_map([&fund.id], |r| {
                Ok(FundPlan {
                    id: r.get(0)?,
                    details: PlanInput {
                        fund_id: fund.id.clone(),
                        name: r.get(1)?,
                        amount_cents: r.get(2)?,
                        every_months: r.get(3)?,
                        next_on: r.get(4)?,
                        is_active: r.get(5)?,
                    },
                })
            })
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?;
    }
    // Keep aggregate UI totals within JavaScript's exact integer range too.
    for values in [
        funds
            .iter()
            .map(|f| f.current_value_cents)
            .collect::<Vec<_>>(),
        funds.iter().map(|f| f.deposited_cents).collect(),
        funds.iter().map(|f| f.withdrawn_cents).collect(),
        funds.iter().map(|f| f.net_input_cents).collect(),
        funds.iter().map(|f| f.gain_cents).collect(),
    ] {
        values.into_iter().try_fold(0, add)?;
    }
    Ok(funds)
}

/// One budget/chart row per fund, using confirmed balances and planned rates.
pub(super) fn budget_items(conn: &Connection) -> Result<Vec<Item>> {
    read_funds(conn)?
        .into_iter()
        .enumerate()
        .map(|(index, fund)| {
            let monthly =
                fund.plans
                    .iter()
                    .filter(|p| p.details.is_active)
                    .try_fold(0, |sum, p| {
                        add(
                            sum,
                            monthly_cents(
                                p.details.amount_cents,
                                i64::from(p.details.every_months),
                            ),
                        )
                    })?;
            let single = (fund.plans.len() == 1).then(|| &fund.plans[0].details);
            Ok(Item {
                id: fund.id.clone(),
                fund_id: Some(fund.id),
                fund_plan_count: fund.plans.len(),
                kind: "saving".into(),
                name: fund.details.name,
                amount_cents: single.map_or(monthly, |p| p.amount_cents),
                every_months: single.map_or(1, |p| i64::from(p.every_months)),
                monthly_cents: monthly,
                color: "saving".into(),
                notes: Some(fund.details.notes),
                is_active: fund.plans.is_empty() || fund.plans.iter().any(|p| p.details.is_active),
                sort_index: index as f64,
                target_cents: fund.details.target_cents,
                saved_cents: fund.current_value_cents,
                created_at: fund.details.opened_on,
                updated_at: fund.value_as_of,
            })
        })
        .collect()
}

/// Transfer legacy saving lines once, preserving their IDs and current balances.
/// The enclosing transaction keeps the old rows intact if any conversion fails.
pub(super) fn migrate_budget(conn: &Connection) -> Result<()> {
    let tx = conn.unchecked_transaction().map_err(err)?;
    super::roll_forward(&tx, super::current_month())?;
    let items = {
        let mut stmt = tx
            .prepare(&format!(
                "SELECT {} FROM items WHERE kind='saving' ORDER BY sort_index",
                super::ITEM_COLUMNS
            ))
            .map_err(err)?;
        let rows = stmt
            .query_map([], super::read_item)
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?;
        rows
    };
    for item in items {
        tx.execute("INSERT INTO funds (id,name,opened_on,opening_cents,opening_value_cents,target_cents,notes) VALUES (?1,?2,?3,?4,?4,?5,?6)",
            params![item.id, if item.name.trim().is_empty() { "New fund" } else { &item.name }, today().to_string(), item.saved_cents, item.target_cents, item.notes.unwrap_or_default()]).map_err(err)?;
        if item.amount_cents > 0 {
            save_plan(
                &tx,
                None,
                PlanInput {
                    fund_id: item.id.clone(),
                    name: "Regular contribution".into(),
                    amount_cents: item.amount_cents,
                    every_months: item.every_months as u32,
                    next_on: next_date(today(), item.every_months as u32, today().day())?
                        .to_string(),
                    is_active: item.is_active,
                },
            )?;
        }
        tx.execute("DELETE FROM items WHERE id=?1", [&item.id])
            .map_err(err)?;
    }
    read_funds(&tx)?;
    tx.commit().map_err(err)
}

pub(super) fn create_budget_item(conn: &Connection, item: ItemInput) -> Result<String> {
    let tx = conn.unchecked_transaction().map_err(err)?;
    let id = new_id();
    amount(item.amount_cents, false)?;
    if let Some(target) = item.target_cents {
        amount(target, true)?;
    }
    tx.execute("INSERT INTO funds (id,name,opened_on,opening_cents,opening_value_cents,target_cents,notes) VALUES (?1,?2,?3,0,0,?4,?5)",
        params![id, if item.name.trim().is_empty() { "New fund" } else { item.name.trim() }, today().to_string(), item.target_cents, item.notes.unwrap_or_default()]).map_err(err)?;
    if item.amount_cents > 0 {
        save_plan(
            &tx,
            None,
            PlanInput {
                fund_id: id.clone(),
                name: "Regular contribution".into(),
                amount_cents: item.amount_cents,
                every_months: item.every_months.unwrap_or(1) as u32,
                next_on: today().to_string(),
                is_active: true,
            },
        )?;
    }
    read_funds(&tx)?;
    tx.commit().map_err(err)?;
    Ok(id)
}

pub(super) fn update_budget_item(conn: &Connection, id: &str, patch: ItemPatch) -> Result<()> {
    let tx = conn.unchecked_transaction().map_err(err)?;
    let fund = read_funds(&tx)?
        .into_iter()
        .find(|f| f.id == id)
        .ok_or("Fund no longer exists.")?;
    if patch.kind.as_deref().is_some_and(|k| k != "saving") {
        return Err("A fund must remain a saving line.".into());
    }
    if let Some(name) = patch.name {
        if name.trim().is_empty() {
            return Err("Give the fund a name.".into());
        }
        tx.execute(
            "UPDATE funds SET name=?2 WHERE id=?1",
            params![id, name.trim()],
        )
        .map_err(err)?;
    }
    if let Some(notes) = patch.notes {
        tx.execute(
            "UPDATE funds SET notes=?2 WHERE id=?1",
            params![id, notes.unwrap_or_default()],
        )
        .map_err(err)?;
    }
    if let Some(target) = patch.target_cents {
        if let Some(value) = target {
            amount(value, true)?;
        }
        tx.execute(
            "UPDATE funds SET target_cents=?2 WHERE id=?1",
            params![id, target],
        )
        .map_err(err)?;
    }
    if let Some(saved) = patch.saved_cents {
        insert_entry(
            &tx,
            &EntryInput {
                fund_id: id.into(),
                kind: "valuation".into(),
                amount_cents: saved,
                occurred_on: today().to_string(),
                notes: "Balance updated from the plan".into(),
            },
            None,
            today(),
        )?;
    }
    if patch.amount_cents.is_some() || patch.every_months.is_some() {
        if fund.plans.len() > 1 {
            return Err("Edit this fund's individual savings plans in Funds.".into());
        }
        let previous = fund.plans.first();
        let cents = patch
            .amount_cents
            .unwrap_or_else(|| previous.map_or(0, |p| p.details.amount_cents));
        amount(cents, false)?;
        if cents == 0 {
            tx.execute("DELETE FROM fund_plans WHERE fund_id=?1", [id])
                .map_err(err)?;
        } else {
            let every = patch
                .every_months
                .unwrap_or_else(|| previous.map_or(1, |p| i64::from(p.details.every_months)));
            save_plan(
                &tx,
                previous.map(|p| p.id.clone()),
                PlanInput {
                    fund_id: id.into(),
                    name: previous
                        .map_or_else(|| "Regular contribution".into(), |p| p.details.name.clone()),
                    amount_cents: cents,
                    every_months: u32::try_from(every)
                        .map_err(|_| "Invalid contribution interval.")?,
                    next_on: previous
                        .map_or_else(|| today().to_string(), |p| p.details.next_on.clone()),
                    is_active: patch
                        .is_active
                        .unwrap_or_else(|| previous.map_or(true, |p| p.details.is_active)),
                },
            )?;
        }
    }
    if let Some(active) = patch.is_active {
        tx.execute(
            "UPDATE fund_plans SET is_active=?2 WHERE fund_id=?1",
            params![id, active],
        )
        .map_err(err)?;
    }
    read_funds(&tx)?;
    tx.commit().map_err(err)
}

pub(super) fn delete_budget_item(conn: &Connection, id: &str) -> Result<()> {
    remove_record(conn, "fund", id)
}

fn save_fund(
    conn: &Connection,
    id: Option<String>,
    input: FundInput,
    now: NaiveDate,
) -> Result<String> {
    if input.name.trim().is_empty() {
        return Err("Give the fund a name.".into());
    }
    if date(&input.opened_on)? > now {
        return Err("Opening date cannot be in the future.".into());
    }
    amount(input.opening_cents, false)?;
    amount(input.opening_value_cents, false)?;
    if let Some(target) = input.target_cents {
        amount(target, true)?;
    }
    let tx = conn.unchecked_transaction().map_err(err)?;
    let fund_id = if let Some(id) = id {
        fund_start(&tx, &id)?;
        let earliest: Option<String> = tx.query_row("SELECT MIN(day) FROM (SELECT occurred_on AS day FROM fund_entries WHERE fund_id=?1 UNION ALL SELECT next_on AS day FROM fund_plans WHERE fund_id=?1)", [&id], |r| r.get(0)).map_err(err)?;
        if earliest.is_some_and(|day| day < input.opened_on) {
            return Err("Opening date must precede the fund's entries and plans.".into());
        }
        tx.execute("UPDATE funds SET name=?2,opened_on=?3,opening_cents=?4,opening_value_cents=?5,target_cents=?6,notes=?7 WHERE id=?1", params![id,input.name.trim(),input.opened_on,input.opening_cents,input.opening_value_cents,input.target_cents,input.notes]).map_err(err)?;
        id
    } else {
        let id = new_id();
        tx.execute(
            "INSERT INTO funds VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                id,
                input.name.trim(),
                input.opened_on,
                input.opening_cents,
                input.opening_value_cents,
                input.target_cents,
                input.notes
            ],
        )
        .map_err(err)?;
        id
    };
    read_funds(&tx)?;
    tx.commit().map_err(err)?;
    Ok(fund_id)
}

fn insert_entry(
    conn: &Connection,
    input: &EntryInput,
    plan_id: Option<&str>,
    now: NaiveDate,
) -> Result<()> {
    if !["deposit", "withdrawal", "valuation"].contains(&input.kind.as_str()) {
        return Err("Unknown entry type.".into());
    }
    amount(input.amount_cents, input.kind != "valuation")?;
    let day = date(&input.occurred_on)?;
    if day > now {
        return Err(
            "Record completed movements only; use a savings plan for future deposits.".into(),
        );
    }
    if input.occurred_on < fund_start(conn, &input.fund_id)? {
        return Err("Entry date cannot precede the fund's opening date.".into());
    }
    conn.execute("INSERT INTO fund_entries (id,fund_id,kind,amount_cents,occurred_on,notes,plan_id) VALUES (?1,?2,?3,?4,?5,?6,?7)", params![new_id(),input.fund_id,input.kind,input.amount_cents,input.occurred_on,input.notes,plan_id]).map_err(err)?;
    read_funds(conn)?;
    Ok(())
}

fn save_plan(conn: &Connection, id: Option<String>, input: PlanInput) -> Result<()> {
    let day = date(&input.next_on)?;
    if input.name.trim().is_empty() {
        return Err("Give the savings plan a name.".into());
    }
    amount(input.amount_cents, true)?;
    if ![1, 3, 6, 12].contains(&input.every_months) {
        return Err("Choose monthly, quarterly, half-yearly or yearly.".into());
    }
    if input.next_on < fund_start(conn, &input.fund_id)? {
        return Err("First deposit cannot precede the fund's opening date.".into());
    }
    if let Some(id) = id {
        let previous: (String, u32) = conn
            .query_row(
                "SELECT next_on,anchor_day FROM fund_plans WHERE id=?1 AND fund_id=?2",
                params![id, input.fund_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(err)?
            .ok_or("Savings plan no longer exists.")?;
        let anchor = if previous.0 == input.next_on {
            previous.1
        } else {
            day.day()
        };
        conn.execute("UPDATE fund_plans SET name=?2,amount_cents=?3,every_months=?4,next_on=?5,anchor_day=?6,is_active=?7 WHERE id=?1", params![id,input.name.trim(),input.amount_cents,input.every_months,input.next_on,anchor,input.is_active]).map_err(err)?;
    } else {
        conn.execute(
            "INSERT INTO fund_plans VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                new_id(),
                input.fund_id,
                input.name.trim(),
                input.amount_cents,
                input.every_months,
                input.next_on,
                day.day(),
                input.is_active
            ],
        )
        .map_err(err)?;
    }
    Ok(())
}

fn next_date(day: NaiveDate, every: u32, anchor: u32) -> Result<NaiveDate> {
    let month = day
        .with_day(1)
        .and_then(|d| d.checked_add_months(Months::new(every)))
        .ok_or("Savings plan date is too far in the future.")?;
    let last = month
        .checked_add_months(Months::new(1))
        .and_then(|d| d.pred_opt())
        .ok_or("Savings plan date is too far in the future.")?;
    month
        .with_day(anchor.min(last.day()))
        .ok_or_else(|| "Invalid savings plan date.".into())
}

fn book_plan(conn: &Connection, id: &str, expected_on: &str, now: NaiveDate) -> Result<()> {
    let tx = conn.unchecked_transaction().map_err(err)?;
    let (fund_id, name, cents, every, next, anchor, active): (String,String,i64,u32,String,u32,bool) = tx.query_row("SELECT fund_id,name,amount_cents,every_months,next_on,anchor_day,is_active FROM fund_plans WHERE id=?1", [id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional().map_err(err)?.ok_or("Savings plan no longer exists.")?;
    if next != expected_on {
        return Err("This installment has already changed. Refresh the funds.".into());
    }
    if !active {
        return Err("Resume the savings plan before booking an installment.".into());
    }
    let day = date(&next)?;
    let following = next_date(day, every, anchor)?
        .format("%Y-%m-%d")
        .to_string();
    date(&following)?;
    insert_entry(
        &tx,
        &EntryInput {
            fund_id,
            kind: "deposit".into(),
            amount_cents: cents,
            occurred_on: next,
            notes: name,
        },
        Some(id),
        now,
    )?;
    tx.execute(
        "UPDATE fund_plans SET next_on=?2 WHERE id=?1",
        params![id, following],
    )
    .map_err(err)?;
    tx.commit().map_err(err)
}

#[tauri::command(async)]
pub fn funds_overview(state: State<'_, AppState>) -> Result<Vec<Fund>> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    read_funds(&conn)
}

#[tauri::command(async)]
pub fn save_fund_account(
    id: Option<String>,
    input: FundInput,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let result = save_fund(&conn, id, input, today())?;
    emit(&app, "finance:funds-changed", json!({}));
    emit(&app, "finance:plan-changed", json!({}));
    Ok(result)
}

#[tauri::command(async)]
pub fn add_fund_entry(input: EntryInput, state: State<'_, AppState>, app: AppHandle) -> Result<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let tx = conn.unchecked_transaction().map_err(err)?;
    insert_entry(&tx, &input, None, today())?;
    tx.commit().map_err(err)?;
    emit(&app, "finance:funds-changed", json!({}));
    emit(&app, "finance:plan-changed", json!({}));
    Ok(())
}

fn remove_record(conn: &Connection, kind: &str, id: &str) -> Result<()> {
    let sql = match kind {
        "fund" => "DELETE FROM funds WHERE id=?1",
        "entry" => "DELETE FROM fund_entries WHERE id=?1",
        "plan" => "DELETE FROM fund_plans WHERE id=?1",
        _ => return Err("Unknown fund record type.".into()),
    };
    let tx = conn.unchecked_transaction().map_err(err)?;
    if tx.execute(sql, [id]).map_err(err)? == 0 {
        return Err("Record no longer exists.".into());
    }
    read_funds(&tx)?;
    tx.commit().map_err(err)
}

#[tauri::command(async)]
pub fn delete_fund_record(
    kind: String,
    id: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    remove_record(&conn, &kind, &id)?;
    emit(&app, "finance:funds-changed", json!({}));
    emit(&app, "finance:plan-changed", json!({}));
    Ok(())
}

#[tauri::command(async)]
pub fn save_fund_plan(
    id: Option<String>,
    input: PlanInput,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    save_plan(&conn, id, input)?;
    emit(&app, "finance:funds-changed", json!({}));
    emit(&app, "finance:plan-changed", json!({}));
    Ok(())
}

#[tauri::command(async)]
pub fn book_fund_plan(
    id: String,
    expected_on: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    book_plan(&conn, &id, &expected_on, today())?;
    emit(&app, "finance:funds-changed", json!({}));
    emit(&app, "finance:plan-changed", json!({}));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        super::super::init_schema(&conn).unwrap();
        super::super::init_schema(&conn).unwrap();
        conn
    }
    fn now() -> NaiveDate {
        date("2026-09-16").unwrap()
    }
    fn fund(conn: &Connection) -> String {
        save_fund(
            conn,
            None,
            FundInput {
                name: "Emergency fund".into(),
                opened_on: "2026-01-01".into(),
                opening_cents: 10000,
                opening_value_cents: 12000,
                target_cents: Some(50000),
                notes: String::new(),
            },
            now(),
        )
        .unwrap()
    }
    fn entry(conn: &Connection, id: &str, kind: &str, cents: i64, day: &str) -> Result<()> {
        let tx = conn.unchecked_transaction().unwrap();
        insert_entry(
            &tx,
            &EntryInput {
                fund_id: id.into(),
                kind: kind.into(),
                amount_cents: cents,
                occurred_on: day.into(),
                notes: String::new(),
            },
            None,
            now(),
        )?;
        tx.commit().map_err(err)
    }
    fn plan(conn: &Connection, id: &str) -> String {
        save_plan(
            conn,
            None,
            PlanInput {
                fund_id: id.into(),
                name: "Monthly".into(),
                amount_cents: 500,
                every_months: 1,
                next_on: "2026-01-31".into(),
                is_active: true,
            },
        )
        .unwrap();
        read_funds(conn).unwrap()[0].plans[0].id.clone()
    }
    #[test]
    fn valuations_separate_cash_inputs_from_market_value() {
        let conn = db();
        let id = fund(&conn);
        entry(&conn, &id, "deposit", 3000, "2026-02-01").unwrap();
        entry(&conn, &id, "valuation", 20000, "2026-03-01").unwrap();
        entry(&conn, &id, "withdrawal", 1000, "2026-04-01").unwrap();
        let f = read_funds(&conn).unwrap().remove(0);
        assert_eq!(
            (
                f.deposited_cents,
                f.withdrawn_cents,
                f.net_input_cents,
                f.current_value_cents,
                f.gain_cents
            ),
            (13000, 1000, 12000, 19000, 7000)
        );
    }
    #[test]
    fn backdated_inputs_do_not_double_count_a_later_valuation() {
        let conn = db();
        let id = fund(&conn);
        entry(&conn, &id, "valuation", 20000, "2026-03-01").unwrap();
        entry(&conn, &id, "deposit", 3000, "2026-02-01").unwrap();
        entry(&conn, &id, "deposit", 1000, "2026-03-01").unwrap();
        let f = read_funds(&conn).unwrap().remove(0);
        assert_eq!((f.net_input_cents, f.current_value_cents), (14000, 21000));
        remove_record(&conn, "entry", &f.entries[1].id).unwrap();
        assert_eq!(read_funds(&conn).unwrap()[0].current_value_cents, 16000);
    }
    #[test]
    fn invalid_movements_rollback_and_zero_valuation_is_valid() {
        let conn = db();
        let id = fund(&conn);
        for (kind, cents, day) in [
            ("withdrawal", 13000, "2026-03-01"),
            ("deposit", 0, "2026-03-01"),
            ("deposit", -1, "2026-03-01"),
            ("deposit", MAX_CENTS, "2026-03-01"),
            ("deposit", 1, "2027-01-01"),
            ("deposit", 1, "2025-01-01"),
            ("deposit", 1, "2026-02-30"),
        ] {
            assert!(entry(&conn, &id, kind, cents, day).is_err());
        }
        assert!(read_funds(&conn).unwrap()[0].entries.is_empty());
        entry(&conn, &id, "valuation", 0, "2026-03-01").unwrap();
        assert_eq!(read_funds(&conn).unwrap()[0].current_value_cents, 0);
    }
    #[test]
    fn savings_installments_are_explicit_atomic_and_idempotent() {
        let conn = db();
        let id = fund(&conn);
        let pid = plan(&conn, &id);
        assert!(read_funds(&conn).unwrap()[0].entries.is_empty());
        book_plan(&conn, &pid, "2026-01-31", now()).unwrap();
        assert!(book_plan(&conn, &pid, "2026-01-31", now()).is_err());
        book_plan(&conn, &pid, "2026-02-28", now()).unwrap();
        let f = read_funds(&conn).unwrap().remove(0);
        assert_eq!(f.plans[0].details.next_on, "2026-03-31");
        assert_eq!((f.entries.len(), f.current_value_cents), (2, 13000));
        let mut paused = f.plans[0].details.clone();
        paused.is_active = false;
        save_plan(&conn, Some(pid.clone()), paused).unwrap();
        assert!(book_plan(&conn, &pid, "2026-03-31", now()).is_err());
        assert_eq!(read_funds(&conn).unwrap()[0].entries.len(), 2);
    }
    #[test]
    fn leap_year_and_month_end_preserve_anchor() {
        assert_eq!(
            next_date(date("2024-01-31").unwrap(), 1, 31).unwrap(),
            date("2024-02-29").unwrap()
        );
        assert_eq!(
            next_date(date("2024-02-29").unwrap(), 1, 31).unwrap(),
            date("2024-03-31").unwrap()
        );
        assert_eq!(
            next_date(date("2024-02-29").unwrap(), 12, 29).unwrap(),
            date("2025-02-28").unwrap()
        );
    }
    #[test]
    fn deleting_plans_keeps_history_and_deleting_funds_cascades() {
        let conn = db();
        let id = fund(&conn);
        let pid = plan(&conn, &id);
        book_plan(&conn, &pid, "2026-01-31", now()).unwrap();
        remove_record(&conn, "plan", &pid).unwrap();
        let f = read_funds(&conn).unwrap().remove(0);
        assert!(f.plans.is_empty());
        assert!(f.entries[0].plan_id.is_none());
        assert_eq!(f.current_value_cents, 12500);
        remove_record(&conn, "fund", &id).unwrap();
        assert!(read_funds(&conn).unwrap().is_empty());
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM fund_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn removing_a_deposit_cannot_overdraw_history() {
        let conn = db();
        let id = fund(&conn);
        entry(&conn, &id, "deposit", 10000, "2026-02-01").unwrap();
        let deposit = read_funds(&conn).unwrap()[0].entries[0].id.clone();
        entry(&conn, &id, "withdrawal", 20000, "2026-03-01").unwrap();
        assert!(remove_record(&conn, "entry", &deposit).is_err());
        assert_eq!(read_funds(&conn).unwrap()[0].entries.len(), 2);
    }
    #[test]
    fn future_installments_do_not_advance_or_book() {
        let conn = db();
        let id = fund(&conn);
        let pid = plan(&conn, &id);
        assert!(book_plan(&conn, &pid, "2026-01-31", date("2026-01-01").unwrap()).is_err());
        let f = read_funds(&conn).unwrap().remove(0);
        assert!(f.entries.is_empty());
        assert_eq!(f.plans[0].details.next_on, "2026-01-31");
    }

    #[test]
    fn fund_rates_balances_targets_and_chart_share_the_same_identity() {
        let conn = db();
        let id = fund(&conn);
        let pid = plan(&conn, &id);
        save_plan(
            &conn,
            None,
            PlanInput {
                fund_id: id.clone(),
                name: "Quarterly".into(),
                amount_cents: 10000,
                every_months: 3,
                next_on: "2026-04-01".into(),
                is_active: true,
            },
        )
        .unwrap();
        let item = budget_items(&conn).unwrap().remove(0);
        assert_eq!(item.fund_id.as_deref(), Some(id.as_str()));
        assert_eq!(
            (item.monthly_cents, item.saved_cents, item.fund_plan_count),
            (3833, 12000, 2)
        );
        assert_eq!(
            super::super::build_summary(&conn).unwrap().saving_cents,
            3833
        );
        let chart = super::super::build_sankey(&conn).unwrap();
        assert!(chart
            .nodes
            .iter()
            .any(|n| n.fund_id.as_deref() == Some(id.as_str()) && n.value_cents == 3833));
        book_plan(&conn, &pid, "2026-01-31", now()).unwrap();
        entry(&conn, &id, "withdrawal", 1000, "2026-02-01").unwrap();
        assert_eq!(
            super::super::build_summary(&conn).unwrap().saving_cents,
            3833
        );
        let goal = super::super::build_goals(&conn).unwrap().remove(0);
        assert_eq!(
            (goal.id, goal.saved_cents, goal.monthly_cents),
            (id.clone(), 11500, 3833)
        );
        // Reads and month changes never invent additional fund deposits.
        super::super::roll_forward(&conn, super::super::current_month() + 12).unwrap();
        assert_eq!(budget_items(&conn).unwrap()[0].saved_cents, 11500);
        update_budget_item(
            &conn,
            &id,
            ItemPatch {
                is_active: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(super::super::build_summary(&conn).unwrap().saving_cents, 0);
        assert!(!super::super::build_sankey(&conn)
            .unwrap()
            .nodes
            .iter()
            .any(|n| n.fund_id.as_deref() == Some(id.as_str())));
        assert_eq!(
            super::super::build_goals(&conn).unwrap()[0].monthly_cents,
            0
        );
    }

    #[test]
    fn budget_edits_update_funds_and_plans_atomically() {
        let conn = db();
        let id = create_budget_item(
            &conn,
            ItemInput {
                kind: "saving".into(),
                name: "Trip".into(),
                amount_cents: 1000,
                ..Default::default()
            },
        )
        .unwrap();
        update_budget_item(
            &conn,
            &id,
            ItemPatch {
                name: Some("Holiday".into()),
                amount_cents: Some(3000),
                every_months: Some(3),
                saved_cents: Some(50000),
                target_cents: Some(Some(100000)),
                ..Default::default()
            },
        )
        .unwrap();
        let f = read_funds(&conn).unwrap().remove(0);
        assert_eq!(
            (
                f.details.name.as_str(),
                f.current_value_cents,
                f.plans[0].details.amount_cents
            ),
            ("Holiday", 50000, 3000)
        );
        assert_eq!(budget_items(&conn).unwrap()[0].monthly_cents, 1000);
        assert!(update_budget_item(
            &conn,
            &id,
            ItemPatch {
                name: Some("Must roll back".into()),
                amount_cents: Some(-1),
                ..Default::default()
            }
        )
        .is_err());
        assert_eq!(read_funds(&conn).unwrap()[0].details.name, "Holiday");
        let patch: ItemPatch = serde_json::from_value(json!({ "targetCents": null })).unwrap();
        update_budget_item(&conn, &id, patch).unwrap();
        assert!(read_funds(&conn).unwrap()[0].details.target_cents.is_none());
        remove_record(&conn, "plan", &f.plans[0].id).unwrap();
        assert_eq!(
            (
                budget_items(&conn).unwrap()[0].monthly_cents,
                budget_items(&conn).unwrap()[0].saved_cents
            ),
            (0, 50000)
        );
        delete_budget_item(&conn, &id).unwrap();
        assert!(super::super::read_budget_items(&conn).unwrap().is_empty());
    }

    #[test]
    fn legacy_saving_lines_migrate_once_without_losing_existing_funds() {
        let conn = db();
        let existing = fund(&conn);
        conn.execute("INSERT INTO items (id,kind,name,amount_cents,every_months,color,target_cents,saved_cents,saved_as_of,created_at,updated_at) VALUES ('legacy','saving','Travel',30000,3,'saving',100000,45000,?1,'t','t')", [super::super::current_month()]).unwrap();
        migrate_budget(&conn).unwrap();
        migrate_budget(&conn).unwrap();
        let funds = read_funds(&conn).unwrap();
        assert_eq!(funds.len(), 2);
        assert_eq!(funds[0].id, existing);
        let migrated = funds.iter().find(|f| f.id == "legacy").unwrap();
        assert_eq!(
            (migrated.current_value_cents, migrated.details.target_cents),
            (45000, Some(100000))
        );
        assert!(migrated.entries.is_empty());
        assert_eq!(
            (
                migrated.plans[0].details.amount_cents,
                migrated.plans[0].details.every_months
            ),
            (30000, 3)
        );
        assert_eq!(
            super::super::build_summary(&conn).unwrap().saving_cents,
            10000
        );
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM items WHERE kind='saving'", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap(),
            0
        );
    }

    #[test]
    fn changing_one_of_multiple_plans_preserves_other_rates_and_history() {
        let conn = db();
        let id = fund(&conn);
        let pid = plan(&conn, &id);
        save_plan(
            &conn,
            None,
            PlanInput {
                fund_id: id.clone(),
                name: "Extra".into(),
                amount_cents: 1200,
                every_months: 12,
                next_on: "2026-05-01".into(),
                is_active: true,
            },
        )
        .unwrap();
        assert!(update_budget_item(
            &conn,
            &id,
            ItemPatch {
                amount_cents: Some(9999),
                ..Default::default()
            }
        )
        .is_err());
        assert_eq!(budget_items(&conn).unwrap()[0].monthly_cents, 600);
        let mut changed = read_funds(&conn).unwrap()[0]
            .plans
            .iter()
            .find(|p| p.id == pid)
            .unwrap()
            .details
            .clone();
        changed.amount_cents = 800;
        save_plan(&conn, Some(pid.clone()), changed).unwrap();
        assert_eq!(budget_items(&conn).unwrap()[0].monthly_cents, 900);
        remove_record(&conn, "plan", &pid).unwrap();
        assert_eq!(budget_items(&conn).unwrap()[0].monthly_cents, 100);
    }

    #[test]
    fn a_failed_legacy_conversion_keeps_all_original_rows() {
        let conn = db();
        conn.execute("INSERT INTO items (id,kind,name,amount_cents,every_months,color,saved_cents,saved_as_of,sort_index,created_at,updated_at) VALUES ('valid','saving','Valid',1000,1,'saving',5000,?1,0,'t','t'), ('invalid','saving','Invalid',1000,2,'saving',5000,?1,1,'t','t')", [super::super::current_month()]).unwrap();
        assert!(migrate_budget(&conn).is_err());
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM items", [], |r| r.get::<_, i64>(0)).unwrap(), 2);
        assert!(read_funds(&conn).unwrap().is_empty());
    }
}
