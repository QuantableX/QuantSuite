use std::collections::{HashMap, HashSet};

use rusqlite::params;
use tauri::State;
use crate::{AppState, BotTradeStats, DailyPnl, EquityPoint, Trade, TradeFacets, TradeFilters, TradeStats, TradeStatsBreakdown};

// ---------------------------------------------------------------------------
// Trade / Journal Commands
// ---------------------------------------------------------------------------

/// Every journal read selects these columns: the trade with its bot's mode
/// joined in (PLAN-QUANTALGO §3.4). The single-bot era's rows (NULL bot_id)
/// count as paper; a backtest row has no mode.
const TRADE_SELECT: &str = "SELECT t.id, t.strategy_id, t.exchange, t.pair, t.side, t.entry_price, t.exit_price, t.quantity, t.entry_time, t.exit_time, t.pnl, t.pnl_pct, t.fee, t.is_backtest, t.backtest_id, t.notes, t.created_at, t.bot_id, CASE WHEN t.is_backtest = 1 THEN NULL ELSE COALESCE(b.trading_mode, 'paper') END FROM trades t LEFT JOIN bots b ON b.id = t.bot_id";

fn row_to_trade(row: &rusqlite::Row) -> rusqlite::Result<Trade> {
    Ok(Trade {
        id: row.get(0)?,
        strategy_id: row.get(1)?,
        exchange: row.get(2)?,
        pair: row.get(3)?,
        side: row.get(4)?,
        entry_price: row.get(5)?,
        exit_price: row.get(6)?,
        quantity: row.get(7)?,
        entry_time: row.get(8)?,
        exit_time: row.get(9)?,
        pnl: row.get(10)?,
        pnl_pct: row.get(11)?,
        fee: row.get(12)?,
        is_backtest: row.get::<_, i32>(13)? != 0,
        backtest_id: row.get(14)?,
        notes: row.get(15)?,
        created_at: row.get(16)?,
        bot_id: row.get(17)?,
        trading_mode: row.get(18)?,
    })
}

/// The rows a filter selects, newest first.
pub(crate) fn query_trades(db: &rusqlite::Connection, filters: &TradeFilters) -> Result<Vec<Trade>, String> {
    let mut sql = format!("{TRADE_SELECT} WHERE 1=1");
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1;

    if let Some(ref v) = filters.bot_id {
        sql.push_str(&format!(" AND t.bot_id = ?{idx}"));
        param_values.push(Box::new(v.clone()));
        idx += 1;
    }
    if let Some(ref v) = filters.trading_mode {
        // A mode is a bot property: backtest rows have none and drop out.
        sql.push_str(&format!(" AND t.is_backtest = 0 AND COALESCE(b.trading_mode, 'paper') = ?{idx}"));
        param_values.push(Box::new(v.clone()));
        idx += 1;
    }
    if let Some(ref v) = filters.strategy_id {
        sql.push_str(&format!(" AND t.strategy_id = ?{idx}"));
        param_values.push(Box::new(v.clone()));
        idx += 1;
    }
    if let Some(ref v) = filters.exchange {
        sql.push_str(&format!(" AND t.exchange = ?{idx}"));
        param_values.push(Box::new(v.clone()));
        idx += 1;
    }
    if let Some(ref v) = filters.pair {
        sql.push_str(&format!(" AND t.pair = ?{idx}"));
        param_values.push(Box::new(v.clone()));
        idx += 1;
    }
    if let Some(ref v) = filters.side {
        sql.push_str(&format!(" AND t.side = ?{idx}"));
        param_values.push(Box::new(v.clone()));
        idx += 1;
    }
    if let Some(ref v) = filters.from_date {
        sql.push_str(&format!(" AND t.entry_time >= ?{idx}"));
        param_values.push(Box::new(v.clone()));
        idx += 1;
    }
    if let Some(ref v) = filters.to_date {
        sql.push_str(&format!(" AND t.entry_time <= ?{idx}"));
        param_values.push(Box::new(v.clone()));
        idx += 1;
    }
    if let Some(ref v) = filters.exited_from {
        sql.push_str(&format!(" AND t.exit_time >= ?{idx}"));
        param_values.push(Box::new(v.clone()));
        idx += 1;
    }
    if let Some(ref v) = filters.exited_to {
        sql.push_str(&format!(" AND t.exit_time <= ?{idx}"));
        param_values.push(Box::new(v.clone()));
        idx += 1;
    }
    if let Some(v) = filters.min_pnl {
        sql.push_str(&format!(" AND t.pnl >= ?{idx}"));
        param_values.push(Box::new(v));
        idx += 1;
    }
    if let Some(v) = filters.is_backtest {
        let int_val: i32 = if v { 1 } else { 0 };
        sql.push_str(&format!(" AND t.is_backtest = ?{idx}"));
        param_values.push(Box::new(int_val));
        idx += 1;
    }
    if let Some(ref v) = filters.backtest_id {
        sql.push_str(&format!(" AND t.backtest_id = ?{idx}"));
        param_values.push(Box::new(v.clone()));
        idx += 1;
    }

    sql.push_str(" ORDER BY t.entry_time DESC");

    if let Some(limit) = filters.limit {
        sql.push_str(&format!(" LIMIT ?{idx}"));
        param_values.push(Box::new(limit as i64));
        idx += 1;
    } else if filters.offset.is_some() {
        // SQLite only accepts OFFSET as part of a LIMIT clause; -1 means unlimited.
        sql.push_str(" LIMIT -1");
    }
    if let Some(offset) = filters.offset {
        sql.push_str(&format!(" OFFSET ?{idx}"));
        param_values.push(Box::new(offset as i64));
    }

    let params_refs: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(|b| b.as_ref()).collect();

    let mut stmt = db.prepare(&sql).map_err(|e| format!("Prepare: {e}"))?;
    let rows = stmt
        .query_map(params_refs.as_slice(), row_to_trade)
        .map_err(|e| format!("Query: {e}"))?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| format!("Row: {e}"))?);
    }
    Ok(result)
}

#[tauri::command(async)]
pub(crate) fn list_trades(filters: TradeFilters, state: State<'_, AppState>) -> Result<Vec<Trade>, String> {
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    query_trades(&db, &filters)
}

/// The distinct exchange and pair values in the journal — the only values its
/// filter dropdowns offer (PLAN-QUANTALGO §5).
#[tauri::command(async)]
pub(crate) fn list_trade_facets(state: State<'_, AppState>) -> Result<TradeFacets, String> {
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;

    fn distinct(db: &rusqlite::Connection, column: &str) -> Result<Vec<String>, String> {
        let sql = format!(
            "SELECT DISTINCT {column} FROM trades WHERE {column} IS NOT NULL AND {column} != '' ORDER BY {column}"
        );
        let mut stmt = db.prepare(&sql).map_err(|e| format!("Prepare: {e}"))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| format!("Query: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Row: {e}"))
    }

    Ok(TradeFacets {
        exchanges: distinct(&db, "exchange")?,
        pairs: distinct(&db, "pair")?,
    })
}

#[tauri::command(async)]
pub(crate) fn get_trade_stats(
    mut filters: TradeFilters,
    state: State<'_, AppState>,
) -> Result<TradeStats, String> {
    filters.limit = None;
    filters.offset = None;
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    let trades = query_trades(&db, &filters)?;
    Ok(compute_stats(trades.iter()))
}

/// The same statistics three ways in one call (PLAN-QUANTALGO §3.4): paper
/// vs live, and per bot — every bot, with or without trades, then the
/// trades of deleted bots as one more group. Backtest rows never count.
#[tauri::command(async)]
pub(crate) fn get_trade_stats_breakdown(
    mut filters: TradeFilters,
    state: State<'_, AppState>,
) -> Result<TradeStatsBreakdown, String> {
    filters.limit = None;
    filters.offset = None;
    filters.is_backtest = Some(false);
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    let trades = query_trades(&db, &filters)?;

    let mut bots: Vec<(String, String, String)> = Vec::new();
    {
        let mut stmt = db
            .prepare("SELECT id, name, trading_mode FROM bots ORDER BY created_at ASC")
            .map_err(|e| format!("Prepare: {e}"))?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .map_err(|e| format!("Query: {e}"))?;
        for row in rows {
            bots.push(row.map_err(|e| format!("Row: {e}"))?);
        }
    }

    let paper = compute_stats(trades.iter().filter(|t| t.trading_mode.as_deref() != Some("live")));
    let live = compute_stats(trades.iter().filter(|t| t.trading_mode.as_deref() == Some("live")));

    let known: HashSet<&str> = bots.iter().map(|(id, _, _)| id.as_str()).collect();
    let mut per_bot: Vec<BotTradeStats> = bots
        .iter()
        .map(|(id, name, mode)| BotTradeStats {
            bot_id: Some(id.clone()),
            name: name.clone(),
            trading_mode: mode.clone(),
            stats: compute_stats(trades.iter().filter(|t| t.bot_id.as_deref() == Some(id.as_str()))),
        })
        .collect();
    let deleted = compute_stats(
        trades
            .iter()
            .filter(|t| t.bot_id.as_deref().is_some_and(|id| !known.contains(id))),
    );
    if deleted.total_trades > 0.0 {
        per_bot.push(BotTradeStats {
            bot_id: None,
            name: "Deleted bots".into(),
            trading_mode: "paper".into(),
            stats: deleted,
        });
    }

    Ok(TradeStatsBreakdown { paper, live, bots: per_bot })
}

/// Realised PnL per calendar day for the journal's heatmaps: every closed
/// trade the filters select, keyed by the day it closed in the caller's
/// zone (`tz_offset_minutes` east of UTC — the day rolls over at the user's
/// midnight, like the dashboard's windows). Sorted by date.
#[tauri::command(async)]
pub(crate) fn get_daily_pnl(
    mut filters: TradeFilters,
    tz_offset_minutes: i32,
    state: State<'_, AppState>,
) -> Result<Vec<DailyPnl>, String> {
    filters.limit = None;
    filters.offset = None;
    if filters.is_backtest.is_none() {
        filters.is_backtest = Some(false);
    }
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    let trades = query_trades(&db, &filters)?;
    Ok(daily_pnl(&trades, tz_offset_minutes))
}

pub(crate) fn daily_pnl(trades: &[Trade], tz_offset_minutes: i32) -> Vec<DailyPnl> {
    let zone = chrono::FixedOffset::east_opt(tz_offset_minutes.clamp(-14 * 60, 14 * 60) * 60)
        .unwrap_or_else(|| chrono::FixedOffset::east_opt(0).expect("UTC"));
    let mut days: std::collections::BTreeMap<String, DailyPnl> = std::collections::BTreeMap::new();
    for trade in trades {
        let (Some(exit_time), Some(pnl)) = (trade.exit_time.as_deref(), trade.pnl) else { continue };
        let Ok(closed) = chrono::DateTime::parse_from_rfc3339(exit_time) else { continue };
        let date = closed.with_timezone(&zone).format("%Y-%m-%d").to_string();
        let day = days.entry(date.clone()).or_insert(DailyPnl { date, pnl: 0.0, trades: 0 });
        day.pnl += pnl;
        day.trades += 1;
    }
    days.into_values().collect()
}

/// The journal's statistics over a set of trades. `win_rate` is a
/// percentage, like the page shows it.
pub(crate) fn compute_stats<'a>(trades: impl IntoIterator<Item = &'a Trade>) -> TradeStats {
    let mut total = 0.0_f64;
    let mut wins = 0.0_f64;
    let mut total_win = 0.0_f64;
    let mut total_loss = 0.0_f64;
    let mut win_count = 0_u64;
    let mut loss_count = 0_u64;
    let mut best = f64::MIN;
    let mut worst = f64::MAX;
    let mut total_pnl = 0.0_f64;
    let mut total_pnl_pct = 0.0_f64;
    let mut total_duration = 0.0_f64;
    let mut duration_count = 0_u64;

    for trade in trades {
        total += 1.0;
        let pnl = trade.pnl.unwrap_or(0.0);
        total_pnl += pnl;
        total_pnl_pct += trade.pnl_pct.unwrap_or(0.0);

        if pnl > best {
            best = pnl;
        }
        if pnl < worst {
            worst = pnl;
        }

        if pnl > 0.0 {
            wins += 1.0;
            total_win += pnl;
            win_count += 1;
        } else if pnl < 0.0 {
            total_loss += pnl.abs();
            loss_count += 1;
        }

        if let (Some(ref exit_time), entry_time) = (&trade.exit_time, &trade.entry_time) {
            if let (Ok(entry), Ok(exit)) = (
                chrono::DateTime::parse_from_rfc3339(entry_time),
                chrono::DateTime::parse_from_rfc3339(exit_time),
            ) {
                let dur = (exit - entry).num_seconds().max(0) as f64;
                total_duration += dur;
                duration_count += 1;
            }
        }
    }

    if total == 0.0 {
        return TradeStats {
            total_trades: 0.0,
            win_rate: 0.0,
            avg_win: 0.0,
            avg_loss: 0.0,
            profit_factor: 0.0,
            expectancy: 0.0,
            best_trade: 0.0,
            worst_trade: 0.0,
            total_pnl: 0.0,
            total_pnl_pct: 0.0,
            avg_duration_secs: 0.0,
        };
    }

    let win_rate = wins / total * 100.0;
    let avg_win = if win_count > 0 { total_win / win_count as f64 } else { 0.0 };
    let avg_loss = if loss_count > 0 { total_loss / loss_count as f64 } else { 0.0 };
    let profit_factor = if total_loss > 0.0 {
        total_win / total_loss
    } else if total_win > 0.0 {
        f64::INFINITY
    } else {
        0.0
    };
    let expectancy = total_pnl / total;
    let avg_duration = if duration_count > 0 { total_duration / duration_count as f64 } else { 0.0 };

    if best == f64::MIN {
        best = 0.0;
    }
    if worst == f64::MAX {
        worst = 0.0;
    }

    TradeStats {
        total_trades: total,
        win_rate,
        avg_win,
        avg_loss,
        profit_factor,
        expectancy,
        best_trade: best,
        worst_trade: worst,
        total_pnl,
        total_pnl_pct,
        avg_duration_secs: avg_duration,
    }
}

#[tauri::command(async)]
pub(crate) fn update_trade_notes(
    id: String,
    notes: String,
    state: State<'_, AppState>,
) -> Result<Trade, String> {
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    let affected = db
        .execute(
            "UPDATE trades SET notes = ?1 WHERE id = ?2",
            params![notes, id],
        )
        .map_err(|e| format!("Update: {e}"))?;

    if affected == 0 {
        return Err("Trade not found".into());
    }

    db.query_row(&format!("{TRADE_SELECT} WHERE t.id = ?1"), params![id], row_to_trade)
        .map_err(|e| format!("Query: {e}"))
}

/// The equity curve of one bot (`bot_id`) or — with no bot named — of every bot of a
/// `source` (`paper` | `live`) added up: at each step the sum of the bots'
/// latest equities, so a stopped bot keeps contributing what it ended with
/// (PLAN-QUANTALGO §3.4).
#[tauri::command(async)]
pub(crate) fn get_equity_curve(
    source: Option<String>,
    timeframe: Option<String>,
    bot_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<EquityPoint>, String> {
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    equity_curve(&db, source, timeframe, bot_id)
}

pub(crate) fn equity_curve(
    db: &rusqlite::Connection,
    source: Option<String>,
    timeframe: Option<String>,
    bot_id: Option<String>,
) -> Result<Vec<EquityPoint>, String> {
    let src = source.unwrap_or_else(|| "paper".into());

    // The chart's buttons are ranges — the last hour, day, week, month or
    // everything — each with a bucket that keeps a few hundred points.
    let now = chrono::Utc::now();
    let (since, mut bucket): (Option<String>, i64) = match timeframe.as_deref() {
        Some("1h") => (Some((now - chrono::Duration::hours(1)).to_rfc3339()), 60),
        Some("4h") => (Some((now - chrono::Duration::hours(4)).to_rfc3339()), 60),
        Some("1w") => (Some((now - chrono::Duration::days(7)).to_rfc3339()), 1800),
        Some("1M") => (Some((now - chrono::Duration::days(30)).to_rfc3339()), 7200),
        Some("All") => (None, 0),
        _ => (Some((now - chrono::Duration::days(1)).to_rfc3339()), 300),
    };

    let scope: String;
    let scope_param: String;
    match bot_id.as_deref() {
        Some(id) => {
            scope = "bot_id = ?1".into();
            scope_param = id.to_string();
        }
        None => {
            scope = "source = ?1 AND bot_id IS NOT NULL".into();
            scope_param = src.clone();
        }
    }

    if bucket == 0 {
        // "All": one bucket per ~1/400 of the span, never finer than a minute.
        let (first, last): (Option<String>, Option<String>) = db
            .query_row(
                &format!("SELECT MIN(timestamp), MAX(timestamp) FROM equity_snapshots WHERE {scope}"),
                params![scope_param],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|e| format!("Query: {e}"))?;
        let span_secs = match (first, last) {
            (Some(a), Some(b)) => {
                let a = chrono::DateTime::parse_from_rfc3339(&a).map(|d| d.timestamp()).unwrap_or(0);
                let b = chrono::DateTime::parse_from_rfc3339(&b).map(|d| d.timestamp()).unwrap_or(0);
                (b - a).max(0)
            }
            _ => 0,
        };
        bucket = (span_secs / 400).max(60);
    }

    let mut points: Vec<EquityPoint> = Vec::new();

    if bot_id.is_some() {
        // One series: the first row of each bucket (MIN(timestamp) makes the
        // bare equity column come from that row), then the latest row so the
        // curve ends where the bot is now.
        let mut stmt = db
            .prepare(&format!(
                "SELECT MIN(timestamp), equity FROM equity_snapshots WHERE {scope} AND (?3 IS NULL OR timestamp >= ?3) GROUP BY CAST(strftime('%s', timestamp) AS INTEGER) / ?2 ORDER BY 1 ASC"
            ))
            .map_err(|e| format!("Prepare: {e}"))?;
        let rows = stmt
            .query_map(params![scope_param, bucket, since], |row| {
                Ok(EquityPoint { time: row.get(0)?, equity: row.get(1)? })
            })
            .map_err(|e| format!("Query: {e}"))?;
        for row in rows {
            points.push(row.map_err(|e| format!("Row: {e}"))?);
        }
        let latest: Option<(String, f64)> = db
            .query_row(
                &format!("SELECT timestamp, equity FROM equity_snapshots WHERE {scope} ORDER BY timestamp DESC LIMIT 1"),
                params![scope_param],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();
        if let Some((time, equity)) = latest {
            if points.last().map(|p| time > p.time).unwrap_or(true) {
                points.push(EquityPoint { time, equity });
            }
        }
    } else {
        // Every bot of the mode: each bot's last equity before the range
        // seeds the sum, then the bucketed rows inside it carry it forward.
        let mut latest: HashMap<String, f64> = HashMap::new();
        if let Some(ref since) = since {
            let mut stmt = db
                .prepare(&format!(
                    "SELECT bot_id, MAX(timestamp), equity FROM equity_snapshots WHERE {scope} AND timestamp < ?2 GROUP BY bot_id"
                ))
                .map_err(|e| format!("Prepare: {e}"))?;
            let rows = stmt
                .query_map(params![scope_param, since], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, f64>(2)?))
                })
                .map_err(|e| format!("Query: {e}"))?;
            for row in rows {
                let (bot, equity) = row.map_err(|e| format!("Row: {e}"))?;
                latest.insert(bot, equity);
            }
        }
        let mut stmt = db
            .prepare(&format!(
                "SELECT bot_id, MIN(timestamp) AS ts, equity FROM equity_snapshots WHERE {scope} AND (?3 IS NULL OR timestamp >= ?3) GROUP BY bot_id, CAST(strftime('%s', timestamp) AS INTEGER) / ?2 ORDER BY ts ASC"
            ))
            .map_err(|e| format!("Prepare: {e}"))?;
        let rows = stmt
            .query_map(params![scope_param, bucket, since], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, f64>(2)?))
            })
            .map_err(|e| format!("Query: {e}"))?;
        let mut last_bucket: Option<i64> = None;
        for row in rows {
            let (bot, ts, equity) = row.map_err(|e| format!("Row: {e}"))?;
            latest.insert(bot, equity);
            let total: f64 = latest.values().sum();
            let secs = chrono::DateTime::parse_from_rfc3339(&ts).map(|d| d.timestamp()).unwrap_or(0);
            let b = secs / bucket;
            match (last_bucket == Some(b), points.last_mut()) {
                (true, Some(last)) => last.equity = total,
                _ => {
                    points.push(EquityPoint { time: ts, equity: total });
                    last_bucket = Some(b);
                }
            }
        }
    }

    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;
    use rusqlite::Connection;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        for (id, name, mode) in [("b-paper", "Paper bot", "paper"), ("b-live", "Live bot", "live")] {
            conn.execute(
                "INSERT INTO bots (id, name, strategy_id, exchange_id, pair, timeframe, trading_mode, budget, status, created_at, updated_at) VALUES (?1, ?2, 's', 'e', 'BTC/USDT', '1h', ?3, 1000, 'stopped', 't', 't')",
                params![id, name, mode],
            )
            .unwrap();
        }
        let insert = |id: &str, bot: Option<&str>, pnl: f64, backtest: i32| {
            conn.execute(
                "INSERT INTO trades (id, bot_id, strategy_id, exchange, pair, side, entry_price, exit_price, quantity, entry_time, exit_time, pnl, pnl_pct, fee, is_backtest, created_at) VALUES (?1, ?2, 's', 'e', 'BTC/USDT', 'long', 100, 110, 1, '2026-09-07T10:00:00+00:00', '2026-09-07T11:00:00+00:00', ?3, 1, 0.1, ?4, 't')",
                params![id, bot, pnl, backtest],
            )
            .unwrap();
        };
        insert("t1", Some("b-paper"), 10.0, 0);
        insert("t2", Some("b-paper"), -4.0, 0);
        insert("t3", Some("b-live"), 3.0, 0);
        insert("t4", None, 7.0, 0); // the single-bot era
        insert("t5", Some("b-gone"), 2.0, 0); // a deleted bot
        insert("t6", None, 99.0, 1); // a backtest row never counts
        conn
    }

    fn filters(mode: Option<&str>) -> TradeFilters {
        TradeFilters { trading_mode: mode.map(String::from), ..serde_json::from_str("{}").unwrap() }
    }

    #[test]
    fn a_trade_carries_its_bots_mode_and_the_mode_filter_keeps_backtests_out() {
        let conn = db();
        let all = query_trades(&conn, &filters(None)).unwrap();
        assert_eq!(all.len(), 6);
        let by_id = |id: &str| all.iter().find(|t| t.id == id).unwrap().trading_mode.clone();
        assert_eq!(by_id("t1").as_deref(), Some("paper"));
        assert_eq!(by_id("t3").as_deref(), Some("live"));
        assert_eq!(by_id("t4").as_deref(), Some("paper"), "NULL bot_id counts as paper");
        assert_eq!(by_id("t5").as_deref(), Some("paper"), "a deleted bot's rows count as paper");
        assert_eq!(by_id("t6"), None, "a backtest row has no mode");

        let live = query_trades(&conn, &filters(Some("live"))).unwrap();
        assert_eq!(live.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), vec!["t3"]);
        let paper = query_trades(&conn, &filters(Some("paper"))).unwrap();
        let mut ids: Vec<&str> = paper.iter().map(|t| t.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, vec!["t1", "t2", "t4", "t5"], "no backtest row among the paper trades");
    }

    #[test]
    fn stats_are_split_by_mode_and_by_bot_with_the_catch_all_groups() {
        let conn = db();
        let trades = query_trades(&conn, &TradeFilters { is_backtest: Some(false), ..filters(None) }).unwrap();
        let paper = compute_stats(trades.iter().filter(|t| t.trading_mode.as_deref() != Some("live")));
        assert_eq!(paper.total_trades, 4.0);
        assert!((paper.total_pnl - 15.0).abs() < 1e-9);
        assert!((paper.win_rate - 75.0).abs() < 1e-9, "win rate is a percentage");
        let live = compute_stats(trades.iter().filter(|t| t.trading_mode.as_deref() == Some("live")));
        assert_eq!(live.total_trades, 1.0);
        assert!((live.total_pnl - 3.0).abs() < 1e-9);

        let per_bot = compute_stats(trades.iter().filter(|t| t.bot_id.as_deref() == Some("b-paper")));
        assert_eq!(per_bot.total_trades, 2.0);
        assert!((per_bot.total_pnl - 6.0).abs() < 1e-9);
        assert!((per_bot.profit_factor - 2.5).abs() < 1e-9);
        let nothing = compute_stats(std::iter::empty());
        assert_eq!(nothing.total_trades, 0.0);
        assert_eq!(nothing.win_rate, 0.0);
    }

    #[test]
    fn daily_pnl_keys_a_trade_by_the_day_it_closed_in_the_callers_zone() {
        let conn = db();
        conn.execute(
            "UPDATE trades SET exit_time = '2026-09-07T23:30:00+00:00' WHERE id = 't1'",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE trades SET exit_time = '2026-09-08T00:10:00+00:00' WHERE id = 't2'",
            [],
        )
        .unwrap();
        let trades = query_trades(&conn, &TradeFilters { is_backtest: Some(false), ..filters(None) }).unwrap();
        // UTC: t1 on the 7th, t2 on the 8th.
        let utc = daily_pnl(&trades, 0);
        let day = |rows: &[DailyPnl], date: &str| rows.iter().find(|d| d.date == date).cloned();
        assert_eq!(day(&utc, "2026-09-08").map(|d| d.pnl), Some(-4.0));
        assert_eq!(day(&utc, "2026-09-07").map(|d| d.trades), Some(4), "t1, t3, t4, t5 closed on the 7th");
        // Two hours east both fall on the 8th; two hours west both on the 7th.
        let east = daily_pnl(&trades, 120);
        assert_eq!(day(&east, "2026-09-08").map(|d| (d.pnl, d.trades)), Some((6.0, 2)));
        let west = daily_pnl(&trades, -120);
        assert!(day(&west, "2026-09-08").is_none());
        assert_eq!(day(&west, "2026-09-07").map(|d| d.trades), Some(5));
        assert!(utc.windows(2).all(|w| w[0].date <= w[1].date), "sorted by date");
    }

    #[test]
    fn a_modes_equity_curve_is_the_carry_forward_sum_of_its_bots() {
        let conn = db();
        let rows = [
            ("b-paper", "2026-09-07T10:00:00+00:00", 1000.0, "paper"),
            ("b-live", "2026-09-07T10:00:30+00:00", 500.0, "live"),
            ("b-paper", "2026-09-07T10:01:00+00:00", 1010.0, "paper"),
            ("b-two", "2026-09-07T10:02:00+00:00", 2000.0, "paper"),
            ("b-paper", "2026-09-07T10:03:00+00:00", 990.0, "paper"),
        ];
        for (bot, ts, equity, source) in rows {
            conn.execute(
                "INSERT INTO equity_snapshots (bot_id, timestamp, equity, source) VALUES (?1, ?2, ?3, ?4)",
                params![bot, ts, equity, source],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO equity_snapshots (bot_id, timestamp, equity, source) VALUES (NULL, '2026-04-01T00:00:00+00:00', 123.0, 'paper')",
            [],
        )
        .unwrap();

        let paper = equity_curve(&conn, Some("paper".into()), Some("All".into()), None).unwrap();
        let equities: Vec<f64> = paper.iter().map(|p| p.equity).collect();
        // 10:00 paper 1000 · 10:01 1010 · 10:02 1010 + 2000 · 10:03 990 + 2000;
        // the live bot and the single-bot era's row never join the sum.
        assert_eq!(equities, vec![1000.0, 1010.0, 3010.0, 2990.0]);
        assert_eq!(paper[0].time, "2026-09-07T10:00:00+00:00");

        let live = equity_curve(&conn, Some("live".into()), Some("All".into()), None).unwrap();
        assert_eq!(live.iter().map(|p| p.equity).collect::<Vec<_>>(), vec![500.0]);

        let one = equity_curve(&conn, Some("paper".into()), Some("All".into()), Some("b-paper".into())).unwrap();
        assert_eq!(one.iter().map(|p| p.equity).collect::<Vec<_>>(), vec![1000.0, 1010.0, 990.0]);

        // A range older than every row still ends at the bot's latest equity,
        // and a mode's sum keeps a bot whose last snapshot predates the range.
        let recent = equity_curve(&conn, Some("paper".into()), Some("1h".into()), Some("b-paper".into())).unwrap();
        assert_eq!(recent.iter().map(|p| p.equity).collect::<Vec<_>>(), vec![990.0]);
        conn.execute(
            "INSERT INTO equity_snapshots (bot_id, timestamp, equity, source) VALUES ('b-two', ?1, 2100.0, 'paper')",
            params![chrono::Utc::now().to_rfc3339()],
        )
        .unwrap();
        let recent_sum = equity_curve(&conn, Some("paper".into()), Some("1h".into()), None).unwrap();
        assert_eq!(recent_sum.iter().map(|p| p.equity).collect::<Vec<_>>(), vec![990.0 + 2100.0]);
    }
}
