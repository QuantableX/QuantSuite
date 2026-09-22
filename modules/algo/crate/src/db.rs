use chrono::Utc;
use rusqlite::Connection;
use uuid::Uuid;

const EMA_CROSS_STRATEGY: &str = r#"from quantalgo import Strategy


class EMA_Cross(Strategy):
    """EMA crossover strategy — goes long when the fast EMA crosses above
    the slow EMA, goes short when it crosses below, or both.

    The ``direction`` param controls which sides are traded:
      - ``"both"``  — take both long and short entries
      - ``"long"``  — only take long entries
      - ``"short"`` — only take short entries
    """

    params = {
        "fast_period": 12,
        "slow_period": 21,
        "position_size": 0.95,
        "direction": "both",  # "both" | "long" | "short"
    }

    def on_candle(self, candle):
        history = self._candle_history
        if len(history) < self.params["slow_period"] + 1:
            return

        closes = [c.close for c in history]
        fast = self.ema(closes, self.params["fast_period"])
        slow = self.ema(closes, self.params["slow_period"])

        cur_fast, cur_slow = fast[-1], slow[-1]
        prev_fast, prev_slow = fast[-2], slow[-2]

        if None in (cur_fast, cur_slow, prev_fast, prev_slow):
            return

        pair = self._pair
        pos = self.get_position(pair)
        in_position = pos is not None
        direction = self.params["direction"]

        cross_up = prev_fast <= prev_slow and cur_fast > cur_slow
        cross_down = prev_fast >= prev_slow and cur_fast < cur_slow

        # fast crosses above slow
        if cross_up:
            # Reverse short -> long in one host-side operation so sizing uses post-close cash.
            if in_position and getattr(pos, "side", None) == "short":
                if direction in ("both", "long"):
                    self.reverse(pair, "buy", self.params["position_size"])
                else:
                    self.close()
                return
            # open long
            if not in_position and direction in ("both", "long"):
                balance = self.get_balance()
                capital = list(balance.values())[0] if isinstance(balance, dict) else (balance or 0)
                qty = (capital * self.params["position_size"]) / candle.close
                if qty > 0:
                    self.buy(pair, qty)

        # fast crosses below slow
        elif cross_down:
            # Reverse long -> short in one host-side operation so sizing uses post-close cash.
            if in_position and getattr(pos, "side", None) == "long":
                if direction in ("both", "short"):
                    self.reverse(pair, "sell", self.params["position_size"])
                else:
                    self.close()
                return
            # open short
            if not in_position and direction in ("both", "short"):
                balance = self.get_balance()
                capital = list(balance.values())[0] if isinstance(balance, dict) else (balance or 0)
                qty = (capital * self.params["position_size"]) / candle.close
                if qty > 0:
                    self.sell(pair, qty)
"#;

// ---------------------------------------------------------------------------
// Database
// ---------------------------------------------------------------------------

pub fn init_db(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS trades (
            id TEXT PRIMARY KEY,
            strategy_id TEXT NOT NULL,
            exchange TEXT NOT NULL,
            pair TEXT NOT NULL,
            side TEXT NOT NULL,
            entry_price REAL NOT NULL,
            exit_price REAL,
            quantity REAL NOT NULL,
            entry_time TEXT NOT NULL,
            exit_time TEXT,
            pnl REAL,
            pnl_pct REAL,
            fee REAL DEFAULT 0,
            is_backtest INTEGER DEFAULT 0,
            backtest_id TEXT,
            notes TEXT,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS strategies (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            file_path TEXT NOT NULL,
            params_json TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS backtests (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            strategy_id TEXT NOT NULL,
            config_json TEXT NOT NULL,
            stats_json TEXT NOT NULL,
            equity_curve_json TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS exchanges (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            exchange_type TEXT NOT NULL,
            provider TEXT NOT NULL,
            config_encrypted TEXT,
            is_active INTEGER DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS bot_state (
            id TEXT PRIMARY KEY DEFAULT 'singleton',
            status TEXT DEFAULT 'stopped',
            strategy_id TEXT,
            exchange_id TEXT,
            pair TEXT,
            started_at TEXT,
            config_json TEXT,
            trading_mode TEXT DEFAULT 'paper'
        );

        CREATE TABLE IF NOT EXISTS equity_snapshots (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            equity REAL NOT NULL,
            source TEXT DEFAULT 'paper'
        );

        CREATE TABLE IF NOT EXISTS bots (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            strategy_id TEXT NOT NULL,
            exchange_id TEXT NOT NULL,
            pair TEXT NOT NULL,
            timeframe TEXT NOT NULL DEFAULT '1h',
            trading_mode TEXT NOT NULL DEFAULT 'paper',
            budget REAL NOT NULL DEFAULT 10000,
            status TEXT NOT NULL DEFAULT 'stopped',
            started_at TEXT,
            stopped_at TEXT,
            last_error TEXT,
            config_json TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        INSERT OR IGNORE INTO bot_state (id, status) VALUES ('singleton', 'stopped');

        CREATE TABLE IF NOT EXISTS migrations (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL
        );
        ",
    )?;
    // Databases created before the multi-bot model (PLAN-QUANTALGO §3.1)
    // predate these columns; CREATE TABLE IF NOT EXISTS leaves them alone,
    // so the columns are added here, idempotently, before any migration
    // touches them.
    for (table, column, decl) in [
        ("trades", "bot_id", "TEXT"),
        ("equity_snapshots", "bot_id", "TEXT"),
        // Migration 6 (PLAN-QUANTALGO §4.2): private calls of a sandbox
        // exchange go to the venue's testnet / demo environment.
        ("exchanges", "sandbox", "INTEGER NOT NULL DEFAULT 0"),
    ] {
        if !column_exists(conn, table, column)? {
            conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {column} {decl}"), [])?;
        }
    }
    Ok(())
}

pub(crate) fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool, rusqlite::Error> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for name in names {
        if name? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Run pending data migrations. Each migration only runs once.
pub(crate) fn run_migrations(conn: &Connection, strategy_dir: &std::path::Path) -> Result<(), String> {
    let applied: Vec<i64> = conn
        .prepare("SELECT id FROM migrations")
        .and_then(|mut stmt| {
            stmt.query_map([], |row| row.get(0))?
                .collect::<Result<Vec<i64>, _>>()
        })
        .unwrap_or_default();

    let now = chrono::Utc::now().to_rfc3339();

    // Migration 1: Update EMA Cross strategy to support direction (long/short/both)
    if !applied.contains(&1) {
        let updated = migrate_ema_cross_direction(conn, strategy_dir);
        if let Err(e) = &updated {
            eprintln!("[quantalgo] migration 1 (ema direction) failed: {e}");
        }
        // Mark applied even on soft failure so we don't retry endlessly
        let _ = conn.execute(
            "INSERT INTO migrations (id, name, applied_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![1, "ema_cross_direction", now],
        );
    }

    // Migration 2: Add trading_mode column to bot_state
    if !applied.contains(&2) {
        let has_col = conn
            .prepare("SELECT trading_mode FROM bot_state LIMIT 0")
            .is_ok();
        if !has_col {
            let _ = conn.execute(
                "ALTER TABLE bot_state ADD COLUMN trading_mode TEXT DEFAULT 'paper'",
                [],
            );
        }
        let _ = conn.execute(
            "INSERT INTO migrations (id, name, applied_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![2, "bot_state_trading_mode", now],
        );
    }

    // Migration 3: Synthetic runtime snapshots were previously mislabeled as live.
    if !applied.contains(&3) {
        let _ = conn.execute(
            "UPDATE equity_snapshots SET source = 'paper' WHERE source = 'live'",
            [],
        );
        let _ = conn.execute(
            "INSERT INTO migrations (id, name, applied_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![3, "paper_equity_source", now],
        );
    }

    // Migration 4: Update EMA Cross flips to use host-side reverse sizing.
    if !applied.contains(&4) {
        let updated = migrate_ema_cross_atomic_reverse(conn, strategy_dir);
        if let Err(e) = &updated {
            eprintln!("[quantalgo] migration 4 (ema atomic reverse) failed: {e}");
        }
        let _ = conn.execute(
            "INSERT INTO migrations (id, name, applied_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![4, "ema_cross_atomic_reverse", now],
        );
    }

    // Migration 5: the multi-bot model. The columns were added by init_db;
    // the singleton bot_state row is retired — a bot left "running" there
    // has no process in this build, and rows from the single-bot era keep
    // bot_id NULL (PLAN-QUANTALGO §3.1).
    if !applied.contains(&5) {
        let _ = conn.execute(
            "UPDATE bot_state SET status = 'stopped', strategy_id = NULL, exchange_id = NULL, pair = NULL, started_at = NULL, config_json = NULL, trading_mode = 'paper' WHERE id = 'singleton'",
            [],
        );
        let _ = conn.execute(
            "INSERT INTO migrations (id, name, applied_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![5, "multi_bot", now],
        );
    }

    // Migration 7 (user, 2026-09-07): the single-bot era's rows go — its
    // paper trades and the equity snapshots without a bot. Backtest rows
    // (also without a bot) stay. Migration 6 is the sandbox column above.
    if !applied.contains(&7) {
        let _ = conn.execute("DELETE FROM trades WHERE bot_id IS NULL AND is_backtest = 0", []);
        let _ = conn.execute("DELETE FROM equity_snapshots WHERE bot_id IS NULL", []);
        let _ = conn.execute(
            "INSERT INTO migrations (id, name, applied_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![7, "drop_single_bot_era", now],
        );
    }

    Ok(())
}


/// Migration 1: update existing EMA Cross strategy file + params to include direction.
fn migrate_ema_cross_direction(
    conn: &Connection,
    _strategy_dir: &std::path::Path,
) -> Result<(), String> {
    // Find the EMA Cross strategy by name
    let row: Option<(String, String, Option<String>)> = conn
        .query_row(
            "SELECT id, file_path, params_json FROM strategies WHERE name = 'EMA Cross' LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .ok();

    let (id, file_path, params_json) = match row {
        Some(r) => r,
        None => return Ok(()), // no EMA Cross strategy, nothing to migrate
    };

    // Update the strategy file on disk
    std::fs::write(&file_path, EMA_CROSS_STRATEGY)
        .map_err(|e| format!("Write strategy file: {e}"))?;

    // Merge direction into existing params (preserve user's custom values)
    let mut params: serde_json::Value = params_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    if let Some(obj) = params.as_object_mut() {
        obj.entry("direction".to_string())
            .or_insert(serde_json::json!("both"));
    }

    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE strategies SET params_json = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![serde_json::to_string(&params).unwrap_or_default(), now, id],
    )
    .map_err(|e| format!("Update params: {e}"))?;

    Ok(())
}

/// Migration 4: update the built-in EMA Cross strategy so reversals are atomic.
fn migrate_ema_cross_atomic_reverse(
    conn: &Connection,
    _strategy_dir: &std::path::Path,
) -> Result<(), String> {
    let row: Option<(String, String, Option<String>)> = conn
        .query_row(
            "SELECT id, file_path, params_json FROM strategies WHERE name = 'EMA Cross' LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .ok();

    let (id, file_path, params_json) = match row {
        Some(r) => r,
        None => return Ok(()),
    };

    let current_code = std::fs::read_to_string(&file_path).unwrap_or_default();
    let has_stale_close_then_size = current_code.contains("self.close()")
        && current_code.contains("qty = (capital * self.params[\"position_size\"]) / candle.close")
        && !current_code.contains("self.reverse(");

    if current_code.is_empty() || has_stale_close_then_size {
        std::fs::write(&file_path, EMA_CROSS_STRATEGY)
            .map_err(|e| format!("Write atomic reverse strategy file: {e}"))?;
    }

    let mut params: serde_json::Value = params_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    if let Some(obj) = params.as_object_mut() {
        obj.entry("direction".to_string())
            .or_insert(serde_json::json!("both"));
    }

    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE strategies SET params_json = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![serde_json::to_string(&params).unwrap_or_default(), now, id],
    )
    .map_err(|e| format!("Update params: {e}"))?;

    Ok(())
}

/// Seed the built-in EMA Cross test strategy on first run (no strategies yet).
pub(crate) fn seed_strategies(conn: &Connection, strategy_dir: &std::path::Path) -> Result<(), String> {
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM strategies", [], |row| row.get(0))
        .map_err(|e| format!("Count strategies: {e}"))?;
    if count > 0 {
        return Ok(());
    }

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let file_name = format!("EMA_Cross_{}.py", &id[..8]);
    let file_path = strategy_dir.join(&file_name);

    std::fs::write(&file_path, EMA_CROSS_STRATEGY)
        .map_err(|e| format!("Write seed strategy: {e}"))?;

    let params_json =
        r#"{"fast_period":12,"slow_period":21,"position_size":0.95,"direction":"both"}"#;
    conn.execute(
        "INSERT INTO strategies (id, name, description, file_path, params_json, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            id,
            "EMA Cross",
            "EMA crossover (12/21) — long, short, or both on golden/death cross",
            file_path.to_string_lossy().to_string(),
            params_json,
            now,
            now,
        ],
    )
    .map_err(|e| format!("Insert seed strategy: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A database from before the bots table (the v4 schema) must come out
    /// of `init_db` + `run_migrations` with the columns and table the
    /// multi-bot model needs, and keep its rows.
    #[test]
    fn a_v4_database_gains_bot_id_columns_and_the_bots_table() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE trades (id TEXT PRIMARY KEY, strategy_id TEXT NOT NULL, exchange TEXT NOT NULL, pair TEXT NOT NULL, side TEXT NOT NULL, entry_price REAL NOT NULL, exit_price REAL, quantity REAL NOT NULL, entry_time TEXT NOT NULL, exit_time TEXT, pnl REAL, pnl_pct REAL, fee REAL DEFAULT 0, is_backtest INTEGER DEFAULT 0, backtest_id TEXT, notes TEXT, created_at TEXT NOT NULL);
            CREATE TABLE equity_snapshots (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp TEXT NOT NULL, equity REAL NOT NULL, source TEXT DEFAULT 'paper');
            CREATE TABLE bot_state (id TEXT PRIMARY KEY DEFAULT 'singleton', status TEXT DEFAULT 'stopped', strategy_id TEXT, exchange_id TEXT, pair TEXT, started_at TEXT, config_json TEXT, trading_mode TEXT DEFAULT 'paper');
            CREATE TABLE migrations (id INTEGER PRIMARY KEY, name TEXT NOT NULL, applied_at TEXT NOT NULL);
            INSERT INTO migrations (id, name, applied_at) VALUES (1,'a','t'),(2,'b','t'),(3,'c','t'),(4,'d','t');
            INSERT INTO trades (id, strategy_id, exchange, pair, side, entry_price, quantity, entry_time, created_at) VALUES ('t1','s','e','BTC/USDT','long',1.0,1.0,'t','t');
            INSERT INTO equity_snapshots (timestamp, equity, source) VALUES ('t', 100.0, 'paper');
            INSERT INTO bot_state (id, status, strategy_id) VALUES ('singleton', 'running', 's');
            ",
        )
        .unwrap();

        init_db(&conn).unwrap();
        run_migrations(&conn, std::path::Path::new(".")).unwrap();

        assert!(column_exists(&conn, "trades", "bot_id").unwrap());
        assert!(column_exists(&conn, "equity_snapshots", "bot_id").unwrap());
        assert!(column_exists(&conn, "exchanges", "sandbox").unwrap());
        let legacy: i64 = conn
            .query_row("SELECT COUNT(*) FROM trades WHERE id = 't1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(legacy, 0, "migration 7 drops the single-bot era's rows");
        let legacy_snapshots: i64 = conn
            .query_row("SELECT COUNT(*) FROM equity_snapshots WHERE bot_id IS NULL", [], |r| r.get(0))
            .unwrap();
        assert_eq!(legacy_snapshots, 0, "and its snapshots");
        let bots: i64 = conn.query_row("SELECT COUNT(*) FROM bots", [], |r| r.get(0)).unwrap();
        assert_eq!(bots, 0);
        let status: String = conn
            .query_row("SELECT status FROM bot_state WHERE id = 'singleton'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(status, "stopped", "the singleton is retired, never 'running'");
        let applied: i64 = conn
            .query_row("SELECT COUNT(*) FROM migrations WHERE id = 5", [], |r| r.get(0))
            .unwrap();
        assert_eq!(applied, 1);

        // Idempotent: a second pass changes nothing and adds nothing.
        init_db(&conn).unwrap();
        run_migrations(&conn, std::path::Path::new(".")).unwrap();
        let applied: i64 = conn
            .query_row("SELECT COUNT(*) FROM migrations WHERE id = 5", [], |r| r.get(0))
            .unwrap();
        assert_eq!(applied, 1);
    }

    #[test]
    fn migration_7_drops_the_single_bot_era_rows_and_keeps_backtests() {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO migrations (id, name, applied_at) VALUES (1,'a','t'),(2,'b','t'),(3,'c','t'),(4,'d','t'),(5,'e','t');
             INSERT INTO trades (id, strategy_id, exchange, pair, side, entry_price, quantity, entry_time, pnl, is_backtest, created_at) VALUES
               ('legacy', 's', 'e', 'BTC/USDT', 'long', 1, 1, 't', 1.0, 0, 't'),
               ('bt', 's', 'e', 'BTC/USDT', 'long', 1, 1, 't', 2.0, 1, 't');
             INSERT INTO trades (id, bot_id, strategy_id, exchange, pair, side, entry_price, quantity, entry_time, pnl, is_backtest, created_at) VALUES
               ('mine', 'b1', 's', 'e', 'BTC/USDT', 'long', 1, 1, 't', 3.0, 0, 't');
             INSERT INTO equity_snapshots (bot_id, timestamp, equity, source) VALUES (NULL, 't', 1.0, 'paper'), ('b1', 't', 2.0, 'paper');",
        )
        .unwrap();

        run_migrations(&conn, std::path::Path::new(".")).unwrap();

        let ids: Vec<String> = conn
            .prepare("SELECT id FROM trades ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(ids, vec!["bt", "mine"], "the legacy paper trade is gone, the backtest and the bot's trade stay");
        let snapshots: i64 = conn.query_row("SELECT COUNT(*) FROM equity_snapshots", [], |r| r.get(0)).unwrap();
        assert_eq!(snapshots, 1, "only the bot's snapshot remains");
        let applied: i64 = conn.query_row("SELECT COUNT(*) FROM migrations WHERE id = 7", [], |r| r.get(0)).unwrap();
        assert_eq!(applied, 1);
    }
}
