# QuantSuite server and client — one QuantSuite, used from every PC

Status: plan, 2026-11-03; card 1 (spike) and card 2 (server mode) done 2026-11-04, card 3 (pairing, mutual TLS) 2026-11-05 — see §6.3 and §6.3.1–6.3.6. Cards "QuantSuite Server 1/7" to "7/7" on the QuantSuite board. Moving the data onto the server is [PLAN-MIGRATION.md](PLAN-MIGRATION.md).

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

**Decision (card Server 1/7, 2026-11-04): B.** `crates/qs-core/tests/remote_dispatch_spike.rs` performs calls through `Webview::on_message` with the app's own invoke key (`AppHandle::invoke_key()`, public in Tauri 2.11.5) and a local origin URL, so the ACL resolves exactly as for the UI's own calls. No Tauri patch. Measured on Tauri's mock runtime (the IPC path itself, without WebView2), 2,000 calls each:

| Call (async commands, as every suite command must be) | Option B per call |
|---|---|
| read (a list from managed state) | 0.22 ms |
| write (an insert into managed state) | 0.18 ms |
| streaming (10 events to listeners, then the result) | 0.24 ms |

Most of that is the async hand-off to Tauri's runtime: the same calls as synchronous commands took 12 µs, 12 µs and 59 µs. Either way it is far below one network round trip.

- A wrong invoke key is dropped without an answer (the key never leaves the server). An unknown command answers with an error.
- Plugin commands need the ACL to resolve. The server dispatches as its main window's webview, which carries the app's capabilities. A second, restricted capability for remote devices is a card 3 option, not a need.
- Memory: B needs the server's main webview to exist (hidden in server mode). A needs the same webview plus a JS hop per call, so B costs no more.
- A was not measured: it needs a live WebView2 window, and the app window is the user's. By design it adds two WebView2 IPC crossings and a JS event round trip on top of exactly the path measured above.
- C, costed: a Tauri-free service layer for 652 commands means touching every command body and its state access (each plugin's `State<...>` and `AppHandle` use). Estimate: 4 to 6 weeks of refactoring across 15 crates. Only worth it for a Linux or Docker server.

### 6.3.1 The remote manifest, built (card Server 1/7)

Every `module.json` has a `remote` block, and the core plugin (`qs`), the app's own commands and the shared Tauri plugins have `crates/qs-core/remote.json`. Each command is `"name": "<server|client|deny>:<read|compute|write|external>"`. `scripts/validate-modules.mjs` (via `scripts/remote-inventory.mjs`) fails when a registered command (its plugin's `build.rs` list) has no entry, an entry is not of that form, or an entry names no command. `node scripts/remote-inventory.mjs --init` adds a first classification for new commands, which must be reviewed. `--table` prints:

| Plugin | Commands | server | client | deny | read | compute | write | external |
|---|---|---|---|---|---|---|---|---|
| pilot | 186 | 0 | 186 | 0 | 73 | 6 | 55 | 52 |
| algo | 81 | 80 | 0 | 1 | 43 | 10 | 21 | 7 |
| mcp | 71 | 0 | 71 | 0 | 35 | 1 | 32 | 3 |
| canvas | 47 | 0 | 47 | 0 | 23 | 1 | 15 | 8 |
| hud | 47 | 0 | 47 | 0 | 23 | 1 | 18 | 5 |
| memory | 38 | 0 | 38 | 0 | 22 | 1 | 15 | 0 |
| core | 37 | 3 | 34 | 0 | 15 | 2 | 20 | 0 |
| script | 29 | 28 | 0 | 1 | 12 | 3 | 10 | 4 |
| notes | 26 | 26 | 0 | 0 | 8 | 0 | 18 | 0 |
| finance | 20 | 20 | 0 | 0 | 7 | 0 | 13 | 0 |
| plan | 20 | 20 | 0 | 0 | 8 | 0 | 12 | 0 |
| systems | 18 | 17 | 1 | 0 | 7 | 2 | 9 | 0 |
| console | 11 | 0 | 11 | 0 | 3 | 0 | 7 | 1 |
| habit | 11 | 11 | 0 | 0 | 4 | 0 | 7 | 0 |
| terminal | 10 | 10 | 0 | 0 | 10 | 0 | 0 | 0 |
| **all** | **652** | 215 | 435 | 2 | 293 | 27 | 252 | 80 |

- **server**: the trading and personal-data modules (§3), plus the bundle export (the data root is the server's).
- **client**: QuantPilot, QuantCode/Canvas, console, QuantMCP, memory, HUD, the core's settings, entities, windows, autostart and agent broker, the app's update commands, and the shared Tauri plugins (dialog, fs, shell, clipboard, global shortcut, autostart, updater, window/webview/event/app). These act on the PC you sit at.
- **deny**: `algo/wallet_export_key` (secrets never leave the server) and `script/open_scripts_folder` (the server's explorer). The remote file browser and downloads (card 4) replace dialogs and folders.
- **external** on the server (server PIN, card 3): arming Manual execution, starting a live bot, closing positions, the kill switch, adding, changing or deleting an exchange key, the collection's GitHub token, and installing research or collection items.
- The side-effect classes of client commands are informational: they run locally as today.

### 6.3.2 Events a server-placed module emits (card Server 1/7)

| Module | Tauri events (manifest `remote.events`) | Bus topics (`qs://event`) |
|---|---|---|
| QuantAlgo | `bot:log` (emitted to the window `main`), `bot:trade`, `bot:heartbeat`, `bot:paused`, `algo:kill-switch`, `algo:manual-run` | `algo.bot.started`, `algo.bot.stopped`, `algo.trade.executed`, `algo.backtest.completed` |
| QuantSystems | `engine:status`, `eval:progress`, `backtest:complete`, `cache:updated` | `systems.engine.status`, `systems.backtest.completed`, `systems.signal.generated` |
| QuantScript | none | `script.file.saved`, `script.file.created`, `script.file.deleted`, `script.collection.changed` |
| QuantTerminal | none | `market.symbol.selected`, `market.data.refreshed` |
| Finance, Notes, Plan, Habit | none | `finance.plan.changed`, `notes.note.created`, `notes.note.updated`, `plan.event.created`, `plan.event.updated`, `habit.habit.changed` |

How they reach a client (card 4): the gateway listens with `listen_any` to every event in the manifests' `remote.events` and to the bus channel `qs://event`. An event emitted to a window label (`bot:log` to `main`) is still heard. Each event goes out as an `event` frame with a sequence number. The client re-emits it locally under the same name, so module code keeps its `listen` calls. Bus topics of client-placed modules stay local. A client module subscribing to a server topic (QuantPilot listening to `notes.note.updated`) gets it through the bridge.

### 6.3.3 Data split (card Server 1/7)

| `core.db` part | Server | Client | How the client sees both |
|---|---|---|---|
| `settings` | Keys of server modules and the shared data plane (`notes/sections`, `todos/sections`: the quick notes and to-dos of the drawer and the HUD are personal data) | Appearance, window layout, HUD placement, the open workspace, connection profiles | Routed by key prefix (card 5) |
| `entities`, `links` | Entities of server modules (notes, events, finance items, habits, systems, bots) | Workspaces (`module: core, kind: workspace`) and the developer modules' entities | Entity commands route by their `module` argument. The command palette queries both and merges by score (card 5) |
| `event_log` (bus history) | Its own | Its own | Server events forwarded by the gateway enter the client's log |
| `secrets` / the vault | Exchange keys, wallets, API wallets (trading keys stay home) | The device key (card 3) | Never synced |
| Workspace registry, kanban, code index, memory vaults | none | Per PC (phase 1) | Card 7 decides |

### 6.3.4 Risks for cards 2 to 7 (card Server 1/7)

1. **Hidden main webview.** B dispatches through the main window's webview. Server mode (card 2) must keep it alive while hidden, and a close request must hide it, not destroy it.
2. **Commands that take a `Window` or `Webview` argument** act on the server's hidden window (e.g. positioning). They are placed client-side today; new ones must be too.
3. **Paths in arguments and results.** Some server commands return or accept local paths (strategy files, backtest exports, the bundle). Card 4's files layer must translate them, or the client shows a server path it cannot open.
4. **Streaming via `Channel`.** Commands that answer through a Tauri `Channel` (rather than events) need channel forwarding in the gateway. None of the server-placed commands uses one today; the validator could flag new ones.
5. **Bus echo.** Frontends call `emit_event` themselves. With a bridge, a server topic re-emitted on the client must not be sent back (loop guard by origin id).
6. **Mixed modules.** QuantAlgo has secrets (`wallet_export_key`, denied) next to ordinary commands. New commands default to the module's placement: a reviewer must check what `--init` guessed.
7. **Settings by prefix** needs one table of prefixes kept in sync with the modules; a wrong prefix silently writes to the wrong machine.
8. **One trader.** A client still has the trading plugins compiled. Card 2's guard must stop executors on a connected client, not only hide the UI.
9. **Version skew.** The manifest is part of the protocol. A client and server with different manifests must refuse to connect (card 4's `hello` carries a manifest hash).

### 6.3.5 Built: server mode (card Server 2/7, 2026-11-04)

- **Role** (`qs_core::server`): `local` (default), `server` or `client`, stored as the core setting `server.role` and read at setup before any module's setup. `--server` makes a launch a server whatever the setting says. Settings → Server → "Run as server" switches between local and server (from the next start). A PC becomes a client only by connecting to a server (card 5).
- **Tray only**: a server launch stays in the tray (`main` is created hidden and kept; closing it hides it, as before). The window opens from the tray icon.
- **Start at boot**: the existing "Start QuantSuite with Windows" entry (HKCU Run) under a dedicated, automatically signed-in Windows user.
- **Watchdog**: Settings → Server → "Restart QuantSuite when it exits" creates the Task Scheduler entry *QuantSuite Server Watchdog*. It runs `QuantSuite.exe --server --watchdog` every 5 minutes in the user's session (`/IT`). While the suite runs, the single-instance guard swallows that relaunch without showing the window. After a crash or a kill, it starts the suite again in the tray.
- **One trader**: `qs_core::server::executors_allowed()` refuses on a client. QuantAlgo checks it when a bot starts, when a Manual system is armed, in every nightly attempt, and at startup: no scheduler, no funding reader, no bot resumes on a client. Tested in qs-core.
- **Health** (Settings → Server): role, version, up since, free disk space for the data root, autostart, watchdog, gateway (none until card 4). From QuantAlgo: whether trading is allowed or the kill switch is on, every bot's status, and the newest nightly run of every Manual system (`server_trading_health`).
- The Python engines and the Forge run from Rust and keep working without the window.

### 6.3.6 Built: pairing and mutual TLS (card Server 3/7, 2026-11-05)

The Tauri-free crate `crates/qs-gateway` holds the security; `crates/qs-core/src/gateway.rs` hosts it in the app.

| Piece | How |
|---|---|
| Keys | Ed25519. The server's key is the vault entry `gateway.server-identity`, each PC's own `gateway.device-identity` — made on first use, never in a command result. The TLS certificate is made from the key at each start and self-signed: what is trusted is the **key**. A fingerprint is SHA-256 of the public key in 16 groups of four hex digits. |
| TLS | rustls on ring, TLS 1.3 only, Ed25519 only, no session resumption, no early data: every connection is a full handshake with fresh keys. The client pins the server's key (a different key fails the handshake: "the server's key is not the one this PC paired with"). The server requires a client certificate and checks the handshake signature, then looks the key up in `gateway.db`. |
| Pairing | Settings → Server → Pair a device shows an 8-digit code (OS randomness, valid 5 minutes, used once) and a QR (`quantsuite://pair?addr=…&code=…&key=…`, for a phone app later). 5 wrong codes lock pairing for 15 minutes, new codes included. The PC enters address and code; both screens show four BIP-39 words from SHA-256(`quantsuite-pairing-v1` ‖ server key ‖ device key), 44 bits. The **server's user** allows the device and picks its role; the PC pins the server key only after **its** user confirmed the same words and the server allowed. Either side saying "different" ends it and leaves nothing behind. |
| Roles | Per device, checked on every call against the remote manifest: |
| | Full — read, compute, write; `external` after the server PIN |
| | No trading — read, compute, write; never `external` |
| | Read-only — read and compute |
| | `client` and `deny` commands and anything unlisted are refused for every role. A role change applies to the next call. |
| Manifest | `crates/qs-gateway/remote-manifest.json`, generated by `node scripts/remote-inventory.mjs --write` from every `module.json` and `crates/qs-core/remote.json`, keyed by the invoke name (`plugin:algo|start_bot`). The validator fails when it is stale. Its hash goes into `hello`. |
| Step-up | The server PIN (6–12 digits, set on the server's screen only) is kept as an Argon2id hash in `gateway.db`. A right PIN is valid 10 minutes for that device; 5 wrong PINs lock the device for 15 minutes; a new PIN ends every grant. |
| Audit | `gateway.db` → `audit`: every call with its outcome (`ok`, `error`, `denied`, `step_up_required`), connects and refusals, pairing (`paired`, `rejected`, `failed`), step-ups (never the PIN) and the server screen's actions (role, revoke, PIN). The newest 20,000 rows are kept. Settings → Server → Devices → Show the audit log. |
| Revocation | Revoke closes the device's sockets at once (no goodbye) and ends its PIN grant; it is refused from then on and must pair again. |
| Versions | `hello` carries the protocol (1) and the manifest hash. Older client → "Update this PC"; newer → "Update the server"; same protocol, other manifest → "Install the same version on both". |
| Listening | Settings → Server → Where the server listens: the PC's addresses, tailnet (100.64.0.0/10) first, then the home network; port 47100 by default. `0.0.0.0` only with the explicit tick and a warning; port 3100 (QuantMCP) is refused. Restarted at every start of a server. Health shows "listening on …". |
| Dispatch | An allowed call runs through the main webview's `on_message` with the app's invoke key and the webview's own URL as origin (§6.3, option B), on a blocking thread. |
| Client side | Settings → Server → Connect to a server (shown when this PC is not a server): pair, then each saved connection (core setting `server.connections`: address, the server's public key, this PC's role) can be tested — it connects and reads the server's health through the gateway — or forgotten. Card 5 routes the UI through the connection. |
| Data | `gateway.db` (+ `-wal`, `-shm`) at the data root, class *must* in the migration inventory. |

Commands (core plugin `qs`) and their placement:

| Command | Remote manifest | What |
|---|---|---|
| `gateway_status`, `gateway_configure` | client | Where the server listens; its key, PIN state, this PC's addresses |
| `gateway_pairing_start`, `_status`, `_confirm`, `_cancel` | client | Pairing — on the server's own screen only |
| `gateway_set_pin` | client | The server PIN — on the server's own screen only |
| `gateway_devices`, `gateway_audit` | server:read | A paired PC sees the devices and the log |
| `gateway_device_rename`, `gateway_device_forget` | server:write | |
| `gateway_device_set_role`, `gateway_device_revoke` | server:external | From another PC with the server PIN ("laptop lost: revoke it from the desktop") |
| `gateway_pair_begin`, `_finish`, `gateway_connections`, `gateway_connection_test`, `_forget` | client | This PC's connections to servers |

`server_status` is now `server:read`, so every client can show the server's health.

Tests (`cargo test -p qs-gateway`): a wrong server key is refused; unpaired devices may only pair; a revoked device is dropped at once and refused; read-only and no-trading devices are refused what their role forbids; unknown, `client` and `deny` commands are refused; 5 wrong codes lock (the right code and new codes too); an expired code fails; a recorded handshake replayed to the server does not authenticate; the four words differ when either key is swapped; external calls need the PIN and a new PIN ends the grant; versions must match; no private key, seed, PIN or PIN hash appears in anything a device or the screen receives.

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
| Dispatch on the server | B: `Webview::on_message` with the app's invoke key (card 1, about 0.2 ms per async call on the mock runtime) | No Tauri patch, no JS hop; A needs the same hidden webview plus two WebView2 crossings |
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
