use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use tauri::State;
use crate::{AppSettings, AppState};

pub(crate) const DEFAULT_RISK_PER_TRADE_PCT: f64 = 1.0;
const DEFAULT_MAX_CONCURRENT_POSITIONS: u32 = 2;
pub(crate) const DEFAULT_SLIPPAGE_TOLERANCE_PCT: f64 = 0.0;
pub(crate) const DEFAULT_PAPER_FEE_PCT: f64 = 0.1;

pub(crate) const MAX_RISK_PER_TRADE_PCT: f64 = 10.0;
pub(crate) const WARN_RISK_PER_TRADE_PCT: f64 = 5.0;
pub(crate) const MAX_CONCURRENT_POSITIONS: u64 = 20;
pub(crate) const WARN_CONCURRENT_POSITIONS: u64 = 10;
pub(crate) const MAX_SLIPPAGE_TOLERANCE_PCT: f64 = 5.0;
pub(crate) const WARN_SLIPPAGE_TOLERANCE_PCT: f64 = 1.0;
pub(crate) const MAX_PAPER_FEE_PCT: f64 = 5.0;
pub(crate) const WARN_PAPER_FEE_PCT: f64 = 1.0;

pub(crate) fn default_paper_fee_pct() -> f64 {
    DEFAULT_PAPER_FEE_PCT
}

pub(crate) const DEFAULT_BUDGET: f64 = 10_000.0;
pub(crate) const MIN_BUDGET: f64 = 100.0;
pub(crate) const DEFAULT_WARMUP_CANDLES: u32 = 200;
pub(crate) const MAX_WARMUP_CANDLES: u32 = 3000;

pub(crate) fn default_budget() -> f64 {
    DEFAULT_BUDGET
}

pub(crate) fn default_warmup_candles() -> u32 {
    DEFAULT_WARMUP_CANDLES
}

/// The bot defaults the user set on 2026-09-07. A config below this version
/// gets them written once at load: the old app's 1h had survived there, and
/// the Settings page of an earlier build had auto-saved the first listed
/// pair (BTC/AED) when none was stored.
pub(crate) const BOT_DEFAULTS_VERSION: u32 = 2;

pub(crate) fn apply_bot_defaults(settings: &mut AppSettings) -> bool {
    if settings.defaults_version >= BOT_DEFAULTS_VERSION {
        return false;
    }
    settings.default_pair = "BTC/USDT".into();
    settings.default_timeframe = "1m".into();
    settings.default_budget = DEFAULT_BUDGET;
    settings.risk_per_trade = DEFAULT_RISK_PER_TRADE_PCT;
    settings.max_concurrent_positions = DEFAULT_MAX_CONCURRENT_POSITIONS;
    settings.slippage_tolerance = DEFAULT_SLIPPAGE_TOLERANCE_PCT;
    settings.paper_fee_pct = DEFAULT_PAPER_FEE_PCT;
    settings.default_warmup_candles = DEFAULT_WARMUP_CANDLES;
    settings.defaults_version = BOT_DEFAULTS_VERSION;
    true
}

/// `~/.quantsuite/modules/algo/`. Was `<data_dir>/quantalgo` in the standalone
/// app; [`import_legacy_data`] copies that across on first launch.
pub fn get_data_dir() -> PathBuf {
    let dir = qs_core::paths::module_dir("algo");
    std::fs::create_dir_all(&dir).ok();
    dir
}

/// One-time import of the standalone app's data directory: strategies,
/// backtests, logs and `quantalgo.db`. Copies, never moves.
pub(crate) fn import_legacy_data(target: &Path) {
    let marker = target.join(".migrated");
    if marker.exists() {
        return;
    }

    let legacy = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("quantalgo");

    if legacy.exists() && legacy != target {
        if let Err(e) = copy_dir_recursive(&legacy, target) {
            eprintln!("algo: legacy import failed: {e}");
            return;
        }
    }

    let _ = std::fs::write(&marker, chrono::Utc::now().to_rfc3339());
}

fn copy_dir_recursive(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&src, &dst)?;
        } else if !dst.exists() {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

pub fn get_default_settings() -> AppSettings {
    let data_dir = get_data_dir();
    AppSettings {
        theme: "dark".into(),
        font_size: 14,
        default_exchange_id: None,
        default_pair: "BTC/USDT".into(),
        default_timeframe: "1m".into(),
        python_path: if cfg!(windows) {
            "py".into()
        } else {
            "python3".into()
        },
        strategy_dir: data_dir.join("strategies").to_string_lossy().into_owned(),
        backtest_dir: data_dir.join("backtests").to_string_lossy().into_owned(),
        risk_per_trade: DEFAULT_RISK_PER_TRADE_PCT,
        max_concurrent_positions: DEFAULT_MAX_CONCURRENT_POSITIONS,
        slippage_tolerance: DEFAULT_SLIPPAGE_TOLERANCE_PCT,
        paper_fee_pct: DEFAULT_PAPER_FEE_PCT,
        default_budget: DEFAULT_BUDGET,
        default_warmup_candles: DEFAULT_WARMUP_CANDLES,
        defaults_version: BOT_DEFAULTS_VERSION,
        notify_on_trade: true,
        notify_on_error: true,
        notify_on_daily_summary: false,
    }
}

pub(crate) fn load_settings_from_disk() -> AppSettings {
    let path = get_data_dir().join("config.json");
    let mut settings = if let Ok(data) = std::fs::read_to_string(&path) {
        serde_json::from_str(&data).unwrap_or_else(|_| get_default_settings())
    } else {
        get_default_settings()
    };

    let normalized = normalize_settings_units(&mut settings);
    let reset = apply_bot_defaults(&mut settings);
    if normalized || reset {
        let _ = save_settings_to_disk(&settings);
    }
    settings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_older_config_gets_the_bot_defaults_once() {
        let mut settings = get_default_settings();
        settings.defaults_version = 0;
        settings.default_pair = "BTC/AED".into();
        settings.default_timeframe = "1h".into();
        settings.risk_per_trade = 2.0;
        settings.max_concurrent_positions = 3;
        settings.slippage_tolerance = 0.1;

        assert!(apply_bot_defaults(&mut settings));
        assert_eq!(settings.default_pair, "BTC/USDT");
        assert_eq!(settings.default_timeframe, "1m");
        assert_eq!(settings.default_budget, 10_000.0);
        assert_eq!(settings.risk_per_trade, 1.0);
        assert_eq!(settings.max_concurrent_positions, 2);
        assert_eq!(settings.slippage_tolerance, 0.0);
        assert_eq!(settings.paper_fee_pct, 0.1);
        assert_eq!(settings.default_warmup_candles, 200);
        assert_eq!(settings.defaults_version, BOT_DEFAULTS_VERSION);

        // Once: the user's later choices stay.
        settings.default_pair = "ETH/USDT".into();
        assert!(!apply_bot_defaults(&mut settings));
        assert_eq!(settings.default_pair, "ETH/USDT");
    }

    #[test]
    fn a_config_without_the_version_field_counts_as_old() {
        let json = r#"{"theme":"dark","font_size":14,"default_pair":"BTC/AED","default_timeframe":"1h","python_path":"py","strategy_dir":"s","backtest_dir":"b","risk_per_trade":2.0,"max_concurrent_positions":3,"slippage_tolerance":0.1,"paper_fee_pct":0.1,"notify_on_trade":true,"notify_on_error":true,"notify_on_daily_summary":false}"#;
        let mut settings: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.defaults_version, 0);
        assert!(apply_bot_defaults(&mut settings));
        assert_eq!(settings.default_pair, "BTC/USDT");
        assert_eq!(settings.default_timeframe, "1m");
    }
}

pub(crate) fn save_settings_to_disk(settings: &AppSettings) -> Result<(), String> {
    let path = get_data_dir().join("config.json");
    let json = serde_json::to_string_pretty(settings).map_err(|e| format!("Serialize: {e}"))?;
    qs_core::paths::write_atomic(&path, json.as_bytes()).map_err(|e| format!("Write config: {e}"))
}

pub(crate) fn normalize_settings_units(settings: &mut AppSettings) -> bool {
    let mut changed = false;

    if settings.risk_per_trade > 0.0 && settings.risk_per_trade < 0.1 {
        settings.risk_per_trade *= 100.0;
        changed = true;
    }
    if settings.slippage_tolerance > 0.0 && settings.slippage_tolerance < 0.01 {
        settings.slippage_tolerance *= 100.0;
        changed = true;
    }
    if settings.paper_fee_pct < 0.0 {
        settings.paper_fee_pct = DEFAULT_PAPER_FEE_PCT;
        changed = true;
    }
    if settings.max_concurrent_positions == 0 {
        settings.max_concurrent_positions = DEFAULT_MAX_CONCURRENT_POSITIONS;
        changed = true;
    }

    changed
}

pub(crate) fn validate_app_settings(settings: &AppSettings) -> Result<(), String> {
    if !(settings.risk_per_trade > 0.0 && settings.risk_per_trade <= MAX_RISK_PER_TRADE_PCT) {
        return Err(format!(
            "Risk per trade must be greater than 0 and no more than {MAX_RISK_PER_TRADE_PCT:.0}%."
        ));
    }
    if !(1..=MAX_CONCURRENT_POSITIONS).contains(&(settings.max_concurrent_positions as u64)) {
        return Err(format!(
            "Max concurrent positions must be between 1 and {MAX_CONCURRENT_POSITIONS}."
        ));
    }
    if !(settings.slippage_tolerance >= 0.0
        && settings.slippage_tolerance <= MAX_SLIPPAGE_TOLERANCE_PCT)
    {
        return Err(format!(
            "Slippage tolerance must be between 0 and {MAX_SLIPPAGE_TOLERANCE_PCT:.0}%."
        ));
    }
    if !(settings.paper_fee_pct >= 0.0 && settings.paper_fee_pct <= MAX_PAPER_FEE_PCT) {
        return Err(format!(
            "Paper fee must be between 0 and {MAX_PAPER_FEE_PCT:.0}%."
        ));
    }
    if !(settings.default_budget >= MIN_BUDGET && settings.default_budget.is_finite()) {
        return Err(format!("The default budget must be at least {MIN_BUDGET:.0}."));
    }
    if !(2..=MAX_WARMUP_CANDLES).contains(&settings.default_warmup_candles) {
        return Err(format!("Warm-up candles must be between 2 and {MAX_WARMUP_CANDLES}."));
    }
    Ok(())
}

/// Where `import quantalgo` resolves from, for the runner's PYTHONPATH.
///
/// Resolved once during plugin setup — the only moment an `AppHandle` is in
/// reach, which qs-core needs to check the Tauri resource dir of an installed
/// suite. `build_python_command` runs deep inside command handlers and reads
/// the cache. The previous version baked `CARGO_MANIFEST_DIR` into the binary,
/// which pointed at a directory that only exists on the build machine — every
/// installed copy shipped with a dead PYTHONPATH.
pub(crate) static PYTHON_SDK_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

fn get_python_sdk_dir() -> Option<PathBuf> {
    PYTHON_SDK_DIR.get().cloned().flatten()
}

pub(crate) fn resolve_python_path(settings: &AppSettings) -> String {
    let configured = settings.python_path.trim();
    if configured.is_empty() {
        if cfg!(windows) {
            "py".into()
        } else {
            "python3".into()
        }
    } else {
        configured.to_string()
    }
}

/// CREATE_NO_WINDOW: the release binary is a GUI-subsystem process without a
/// console, so a spawned console child would otherwise open its own visible
/// console window for its whole lifetime. Dev builds have a console the child
/// inherits, which is why the window only ever shows up in installed builds.
pub(crate) fn hide_console_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

pub(crate) fn build_python_command(python_path: &str) -> Command {
    let mut command = Command::new(python_path);
    hide_console_window(&mut command);
    let python_path_buf = PathBuf::from(python_path);
    let file_name = python_path_buf
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(python_path);

    if cfg!(windows)
        && (file_name.eq_ignore_ascii_case("py") || file_name.eq_ignore_ascii_case("py.exe"))
    {
        command.arg("-3");
    }

    command.arg("-u");
    command.env("QUANTSUITE_HOME", qs_core::paths::root());
    command.env("QUANTSCRIPT_INDICATORS_DIR", qs_core::paths::indicators_dir());

    // No resolved SDK dir → leave PYTHONPATH alone. `import quantalgo` then
    // falls back to whatever the interpreter's environment provides, and the
    // runner's own import error names the missing package — more diagnosable
    // than prepending a directory known not to exist.
    if let Some(sdk_dir) = get_python_sdk_dir() {
        let mut python_path_env = OsString::from(sdk_dir);
        if let Some(existing) = std::env::var_os("PYTHONPATH") {
            python_path_env.push(if cfg!(windows) { ";" } else { ":" });
            python_path_env.push(existing);
        }
        command.env("PYTHONPATH", python_path_env);
    }
    command
}

// ---------------------------------------------------------------------------
// Settings Commands
// ---------------------------------------------------------------------------

#[tauri::command(async)]
pub(crate) fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    let settings = state.settings.lock().map_err(|e| format!("Lock: {e}"))?;
    Ok(settings.clone())
}

#[tauri::command]
pub(crate) async fn update_settings(
    mut settings: AppSettings,
    state: State<'_, AppState>,
) -> Result<AppSettings, String> {
    normalize_settings_units(&mut settings);
    validate_app_settings(&settings)?;
    save_settings_to_disk(&settings)?;
    let mut current = state.settings.lock().map_err(|e| format!("Lock: {e}"))?;
    *current = settings.clone();
    Ok(settings)
}

#[tauri::command]
pub(crate) async fn detect_python() -> Result<Option<String>, String> {
    let candidates = if cfg!(windows) {
        vec!["python3", "python", "py"]
    } else {
        vec!["python3", "python"]
    };

    for candidate in candidates {
        let output = {
            let mut probe = Command::new(candidate);
            hide_console_window(&mut probe);
            probe.arg("--version").output()
        };
        if let Ok(out) = output {
            if out.status.success() {
                let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let version = if version.is_empty() {
                    String::from_utf8_lossy(&out.stderr).trim().to_string()
                } else {
                    version
                };
                // Verify it's Python 3
                if version.contains("Python 3") {
                    return Ok(Some(candidate.to_string()));
                }
            }
        }
    }
    Ok(None)
}
