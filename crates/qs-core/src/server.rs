//! Server mode (PLAN-SERVER-CLIENT §5, card Server 2/7).
//!
//! An installation has one **role**:
//!
//! - `local` — today's QuantSuite: the window, the data and the executors on
//!   this PC.
//! - `server` — the machine at home that runs day and night: it starts in the
//!   tray without the window (the window opens on demand for local admin),
//!   and it is the one that trades.
//! - `client` — a PC connected to a server (card 5). The data lives on the
//!   server, so **this installation never runs executors**: no bots, no
//!   nightly Manual runs, no resumes. That is the one-trader rule, enforced
//!   by [`executors_allowed`] in the modules that own executors, not by hiding
//!   buttons.
//!
//! The role is a core setting (`server.role`); `--server` on the command line
//! makes a launch a server for that run whatever the setting says.
//!
//! **Watchdog**: a Task Scheduler entry relaunches the suite every few
//! minutes with [`WATCHDOG_FLAG`]. While it runs, the single-instance guard
//! swallows that relaunch (the window stays as it is); after a crash or a
//! kill, the relaunch starts it again — in the tray, as a server.

use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;
use std::time::{Instant, SystemTime};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db;

/// Starts this launch as a server, whatever the setting says.
pub const SERVER_FLAG: &str = "--server";
/// Marks the watchdog's relaunch: a running suite ignores it.
pub const WATCHDOG_FLAG: &str = "--watchdog";
/// The Task Scheduler entry's name.
pub const WATCHDOG_TASK: &str = "QuantSuite Server Watchdog";
/// How often the watchdog checks, in minutes.
pub const WATCHDOG_EVERY_MIN: u32 = 5;

const SCOPE: &str = "core";
const KEY: &str = "server.role";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Local,
    Server,
    Client,
}

impl Role {
    fn code(self) -> u8 {
        match self {
            Role::Local => 0,
            Role::Server => 1,
            Role::Client => 2,
        }
    }

    fn from_code(c: u8) -> Role {
        match c {
            1 => Role::Server,
            2 => Role::Client,
            _ => Role::Local,
        }
    }

    pub fn parse(s: &str) -> Option<Role> {
        match s.trim().to_lowercase().as_str() {
            "local" => Some(Role::Local),
            "server" => Some(Role::Server),
            "client" => Some(Role::Client),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Local => "local",
            Role::Server => "server",
            Role::Client => "client",
        }
    }
}

/// The role of this process, read once at setup and changed with
/// [`set_role`]. Starts as `local` until setup has run.
static ROLE: AtomicU8 = AtomicU8::new(0);

fn started() -> &'static (Instant, SystemTime) {
    static STARTED: OnceLock<(Instant, SystemTime)> = OnceLock::new();
    STARTED.get_or_init(|| (Instant::now(), SystemTime::now()))
}

pub fn role() -> Role {
    Role::from_code(ROLE.load(Ordering::SeqCst))
}

fn launched_as_server(args: &[String]) -> bool {
    args.iter().any(|a| a == SERVER_FLAG)
}

/// The stored role, with `--server` winning for this launch.
pub fn resolve(stored: Option<Role>, args: &[String]) -> Role {
    if launched_as_server(args) {
        return Role::Server;
    }
    stored.unwrap_or(Role::Local)
}

pub fn stored_role(conn: &Connection) -> Option<Role> {
    db::get_setting(conn, SCOPE, KEY).ok().flatten().and_then(|v| v.as_str().and_then(Role::parse))
}

/// Read the role at setup (before the modules' setups ask for it).
pub fn init(conn: &Connection) {
    let _ = started();
    let args: Vec<String> = std::env::args().collect();
    let r = resolve(stored_role(conn), &args);
    ROLE.store(r.code(), Ordering::SeqCst);
    log::info!("role: {}", r.as_str());
}

/// Store and apply a role. `client` comes with a server connection (card 5).
pub fn set_role(conn: &Connection, r: Role) -> Result<(), String> {
    db::set_setting(conn, SCOPE, KEY, &serde_json::json!(r.as_str())).map_err(|e| e.to_string())?;
    ROLE.store(r.code(), Ordering::SeqCst);
    Ok(())
}

#[cfg(test)]
pub(crate) fn set_role_for_test(r: Role) {
    ROLE.store(r.code(), Ordering::SeqCst);
}

/// The one-trader rule: executors (bots, nightly Manual runs, resumes) run
/// only where the data lives. A client refuses them.
pub fn executors_allowed() -> Result<(), String> {
    check_executors(role())
}

pub fn check_executors(r: Role) -> Result<(), String> {
    if r == Role::Client {
        return Err("This QuantSuite is connected to a server: bots and nightly runs run only on the server (one trader). \
                    Start them there, or switch this PC back to local."
            .into());
    }
    Ok(())
}

/// A server starts in the tray: the window opens on demand.
pub fn starts_hidden() -> bool {
    role() == Role::Server
}

/// A relaunch by the watchdog: the running suite ignores it.
pub fn is_watchdog_launch(args: &[String]) -> bool {
    args.iter().any(|a| a == WATCHDOG_FLAG)
}

#[derive(Debug, Clone, Serialize)]
pub struct Health {
    pub role: Role,
    pub version: String,
    pub started_at: String,
    pub uptime_secs: u64,
    pub data_root: String,
    pub disk_free_bytes: Option<u64>,
    pub disk_total_bytes: Option<u64>,
    /// The gateway that serves clients (card 4); `None` until it exists.
    pub gateway: Option<String>,
    pub watchdog: Option<bool>,
    pub autostart: Option<bool>,
}

pub fn health(version: &str, data_root: &Path, watchdog: Option<bool>, autostart: Option<bool>) -> Health {
    let (t0, wall) = started();
    Health {
        role: role(),
        version: version.to_string(),
        started_at: chrono::DateTime::<chrono::Utc>::from(*wall).to_rfc3339(),
        uptime_secs: t0.elapsed().as_secs(),
        data_root: data_root.display().to_string(),
        disk_free_bytes: fs4::available_space(data_root).ok(),
        disk_total_bytes: fs4::total_space(data_root).ok(),
        gateway: None,
        watchdog,
        autostart,
    }
}

// ── the watchdog ────────────────────────────────────────────────────────────

/// `schtasks` arguments that create the watchdog for `exe`: every
/// [`WATCHDOG_EVERY_MIN`] minutes while the user is logged on (the suite is a
/// tray app; `/IT` keeps it in the user's session).
pub fn watchdog_create_args(exe: &Path) -> Vec<String> {
    vec![
        "/Create".into(),
        "/TN".into(),
        WATCHDOG_TASK.into(),
        "/SC".into(),
        "MINUTE".into(),
        "/MO".into(),
        WATCHDOG_EVERY_MIN.to_string(),
        "/TR".into(),
        format!("\"{}\" {SERVER_FLAG} {WATCHDOG_FLAG}", exe.display()),
        "/IT".into(),
        "/F".into(),
    ]
}

pub fn watchdog_delete_args() -> Vec<String> {
    vec!["/Delete".into(), "/TN".into(), WATCHDOG_TASK.into(), "/F".into()]
}

pub fn watchdog_query_args() -> Vec<String> {
    vec!["/Query".into(), "/TN".into(), WATCHDOG_TASK.into()]
}

#[cfg(windows)]
fn schtasks(args: &[String]) -> Result<std::process::Output, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("schtasks")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("schtasks: {e}"))
}

/// Whether the watchdog task exists. `None` off Windows.
pub fn watchdog_enabled() -> Option<bool> {
    #[cfg(windows)]
    {
        schtasks(&watchdog_query_args()).ok().map(|o| o.status.success())
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Create or remove the watchdog task.
pub fn set_watchdog(enabled: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        let args = if enabled {
            let exe = std::env::current_exe().map_err(|e| format!("this program's path: {e}"))?;
            watchdog_create_args(&exe)
        } else {
            if watchdog_enabled() != Some(true) {
                return Ok(());
            }
            watchdog_delete_args()
        };
        let out = schtasks(&args)?;
        if !out.status.success() {
            return Err(format!("schtasks: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Err("The watchdog uses the Windows Task Scheduler; on this OS use the system's service manager.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The role is process-wide: the tests that change it take turns.
    static ROLE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_flag_wins_over_the_setting() {
        assert_eq!(resolve(None, &args(&["qs.exe"])), Role::Local);
        assert_eq!(resolve(Some(Role::Client), &args(&["qs.exe"])), Role::Client);
        assert_eq!(resolve(Some(Role::Local), &args(&["qs.exe", "--server"])), Role::Server);
        assert_eq!(resolve(Some(Role::Client), &args(&["qs.exe", "--server", "--watchdog"])), Role::Server);
        assert!(is_watchdog_launch(&args(&["qs.exe", "--server", "--watchdog"])));
        assert!(!is_watchdog_launch(&args(&["qs.exe", "--autostart"])));
    }

    #[test]
    fn a_client_never_runs_executors() {
        let _turn = ROLE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        assert!(check_executors(Role::Local).is_ok());
        assert!(check_executors(Role::Server).is_ok());
        let e = check_executors(Role::Client).unwrap_err();
        assert!(e.contains("one trader"), "{e}");
        set_role_for_test(Role::Client);
        assert!(executors_allowed().is_err());
        assert!(!starts_hidden());
        set_role_for_test(Role::Server);
        assert!(executors_allowed().is_ok());
        assert!(starts_hidden());
        set_role_for_test(Role::Local);
    }

    #[test]
    fn the_role_is_stored_in_core_settings() {
        let _turn = ROLE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let conn = Connection::open_in_memory().unwrap();
        db::migrate(&conn).unwrap();
        assert_eq!(stored_role(&conn), None);
        set_role(&conn, Role::Server).unwrap();
        assert_eq!(stored_role(&conn), Some(Role::Server));
        assert_eq!(role(), Role::Server);
        set_role(&conn, Role::Local).unwrap();
        assert_eq!(stored_role(&conn), Some(Role::Local));
        assert_eq!(Role::parse("CLIENT"), Some(Role::Client));
        assert_eq!(Role::parse("nope"), None);
    }

    #[test]
    fn the_watchdog_relaunches_as_a_server_in_the_users_session() {
        let a = watchdog_create_args(Path::new(r"C:\Program Files\QuantSuite\QuantSuite.exe"));
        let tr = a.iter().position(|x| x == "/TR").map(|i| a[i + 1].clone()).unwrap();
        assert_eq!(tr, r#""C:\Program Files\QuantSuite\QuantSuite.exe" --server --watchdog"#);
        assert!(a.contains(&"/IT".to_string()), "interactive: a tray app needs the user's session");
        assert!(a.contains(&"MINUTE".to_string()) && a.contains(&"5".to_string()));
        assert_eq!(watchdog_delete_args()[2], WATCHDOG_TASK);
    }

    #[test]
    fn health_reports_disk_and_uptime() {
        let dir = std::env::temp_dir();
        let h = health("1.2.3", &dir, Some(false), None);
        assert_eq!(h.version, "1.2.3");
        assert!(h.disk_total_bytes.unwrap_or(0) > 0);
        assert!(h.disk_free_bytes.unwrap_or(0) <= h.disk_total_bytes.unwrap_or(0));
        assert!(h.gateway.is_none());
    }
}
