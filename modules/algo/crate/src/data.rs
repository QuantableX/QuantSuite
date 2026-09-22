use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::State;
use crate::bots::Bot;
use crate::{AppSettings, AppState, BotStatus, Exchange, Strategy, Trade};
use crate::settings::{
    get_data_dir,
    normalize_settings_units,
    save_settings_to_disk,
    validate_app_settings,
};

#[derive(Debug, Serialize, Deserialize)]
struct StrategyExport {
    metadata: Strategy,
    code: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct BacktestExportRow {
    id: String,
    name: String,
    strategy_id: String,
    config_json: String,
    stats_json: String,
    equity_curve_json: String,
    created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExchangeExportRow {
    exchange: Exchange,
    config_encrypted: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct EquitySnapshotRow {
    #[serde(default)]
    bot_id: Option<String>,
    timestamp: String,
    equity: f64,
    source: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct DataExport {
    exported_at: String,
    settings: AppSettings,
    strategies: Vec<StrategyExport>,
    backtests: Vec<BacktestExportRow>,
    exchanges: Vec<ExchangeExportRow>,
    trades: Vec<Trade>,
    equity_snapshots: Vec<EquitySnapshotRow>,
    bot_state: BotStatus,
    /// The bots table (PLAN-QUANTALGO §3.1); absent in exports from the
    /// single-bot era.
    #[serde(default)]
    bots: Vec<Bot>,
}

fn latest_export_file() -> Result<PathBuf, String> {
    let export_dir = get_data_dir().join("exports");
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(&export_dir)
        .map_err(|e| format!("Read export dir: {e}"))?
        .filter_map(|entry| entry.ok().map(|item| item.path()))
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .collect();

    candidates.sort();
    candidates
        .pop()
        .ok_or_else(|| "No export snapshot found. Export data first.".to_string())
}

// ---------------------------------------------------------------------------
// Data Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub(crate) async fn export_all_data(state: State<'_, AppState>) -> Result<String, String> {
    let settings = state
        .settings
        .lock()
        .map_err(|e| format!("Lock: {e}"))?
        .clone();
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;

    let mut strategies_stmt = db
        .prepare("SELECT id, name, description, file_path, params_json, created_at, updated_at FROM strategies ORDER BY updated_at DESC")
        .map_err(|e| format!("Prepare strategies: {e}"))?;
    let strategies = strategies_stmt
        .query_map([], |row| {
            Ok(Strategy {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                file_path: row.get(3)?,
                params_json: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .map_err(|e| format!("Query strategies: {e}"))?
        .filter_map(|row| row.ok())
        .map(|strategy| StrategyExport {
            code: std::fs::read_to_string(&strategy.file_path).unwrap_or_default(),
            metadata: strategy,
        })
        .collect::<Vec<_>>();

    let mut backtests_stmt = db
        .prepare("SELECT id, name, strategy_id, config_json, stats_json, equity_curve_json, created_at FROM backtests ORDER BY created_at DESC")
        .map_err(|e| format!("Prepare backtests: {e}"))?;
    let backtests = backtests_stmt
        .query_map([], |row| {
            Ok(BacktestExportRow {
                id: row.get(0)?,
                name: row.get(1)?,
                strategy_id: row.get(2)?,
                config_json: row.get(3)?,
                stats_json: row.get(4)?,
                equity_curve_json: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|e| format!("Query backtests: {e}"))?
        .filter_map(|row| row.ok())
        .collect::<Vec<_>>();

    let mut exchanges_stmt = db
        .prepare("SELECT id, name, exchange_type, provider, is_active, created_at, updated_at, config_encrypted, sandbox FROM exchanges ORDER BY name")
        .map_err(|e| format!("Prepare exchanges: {e}"))?;
    let exchanges = exchanges_stmt
        .query_map([], |row| {
            Ok(ExchangeExportRow {
                exchange: Exchange {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    exchange_type: row.get(2)?,
                    provider: row.get(3)?,
                    is_active: row.get::<_, i32>(4)? != 0,
                    sandbox: row.get::<_, i32>(8)? != 0,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                },
                config_encrypted: row.get(7)?,
            })
        })
        .map_err(|e| format!("Query exchanges: {e}"))?
        .filter_map(|row| row.ok())
        .collect::<Vec<_>>();

    let mut trades_stmt = db
        .prepare("SELECT id, strategy_id, exchange, pair, side, entry_price, exit_price, quantity, entry_time, exit_time, pnl, pnl_pct, fee, is_backtest, backtest_id, notes, created_at, bot_id FROM trades ORDER BY entry_time DESC")
        .map_err(|e| format!("Prepare trades: {e}"))?;
    let trades = trades_stmt
        .query_map([], |row| {
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
                trading_mode: None,
            })
        })
        .map_err(|e| format!("Query trades: {e}"))?
        .filter_map(|row| row.ok())
        .collect::<Vec<_>>();

    let mut equity_stmt = db
        .prepare("SELECT bot_id, timestamp, equity, source FROM equity_snapshots ORDER BY timestamp ASC")
        .map_err(|e| format!("Prepare equity snapshots: {e}"))?;
    let equity_snapshots = equity_stmt
        .query_map([], |row| {
            Ok(EquitySnapshotRow {
                bot_id: row.get(0)?,
                timestamp: row.get(1)?,
                equity: row.get(2)?,
                source: row.get(3)?,
            })
        })
        .map_err(|e| format!("Query equity snapshots: {e}"))?
        .filter_map(|row| row.ok())
        .collect::<Vec<_>>();

    let bot_state = db
        .query_row(
            "SELECT status, strategy_id, exchange_id, pair, started_at, config_json, trading_mode FROM bot_state WHERE id = 'singleton'",
            [],
            |row| {
                Ok(BotStatus {
                    status: row.get(0)?,
                    strategy_id: row.get(1)?,
                    exchange_id: row.get(2)?,
                    pair: row.get(3)?,
                    started_at: row.get(4)?,
                    config_json: row.get(5)?,
                    trading_mode: row.get::<_, Option<String>>(6)?.unwrap_or_else(|| "paper".into()),
                })
            },
        )
        .map_err(|e| format!("Query bot state: {e}"))?;

    let bots = crate::bots::load_bots(&db)?;

    let export = DataExport {
        exported_at: Utc::now().to_rfc3339(),
        settings,
        strategies,
        backtests,
        exchanges,
        trades,
        equity_snapshots,
        bot_state,
        bots,
    };

    let export_dir = get_data_dir().join("exports");
    std::fs::create_dir_all(&export_dir).map_err(|e| format!("Create export dir: {e}"))?;
    let path = export_dir.join(format!(
        "quantalgo-export-{}.json",
        Utc::now().format("%Y%m%d-%H%M%S")
    ));

    let json =
        serde_json::to_string_pretty(&export).map_err(|e| format!("Serialize export: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("Write export: {e}"))?;

    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub(crate) async fn import_data(state: State<'_, AppState>) -> Result<String, String> {
    let path = latest_export_file()?;
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("Read import: {e}"))?;
    let mut export: DataExport =
        serde_json::from_str(&raw).map_err(|e| format!("Parse import: {e}"))?;

    normalize_settings_units(&mut export.settings);
    validate_app_settings(&export.settings)?;
    save_settings_to_disk(&export.settings)?;
    {
        let mut settings = state.settings.lock().map_err(|e| format!("Lock: {e}"))?;
        *settings = export.settings.clone();
    }

    let strategy_dir = PathBuf::from(&export.settings.strategy_dir);
    std::fs::create_dir_all(&strategy_dir).map_err(|e| format!("Create strategy dir: {e}"))?;

    let mut db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    // Wipe + repopulate in one transaction — a failure halfway must not leave the
    // user with an emptied database. The strategy files are collected here and
    // only written after the commit: a rollback cannot undo an fs::write, and a
    // restored strategies row pointing at an overwritten .py file would feed the
    // bot foreign code under the old name.
    let mut pending_files: Vec<(PathBuf, String)> = Vec::new();
    let tx = db
        .transaction()
        .map_err(|e| format!("Begin import: {e}"))?;
    tx.execute_batch(
        "
        DELETE FROM trades;
        DELETE FROM backtests;
        DELETE FROM exchanges;
        DELETE FROM strategies;
        DELETE FROM equity_snapshots;
        DELETE FROM bots;
        DELETE FROM bot_state;
        INSERT INTO bot_state (id, status, strategy_id, exchange_id, pair, started_at, config_json, trading_mode)
        VALUES ('singleton', 'stopped', NULL, NULL, NULL, NULL, NULL, 'paper');
        ",
    )
    .map_err(|e| format!("Reset database: {e}"))?;

    for strategy in export.strategies {
        let file_name = PathBuf::from(&strategy.metadata.file_path)
            .file_name()
            .and_then(|value| value.to_str())
            .map(|value| value.to_string())
            .unwrap_or_else(|| format!("strategy_{}.py", strategy.metadata.id));
        let target_path = strategy_dir.join(file_name);
        pending_files.push((target_path.clone(), strategy.code));

        tx.execute(
            "INSERT INTO strategies (id, name, description, file_path, params_json, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                strategy.metadata.id,
                strategy.metadata.name,
                strategy.metadata.description,
                target_path.to_string_lossy().to_string(),
                strategy.metadata.params_json,
                strategy.metadata.created_at,
                strategy.metadata.updated_at,
            ],
        )
        .map_err(|e| format!("Insert strategy: {e}"))?;
    }

    for backtest in export.backtests {
        tx.execute(
            "INSERT INTO backtests (id, name, strategy_id, config_json, stats_json, equity_curve_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                backtest.id,
                backtest.name,
                backtest.strategy_id,
                backtest.config_json,
                backtest.stats_json,
                backtest.equity_curve_json,
                backtest.created_at,
            ],
        )
        .map_err(|e| format!("Insert backtest: {e}"))?;
    }

    for exchange in export.exchanges {
        tx.execute(
            "INSERT INTO exchanges (id, name, exchange_type, provider, config_encrypted, is_active, sandbox, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                exchange.exchange.id,
                exchange.exchange.name,
                exchange.exchange.exchange_type,
                exchange.exchange.provider,
                exchange.config_encrypted,
                if exchange.exchange.is_active { 1 } else { 0 },
                exchange.exchange.sandbox as i32,
                exchange.exchange.created_at,
                exchange.exchange.updated_at,
            ],
        )
        .map_err(|e| format!("Insert exchange: {e}"))?;
    }

    for trade in export.trades {
        tx.execute(
            "INSERT INTO trades (id, strategy_id, exchange, pair, side, entry_price, exit_price, quantity, entry_time, exit_time, pnl, pnl_pct, fee, is_backtest, backtest_id, notes, created_at, bot_id) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
            params![
                trade.id,
                trade.strategy_id,
                trade.exchange,
                trade.pair,
                trade.side,
                trade.entry_price,
                trade.exit_price,
                trade.quantity,
                trade.entry_time,
                trade.exit_time,
                trade.pnl,
                trade.pnl_pct,
                trade.fee,
                if trade.is_backtest { 1 } else { 0 },
                trade.backtest_id,
                trade.notes,
                trade.created_at,
                trade.bot_id,
            ],
        )
        .map_err(|e| format!("Insert trade: {e}"))?;
    }

    for snapshot in export.equity_snapshots {
        tx.execute(
            "INSERT INTO equity_snapshots (bot_id, timestamp, equity, source) VALUES (?1, ?2, ?3, ?4)",
            params![snapshot.bot_id, snapshot.timestamp, snapshot.equity, snapshot.source],
        )
        .map_err(|e| format!("Insert snapshot: {e}"))?;
    }

    for bot in export.bots {
        tx.execute(
            "INSERT INTO bots (id, name, strategy_id, exchange_id, pair, timeframe, trading_mode, budget, status, started_at, stopped_at, last_error, config_json, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'stopped', NULL, ?9, ?10, ?11, ?12, ?13)",
            params![
                bot.id,
                bot.name,
                bot.strategy_id,
                bot.exchange_id,
                bot.pair,
                bot.timeframe,
                bot.trading_mode,
                bot.budget,
                bot.stopped_at,
                bot.last_error,
                bot.config_json,
                bot.created_at,
                bot.updated_at,
            ],
        )
        .map_err(|e| format!("Insert bot: {e}"))?;
    }

    tx.execute(
        "UPDATE bot_state SET status = ?1, strategy_id = ?2, exchange_id = ?3, pair = ?4, started_at = ?5, config_json = ?6, trading_mode = ?7 WHERE id = 'singleton'",
        params![
            export.bot_state.status,
            export.bot_state.strategy_id,
            export.bot_state.exchange_id,
            export.bot_state.pair,
            export.bot_state.started_at,
            export.bot_state.config_json,
            export.bot_state.trading_mode,
        ],
    )
    .map_err(|e| format!("Restore bot state: {e}"))?;

    tx.commit().map_err(|e| format!("Commit import: {e}"))?;

    for (target_path, code) in pending_files {
        std::fs::write(&target_path, code).map_err(|e| format!("Write strategy file: {e}"))?;
    }

    Ok(path.to_string_lossy().to_string())
}
