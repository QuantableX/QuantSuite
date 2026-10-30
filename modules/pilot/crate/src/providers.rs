//! Finding the CLIs and asking them who they are.
//!
//! Nothing here touches credentials: `claude auth status` and `codex login
//! status` are the CLIs' own reports, and the login itself happens outside
//! the suite (`claude auth login`, `codex login`).
//!
//! Windows detail that matters for a PTY: an npm-installed CLI is a `.cmd`
//! shim, and ConPTY cannot start a `.cmd` directly. The shim is read once
//! and turned into `node <script>` — the same thing the shim does, minus
//! `cmd.exe` and its quoting between us and the program.

use crate::adapters::{Adapter, Kind};
use crate::settings::Settings;
use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// What to hand the PTY: the program, arguments that come before the
/// adapter's own, and the path that was found (for the settings panel).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Launchable {
    pub program: String,
    pub prefix: Vec<String>,
    pub found: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AdapterStatus {
    pub id: String,
    pub label: String,
    pub custom: bool,
    pub signals: Option<crate::adapters::Signals>,
    pub found: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    /// `None` = could not tell (only Claude and Codex are asked).
    pub logged_in: Option<bool>,
    pub detail: Option<String>,
    /// How Update would run: `npm`, `winget`, `native` or `cli` (card 19).
    pub update: Option<String>,
}

/// How an installed CLI updates (card 19): through the installer that put it
/// there — npm, winget, Claude's native installer — else the CLI's own
/// `update`. Never as a side effect of a status check.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePlan {
    pub method: String,
    pub program: String,
    pub args: Vec<String>,
    /// The command as a user would type it.
    pub display: String,
}

fn npm_package(kind: Kind) -> Option<&'static str> {
    Some(match kind {
        Kind::Claude => "@anthropic-ai/claude-code",
        Kind::Codex => "@openai/codex",
        Kind::Gemini => "@google/gemini-cli",
        Kind::Opencode => "opencode-ai",
        Kind::Pi => "@earendil-works/pi-coding-agent",
        _ => return None,
    })
}

pub fn update_plan(kind: Kind, found: &Path) -> Option<UpdatePlan> {
    let path = found.to_string_lossy().replace('/', "\\");
    let lower = path.to_ascii_lowercase();
    let plan = |method: &str, program: &str, args: Vec<String>| UpdatePlan {
        method: method.into(),
        display: std::iter::once(program.rsplit('\\').next().unwrap_or(program).trim_end_matches(".exe").to_string()).chain(args.iter().cloned()).collect::<Vec<_>>().join(" "),
        program: program.into(),
        args,
    };
    if let Some(i) = lower.find("\\winget\\packages\\") {
        // …\WinGet\Packages\Anthropic.ClaudeCode_Microsoft.Winget.Source_8wekyb3d8bbwe\claude.exe
        let rest = &path[i + "\\winget\\packages\\".len()..];
        let id = rest.split(['\\', '_']).next().unwrap_or_default();
        if !id.is_empty() {
            return Some(plan("winget", "winget", vec!["upgrade".into(), "--id".into(), id.into(), "--exact".into(), "--silent".into()]));
        }
    }
    if let Some(pkg) = npm_package(kind) {
        if lower.contains("\\npm\\") || lower.contains("\\node_modules\\") {
            return Some(plan("npm", "npm", vec!["install".into(), "-g".into(), format!("{pkg}@latest")]));
        }
    }
    let exe = path.clone();
    match kind {
        // The native installer puts claude.exe in ~/.local/bin and updates with `claude update`.
        Kind::Claude if lower.ends_with("\\.local\\bin\\claude.exe") => Some(plan("native", &exe, vec!["update".into()])),
        Kind::Claude | Kind::Codex => Some(plan("cli", &exe, vec!["update".into()])),
        _ => None,
    }
}

/// A windowless command for the executable — the release binary has no
/// console for a child to inherit, and a `.cmd` shim needs `cmd /C`.
pub fn base_cmd(exe: &Path) -> Command {
    let is_script = exe
        .extension()
        .map(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"))
        .unwrap_or(false);
    let mut cmd = if is_script {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(exe);
        c
    } else {
        Command::new(exe)
    };
    // A `.cmd` shim finds `node` through PATH, and the suite's own can
    // predate its installation (see `qs_core::system_path`).
    cmd.env("PATH", qs_core::system_path::current());
    #[cfg(windows)]
    let cmd = {
        use std::os::windows::process::CommandExt;
        let mut cmd = cmd;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        cmd
    };
    cmd
}

/// Found through the current PATH, not the suite's own: a CLI installed
/// after the suite started is on the first but not always on the second.
pub fn which(name: &str) -> Option<PathBuf> {
    let path = qs_core::system_path::current();
    let names: Vec<String> = if cfg!(windows) {
        vec![format!("{name}.exe"), format!("{name}.cmd"), name.to_string()]
    } else {
        vec![name.to_string()]
    };
    for dir in std::env::split_paths(&path) {
        for n in &names {
            let candidate = dir.join(n);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn override_path(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let p = PathBuf::from(trimmed);
    if p.is_file() {
        return Some(p);
    }
    // A bare name in the override box means "this one on PATH".
    if !trimmed.contains(['\\', '/']) {
        return which(trimmed);
    }
    None
}

fn npm_global_root() -> Option<PathBuf> {
    if cfg!(windows) {
        dirs::data_dir().map(|d| d.join("npm").join("node_modules"))
    } else {
        None
    }
}

/// The npm global install's native Codex binary (the `.cmd` shim would put
/// `cmd.exe` between the PTY and the TUI).
fn codex_native() -> Option<PathBuf> {
    let pkg = npm_global_root()?.join("@openai").join("codex").join("node_modules");
    for entry in std::fs::read_dir(&pkg).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("codex-win32") {
            continue;
        }
        let vendor = entry.path().join("vendor");
        for triple in std::fs::read_dir(&vendor).ok()?.flatten() {
            let exe = triple.path().join("bin").join("codex.exe");
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    None
}

fn opencode_native() -> Option<PathBuf> {
    if let Some(root) = npm_global_root() {
        let nested = root.join("opencode-ai").join("node_modules");
        if let Ok(entries) = std::fs::read_dir(&nested) {
            for entry in entries.flatten() {
                if !entry.file_name().to_string_lossy().starts_with("opencode-windows") {
                    continue;
                }
                let exe = entry.path().join("bin").join("opencode.exe");
                if exe.is_file() {
                    return Some(exe);
                }
            }
        }
    }
    let local = dirs::data_local_dir()?.join("opencode").join("opencode-cli.exe");
    local.is_file().then_some(local)
}

fn home_bin(parts: &[&str], names: &[&str]) -> Option<PathBuf> {
    let mut dir = dirs::home_dir()?;
    for p in parts {
        dir = dir.join(p);
    }
    names.iter().map(|n| dir.join(n)).find(|p| p.is_file())
}

/// Settings override → the adapter's known install → PATH.
pub fn resolve(adapter: &Adapter, settings: &Settings) -> Option<Launchable> {
    let override_value = settings.exe_for(&adapter.id);
    if let Some(p) = override_path(override_value) {
        return Some(launchable(p));
    }
    let path = match adapter.kind {
        Kind::Claude => home_bin(&[".local", "bin"], &["claude.exe", "claude"]).or_else(|| which("claude")),
        Kind::Codex => codex_native().or_else(|| which("codex")),
        Kind::Pi => which("pi"),
        Kind::Omp => home_bin(&[".bun", "bin"], &["omp.exe", "omp"]).or_else(|| which("omp")),
        Kind::Opencode => opencode_native().or_else(|| which("opencode")),
        Kind::Gemini => which("gemini"),
        Kind::Custom => {
            let exe = adapter.custom.as_ref().map(|c| c.exe.as_str()).unwrap_or("");
            override_path(exe)
        }
    }?;
    Some(launchable(path))
}

/// The shim's last line is `"%_prog%" [flags] "%dp0%\node_modules\…" %*`.
/// Returns the flags and the script's absolute path.
pub fn parse_npm_shim(text: &str, shim_dir: &Path) -> Option<(Vec<String>, PathBuf)> {
    let line = text.lines().rev().find(|l| l.contains("%_prog%"))?;
    let after = line.split("\"%_prog%\"").nth(1)?;
    let (flags_part, rest) = after.split_once('"')?;
    let script_rel = rest.split('"').next()?.strip_prefix("%dp0%")?;
    let script = shim_dir.join(script_rel.trim_start_matches(['\\', '/']));
    let flags = flags_part.split_whitespace().map(String::from).collect();
    Some((flags, script))
}

fn launchable(path: PathBuf) -> Launchable {
    let found = path.to_string_lossy().into_owned();
    let is_script = path
        .extension()
        .map(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"))
        .unwrap_or(false);
    if !is_script {
        return Launchable { program: found.clone(), prefix: Vec::new(), found };
    }
    let shim_dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let parsed = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| parse_npm_shim(&text, &shim_dir))
        .filter(|(_, script)| script.is_file());
    if let Some((flags, script)) = parsed {
        let node = [shim_dir.join("node.exe")]
            .into_iter()
            .find(|p| p.is_file())
            .or_else(|| which("node"));
        if let Some(node) = node {
            let mut prefix = flags;
            prefix.push(script.to_string_lossy().into_owned());
            return Launchable { program: node.to_string_lossy().into_owned(), prefix, found };
        }
    }
    // Last resort: let cmd.exe run the shim. Arguments with quotes may not
    // survive it, which is why the shim is parsed first.
    Launchable { program: "cmd.exe".into(), prefix: vec!["/d".into(), "/c".into(), found.clone()], found }
}

/// One bounded command run, stdout and stderr as one text.
pub fn run_capture(cmd: Command, timeout: Duration) -> Result<(bool, String), String> {
    run_capture_split(cmd, timeout).map(|(ok, out, err)| (ok, format!("{out}{err}")))
}

/// [`run_capture`] with stdout and stderr apart — for an answer that must
/// not pick up a hook's or a plugin's notice.
pub fn run_capture_split(cmd: Command, timeout: Duration) -> Result<(bool, String, String), String> {
    run_with_input(cmd, None, timeout)
}

/// [`run_capture_split`] with `input` written to stdin — for a prompt too
/// long for the Windows command line (32,767 characters).
pub fn run_with_input(mut cmd: Command, input: Option<String>, timeout: Duration) -> Result<(bool, String, String), String> {
    let mut child = cmd
        .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
        std::thread::spawn(move || {
            use std::io::Write;
            let _ = stdin.write_all(text.as_bytes());
        });
    }
    let mut out = child.stdout.take();
    let mut err = child.stderr.take();
    let t_out = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(s) = out.as_mut() {
            let _ = s.read_to_end(&mut buf);
        }
        buf
    });
    let t_err = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(s) = err.as_mut() {
            let _ = s.read_to_end(&mut buf);
        }
        buf
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let stdout = t_out.join().unwrap_or_default();
    let stderr = t_err.join().unwrap_or_default();
    let (out, err) = (String::from_utf8_lossy(&stdout).into_owned(), String::from_utf8_lossy(&stderr).into_owned());
    match status {
        Some(s) => Ok((s.success(), out, err)),
        None => Err(format!("timed out after {}s", timeout.as_secs())),
    }
}

pub fn version_of(exe: &Path) -> Option<String> {
    let mut cmd = base_cmd(exe);
    cmd.arg("--version");
    run_capture(cmd, Duration::from_secs(20))
        .ok()
        .and_then(|(ok, text)| ok.then_some(text))
        .and_then(|t| t.lines().find(|l| !l.trim().is_empty()).map(|l| l.trim().to_string()))
}

/// The settings' environment on a probe, so it reports what new processes will see (card 19).
fn with_env(mut cmd: Command, env: &[(String, String)]) -> Command {
    for (k, v) in env {
        match k.strip_prefix('-') {
            Some(name) => cmd.env_remove(name),
            None => cmd.env(k, v),
        };
    }
    cmd
}

fn claude_login(exe: &Path, env: &[(String, String)], status: &mut AdapterStatus) {
    let mut cmd = with_env(base_cmd(exe), env);
    cmd.args(["auth", "status", "--json"]);
    match run_capture(cmd, Duration::from_secs(20)) {
        Ok((_, text)) => {
            let json_start = text.find('{');
            let parsed = json_start.and_then(|i| serde_json::from_str::<serde_json::Value>(&text[i..]).ok());
            match parsed {
                Some(v) => {
                    let logged = v.get("loggedIn").and_then(|b| b.as_bool());
                    status.logged_in = logged;
                    status.detail = Some(match logged {
                        Some(true) => format!(
                            "logged in ({})",
                            v.get("authMethod").and_then(|m| m.as_str()).unwrap_or("account")
                        ),
                        Some(false) => "not logged in — run `claude auth login` in a terminal".into(),
                        None => "auth status unreadable".into(),
                    });
                }
                None => status.detail = Some(text.lines().next().unwrap_or("").trim().to_string()),
            }
        }
        Err(e) => status.detail = Some(e),
    }
}

fn codex_login(exe: &Path, env: &[(String, String)], status: &mut AdapterStatus) {
    let mut cmd = with_env(base_cmd(exe), env);
    cmd.args(["login", "status"]);
    match run_capture(cmd, Duration::from_secs(20)) {
        Ok((_, text)) => {
            let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
            let lower = line.to_ascii_lowercase();
            status.logged_in = Some(lower.starts_with("logged in") && !lower.contains("not logged"));
            status.detail = Some(if status.logged_in == Some(true) {
                line
            } else {
                format!("{line} — run `codex login` in a terminal")
            });
        }
        Err(e) => status.detail = Some(e),
    }
}

fn install_hint(kind: Kind) -> &'static str {
    match kind {
        Kind::Claude => "not found — install Claude Code or set the path in Settings",
        Kind::Codex => "not found — `npm i -g @openai/codex` or set the path in Settings",
        Kind::Pi => "not found — `npm i -g @earendil-works/pi-coding-agent` or set the path in Settings",
        Kind::Omp => "not found — install omp or set the path in Settings",
        Kind::Opencode => "not found — `npm i -g opencode-ai` or set the path in Settings",
        Kind::Gemini => "not found — `npm i -g @google/gemini-cli` or set the path in Settings",
        Kind::Custom => "not found — check the executable in Settings",
    }
}

fn status_one(adapter: &Adapter, settings: &Settings) -> AdapterStatus {
    let mut status = AdapterStatus {
        id: adapter.id.clone(),
        label: adapter.label.clone(),
        custom: adapter.kind == Kind::Custom,
        signals: Some(adapter.signals()),
        ..Default::default()
    };
    let Some(exe) = resolve(adapter, settings) else {
        status.detail = Some(install_hint(adapter.kind).into());
        return status;
    };
    let found = PathBuf::from(&exe.found);
    status.found = true;
    status.path = Some(exe.found.clone());
    status.version = version_of(&found);
    let env = settings.launch_env(&adapter.id);
    status.update = update_plan(adapter.kind, &found).map(|p| p.method);
    match adapter.kind {
        Kind::Claude => claude_login(&found, &env, &mut status),
        Kind::Codex => codex_login(&found, &env, &mut status),
        _ => status.detail = Some(if status.version.is_some() { "found".into() } else { "found, version unreadable".into() }),
    }
    status
}

/// Every adapter's status, probed in parallel — six `--version` runs in a
/// row would keep the settings panel waiting for the slowest node startup
/// times six.
pub fn status_all(adapters: &[Adapter], settings: &Settings) -> Vec<AdapterStatus> {
    std::thread::scope(|scope| {
        let handles: Vec<_> = adapters
            .iter()
            .map(|a| scope.spawn(move || status_one(a, settings)))
            .collect();
        handles
            .into_iter()
            .zip(adapters)
            .map(|(h, a)| {
                h.join().unwrap_or_else(|_| AdapterStatus {
                    id: a.id.clone(),
                    label: a.label.clone(),
                    detail: Some("status probe panicked".into()),
                    ..Default::default()
                })
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Card 19: Update goes through the installer that put the CLI there.
    #[test]
    fn update_follows_the_installer() {
        let p = |s: &str| PathBuf::from(s);
        let npm = update_plan(Kind::Codex, &p("C:\\Users\\me\\AppData\\Roaming\\npm\\codex.cmd")).unwrap();
        assert_eq!((npm.method.as_str(), npm.display.as_str()), ("npm", "npm install -g @openai/codex@latest"));
        let wg = update_plan(Kind::Claude, &p("C:\\Users\\me\\AppData\\Local\\Microsoft\\WinGet\\Packages\\Anthropic.ClaudeCode_Microsoft.Winget.Source_8wekyb3d8bbwe\\claude.exe")).unwrap();
        assert_eq!(wg.display, "winget upgrade --id Anthropic.ClaudeCode --exact --silent");
        let native = update_plan(Kind::Claude, &p("C:\\Users\\me\\.local\\bin\\claude.exe")).unwrap();
        assert_eq!((native.method.as_str(), native.display.as_str()), ("native", "claude update"));
        let other = update_plan(Kind::Codex, &p("D:\\tools\\codex.exe")).unwrap();
        assert_eq!((other.method.as_str(), other.args.as_slice()), ("cli", &["update".to_string()][..]));
        assert!(update_plan(Kind::Omp, &p("D:\\tools\\omp.exe")).is_none(), "no known way: no button");
    }

    #[test]
    fn parses_the_npm_shim() {
        let shim = "@ECHO off\r\n...\r\nendLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & \"%_prog%\" --no-warnings=DEP0040 \"%dp0%\\node_modules\\@google\\gemini-cli\\dist\\index.js\" %*\r\n";
        let (flags, script) = parse_npm_shim(shim, Path::new("C:\\npm")).unwrap();
        assert_eq!(flags, vec!["--no-warnings=DEP0040"]);
        assert_eq!(script, PathBuf::from("C:\\npm").join("node_modules\\@google\\gemini-cli\\dist\\index.js"));

        let plain = "\"%_prog%\"  \"%dp0%\\node_modules\\@earendil-works\\pi-coding-agent\\dist\\bundle\\cli.js\" %*";
        let (flags, script) = parse_npm_shim(plain, Path::new("D:\\x")).unwrap();
        assert!(flags.is_empty());
        assert!(script.ends_with("cli.js"));
    }
}

#[cfg(test)]
mod live {
    use super::*;

    /// Live, opt-in (card 19): how this machine's Claude Code and Codex would update — nothing runs.
    #[test]
    #[ignore = "live: resolves the installed CLIs and reads their versions"]
    fn live_update_dry_run() {
        let settings = Settings::default();
        for id in ["claude", "codex"] {
            let adapter = crate::adapters::find(&settings, id).unwrap();
            let exe = resolve(&adapter, &settings).expect("installed");
            let plan = update_plan(adapter.kind, Path::new(&exe.found)).expect("a plan");
            println!("{id}: {} → {} via {} ({:?})", exe.found, plan.display, plan.method, version_of(Path::new(&exe.found)));
        }
    }
}
