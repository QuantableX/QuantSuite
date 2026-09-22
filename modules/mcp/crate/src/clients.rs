//! The AI-client table (PLAN-QUANTMCP-CONNECT §4): every MCP client QuantMCP
//! can find on this machine and write itself into, as data. Detection,
//! install, the Connect report, the standard snippet and the
//! `clientInfo.name` → client match are all derived from one row here —
//! a new client is one new row, nothing else.
//!
//! MCP activation and AgentOS imports require explicit selections from the
//! same detection table. A client the table
//! does not know takes `snippet()`, which is plain MCP (`mcpServers` →
//! `{ "type": "http", "url" }`) and works wherever Streamable HTTP does.

use crate::mcp::{McpEntry, McpTransport};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

mod instructions;
use instructions::Instructions;
pub use instructions::import_agent_instructions;
pub use instructions::custom::{load_custom_recipients, validate_custom_recipients, CustomRecipient};

/// The name QuantMCP registers itself under in every client.
pub const QUANTMCP_NAME: &str = "QuantMCP";

// ── The table ─────────────────────────────────────────────────────────────

pub struct ClientSpec {
    pub id: &'static str,
    pub name: &'static str,
    /// `clientInfo.name` values this client reports on `initialize`, already
    /// normalised (lowercase, no `(via mcp-remote …)`, no `/version` suffix).
    /// A trailing `*` matches by prefix, for names that carry a variable
    /// tail (`local-agent-mode-<server>`).
    pub aliases: &'static [&'static str],
    /// The client counts as installed when any one of these hits.
    pub detect: &'static [Detect],
    pub install: Install,
    pub instructions: Instructions,
    /// The client reads its config only at start — the report says so.
    pub restart: bool,
}

pub enum Detect {
    File(ConfigPath),
    Dir(ConfigPath),
    Binary(&'static str),
    Extension(&'static str),
}

/// A config location, resolved per platform when it is needed.
pub enum ConfigPath {
    /// Under the home directory: `~/.cursor/mcp.json`.
    Home(&'static [&'static str]),
    /// Under the per-user application config directory — `%APPDATA%` on
    /// Windows, `~/Library/Application Support` on macOS, `$XDG_CONFIG_HOME`
    /// or `~/.config` on Linux.
    AppConfig(&'static [&'static str]),
    /// Under `~/.config` on every platform (tools that ignore the OS rule).
    DotConfig(&'static [&'static str]),
    /// The user's Documents directory (including relocated Windows folders).
    Documents(&'static [&'static str]),
    /// A VS Code extension's global storage:
    /// `<AppConfig>/Code/User/globalStorage/<extension id>/…`.
    VsCodeExtension(&'static str, &'static [&'static str]),
    OmpAgent(&'static [&'static str]),
}

pub enum Install {
    /// A JSON config file with a map of servers under `servers_key`.
    JsonFile {
        path: ConfigPath,
        servers_key: &'static str,
        entry: Entry,
    },
    /// The client's own CLI adds and removes servers. Templates expand
    /// `{name}`, `{url}`, `{command}`, `{args}` (one item per argument) and
    /// `{env:FLAG}` (`FLAG K=V` per variable). `add_stdio` is `None` for a
    /// CLI that only takes URLs. `config` names the files the CLI writes,
    /// so `configured()` can look there.
    Cli {
        binary: &'static str,
        add_http: &'static [&'static str],
        add_stdio: Option<&'static [&'static str]>,
        remove: &'static [&'static [&'static str]],
        config: &'static [ConfigPath],
    },
    /// A YAML file with a plain list of server URLs under `key` (aider).
    YamlList { path: ConfigPath, key: &'static str },
    /// No local config to write — the report carries `note`, the user takes
    /// the snippet.
    Manual { note: &'static str },
}

/// The shape of one server entry inside a JSON config.
pub enum Entry {
    /// `{ "type": "http", "url" }` — VS Code, Claude Code's `.mcp.json`.
    Http,
    /// `{ "type": "streamable-http", "url" }` — Cursor and most others.
    StreamableHttp,
    /// `{ "url", "transportType": "streamableHttp", "disabled": false }` —
    /// Cline and its forks.
    Cline,
    /// `{ "serverUrl" }` — Windsurf.
    ServerUrl,
    /// `{ "url" }` — Continue, Amp.
    UrlOnly,
    /// `{ "type": "remote", "url" }` / `{ "type": "local", "command": [...] }`
    /// — OpenCode.
    OpenCode,
    /// A stdio bridge through `npx -y mcp-remote <url>` — Claude Desktop,
    /// whose config takes no HTTP URL.
    McpRemote,
}

use ConfigPath::{AppConfig, Documents, DotConfig, Home, VsCodeExtension};

pub static CLIENTS: &[ClientSpec] = &[
    ClientSpec {
        id: "pi",
        name: "Pi",
        aliases: &["pi", "pi-coding-agent"],
        detect: &[Detect::Binary("pi"), Detect::Dir(Home(&[".pi", "agent"]))],
        instructions: Instructions::Pi,
        install: Install::Manual { note: "Pi loads AgentOS without MCP. To connect MCP servers, install and configure a Pi MCP extension; Pi has no built-in MCP client." },
        restart: false,
    },
    ClientSpec {
        id: "omp",
        name: "Oh My Pi (OMP)",
        aliases: &["omp", "oh-my-pi"],
        detect: &[Detect::Binary("omp"), Detect::Dir(Home(&[".omp", "agent"]))],
        instructions: Instructions::Omp,
        install: Install::JsonFile {
            path: ConfigPath::OmpAgent(&["mcp.json"]),
            servers_key: "mcpServers",
            entry: Entry::Http,
        },
        restart: true,
    },
    ClientSpec {
        id: "claude-code",
        instructions: Instructions::Claude,
        name: "Claude Code",
        aliases: &["claude-code"],
        detect: &[
            Detect::Binary("claude"),
            Detect::File(Home(&[".claude.json"])),
        ],
        install: Install::Cli {
            binary: "claude",
            add_http: &["mcp", "add", "--transport", "http", "--scope", "user", "{name}", "{url}"],
            add_stdio: Some(&[
                "mcp", "add", "--transport", "stdio", "--scope", "user", "{env:--env}", "{name}",
                "--", "{command}", "{args}",
            ]),
            remove: &[
                &["mcp", "remove", "--scope", "user", "{name}"],
                &["mcp", "remove", "{name}"],
            ],
            config: &[
                Home(&[".claude.json"]),
                Home(&[".claude", "settings.json"]),
                Home(&[".claude", "settings.local.json"]),
            ],
        },
        restart: false,
    },
    ClientSpec {
        id: "claude-desktop",
        instructions: Instructions::Manual("Add the General AgentOS text to your Claude project instructions; desktop chats do not load a global local instruction file."),
        name: "Claude Desktop",
        // Seen on this machine (2026-09-07): `claude-ai (via mcp-remote 0.8.3)`
        // and `local-agent-mode-QuantMCP (via mcp-remote 0.8.3)` — the
        // latter is Claude Desktop's local agent mode, named after the server.
        aliases: &["claude-ai", "claude-desktop", "local-agent-mode-*"],
        detect: &[Detect::Dir(AppConfig(&["Claude"]))],
        install: Install::JsonFile {
            path: AppConfig(&["Claude", "claude_desktop_config.json"]),
            servers_key: "mcpServers",
            entry: Entry::McpRemote,
        },
        restart: true,
    },
    ClientSpec {
        id: "cursor",
        instructions: Instructions::Manual("Paste the General AgentOS text into Cursor Settings > Rules > User Rules. Cursor does not document a writable global rules file."),
        name: "Cursor",
        aliases: &["cursor-vscode", "cursor"],
        detect: &[Detect::Dir(Home(&[".cursor"]))],
        install: Install::JsonFile {
            path: Home(&[".cursor", "mcp.json"]),
            servers_key: "mcpServers",
            entry: Entry::StreamableHttp,
        },
        restart: false,
    },
    ClientSpec {
        id: "windsurf",
        instructions: Instructions::LimitedFile(Home(&[".codeium", "windsurf", "memories", "global_rules.md"]), 6000),
        name: "Windsurf",
        aliases: &["windsurf"],
        detect: &[Detect::Dir(Home(&[".codeium", "windsurf"]))],
        install: Install::JsonFile {
            path: Home(&[".codeium", "windsurf", "mcp_config.json"]),
            servers_key: "mcpServers",
            entry: Entry::ServerUrl,
        },
        restart: true,
    },
    ClientSpec {
        id: "vscode",
        instructions: Instructions::Manual("Add the General AgentOS text as a user instruction file in VS Code Chat > Configure Instructions, with applyTo: '**'."),
        name: "VS Code (Copilot)",
        aliases: &[
            "visual studio code",
            "vscode",
            "copilot-chat",
            "github-copilot-chat",
            "github-copilot",
        ],
        detect: &[Detect::Dir(AppConfig(&["Code", "User"]))],
        install: Install::JsonFile {
            path: AppConfig(&["Code", "User", "mcp.json"]),
            servers_key: "servers",
            entry: Entry::Http,
        },
        restart: false,
    },
    ClientSpec {
        id: "cline",
        instructions: Instructions::File(Documents(&["Cline", "Rules", "quantmcp-agentos.md"])),
        name: "Cline",
        aliases: &["cline"],
        detect: &[Detect::Dir(VsCodeExtension("saoudrizwan.claude-dev", &[]))],
        install: Install::JsonFile {
            path: VsCodeExtension("saoudrizwan.claude-dev", &["settings", "cline_mcp_settings.json"]),
            servers_key: "mcpServers",
            entry: Entry::Cline,
        },
        restart: false,
    },
    ClientSpec {
        id: "kilo-code",
        instructions: Instructions::Kilo,
        name: "Kilo Code",
        aliases: &["kilo-code"],
        detect: &[
            Detect::Dir(VsCodeExtension("kilocode.kilo-code", &[])),
            Detect::Extension("kilocode.kilo-code"),
        ],
        install: Install::JsonFile {
            path: VsCodeExtension("kilocode.kilo-code", &["settings", "cline_mcp_settings.json"]),
            servers_key: "mcpServers",
            entry: Entry::Cline,
        },
        restart: false,
    },
    ClientSpec {
        id: "roo-code",
        instructions: Instructions::File(Home(&[".roo", "rules", "quantmcp-agentos.md"])),
        name: "Roo Code",
        aliases: &["roo-cline", "roo-code"],
        detect: &[Detect::Dir(VsCodeExtension("rooveterinaryinc.roo-cline", &[]))],
        install: Install::JsonFile {
            path: VsCodeExtension(
                "rooveterinaryinc.roo-cline",
                &["settings", "cline_mcp_settings.json"],
            ),
            servers_key: "mcpServers",
            entry: Entry::Cline,
        },
        restart: false,
    },
    ClientSpec {
        id: "codex-cli",
        instructions: Instructions::Codex,
        name: "Codex CLI",
        aliases: &["codex-mcp-client", "openai-codex", "codex"],
        detect: &[
            Detect::Binary("codex"),
            Detect::File(Home(&[".codex", "config.toml"])),
        ],
        install: Install::Cli {
            binary: "codex",
            add_http: &["mcp", "add", "{name}", "--url", "{url}"],
            add_stdio: Some(&["mcp", "add", "{name}", "{env:--env}", "--", "{command}", "{args}"]),
            remove: &[&["mcp", "remove", "{name}"]],
            config: &[Home(&[".codex", "config.toml"])],
        },
        restart: false,
    },
    ClientSpec {
        id: "gemini-cli",
        instructions: Instructions::File(Home(&[".gemini", "GEMINI.md"])),
        name: "Gemini CLI",
        aliases: &["gemini-cli-mcp-client", "gemini-cli", "gemini"],
        detect: &[
            Detect::Binary("gemini"),
            Detect::File(Home(&[".gemini", "settings.json"])),
        ],
        install: Install::Cli {
            binary: "gemini",
            add_http: &["mcp", "add", "--transport", "http", "--scope", "user", "{name}", "{url}"],
            add_stdio: Some(&[
                "mcp", "add", "--transport", "stdio", "--scope", "user", "{env:-e}", "{name}",
                "{command}", "{args}",
            ]),
            remove: &[
                &["mcp", "remove", "--scope", "user", "{name}"],
                &["mcp", "remove", "{name}"],
            ],
            config: &[Home(&[".gemini", "settings.json"])],
        },
        restart: false,
    },
    ClientSpec {
        id: "kilo-cli",
        instructions: Instructions::Kilo,
        name: "Kilo CLI",
        aliases: &["kilo"],
        detect: &[Detect::Binary("kilo")],
        // `kilo mcp add` takes no arguments (interactive prompts) and there
        // is no `kilo mcp remove` — checked 2026-09-07.
        install: Install::Manual {
            note: "Kilo CLI adds MCP servers interactively: run `kilo mcp add` and paste the URL.",
        },
        restart: false,
    },
    ClientSpec {
        id: "opencode",
        instructions: Instructions::OpenCode,
        name: "OpenCode",
        aliases: &["opencode"],
        detect: &[Detect::Dir(DotConfig(&["opencode"]))],
        install: Install::JsonFile {
            path: DotConfig(&["opencode", "opencode.json"]),
            servers_key: "mcp",
            entry: Entry::OpenCode,
        },
        restart: false,
    },
    ClientSpec {
        id: "continue",
        instructions: Instructions::File(Home(&[".continue", "rules", "quantmcp-agentos.md"])),
        name: "Continue",
        aliases: &["continue"],
        detect: &[Detect::Dir(Home(&[".continue"]))],
        install: Install::JsonFile {
            path: Home(&[".continue", "config.json"]),
            servers_key: "mcpServers",
            entry: Entry::UrlOnly,
        },
        restart: false,
    },
    ClientSpec {
        id: "amp",
        instructions: Instructions::File(DotConfig(&["amp", "AGENTS.md"])),
        name: "Amp",
        aliases: &["amp", "sourcegraph"],
        detect: &[Detect::Dir(Home(&[".amp"]))],
        install: Install::JsonFile {
            path: Home(&[".amp", "settings.json"]),
            servers_key: "amp.mcpServers",
            entry: Entry::UrlOnly,
        },
        restart: false,
    },
    ClientSpec {
        id: "aider",
        instructions: Instructions::Manual("Add the General AgentOS file to aider's read list in ~/.aider.conf.yml."),
        name: "Aider",
        aliases: &["aider"],
        detect: &[
            Detect::Binary("aider"),
            Detect::File(Home(&[".aider.conf.yml"])),
        ],
        install: Install::YamlList {
            path: Home(&[".aider.conf.yml"]),
            key: "mcp-servers",
        },
        restart: false,
    },
    ClientSpec {
        id: "chatgpt",
        instructions: Instructions::Manual("Add the General AgentOS text to your ChatGPT project instructions."),
        name: "ChatGPT",
        aliases: &["chatgpt", "openai-mcp", "vscode-chatgpt", "openai.chatgpt"],
        detect: &[],
        install: Install::Manual {
            note: "ChatGPT reaches MCP servers over public HTTPS only — localhost is out of reach. \
                   Expose the URL through a tunnel, then add it under Settings → Connectors.",
        },
        restart: false,
    },
];

pub fn spec(id: &str) -> Option<&'static ClientSpec> {
    CLIENTS.iter().find(|c| c.id == id)
}

// ── What gets connected ───────────────────────────────────────────────────

/// What Connect writes into the clients: QuantMCP itself, or one of the
/// registered external MCP entries.
pub enum Target<'a> {
    QuantMcp { port: u16 },
    Entry(&'a McpEntry),
}

struct Stdio<'a> {
    command: &'a str,
    args: &'a [String],
    env: &'a HashMap<String, String>,
}

impl Target<'_> {
    pub fn name(&self) -> &str {
        match self {
            Target::QuantMcp { .. } => QUANTMCP_NAME,
            Target::Entry(e) => &e.name,
        }
    }

    /// The Streamable HTTP URL, when the target speaks HTTP.
    pub fn url(&self) -> Option<String> {
        match self {
            Target::QuantMcp { port } => Some(mcp_url(*port)),
            Target::Entry(e) => match &e.transport {
                McpTransport::Http { port } => Some(mcp_url(*port)),
                McpTransport::Stdio => None,
            },
        }
    }

    fn stdio(&self) -> Option<Stdio<'_>> {
        match self {
            Target::Entry(e) if matches!(e.transport, McpTransport::Stdio) => Some(Stdio {
                command: e.command.as_deref().unwrap_or_default(),
                args: &e.args,
                env: &e.env,
            }),
            _ => None,
        }
    }
}

pub fn mcp_url(port: u16) -> String {
    format!("http://localhost:{port}/mcp")
}

/// The standard snippet — plain MCP, for any client the table does not know.
pub fn snippet(target: &Target) -> String {
    let entry = match target.url() {
        Some(url) => json!({ "type": "http", "url": url }),
        None => target
            .stdio()
            .map(stdio_json)
            .unwrap_or_else(|| json!({})),
    };
    serde_json::to_string_pretty(&json!({ "mcpServers": { target.name(): entry } }))
        .unwrap_or_default()
}

fn stdio_json(s: Stdio<'_>) -> Value {
    let mut v = json!({ "command": s.command, "args": s.args });
    if !s.env.is_empty() {
        v["env"] = json!(sorted_env(s.env));
    }
    v
}

/// Env as a sorted map, so a written config is byte-stable between runs.
fn sorted_env(env: &HashMap<String, String>) -> BTreeMap<String, String> {
    env.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

impl Entry {
    fn render(&self, target: &Target) -> Result<Value, String> {
        if let Some(url) = target.url() {
            return Ok(match self {
                Entry::Http => json!({ "type": "http", "url": url }),
                Entry::StreamableHttp => json!({ "type": "streamable-http", "url": url }),
                Entry::Cline => {
                    json!({ "url": url, "transportType": "streamableHttp", "disabled": false })
                }
                Entry::ServerUrl => json!({ "serverUrl": url }),
                Entry::UrlOnly => json!({ "url": url }),
                Entry::OpenCode => json!({ "type": "remote", "url": url }),
                Entry::McpRemote => json!({ "command": "npx", "args": ["-y", "mcp-remote", url] }),
            });
        }
        let s = target
            .stdio()
            .ok_or_else(|| format!("'{}' has neither an HTTP URL nor a command", target.name()))?;
        Ok(match self {
            Entry::OpenCode => {
                let mut command = vec![s.command.to_string()];
                command.extend(s.args.iter().cloned());
                let mut v = json!({ "type": "local", "command": command });
                if !s.env.is_empty() {
                    v["environment"] = json!(sorted_env(s.env));
                }
                v
            }
            Entry::Cline => {
                let mut v = stdio_json(s);
                v["disabled"] = json!(false);
                v
            }
            _ => stdio_json(s),
        })
    }
}

// ── Paths and detection ───────────────────────────────────────────────────

impl ConfigPath {
    pub fn resolve(&self) -> Option<PathBuf> {
        let (base, parts): (PathBuf, &[&str]) = match self {
            Home(parts) => (dirs::home_dir()?, parts),
            AppConfig(parts) => (dirs::config_dir()?, parts),
            DotConfig(parts) => (dirs::home_dir()?.join(".config"), parts),
            ConfigPath::OmpAgent(parts) => (instructions::pi::omp_home().ok()?, parts),
            Documents(parts) => (dirs::document_dir().or_else(|| dirs::home_dir().map(|p| p.join("Documents")))?, parts),
            VsCodeExtension(ext, parts) => (
                dirs::config_dir()?
                    .join("Code")
                    .join("User")
                    .join("globalStorage")
                    .join(ext),
                parts,
            ),
        };
        Some(parts.iter().fold(base, |p, part| p.join(part)))
    }
}

fn find_on_path(binary: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exts: &[&str] = if cfg!(windows) {
        &["", ".exe", ".cmd", ".bat"]
    } else {
        &[""]
    };
    for dir in std::env::split_paths(&path) {
        for ext in exts {
            let candidate = dir.join(format!("{binary}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

impl Detect {
    fn hit(&self) -> bool {
        match self {
            Detect::File(p) => p.resolve().is_some_and(|p| p.is_file()),
            Detect::Dir(p) => p.resolve().is_some_and(|p| p.is_dir()),
            Detect::Binary(b) => find_on_path(b).is_some(),
            Detect::Extension(id) => dirs::home_dir().is_some_and(|home| {
                [".vscode", ".vscode-insiders", ".cursor"].iter().any(|editor| {
                    extension_installed(&home.join(editor).join("extensions"), id)
                })
            }),
        }
    }
}

fn extension_installed(directory: &Path, id: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return false;
    };
    let prefix = format!("{id}-");
    entries.flatten().any(|entry| {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        (name == id || name.starts_with(&prefix)) && entry.path().join("package.json").is_file()
    })
}

/// Keys under which clients keep their server maps — for the CLI clients,
/// whose files we read but never write ourselves.
const SERVER_MAP_KEYS: &[&str] = &[
    "mcpServers",
    "mcp_servers",
    "servers",
    "mcp",
    "amp.mcpServers",
    "github.copilot.chat.mcpServers",
];

fn json_mentions(value: &Value, name: &str) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(k, v)| {
            (SERVER_MAP_KEYS.contains(&k.as_str()) && v.get(name).is_some()) || json_mentions(v, name)
        }),
        Value::Array(items) => items.iter().any(|v| json_mentions(v, name)),
        _ => false,
    }
}

fn toml_mentions(content: &str, name: &str) -> bool {
    let s = content.to_lowercase();
    let n = name.to_lowercase();
    s.contains(&format!("[mcp_servers.{n}]"))
        || s.contains(&format!("[mcp_servers.\"{n}\"]"))
        || (s.contains("[mcp_servers]") && s.contains(&n))
}

fn file_mentions(path: &Path, name: &str) -> bool {
    let Ok(content) = std::fs::read_to_string(path) else {
        return false;
    };
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return false;
    }
    match serde_json::from_str::<Value>(trimmed) {
        Ok(v) => json_mentions(&v, name),
        Err(_) => toml_mentions(trimmed, name),
    }
}

impl ClientSpec {
    pub fn installed(&self) -> bool {
        self.detect.iter().any(Detect::hit)
            || (self.id == "codex-cli" && instructions::has_codex_profile())
            || (self.id == "pi" && instructions::pi::pi_home().is_ok_and(|path| path.is_dir()))
            || (self.id == "omp" && instructions::pi::omp_home().is_ok_and(|path| path.is_dir()))
    }

    /// Where this client already lists the target, if anywhere.
    pub fn configured(&self, target: &Target) -> Option<PathBuf> {
        let name = target.name();
        match &self.install {
            Install::JsonFile { path, servers_key, .. } => {
                let p = path.resolve()?;
                let content = std::fs::read_to_string(&p).ok()?;
                let v: Value = serde_json::from_str(content.trim()).ok()?;
                v.get(servers_key)?.get(name).map(|_| p)
            }
            Install::Cli { config, .. } => config
                .iter()
                .filter_map(ConfigPath::resolve)
                .find(|p| file_mentions(p, name)),
            Install::YamlList { path, .. } => {
                let p = path.resolve()?;
                let url = target.url()?;
                let content = std::fs::read_to_string(&p).ok()?;
                content.contains(&url).then_some(p)
            }
            Install::Manual { .. } => None,
        }
    }

    /// The file Connect writes, for the report — none for CLIs and manual clients.
    fn config_file(&self) -> Option<PathBuf> {
        match &self.install {
            Install::JsonFile { path, .. } | Install::YamlList { path, .. } => path.resolve(),
            Install::Cli { .. } | Install::Manual { .. } => None,
        }
    }
}

// ── Scan ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ClientScan {
    pub id: String,
    pub name: String,
    pub installed: bool,
    pub configured: bool,
    pub config_path: Option<String>,
    pub restart: bool,
    pub manual: bool,
    /// Capability guard: older backends ignore the MCP client selection.
    pub mcp_selection_supported: bool,
    pub note: Option<String>,
    /// Instruction import support is independent of MCP configuration support.
    pub instructions_manual: bool,
    pub instructions_note: Option<String>,
    pub instructions_paths: Vec<String>,
    pub custom: bool,
    pub detection: Option<String>,
    pub mcp_support: String,
}

pub fn scan(target: &Target) -> Vec<ClientScan> {
    CLIENTS
        .iter()
        .map(|spec| {
            let installed = spec.installed();
            let configured_at = if installed {
                spec.configured(target)
            } else {
                None
            };
            let (manual, note) = match &spec.install {
                Install::Manual { note } => (true, Some(note.to_string())),
                _ => (false, None),
            };
            ClientScan {
                id: spec.id.into(),
                name: spec.name.into(),
                installed,
                configured: configured_at.is_some(),
                config_path: configured_at
                    .or_else(|| spec.config_file())
                    .map(|p| p.display().to_string()),
                restart: spec.restart,
                manual,
                mcp_selection_supported: true,
                note,
                instructions_manual: matches!(spec.instructions, Instructions::Manual(_)),
                instructions_note: spec.instructions.note().map(str::to_owned),
                instructions_paths: spec.instructions.paths().unwrap_or_default().iter()
                    .map(|path| path.display().to_string()).collect(),
                custom: false,
                detection: spec.detect.iter().find_map(|detect| match detect {
                    Detect::Binary(binary) => find_on_path(binary).map(|path| path.display().to_string()),
                    Detect::File(path) | Detect::Dir(path) => path.resolve().filter(|path| path.exists()).map(|path| path.display().to_string()),
                    _ => None,
                }).or_else(|| installed.then(|| "Existing agent profile or editor extension".into())),
                mcp_support: if spec.id == "pi" { "extension" } else { "native" }.into(),
            }
        })
        .collect()
}

// ── Connect ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// A config file was written; `detail` is its path.
    Written,
    /// The client's CLI ran; `detail` is the command line.
    Ran,
    /// Not installed on this machine.
    Skipped,
    /// `detail` is the error.
    Failed,
    /// No local config; `detail` is the note.
    Manual,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConnectResult {
    pub id: String,
    pub name: String,
    pub outcome: Outcome,
    pub detail: String,
    pub restart: bool,
}

/// Write the target into every client found on this machine. Idempotent:
/// an existing entry is replaced, so a changed port travels with it.
pub async fn connect(target: &Target<'_>) -> Vec<ConnectResult> {
    connect_specs(target, &CLIENTS.iter().collect::<Vec<_>>()).await
}

fn selected_mcp_clients(ids: &[String]) -> Result<Vec<&'static ClientSpec>, String> {
    if ids.is_empty() {
        return Err("Select at least one agent before connecting.".into());
    }
    let mut selected = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for id in ids {
        let client = spec(id).ok_or_else(|| format!("Unknown MCP agent: {id}. Rescan the agent list."))?;
        if seen.insert(client.id) {
            selected.push(client);
        }
    }
    Ok(selected)
}

/// Validate the entire selection before writing or running any client CLI.
pub async fn connect_selected(target: &Target<'_>, ids: &[String]) -> Result<Vec<ConnectResult>, String> {
    let selected = selected_mcp_clients(ids)?;
    Ok(connect_specs(target, &selected).await)
}

async fn connect_specs(target: &Target<'_>, specs: &[&ClientSpec]) -> Vec<ConnectResult> {
    let mut report = Vec::with_capacity(specs.len());
    for spec in specs {
        let (outcome, detail) = match &spec.install {
            _ if !spec.detect.is_empty() && !spec.installed() => {
                (Outcome::Skipped, "not installed on this machine".into())
            }
            Install::Manual { note } => (Outcome::Manual, note.to_string()),
            Install::JsonFile { path, servers_key, entry } => {
                match install_json(path, servers_key, entry, target) {
                    Ok(p) => (Outcome::Written, p),
                    Err(e) => (Outcome::Failed, e),
                }
            }
            Install::YamlList { path, key } => match install_yaml_list(path, key, target) {
                Ok(p) => (Outcome::Written, p),
                Err(e) => (Outcome::Failed, e),
            },
            Install::Cli { binary, add_http, add_stdio, remove, .. } => {
                match install_cli(binary, add_http, *add_stdio, remove, target).await {
                    Ok(cmd) => (Outcome::Ran, cmd),
                    Err(e) => (Outcome::Failed, e),
                }
            }
        };
        report.push(ConnectResult {
            id: spec.id.into(),
            name: spec.name.into(),
            outcome,
            detail,
            restart: spec.restart && matches!(outcome, Outcome::Written | Outcome::Ran),
        });
    }
    report
}

fn install_json(
    path: &ConfigPath,
    servers_key: &str,
    entry: &Entry,
    target: &Target,
) -> Result<String, String> {
    let p = path.resolve().ok_or("Could not resolve the config directory")?;
    let value = entry.render(target)?;
    write_json_entry(&p, servers_key, target.name(), value)?;
    Ok(p.display().to_string())
}

/// Set `<servers_key>.<name>` in a JSON config file, creating the file and
/// the map when they are missing. Another application's config: the write is
/// atomic and the previous file is kept as `.bak`.
pub fn write_json_entry(
    path: &Path,
    servers_key: &str,
    name: &str,
    value: Value,
) -> Result<(), String> {
    let mut config = read_json_or_empty(path)?;
    let obj = config
        .as_object_mut()
        .ok_or_else(|| format!("{} is not a JSON object", path.display()))?;
    let servers = obj
        .entry(servers_key)
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| format!("{} in {} is not an object", servers_key, path.display()))?;
    servers.insert(name.to_string(), value);
    write_pretty(path, &config)
}

fn read_json_or_empty(path: &Path) -> Result<Value, String> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(trimmed).map_err(|e| format!("Failed to parse {}: {}", path.display(), e))
}

fn write_pretty(path: &Path, config: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
    }
    let pretty = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    qs_core::paths::write_atomic_with_backup(path, pretty.as_bytes())
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}

fn install_yaml_list(path: &ConfigPath, key: &str, target: &Target) -> Result<String, String> {
    let p = path.resolve().ok_or("Could not resolve the config directory")?;
    let url = target
        .url()
        .ok_or_else(|| format!("'{}' has no HTTP URL — this client takes URLs only", target.name()))?;
    let existing = if p.exists() {
        std::fs::read_to_string(&p).map_err(|e| format!("Failed to read {}: {}", p.display(), e))?
    } else {
        String::new()
    };
    let updated = yaml_list_with(&existing, key, &url);
    if updated != existing {
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
        }
        qs_core::paths::write_atomic_with_backup(&p, updated.as_bytes())
            .map_err(|e| format!("Failed to write {}: {}", p.display(), e))?;
    }
    Ok(p.display().to_string())
}

/// `content` with `item` listed under `key:` — unchanged when it already is.
pub fn yaml_list_with(content: &str, key: &str, item: &str) -> String {
    if content.contains(item) {
        return content.to_string();
    }
    let header = format!("{key}:");
    if content.contains(&header) {
        return content.replacen(&header, &format!("{header}\n  - {item}"), 1);
    }
    let sep = if content.is_empty() || content.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    format!("{content}{sep}{header}\n  - {item}\n")
}

/// Expand a CLI template for the target. See `Install::Cli` for the tokens.
pub fn expand(template: &[&str], target: &Target) -> Result<Vec<String>, String> {
    let mut out = Vec::with_capacity(template.len());
    for token in template {
        match *token {
            "{name}" => out.push(target.name().to_string()),
            "{url}" => out.push(
                target
                    .url()
                    .ok_or_else(|| format!("'{}' has no HTTP URL", target.name()))?,
            ),
            "{command}" => out.push(stdio_of(target)?.command.to_string()),
            "{args}" => out.extend(stdio_of(target)?.args.iter().cloned()),
            t if t.starts_with("{env:") && t.ends_with('}') => {
                let flag = &t[5..t.len() - 1];
                for (k, v) in sorted_env(stdio_of(target)?.env) {
                    out.push(flag.to_string());
                    out.push(format!("{k}={v}"));
                }
            }
            t => out.push(t.to_string()),
        }
    }
    Ok(out)
}

fn stdio_of<'a>(target: &'a Target<'a>) -> Result<Stdio<'a>, String> {
    target
        .stdio()
        .ok_or_else(|| format!("'{}' has no command — it is not a stdio server", target.name()))
}

fn cli_command(binary: &str, args: &[String]) -> tokio::process::Command {
    // On Windows the npm-installed CLIs are .cmd scripts that need cmd /C;
    // native .exe binaries run through it just as well.
    let mut cmd = if cfg!(windows) {
        let mut c = tokio::process::Command::new("cmd");
        c.arg("/C").arg(binary).args(args);
        c
    } else {
        let mut c = tokio::process::Command::new(binary);
        c.args(args);
        c
    };
    #[cfg(windows)]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    cmd
}

/// Run the CLI and return `(success, first meaningful output line)`.
async fn run_cli(binary: &str, args: &[String]) -> Result<(bool, String), String> {
    let output = cli_command(binary, args)
        .output()
        .await
        .map_err(|e| format!("Failed to run {}: {}", binary, e))?;
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let text = if output.status.success() {
        stdout
    } else if stderr.is_empty() {
        if stdout.is_empty() {
            format!("{} exited with code {:?}", binary, output.status.code())
        } else {
            stdout
        }
    } else {
        stderr
    };
    Ok((output.status.success(), text))
}

async fn install_cli(
    binary: &str,
    add_http: &[&str],
    add_stdio: Option<&[&str]>,
    remove: &[&[&str]],
    target: &Target<'_>,
) -> Result<String, String> {
    let template = if target.url().is_some() {
        add_http
    } else {
        add_stdio.ok_or_else(|| {
            format!("{} takes URLs only — a stdio server cannot be added through its CLI", binary)
        })?
    };
    let args = expand(template, target)?;
    let command_line = format!("{} {}", binary, args.join(" "));

    let (ok, text) = run_cli(binary, &args).await?;
    if ok {
        return Ok(command_line);
    }
    if !text.to_lowercase().contains("already exists") {
        return Err(format!("{}: {}", command_line, first_line(&text)));
    }

    // The entry exists under this name — replace it, so a changed port or
    // transport lands: remove (first candidate that succeeds), then add again.
    let mut removed = false;
    for candidate in remove {
        let remove_args = expand(candidate, target)?;
        if let Ok((true, _)) = run_cli(binary, &remove_args).await {
            removed = true;
            break;
        }
    }
    if !removed {
        return Err(format!(
            "'{}' already exists in {} and could not be replaced: {}",
            target.name(),
            binary,
            first_line(&text)
        ));
    }
    let (ok, text) = run_cli(binary, &args).await?;
    if ok {
        Ok(command_line)
    } else {
        Err(format!("{} (re-add after remove): {}", command_line, first_line(&text)))
    }
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or("").trim()
}

// ── clientInfo → client ───────────────────────────────────────────────────

/// `clientInfo.name` as the table spells it: no `(via mcp-remote …)`, no
/// `/version` suffix, trimmed, lowercase.
pub fn normalize_client_name(raw: &str) -> String {
    let mut s = raw.trim();
    if let Some(idx) = s.find("(via mcp-remote") {
        s = s[..idx].trim_end();
    }
    if let Some(idx) = s.find('/') {
        s = s[..idx].trim_end();
    }
    s.trim().to_lowercase()
}

pub fn match_client_info(raw: &str) -> Option<&'static ClientSpec> {
    let n = normalize_client_name(raw);
    if n.is_empty() {
        return None;
    }
    CLIENTS.iter().find(|c| {
        c.id == n
            || c.aliases.iter().any(|alias| match alias.strip_suffix('*') {
                Some(prefix) => n.starts_with(prefix),
                None => *alias == n,
            })
    })
}

/// Names that are not a client at all: mcp-remote opens a throwaway session
/// under this name to probe which transport the server speaks, before the
/// real client connects. Seen on this machine as three "sessions" of nothing.
const PROBE_CLIENT_NAMES: &[&str] = &["mcp-remote-fallback-test"];

pub fn is_probe(raw: &str) -> bool {
    let n = normalize_client_name(raw);
    PROBE_CLIENT_NAMES.contains(&n.as_str())
}

// ── The client registry (core.db) ─────────────────────────────────────────
//
// Every AI that ever connected, or was ever connected to, is an entity
// `mcp:client:<key>` (`module: "mcp"`, `kind: "client"`) — the registry the
// workspaces live in, so the suite's search and dashboard see it and no
// second file appears on disk. The record is the *offline* half of the
// Connected-AIs list; the live sessions in `mcp_server` are the online half.
// A client the table does not know keys by a slug of the name it reports.

pub const CLIENT_MODULE: &str = "mcp";
pub const CLIENT_KIND: &str = "client";
const CLIENT_ID_PREFIX: &str = "mcp:client:";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClientRecord {
    #[serde(default)]
    pub spec_id: Option<String>,
    /// Every `clientInfo.name` this client has reported, as reported.
    #[serde(default)]
    pub reported_names: Vec<String>,
    /// First and latest session (ms since the epoch); none = never connected.
    #[serde(default)]
    pub first_seen: Option<i64>,
    #[serde(default)]
    pub last_seen: Option<i64>,
    #[serde(default)]
    pub sessions_total: u64,
    #[serde(default)]
    pub last_version: Option<String>,
    /// The latest Connect that wrote this client; none = never by us.
    #[serde(default)]
    pub installed_at: Option<i64>,
    /// Config paths or CLI command lines Connect wrote, or the config a scan
    /// found QuantMCP in.
    #[serde(default)]
    pub installed_to: Vec<String>,
    /// `"connect"` (we wrote it) or `"detected"` (found in its config).
    #[serde(default)]
    pub install_source: Option<String>,
}

/// The registry identity of a client as it reports itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientKey {
    pub id: String,
    pub title: String,
    pub spec_id: Option<String>,
}

pub fn client_key(raw_name: &str) -> ClientKey {
    if let Some(spec) = match_client_info(raw_name) {
        return client_key_for_spec(spec);
    }
    let normalized = normalize_client_name(raw_name);
    let slug = slugify(&normalized);
    ClientKey {
        id: format!("{CLIENT_ID_PREFIX}{slug}"),
        title: display_name(raw_name),
        spec_id: None,
    }
}

pub fn client_key_for_spec(spec: &ClientSpec) -> ClientKey {
    ClientKey {
        id: format!("{CLIENT_ID_PREFIX}{}", spec.id),
        title: spec.name.to_string(),
        spec_id: Some(spec.id.to_string()),
    }
}

pub fn is_client_id(id: &str) -> bool {
    id.starts_with(CLIENT_ID_PREFIX) && id.len() > CLIENT_ID_PREFIX.len()
}

/// The reported name without the transport and version suffixes, case kept.
fn display_name(raw: &str) -> String {
    let mut s = raw.trim();
    if let Some(idx) = s.find("(via mcp-remote") {
        s = s[..idx].trim_end();
    }
    if let Some(idx) = s.find('/') {
        s = s[..idx].trim_end();
    }
    let s = s.trim();
    if s.is_empty() {
        "Unknown".to_string()
    } else {
        s.to_string()
    }
}

fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut dash = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        "unknown".to_string()
    } else {
        out
    }
}

impl ClientRecord {
    pub fn note_session(&mut self, key: &ClientKey, raw_name: &str, version: Option<&str>, now_ms: i64) {
        self.spec_id = key.spec_id.clone();
        let name = raw_name.trim();
        if !name.is_empty() && !self.reported_names.iter().any(|n| n == name) {
            self.reported_names.push(name.to_string());
        }
        if self.first_seen.is_none() {
            self.first_seen = Some(now_ms);
        }
        self.last_seen = Some(now_ms);
        self.sessions_total += 1;
        if let Some(v) = version.map(str::trim).filter(|v| !v.is_empty()) {
            self.last_version = Some(v.to_string());
        }
    }

    /// Connect wrote this client: `detail` is the path or the command line.
    pub fn note_install(&mut self, key: &ClientKey, detail: &str, now_ms: i64) {
        self.spec_id = key.spec_id.clone();
        self.installed_at = Some(now_ms);
        self.install_source = Some("connect".into());
        if !self.installed_to.iter().any(|d| d == detail) {
            self.installed_to.push(detail.to_string());
        }
    }

    /// A scan found QuantMCP in this client's config, and we never wrote it:
    /// it was installed by hand or by an earlier version — "eingepflegt" all
    /// the same. A record Connect wrote keeps its own provenance.
    pub fn note_detected(&mut self, key: &ClientKey, path: &str) {
        self.spec_id = key.spec_id.clone();
        if self.install_source.is_none() {
            self.install_source = Some("detected".into());
        }
        if !self.installed_to.iter().any(|d| d == path) {
            self.installed_to.push(path.to_string());
        }
    }

    pub fn installed(&self) -> bool {
        !self.installed_to.is_empty()
    }

    pub fn from_entity(e: &qs_core::db::Entity) -> ClientRecord {
        e.payload
            .as_ref()
            .and_then(|p| serde_json::from_value(p.clone()).ok())
            .unwrap_or_default()
    }

    pub fn entity(&self, key: &ClientKey, updated_at: i64) -> qs_core::db::Entity {
        qs_core::db::Entity {
            id: key.id.clone(),
            module: CLIENT_MODULE.into(),
            kind: CLIENT_KIND.into(),
            title: key.title.clone(),
            subtitle: self.last_version.as_ref().map(|v| format!("v{v}")),
            route: "/mcp".into(),
            icon: None,
            updated_at,
            payload: serde_json::to_value(self).ok(),
        }
    }
}

/// Every record, keyed by entity id.
pub fn load_records(app: &tauri::AppHandle) -> Result<Vec<(qs_core::db::Entity, ClientRecord)>, String> {
    crate::settings::with_core_db(app, |conn| {
        qs_core::db::list_entities(conn, Some(CLIENT_MODULE), Some(CLIENT_KIND), 1000)
            .map_err(|e| e.to_string())
    })
    .map(|entities| {
        entities
            .into_iter()
            .map(|e| {
                let record = ClientRecord::from_entity(&e);
                (e, record)
            })
            .collect()
    })
}

fn load_record(app: &tauri::AppHandle, id: &str) -> Result<ClientRecord, String> {
    Ok(load_records(app)?
        .into_iter()
        .find(|(e, _)| e.id == id)
        .map(|(_, r)| r)
        .unwrap_or_default())
}

fn save_record(key: &ClientKey, record: &ClientRecord, now_ms: i64) -> Result<(), String> {
    qs_core::runtime::upsert_entity(record.entity(key, now_ms))
}

/// A client just connected: the record gains a session.
pub fn record_session(
    app: &tauri::AppHandle,
    raw_name: &str,
    version: Option<&str>,
    now_ms: i64,
) -> Result<ClientKey, String> {
    let key = client_key(raw_name);
    let mut record = load_record(app, &key.id)?;
    record.note_session(&key, raw_name, version, now_ms);
    save_record(&key, &record, now_ms)?;
    Ok(key)
}

/// Connect ran: every client it wrote is on record as installed by us.
pub fn record_connect(app: &tauri::AppHandle, report: &[ConnectResult], now_ms: i64) -> Result<(), String> {
    for r in report {
        if !matches!(r.outcome, Outcome::Written | Outcome::Ran) {
            continue;
        }
        let Some(spec) = spec(&r.id) else { continue };
        let key = client_key_for_spec(spec);
        let mut record = load_record(app, &key.id)?;
        record.note_install(&key, &r.detail, now_ms);
        save_record(&key, &record, now_ms)?;
    }
    Ok(())
}

/// A scan ran: clients whose config already lists QuantMCP are on record.
pub fn record_scan(app: &tauri::AppHandle, scans: &[ClientScan], now_ms: i64) -> Result<(), String> {
    for scan in scans {
        if !scan.configured {
            continue;
        }
        let (Some(spec), Some(path)) = (spec(&scan.id), scan.config_path.as_deref()) else {
            continue;
        };
        let key = client_key_for_spec(spec);
        let mut record = load_record(app, &key.id)?;
        let before = serde_json::to_value(&record).ok();
        record.note_detected(&key, path);
        if serde_json::to_value(&record).ok() != before {
            save_record(&key, &record, now_ms)?;
        }
    }
    Ok(())
}

pub fn forget_record(id: &str) -> Result<(), String> {
    if !is_client_id(id) {
        return Err(format!("'{id}' is not a client record"));
    }
    qs_core::runtime::delete_entity(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_selection_is_explicit_validated_and_deduplicated() {
        assert!(selected_mcp_clients(&[]).is_err());
        assert!(selected_mcp_clients(&["cursor".into(), "unknown".into()]).is_err());
        assert!(selected_mcp_clients(&["custom:instruction-file".into()]).is_err());
        let selected = selected_mcp_clients(&["omp".into(), "cursor".into(), "omp".into()]).unwrap();
        assert_eq!(selected.iter().map(|client| client.id).collect::<Vec<_>>(), ["omp", "cursor"]);
    }

    #[tokio::test]
    async fn selected_manual_mcp_client_never_connects_other_agents() {
        let target = Target::QuantMcp { port: 3100 };
        assert!(connect_selected(&target, &[]).await.is_err());
        assert!(connect_selected(&target, &["chatgpt".into(), "unknown".into()]).await.is_err());
        let report = connect_selected(&target, &["chatgpt".into(), "chatgpt".into()]).await.unwrap();
        assert_eq!(report.len(), 1);
        assert_eq!(report[0].id, "chatgpt");
        assert_eq!(report[0].outcome, Outcome::Manual);
        assert!(!report[0].restart);
    }

    #[tokio::test]
    async fn connect_rechecks_missing_installations() {
        let missing = ClientSpec {
            id: "missing-test-agent", name: "Missing test agent", aliases: &[],
            detect: &[Detect::Binary("qs-missing-test-agent-77b6b418")],
            install: Install::Manual { note: "manual" },
            instructions: Instructions::Manual("manual"), restart: true,
        };
        let report = connect_specs(&Target::QuantMcp { port: 3100 }, &[&missing]).await;
        assert_eq!(report.len(), 1);
        assert_eq!(report[0].outcome, Outcome::Skipped);
        assert!(!report[0].restart);
    }

    fn stdio_entry() -> McpEntry {
        let mut env = HashMap::new();
        env.insert("TOKEN".to_string(), "abc".to_string());
        McpEntry {
            id: "1".into(),
            name: "Local".into(),
            description: String::new(),
            enabled: true,
            transport: McpTransport::Stdio,
            command: Some("npx".into()),
            args: vec!["-y".into(), "some-mcp".into()],
            env,
            working_dir: None,
        }
    }

    #[test]
    fn detects_installed_kilo_extension_without_global_storage() {
        let directory = std::env::temp_dir().join(format!("qs-extension-{}", uuid::Uuid::new_v4()));
        let kilo = directory.join("kilocode.kilo-code-7.0.37");
        std::fs::create_dir_all(&kilo).unwrap();
        assert!(!extension_installed(&directory, "kilocode.kilo-code"));
        std::fs::write(kilo.join("package.json"), "{}").unwrap();
        assert!(extension_installed(&directory, "kilocode.kilo-code"));
        assert!(!extension_installed(&directory, "different.extension"));
        std::fs::remove_dir_all(&directory).unwrap();
        assert!(!extension_installed(&directory, "kilocode.kilo-code"));
    }

    #[test]
    fn every_row_is_well_formed() {
        let mut ids = std::collections::HashSet::new();
        for spec in CLIENTS {
            assert!(ids.insert(spec.id), "duplicate id {}", spec.id);
            assert!(!spec.name.is_empty());
            assert!(std::ptr::eq(super::spec(spec.id).expect("row by id"), spec));
            for alias in spec.aliases {
                assert_eq!(*alias, alias.to_lowercase(), "alias {alias} must be lowercase");
                assert!(
                    std::ptr::eq(match_client_info(alias).expect("alias resolves"), spec),
                    "alias {alias} must resolve to {}",
                    spec.id
                );
            }
            if let Install::JsonFile { path, servers_key, entry } = &spec.install {
                assert!(path.resolve().is_some(), "{}: path resolves", spec.id);
                assert!(!servers_key.is_empty());
                let http = entry.render(&Target::QuantMcp { port: 3100 }).unwrap();
                assert!(http.is_object(), "{}: http entry", spec.id);
                let e = stdio_entry();
                let stdio = entry.render(&Target::Entry(&e)).unwrap();
                assert!(stdio.is_object(), "{}: stdio entry", spec.id);
            }
            if let Install::Cli { add_http, add_stdio, remove, .. } = &spec.install {
                let q = Target::QuantMcp { port: 3100 };
                let args = expand(add_http, &q).unwrap();
                assert!(args.iter().all(|a| !a.contains('{')), "{}: {:?}", spec.id, args);
                assert!(args.contains(&"QuantMCP".to_string()));
                assert!(args.contains(&"http://localhost:3100/mcp".to_string()));
                for r in *remove {
                    let args = expand(r, &q).unwrap();
                    assert!(args.iter().all(|a| !a.contains('{')));
                }
                if let Some(t) = add_stdio {
                    let e = stdio_entry();
                    let args = expand(t, &Target::Entry(&e)).unwrap();
                    assert!(args.iter().all(|a| !a.contains('{')), "{}: {:?}", spec.id, args);
                    assert!(args.contains(&"TOKEN=abc".to_string()));
                    assert!(args.contains(&"some-mcp".to_string()));
                }
            }
        }
    }

    #[test]
    fn snippet_is_plain_mcp() {
        let s = snippet(&Target::QuantMcp { port: 3101 });
        let v: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["mcpServers"]["QuantMCP"]["type"], "http");
        assert_eq!(v["mcpServers"]["QuantMCP"]["url"], "http://localhost:3101/mcp");

        let e = stdio_entry();
        let v: Value = serde_json::from_str(&snippet(&Target::Entry(&e))).unwrap();
        assert_eq!(v["mcpServers"]["Local"]["command"], "npx");
        assert_eq!(v["mcpServers"]["Local"]["env"]["TOKEN"], "abc");
    }

    #[test]
    fn client_info_matching() {
        assert_eq!(match_client_info("claude-code").unwrap().id, "claude-code");
        assert_eq!(
            match_client_info("claude-ai (via mcp-remote 0.1.37)").unwrap().id,
            "claude-desktop"
        );
        assert_eq!(match_client_info("Cursor-VSCode/1.7.2").unwrap().id, "cursor");
        assert_eq!(match_client_info("Visual Studio Code").unwrap().id, "vscode");
        assert!(match_client_info("my-own-agent").is_none());
        // Prefix alias: Claude Desktop's local agent mode names itself after the server.
        assert_eq!(
            match_client_info("local-agent-mode-QuantMCP (via mcp-remote 0.8.3)").unwrap().id,
            "claude-desktop"
        );
        assert!(is_probe("mcp-remote-fallback-test"));
        assert!(is_probe("mcp-remote-fallback-test/0.0.0"));
        assert!(!is_probe("claude-code"));
        assert!(match_client_info("   ").is_none());
        // No substring guessing: "codex" is an alias, "vscodex" is not.
        assert!(match_client_info("vscodex").is_none());
    }


    #[test]
    fn client_keys_known_and_unknown() {
        let k = client_key("claude-code/2.0.1");
        assert_eq!(k.id, "mcp:client:claude-code");
        assert_eq!(k.title, "Claude Code");
        assert_eq!(k.spec_id.as_deref(), Some("claude-code"));
        assert_eq!(client_key_for_spec(spec("cursor").unwrap()), client_key("Cursor-VSCode"));

        let u = client_key("My Agent/0.3 (via mcp-remote 0.1.2)");
        assert_eq!(u.id, "mcp:client:my-agent");
        assert_eq!(u.title, "My Agent");
        assert!(u.spec_id.is_none());
        assert_eq!(client_key("   ").id, "mcp:client:unknown");
        assert!(is_client_id("mcp:client:cursor"));
        assert!(!is_client_id("core:workspace:x"));
    }

    #[test]
    fn record_notes_sessions_installs_and_detections() {
        let key = client_key("cursor-vscode");
        let mut r = ClientRecord::default();
        r.note_session(&key, "cursor-vscode", Some("1.7.2"), 1000);
        r.note_session(&key, "cursor-vscode", None, 2000);
        assert_eq!(r.first_seen, Some(1000));
        assert_eq!(r.last_seen, Some(2000));
        assert_eq!(r.sessions_total, 2);
        assert_eq!(r.last_version.as_deref(), Some("1.7.2"));
        assert_eq!(r.reported_names, vec!["cursor-vscode".to_string()]);
        assert!(!r.installed());

        r.note_detected(&key, "C:\\u\\.cursor\\mcp.json");
        assert_eq!(r.install_source.as_deref(), Some("detected"));
        assert!(r.installed());
        r.note_install(&key, "C:\\u\\.cursor\\mcp.json", 3000);
        assert_eq!(r.install_source.as_deref(), Some("connect"));
        assert_eq!(r.installed_at, Some(3000));
        assert_eq!(r.installed_to.len(), 1);
        // A record Connect wrote keeps its provenance through later scans.
        r.note_detected(&key, "C:\\u\\.cursor\\mcp.json");
        assert_eq!(r.install_source.as_deref(), Some("connect"));

        let e = r.entity(&key, 3000);
        assert_eq!(e.id, "mcp:client:cursor");
        assert_eq!(e.module, CLIENT_MODULE);
        assert_eq!(e.kind, CLIENT_KIND);
        assert_eq!(e.subtitle.as_deref(), Some("v1.7.2"));
        let back = ClientRecord::from_entity(&e);
        assert_eq!(back.sessions_total, 2);
        assert_eq!(back.installed_to, r.installed_to);
    }

    #[test]
    fn json_entry_write_is_idempotent_and_keeps_the_rest() {
        let dir = std::env::temp_dir().join(format!("qs-clients-{}", uuid::Uuid::new_v4()));
        let path = dir.join("mcp.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, r#"{ "other": 1, "mcpServers": { "keep": { "url": "x" } } }"#).unwrap();

        write_json_entry(&path, "mcpServers", "QuantMCP", json!({ "url": "a" })).unwrap();
        write_json_entry(&path, "mcpServers", "QuantMCP", json!({ "url": "b" })).unwrap();

        let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["other"], 1);
        assert_eq!(v["mcpServers"]["keep"]["url"], "x");
        assert_eq!(v["mcpServers"]["QuantMCP"]["url"], "b");
        assert!(dir.join("mcp.json.bak").exists());

        // A missing file and a missing map are created.
        let fresh = dir.join("sub").join("new.json");
        write_json_entry(&fresh, "servers", "QuantMCP", json!({ "type": "http" })).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&fresh).unwrap()).unwrap();
        assert_eq!(v["servers"]["QuantMCP"]["type"], "http");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn json_mentions_finds_nested_maps() {
        let claude = json!({
            "projects": { "C:\\x": { "mcpServers": { "QuantMCP": {} } } }
        });
        assert!(json_mentions(&claude, "QuantMCP"));
        assert!(!json_mentions(&claude, "Other"));
        assert!(toml_mentions("[mcp_servers.QuantMCP]\nurl = \"x\"", "QuantMCP"));
        assert!(!toml_mentions("[mcp_servers.other]\nurl = \"x\"", "QuantMCP"));
    }

    #[test]
    fn yaml_list_insertion() {
        let url = "http://localhost:3100/mcp";
        assert_eq!(yaml_list_with("", "mcp-servers", url), "mcp-servers:\n  - http://localhost:3100/mcp\n");
        assert_eq!(
            yaml_list_with("model: gpt\n", "mcp-servers", url),
            "model: gpt\nmcp-servers:\n  - http://localhost:3100/mcp\n"
        );
        assert_eq!(
            yaml_list_with("mcp-servers:\n  - other\n", "mcp-servers", url),
            "mcp-servers:\n  - http://localhost:3100/mcp\n  - other\n"
        );
        let already = "mcp-servers:\n  - http://localhost:3100/mcp\n";
        assert_eq!(yaml_list_with(already, "mcp-servers", url), already);
    }
}
