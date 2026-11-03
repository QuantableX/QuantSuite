//! Run and preview for agents (parity 07): the project's own actions
//! (Settings → Actions, the drawer's Run terminals — no second registry),
//! started in the calling thread's checkout, watched through their terminal's
//! output and their process tree's listening ports, and connected to the
//! Browser tab (parity 06).
//!
//! Nothing runs implicitly: listing actions never starts one, and nothing
//! is guessed (no installs, no package scripts). A running action is reused,
//! not started twice. Stop touches only the action's own process tree;
//! ports held by anything else — the source checkout's server — are reported,
//! never stopped.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::process::Command;
#[cfg(windows)]
use std::process::Stdio;
use std::time::Duration;

/// Output lines an agent gets back.
pub const TAIL_LINES: usize = 40;

#[derive(Debug, Clone, PartialEq)]
pub struct Proc {
    pub pid: u32,
    pub parent: u32,
    pub name: String,
}

#[cfg(windows)]
fn hidden(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000);
}

/// Every process with its parent.
pub fn processes() -> Vec<Proc> {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("powershell");
        cmd.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-CimInstance Win32_Process | ForEach-Object { \"$($_.ProcessId)`t$($_.ParentProcessId)`t$($_.Name)\" }",
        ])
        .stdin(Stdio::null());
        hidden(&mut cmd);
        let out = cmd.output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
        parse_processes(&out)
    }
    #[cfg(not(windows))]
    {
        let out = Command::new("ps").args(["-eo", "pid=,ppid=,comm="]).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
        out.lines()
            .filter_map(|l| {
                let mut p = l.split_whitespace();
                Some(Proc { pid: p.next()?.parse().ok()?, parent: p.next()?.parse().ok()?, name: p.collect::<Vec<_>>().join(" ") })
            })
            .collect()
    }
}

#[cfg(any(windows, test))]
pub fn parse_processes(out: &str) -> Vec<Proc> {
    out.lines()
        .filter_map(|l| {
            let mut p = l.trim().splitn(3, '\t');
            Some(Proc { pid: p.next()?.trim().parse().ok()?, parent: p.next()?.trim().parse().ok()?, name: p.next().unwrap_or("").trim().to_string() })
        })
        .collect()
}

/// The processes under `root` (not `root` itself).
pub fn descendants(root: u32, all: &[Proc]) -> Vec<Proc> {
    let mut children: HashMap<u32, Vec<&Proc>> = HashMap::new();
    for p in all {
        if p.pid != p.parent {
            children.entry(p.parent).or_default().push(p);
        }
    }
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut stack = vec![root];
    while let Some(pid) = stack.pop() {
        for c in children.get(&pid).into_iter().flatten() {
            if seen.insert(c.pid) && out.len() < 512 {
                out.push((*c).clone());
                stack.push(c.pid);
            }
        }
    }
    out
}

/// Listening TCP ports with their process: `(port, pid)`.
pub fn listening() -> Vec<(u16, u32)> {
    #[cfg(windows)]
    let out = {
        let mut cmd = Command::new("netstat");
        cmd.args(["-ano", "-p", "TCP"]).stdin(Stdio::null());
        hidden(&mut cmd);
        cmd.output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
    };
    #[cfg(not(windows))]
    let out = String::new();
    parse_listening(&out)
}

pub fn parse_listening(netstat: &str) -> Vec<(u16, u32)> {
    let mut out = Vec::new();
    for (port, pid) in netstat.lines().filter_map(crate::files::netstat_listener) {
        if !out.contains(&(port, pid)) {
            out.push((port, pid));
        }
    }
    out.sort();
    out
}

/// Terminal output as a screen would show it, newest lines last: colors and
/// titles dropped, carriage returns, backspaces, line erases and cursor moves
/// applied — an interactive shell redraws its input line (PSReadLine) with
/// them, which read as garbage when only stripped.
pub fn plain_tail(raw: &str, lines: usize) -> String {
    let mut screen: Vec<Vec<char>> = vec![Vec::new()];
    let (mut row, mut col) = (0usize, 0usize);
    // Absolute cursor rows count from the top of the visible screen.
    const ROWS: usize = 30;
    let mut chars = raw.chars().peekable();
    let put = |screen: &mut Vec<Vec<char>>, row: usize, col: usize, c: char| {
        while screen.len() <= row {
            screen.push(Vec::new());
        }
        let line = &mut screen[row];
        while line.len() < col {
            line.push(' ');
        }
        if col < line.len() {
            line[col] = c;
        } else {
            line.push(c);
        }
    };
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => match chars.peek().copied() {
                Some('[') => {
                    chars.next();
                    let mut params = String::new();
                    let mut fin = ' ';
                    for d in chars.by_ref() {
                        if ('@'..='~').contains(&d) {
                            fin = d;
                            break;
                        }
                        params.push(d);
                    }
                    let nums: Vec<usize> = params.trim_start_matches('?').split(';').map(|n| n.parse().unwrap_or(0)).collect();
                    let n = nums.first().copied().unwrap_or(0).max(1);
                    match fin {
                        'K' => {
                            if let Some(line) = screen.get_mut(row) {
                                match nums.first().copied().unwrap_or(0) {
                                    0 => line.truncate(col),
                                    1 => line.iter_mut().take(col + 1).for_each(|x| *x = ' '),
                                    _ => line.clear(),
                                }
                            }
                        }
                        'G' => col = n - 1,
                        'C' => col += n,
                        'D' => col = col.saturating_sub(n),
                        'A' => row = row.saturating_sub(n),
                        'B' => row += n,
                        'H' | 'f' => {
                            let r = nums.first().copied().unwrap_or(1).max(1);
                            let ccol = nums.get(1).copied().unwrap_or(1).max(1);
                            let top = screen.len().saturating_sub(ROWS);
                            row = top + r - 1;
                            col = ccol - 1;
                        }
                        'J' if nums.first().copied().unwrap_or(0) == 2 => {
                            screen.clear();
                            screen.push(Vec::new());
                            row = 0;
                            col = 0;
                        }
                        _ => {}
                    }
                }
                Some(']') => {
                    chars.next();
                    while let Some(d) = chars.next() {
                        if d == '\u{7}' {
                            break;
                        }
                        if d == '\u{1b}' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => {
                    chars.next();
                }
            },
            '\r' => col = 0,
            '\n' => {
                row += 1;
                col = 0;
                while screen.len() <= row {
                    screen.push(Vec::new());
                }
            }
            '\u{8}' => col = col.saturating_sub(1),
            '\t' => {
                for _ in 0..(8 - col % 8) {
                    put(&mut screen, row, col, ' ');
                    col += 1;
                }
            }
            c if c.is_control() => {}
            c => {
                put(&mut screen, row, col, c);
                col += 1;
            }
        }
    }
    let text: Vec<String> = screen.iter().map(|l| l.iter().collect::<String>().trim_end().to_string()).filter(|l| !l.trim().is_empty()).collect();
    text[text.len().saturating_sub(lines)..].join("\n")
}

/// Local addresses an output names (`http://localhost:5173/`), newest last.
pub fn local_urls(text: &str) -> Vec<(String, u16)> {
    let re = regex::Regex::new(r"https?://(?:localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1\]):(\d{2,5})(/[^\s'\x22<>]*)?").expect("a valid pattern");
    let mut out: Vec<(String, u16)> = Vec::new();
    for c in re.captures_iter(text) {
        let Some(port) = c.get(1).and_then(|p| p.as_str().parse::<u16>().ok()) else { continue };
        let url = c.get(0).map(|m| m.as_str()).unwrap_or_default().replace("0.0.0.0", "localhost").replace("[::1]", "localhost");
        let url = url.trim_end_matches(['.', ',', ')', ';']).to_string();
        out.retain(|(_, p)| *p != port);
        out.push((url, port));
    }
    out
}

/// One HTTP request to a local address: the status code, or why not.
pub fn probe(url: &str, timeout: Duration) -> Result<u16, String> {
    let parsed = tauri::Url::parse(url).map_err(|_| "not an address".to_string())?;
    let host = parsed.host_str().ok_or("no host")?.to_string();
    if !matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]" | "::1") {
        return Err("only local addresses are checked".into());
    }
    let port = parsed.port_or_known_default().ok_or("no port")?;
    let addrs: Vec<_> = (host.as_str(), port).to_socket_addrs().map_err(|e| e.to_string())?.collect();
    let mut last = "nothing listens there".to_string();
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(mut s) => {
                let _ = s.set_read_timeout(Some(timeout));
                let _ = s.set_write_timeout(Some(timeout));
                let path = match parsed.query() {
                    Some(q) => format!("{}?{q}", parsed.path()),
                    None => parsed.path().to_string(),
                };
                let req = format!("GET {path} HTTP/1.0\r\nHost: {host}:{port}\r\nUser-Agent: QuantPilot\r\nConnection: close\r\n\r\n");
                s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
                let mut head = [0u8; 64];
                let n = s.read(&mut head).map_err(|e| format!("no answer ({e})"))?;
                let line = String::from_utf8_lossy(&head[..n]);
                return line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|c| c.parse::<u16>().ok())
                    .ok_or_else(|| "the answer is not HTTP".to_string());
            }
            Err(e) => last = e.to_string(),
        }
    }
    Err(last)
}

/// What an action's terminal is doing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Observed {
    /// Processes under the terminal's shell (the action's command and its children).
    pub running: Vec<Proc>,
    /// Ports the action's processes listen on.
    pub ports: Vec<u16>,
    /// Local addresses in the output, with who listens there: `Some(true)` the
    /// action, `Some(false)` something else, `None` nobody.
    pub urls: Vec<(String, u16, Option<bool>)>,
}

pub fn observe(shell_pid: Option<u32>, output: &str) -> Observed {
    let all = processes();
    let running = shell_pid.map(|p| descendants(p, &all)).unwrap_or_default();
    let mine: HashSet<u32> = running.iter().map(|p| p.pid).chain(shell_pid).collect();
    let listen = listening();
    let ports: Vec<u16> = listen.iter().filter(|(_, pid)| mine.contains(pid)).map(|(p, _)| *p).collect::<Vec<_>>();
    let urls = local_urls(output)
        .into_iter()
        .map(|(u, port)| {
            let owner = listen.iter().find(|(p, _)| *p == port).map(|(_, pid)| mine.contains(pid));
            (u, port, owner)
        })
        .collect();
    Observed { running, ports, urls }
}

/// Stop the action's processes (its shell stays for the next run).
pub fn kill_tree(procs: &[Proc]) {
    for p in procs {
        #[cfg(windows)]
        {
            let mut cmd = Command::new("taskkill");
            cmd.args(["/PID", &p.pid.to_string(), "/T", "/F"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
            hidden(&mut cmd);
            let _ = cmd.status();
        }
        #[cfg(not(windows))]
        {
            let _ = Command::new("kill").arg(p.pid.to_string()).status();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_trees_follow_parents() {
        let all = parse_processes("1\t0\tshell.exe\n2\t1\tnode.exe\n3\t2\tesbuild.exe\n4\t0\tother.exe\n5\t4\tchild.exe\n");
        let d: Vec<u32> = descendants(1, &all).iter().map(|p| p.pid).collect();
        assert_eq!(d.len(), 2);
        assert!(d.contains(&2) && d.contains(&3) && !d.contains(&5));
        assert!(descendants(3, &all).is_empty());
    }

    #[test]
    fn netstat_listeners_are_parsed() {
        let n = "  Proto  Local Address          Foreign Address        State           PID\n  TCP    0.0.0.0:135            0.0.0.0:0              LISTENING       1188\n  TCP    127.0.0.1:5173         0.0.0.0:0              LISTENING       4242\n  TCP    [::1]:5173             [::]:0                 LISTENING       4242\n  TCP    127.0.0.1:50000        127.0.0.1:5173         ESTABLISHED     99\n";
        assert_eq!(parse_listening(n), vec![(135, 1188), (5173, 4242)]);
        // German Windows (2026-11-02): the state column says ABHÖREN.
        let de = "  Proto  Lokale Adresse         Remoteadresse          Status           PID\n  TCP    127.0.0.1:8790         0.0.0.0:0              ABHÖREN         7000\n  TCP    127.0.0.1:50001        127.0.0.1:8790         HERGESTELLT     7001\n";
        assert_eq!(parse_listening(de), vec![(8790, 7000)]);
    }

    #[test]
    fn terminal_output_loses_its_escape_codes() {
        let raw = "\u{1b}[32m  VITE v5\u{1b}[0m ready\r\n\u{1b}]0;title\u{7}  ➜  Local:   \u{1b}[36mhttp://localhost:\u{1b}[1m5173\u{1b}[22m/\u{1b}[39m\r\nprogress 10%\rprogress 100%\r\n";
        let t = plain_tail(raw, 10);
        assert_eq!(t, "  VITE v5 ready\n  ➜  Local:   http://localhost:5173/\nprogress 100%");
        // An input line redrawn as it is typed (PSReadLine): the screen shows it once.
        let typed = "PS C:\\p> \u{1b}[?25lp\u{1b}[?25h\u{1b}[1;10Hpy\u{1b}[1;10Hpython x.py\u{1b}[K\r\nstarting...\r\n";
        assert_eq!(plain_tail(typed, 10), "PS C:\\p> python x.py\nstarting...");
        assert_eq!(plain_tail("abc\u{8}\u{8}X\n", 5), "aXc");
        assert_eq!(plain_tail("a\nb\nc\n", 2), "b\nc");
    }

    #[test]
    fn local_addresses_come_from_the_output() {
        let urls = local_urls("Local: http://localhost:5173/\nNetwork: http://192.168.1.2:5173/\nalso http://0.0.0.0:8080/app). and http://127.0.0.1:5173/x");
        assert_eq!(urls, vec![("http://localhost:8080/app".to_string(), 8080), ("http://127.0.0.1:5173/x".to_string(), 5173)]);
    }

    #[test]
    fn a_probe_reads_the_status_and_refuses_remote_hosts() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                let mut buf = [0u8; 512];
                let _ = s.read(&mut buf);
                let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
            }
        });
        assert_eq!(probe(&format!("http://127.0.0.1:{port}/"), Duration::from_secs(2)), Ok(200));
        assert!(probe("http://example.com/", Duration::from_secs(1)).is_err());
    }
}
