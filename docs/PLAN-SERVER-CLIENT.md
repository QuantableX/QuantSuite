# QuantSuite server and client — one QuantSuite, used from every PC

Status: plan, 2026-11-03. Nothing here is built yet. Cards "QuantSuite Server 1/7" to "7/7" on the QuantSuite board. Moving the data onto the server is [PLAN-MIGRATION.md](PLAN-MIGRATION.md).

## In short

- **The server** is a machine at home that runs QuantSuite day and night: the bots, the nightly Manual trades ([PLAN-AUTO-EXECUTION.md](PLAN-AUTO-EXECUTION.md)) and all the data.
- **Clients** are your PCs and laptops. They open QuantSuite in client mode and work on the server's data, at home and over the internet.
- **Nothing is opened to the internet.** The connection runs through a private encrypted network (Tailscale, built on WireGuard), and on top of that QuantSuite's own device pairing. Both locks are needed; either alone is not enough to get in.
- **Trading keys stay on the server.** A client can enter a key, never read it back.
- **Exactly one machine trades:** the server. A client connected to it never runs bots or nightly runs itself.
- **Phases:** first the server mode and the trading and personal-data modules (QuantAlgo, QuantSystems, QuantTerminal, QuantScript, Finance, Notes, Plan, Habit, Flow). The developer modules (QuantPilot, QuantCode, QuantCanvas, console, QuantMCP, memory) stay local on each PC in phase 1, because they work on that PC's folders and CLIs; card 7 decides their remote story.

## 1. What exists today (from the code)

| Fact | Where | Why it matters |
|---|---|---|
| One Tauri 2 desktop app: Nuxt frontend in WebView2, 16 modules as Rust plugins, Python sidecars | `apps/`, `modules/`, `sidecars/` | The UI and the backend are already separated by IPC. |
| The frontend reaches the backend only through Tauri IPC: 602 `#[tauri::command]`s (pilot 174, mcp 71, algo 60, hud 48, memory 38, canvas 31, script 29, notes 26, plan 20, finance 20, systems 18, console 12, habit 11, terminal 10), called from 125 frontend files | `modules/*/crate`, `packages/core` | Every call passes one seam: `window.__TAURI_INTERNALS__.invoke` (the browser QA stub already replaces it). |
| QuantMCP serves MCP over HTTP on `127.0.0.1:3100`, without auth, loopback only | `modules/mcp/crate/src/mcp_server.rs` | Fine locally; it must never be exposed. |
| For `quantsuite.*` tools the agent broker applies the approval gate and lets the shell perform the invoke | `crates/qs-core/src/agent.rs`, `crates/qs-mcp-bridge` | A non-UI caller already drives module commands through the shell — one candidate for the server's dispatcher. |
| Tauri 2.11.5 has a public `Webview::on_message(InvokeRequest, responder)` | tauri `src/webview/mod.rs` | The other candidate: dispatch from Rust. It checks the invoke key and the ACL origin. |
| All data under `~/.quantsuite` | `crates/qs-core/src/paths.rs` | The server owns one data root. |
| Running bots are marked stopped on restart | `modules/algo/crate/src/lib.rs` | Unattended running needs Automation card 4. |

## 2. The target picture

```
 Home server (24/7)                              Your PCs, laptop
 ┌─────────────────────────────────┐             ┌─────────────────────────────┐
 │ QuantSuite — server mode        │             │ QuantSuite — client mode    │
 │  all data (~/.quantsuite)       │   tailnet   │  window, HUD, clipboard     │
 │  bots, nightly Manual runs      │◄───────────►│  developer modules (local)  │
 │  Python engines, vault, keys    │  TLS 1.3 +  │  no trading, no keys        │
 │  gateway on the tailnet address │  paired key │  a cache, never the truth   │
 └─────────────────────────────────┘             └─────────────────────────────┘
```

## 3. What runs where in phase 1

| Module | Runs on | Note |
|---|---|---|
| QuantAlgo, QuantSystems | Server | Data, bots, nightly runs, exchange keys |
| QuantTerminal, QuantScript (Forge jobs) | Server | Heavy research can still run on a strong PC as a separate local profile |
| Finance, Notes, Plan, Habit, Flow | Server | Personal data, one truth |
| QuantPilot, QuantCode, QuantCanvas, console, QuantMCP (kanban, index, worktrees), memory | Client (local) | They work on the folders and agent CLIs of the PC they run on; card 7 |
| HUD, voice, window, tray, clipboard, file dialogs, global shortcuts | Client | Hardware and screen of the PC you sit at |
| Appearance, window layout | Client | Per PC |
| Module settings | With their module | |

## 4. Security

### 4.1 Layers

| # | Layer | What it does |
|---|---|---|
| 1 | Private network | The server is reachable only inside your tailnet (Tailscale/WireGuard, end-to-end encrypted) or your LAN. The gateway binds to that address; `0.0.0.0` only by explicit choice; no router port forwarding. |
| 2 | TLS 1.3 | `rustls`. The server's certificate is self-signed and pinned on the client at pairing. |
| 3 | Device keys | Each client has its own Ed25519 key in its vault. Every connection is mutual TLS with that key. |
| 4 | Pairing | The server shows a one-time 8-digit code (and a QR), valid 5 minutes, 5 attempts then a 15-minute lock. Both screens then show the same four words derived from both keys — you compare them, which defeats a man in the middle. |
| 5 | Roles | Per device: Full, No trading, Read-only. The server checks every call against the remote manifest (§6.2); anything not listed is denied. |
| 6 | Step-up | `external` actions from a remote device (arm live, close positions, start a live bot, change a key, remote shell) ask for the server PIN; it stays valid 10 minutes. |
| 7 | Secrets stay home | Keys are written to the server's vault and never sent to a client. |
| 8 | Audit | The server logs device, command, result and time; Settings → Server → Devices shows it. |
| 9 | Revocation | Removing a device drops its connections at once. |
| 10 | Versions | Client and server must speak the same protocol version; otherwise the client says which side to update and stays disconnected. |

QuantMCP's port 3100 stays loopback-only on every machine.

### 4.2 Why Tailscale

| Option | Verdict |
|---|---|
| **Tailscale** (WireGuard) | **Recommended.** Free Personal plan (6 users, unlimited personal devices since April 2026), end-to-end encrypted, no port forwarding, works behind any router. Tailscale's coordination server sees which devices exist, not the traffic. |
| Headscale | Self-hosted Tailscale control server — choose it if you want no third party at all. Same clients. |
| Plain WireGuard | Works; you manage keys, port forwarding on one UDP port and dynamic DNS yourself. |
| Router port forwarding to QuantSuite | Rejected: the whole internet can knock. |
| Cloudflare Tunnel | Rejected for this: TLS ends at Cloudflare, so a third party sits inside the trading connection. |

### 4.3 What if …

| Event | What protects you |
|---|---|
| Laptop stolen | Revoke it on any other device; it holds no keys and only a cache. |
| Tailscale account taken over | The attacker is in the network, but has no paired device key (layers 3–4). |
| Someone on your home Wi-Fi | Same: no pairing, no access. |
| Server stolen | BitLocker; the vault's master key sits in the OS keystore. |
| Pairing code guessed | 5 minutes, 5 attempts, then locked; and the four words would not match. |
| A client sends a command it may not | Denied by the manifest and the device's role, and logged. |

## 5. Reliability

- **Server mode** starts at boot. Recommended setup: a dedicated Windows user with automatic sign-in (BitLocker with TPM unlocks without a prompt), QuantSuite in the tray, sleep off, Windows Update restarts outside 23:00–02:00 UTC, Task Scheduler restarts QuantSuite if it exits.
- **Hardware:** an always-on small PC — 4+ cores, 16–32 GB RAM, SSD, wired network, a UPS (protects SQLite and open orders), BIOS "power on after power loss".
- **One trader:** only the server runs executors. A PC has either a local profile or a server connection; the two never trade at the same time.
- **Connection:** heartbeat every 10 s, reconnect with backoff. Events carry a sequence number; the server keeps recent events, and a reconnecting client resumes from its last number or reloads the page's state.
- **No silent repeats:** every changing call carries an idempotency key. After a timeout the client asks the server what happened instead of sending again.
- **Offline:** "Server not reachable since 12:03"; pages keep their last data greyed out; actions are disabled.
- **Health page** on the server and in every client: uptime, bots, last nightly run, disk space, version.

## 6. Technical design

### 6.1 One seam on the client

Client mode wraps `window.__TAURI_INTERNALS__.invoke`. A command the manifest places on the server goes to the gateway; a client command goes to Tauri as today. Event listening (`plugin:event|listen`) and the suite bus are bridged the same way. Module code does not change.

### 6.2 The remote manifest

Every `module.json` gets a `remote` section: per command `server`, `client` or `deny`, with its side-effect class (read / compute / write / external, as for MCP capabilities). `scripts/validate-modules.mjs` fails when a registered command has no entry, so a new command cannot slip through. Default: deny.

### 6.3 Dispatch on the server — the spike decides (card 1)

| Option | How | For | Against |
|---|---|---|---|
| A | The server's own shell (hidden window) performs the invoke — the path QuantMCP's agent broker uses today | Least new code, proven | Needs the hidden webview; one JS hop |
| B | Rust builds an `InvokeRequest` and calls `Webview::on_message` | No JS hop | Must satisfy the invoke key and ACL origin without patching Tauri |
| C | A Tauri-free service layer per module; Tauri commands and the gateway both call it | True headless daemon (Linux, Docker) | Large refactor of 602 commands |

The spike measures latency and memory of A and B and records the decision here. Going in: B if it works without patching Tauri, otherwise A. C only if a Linux or Docker server becomes a goal.

### 6.4 Gateway protocol

WebSocket over TLS. Frames: `hello` (protocol version, device), `call` {id, cmd, args, idempotencyKey}, `result` {id, ok | error}, `event` {seq, topic, payload}, `resume` {lastSeq}, `cancel` {id}, binary chunks for files. Limits: frame size, timeouts per side-effect class, a rate limit per device.

### 6.5 Files

A file dialog on the client cannot see the server's disk. Server paths get a remote file browser over the gateway; imports and exports get upload and download.

### 6.6 Server operating system

Phase 1: Windows — the same build, WebView2 is there, the Python sidecars are proven. Linux or Docker only with option C.

## 7. Worked example

**Setup (once):**
1. Install QuantSuite and Tailscale on the server. Settings → Server → "Run as server". It shows "Pair a device: 4821 9937 — 4:59".
2. Install Tailscale on the PC. QuantSuite → Settings → Connections → Add server → `homeserver` → enter the code.
3. Both screens show "harbor · velvet · seven · lamp". They match → confirm. The PC is "Full".

**Every day:** at 00:15 UTC the server trades. At 08:00 you open QuantSuite on the PC; it connects and shows the night's runs. In a café the laptop does the same through Tailscale.

**Laptop lost:** on the PC, Settings → Server → Devices → "Laptop" → Revoke.

## 8. Cards

| Card | What | Blocked by |
|---|---|---|
| QuantSuite Server 1/7 | Spike: dispatch path, command inventory, remote manifest, ADR | — |
| QuantSuite Server 2/7 | Server mode: no window, autostart, watchdog, health, one trader | 1 |
| QuantSuite Server 3/7 | Pairing, mutual TLS, roles, step-up, audit, revocation | 1, Automation 1 |
| QuantSuite Server 4/7 | Gateway: calls, events with resume, idempotency, files | 2, 3 |
| QuantSuite Server 5/7 | Client mode: connection profiles, the seam, offline behaviour | 4 |
| QuantSuite Server 6/7 | Over the internet: tailnet binding, setup guide, test from outside | 5 |
| QuantSuite Server 7/7 | Developer modules over the server (decision + implementation) | 5 |

## 9. Decisions taken (change them on the card if you disagree)

| Decision | Default | Why |
|---|---|---|
| Internet path | Tailscale; Headscale as the self-hosted alternative | No open port, end-to-end encryption, free for personal use |
| Server OS | Windows | Same build and sidecars; Linux needs the service-layer refactor |
| Phase-1 modules on the server | Trading + personal data | The reason for a server is 24/7 trading and one truth for your data |
| Developer modules | Local per PC in phase 1 | They work on the PC's own folders and agent CLIs |
| Auth | Paired device keys + mutual TLS + server PIN for external actions | Passwords alone are guessable; a key on a revocable device is not |

## 10. Options — not planned unless you ask

- Phone access (an app or a browser client) — a browser client means serving the UI over HTTPS, more surface.
- Push notifications to the phone from the server.
- Scheduled backups on the server with the migration exporter ([PLAN-MIGRATION.md](PLAN-MIGRATION.md)).

## Sources (checked 2026-11-03)

- Tailscale free plan: https://www.ssdnodes.com/learn/tailscale-pricing-explained , https://agentdeals.dev/vendor/tailscale
- Headscale: https://github.com/juanfont/headscale
- Tauri `Webview::on_message`: tauri 2.11.5 `src/webview/mod.rs` (local cargo registry)
