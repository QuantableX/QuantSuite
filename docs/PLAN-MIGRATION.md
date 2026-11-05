# Moving QuantSuite to another machine

Status: plan, 2026-11-03. Nothing here is built yet. Cards "QuantSuite Migration 1/4" to "4/4" on the QuantSuite board. The target machine is usually the home server of [PLAN-SERVER-CLIENT.md](PLAN-SERVER-CLIENT.md).

## In short

- **Export:** one button packs everything into one encrypted file (a `.qsbundle`). **Import:** on the new machine one button unpacks it and checks every piece.
- **Included:** all databases and settings — bots, trades, strategies, systems and Ranks, scripts, notes, finance, plans, habits, kanban boards, memories, QuantPilot threads, each workspace's private data — and the API and wallet keys, locked with a passphrase you choose.
- **Not included by default:** the agents' scratch files (`artifacts/`, over 250 GB on this PC), logs, the project folders themselves (they are git repositories), browser logins and the logins of the Claude/Codex command-line tools. The import report lists every one with what to do.
- **No double trading:** everything arrives switched off on the new machine; you arm trading there yourself. The old PC stops trading after the move. Nothing is ever deleted on the old PC.
- **Later, with the server:** "Move to server" sends the bundle straight over the paired connection, and this PC becomes a client.
- **Two problems to fix first:** today's exchange keys can only be decrypted on this PC under this username and folder, and several files store absolute paths like `C:\Projects\…`. Card 1 makes both portable.

## 1. What has to move (from the disk, 2026-11-03)

Data root: `~/.quantsuite` (release) — `qs_core::paths` is the single source of its paths.

Measured 2026-11-03: the data root holds over 200 GB, and almost all of it is `workspaces/*/artifacts/` (LCES tournament runs 97 GB, a Rust build cache 63 GB, game QA runs, venvs, release builds). Everything else is about 11 GB, of which 5.6 GB is per-machine (Python environments, Whisper and embedding models). A bundle with the defaults below is about 5.5 GB before compression; `modules/script/research/` (3.9 GB) is the largest part.

| What | Where | Class |
|---|---|---|
| Suite database: entities, settings, workspace registry, event log | `core.db` (+ `-wal`, `-shm`) | Must |
| QuantAlgo: bots, trades, exchanges (keys), strategies, backtests, settings | `modules/algo/` (`quantalgo.db`, `strategies/`, `backtests/`, `config.json`) | Must; `logs/` optional |
| QuantSystems: systems, Ranks, research | `modules/systems/` (`config.json`, `research/`) | Must |
| QuantSystems candle and ranking cache | `modules/systems/cache/` | Optional, on by default (rebuilding takes hours and hits rate limits) |
| Shared candle archive | `market-data/candles.sqlite` (created on first use) | Optional, on by default |
| QuantScript | `modules/script/` (`script.db`, `collection/`, `research/`, 3.9 GB) | Must |
| Notes | `modules/notes/` (`notes.db`, `quantzen.db`, `assets/`, `config.json`, `workspaces.json`) | Must; `backups/`, `exports/` optional |
| Finance, Plan, Habit, Terminal, console, control | `modules/<id>/` | Must |
| HUD and dictation settings | `modules/hud/` (`config.json`) | Must; `pyenv/` per machine (2.2 GB); `models/` off by default (Whisper models, 2.1 GB, downloaded again on first use) |
| QuantPilot threads and settings | `modules/pilot/` (`pilot.db`, `sessions/`, `settings.json`) | Must |
| QuantMCP: kanban boards, agent runs, server list | `modules/mcp/` (`kanban.db`, `agent-runs.db`, `mcps.json`) | Must; `indexes/` optional (rebuildable); `*.legacy*`, `*.bak` never |
| Memory vaults | `modules/memory/` (`memory.db`, `vault/`, `workspaces/`) | Must; `engine/` per machine (1.3 GB) |
| QuantCanvas | `modules/canvas/` (`workspaces.json`, `browser-data.json`) | Must; `bin/` per machine |
| Per-workspace private data | `workspaces/<id>/` | Must (small); `artifacts/` optional, off by default — over 250 GB here, nearly all re-creatable agent scratch; ticked per workspace or per subfolder. Worth ticking: the small folders that hold kept tools and revert points (`indicator-versions/`, `aggregate-playbook/`, `qa-native/`, `quantmcp-http-client/`, under 600 MB together) |
| Logs | `logs/` | Optional, off by default |
| Secrets | the vault ([PLAN-AUTO-EXECUTION.md](PLAN-AUTO-EXECUTION.md) §2.1) | Must, re-encrypted |
| Release signing key | `signing/quantsuite.key` | Never by default; opt-in only if releases are built on the new machine |
| Old backups | `core.db.bak-*` | Never |

Outside the data root:

| What | Where | Class |
|---|---|---|
| Global AgentOS, QuantMCP registry | `~/.quantmcp/` (`AGENT.md`, `CONCEPT.md`, `MEMORY.md`, `registry.json`) | Must; `indexes/` optional |
| Claude Code's AgentOS link | `~/.claude/rules/AGENT.md` | Re-created by AgentOS import, not copied |
| Private indicator library | the QuantScript-Collection-Private folder (`paths::indicators_dir`, `QUANTSCRIPT_INDICATORS_DIR`) | Opt-in copy, or clone it; the path is remapped |
| Project folders | `C:\Projects\…` (workspaces) | Not copied by default (git); opt-in per workspace with its size |
| Dev builds' data | `~/.quantsuite-dev/` | Never |
| WebView2 profile (QuantCanvas browser logins, cookies) | the app's WebView2 folder | Never — bound to the machine; log in again |
| Python environments, sidecar packages | venvs | Never — the new machine sets them up |
| Claude/Codex CLI logins, other apps' MCP configs | `~/.claude`, `~/.codex`, … | Not QuantSuite's; steps in the import report |

### 1.1 Portability problems found

1. **Exchange keys cannot leave this PC.** `exchanges.rs` derives the encryption key from the Windows username and the data folder. Another machine, user or folder cannot decrypt them. Fixed by the vault (Automation 1) plus re-encryption under the bundle passphrase.
2. **Absolute paths** are stored in at least `core.db` (workspace registry), `modules/canvas/workspaces.json`, `modules/notes/workspaces.json`, `modules/hud/config.json`, `modules/algo/config.json`, `workspaces/<id>/canvas/canvas.json`, and probably in SQLite rows (kanban worktree paths, QuantPilot thread folders, memory vault paths). Card 1 finds every one and makes it remappable.
3. **Workspace ids:** boards, vaults and `workspaces/<id>/` key off the registry id. Remapping a folder must keep its id; card 1 proves it with a test.
4. **Live databases:** they run in WAL mode, so copying files while the app runs can produce a torn copy. The export uses SQLite's online backup API.

### 1.2 Built (card 1, 2026-11-04)

`crates/qs-core/src/inventory.rs`:

- `RULES` turn the first table above into code: one rule per pattern under the data root, the most specific wins. A test fails when a module of `modules/*/module.json` or a top-level root has no rule. `EXTERNAL` lists the roots outside the data root.
- `dry_run(root)` walks a data root read-only, with names and sizes only, and totals it per rule. Files no rule covers are listed; the export (card 2) refuses them until they are classified.
- `PathMap` is the one remapping function: old prefix → new prefix, with the longest prefix first. It accepts any separator (`/`, `\`, or the JSON-escaped `\`) and any case, and matches only on whole path segments, so `C:/Projects/Quant` never matches `C:/Projects/QuantSuite`. The rest of a path takes the new prefix's separator. `remap_json` / `remap_json_file` rewrite every string of a JSON store. `remap_sqlite` rewrites every TEXT cell of every table, so a store nobody listed is covered too. `find_paths` lists every absolute path with its store, location and remap result.
- Workspace ids: the registry keeps its ids, and a remap changes only the path inside the entity. `workspaces::storage_id` returns the registered id for a folder before it hashes a path. `paths::workspace_dir`, QuantMCP's `ensure_registered_workspace` and the kanban drawer use it, so a moved folder keeps its board, memory vault and `workspaces/<id>/` folder.
- Secrets: QuantScript's Collection tokens sat in the OS credential store, bound to the machine. They now live in the vault (`script/collection/<source>`); a token still in the credential store moves into the vault the first time it is read. No secret is left outside the vault.
- Dry run on the user's data (2026-11-04, `artifacts/migration-dryrun-20261104/`): the live root walked read-only with 0 unclassified files. Must-move is 17.1 GB (QuantScript's research runs and the systems research are "must"), optional-on 0.2 GB. In a copy of the 30 JSON and SQLite stores there are 2,523 absolute paths in 13 stores. The largest are console history (1,505), memory notes (588) and kanban cards and worktrees (352); the rest is core.db 35, canvas 21 + 1, hud 7, algo 2 + 1, notes 2 + 1, pilot 1, plus legacy MCP projects. The sample map covers all of them except 645 hits inside memory note prose (other drives, `~/` notation), which are text, not stored paths.

## 2. The design

### 2.1 The bundle

- One file, `<machine>-<date>.qsbundle`: tar + zstd, encrypted as a whole (AEAD in streaming chunks, key from the passphrase with Argon2id). It holds finances, notes and keys, so nothing in it is readable without the passphrase.
- Inside: `manifest.json` (suite version, schema version per database, source machine and data root, creation time; per file: root, relative path, size, SHA-256, class), the data by root (`quantsuite/`, `quantmcp/`, `external/<name>/`), the secrets re-encrypted under the bundle key, and the list of what was left out and why.
- Databases are copied with SQLite's backup API — a consistent snapshot while the app runs.
- Before export: the size of each optional part, to tick or untick. The defaults follow the Class column, so `artifacts/`, logs and the Whisper models start unticked; the wizard shows the total before it starts.
- The export refuses while live bots or live Manual execution are armed and offers to disarm them (disarming never sells).
- **The passphrase cannot be recovered.** Lost passphrase = unreadable bundle. The wizard says so and asks for it twice.

**Built (card 2, 2026-11-04).** `crates/qs-core/src/bundle.rs`, Settings → Data, and the agent tool `export_everything` (`plugin:mcp|export_everything`, external):

- File format: `QSBUNDLE1
`, then a JSON header (Argon2id parameters and salt, nonce prefix, chunk size), then 1 MiB chunks of AES-256-GCM. Each chunk is authenticated with the STREAM nonce (prefix, counter, last-chunk flag) and the header as associated data. A changed, reordered, dropped or cut chunk fails, and so does a wrong passphrase. Inside: tar + zstd (level 3).
- Content: every file the inventory moves (`quantsuite/<path>`, defaults per rule; Settings → Data ticks optional parts). Databases are copied via SQLite's online backup, and `user_version` is recorded as the schema version. `secrets.json` holds the vault's secrets re-encrypted under a key derived from the bundle key, with the secret id as associated data; they exist in plaintext only in memory. `manifest.json` comes last: suite version, machine, source root, per file size, SHA-256, class and schema version, the left-out patterns with reasons, and the secret ids.
- Folders that are left out are not walked (artifacts can be hundreds of GB). WAL/SHM files and `secrets/` are never copied as they are.
- Refusals: an unclassified file (named), live trading armed (live bots running or Manual systems armed; "Stop all trading" first, which never sells), not enough free space (the total plus 10 %), and a passphrase under 12 characters. A cancelled or failed export leaves no file: it writes `.part`, then renames.
- `open(bundle, passphrase, unpack_to)` verifies the passphrase, every chunk and every checksum, and optionally unpacks into a staging folder. Import (card 3) builds on it.
- Checks: 4 unit tests (round trip with the manifest following the inventory; wrong passphrase, tampered and cut bundles; refusals; a database written to during the export comes out as a consistent snapshot). Profile export of the copy of the user's stores: 29 files, 32.6 MB, of which 14 databases came in through the backup API; the bundle is 13.5 MB and was written in 7.8 s, then opened and verified in 7.0 s.

### 2.2 Import

1. **Check:** passphrase, manifest, every checksum, version (same or newer QuantSuite; an older one refuses), free disk space.
2. **Paths:** a table old → new for every workspace, the indicator library and other external folders. A missing folder can be chosen, cloned from its git remote, or left unlinked.
3. **Target:** an empty data root, or "replace" — then the existing data is first saved into a bundle of its own.
4. **Write:** into a staging folder, then one switch; the normal schema migrations run on first start.
5. **Secrets:** into the new machine's vault.
6. **Verify:** `PRAGMA integrity_check` on every database, row counts per table against the manifest, every secret decrypts, every mapped path exists, each module's own check (systems config loads, bots list, notes open). The report is saved.
7. **Disarmed:** bots stopped, Manual execution not armed (its settings kept). A banner "Arm trading on this machine" is your step.
8. **Old PC:** you confirm the move there (the import shows a short code to enter). It disarms its executors and shows "Moved to <machine> on <date>"; trading there can only be re-enabled explicitly. Nothing is deleted.

**Built (card 3, 2026-11-04).** `crates/qs-core/src/import.rs`, Settings → Data ("Import from a QuantSuite bundle", "This installation moved to another machine"), and the Trading overview notices:

- Check: one full pass decrypts and verifies the bundle, then the version is compared (a bundle from a newer QuantSuite refuses) and the free space checked. "Check the bundle" stages it once to read its workspace registry and proposes the path table: the old data root becomes this one, and each workspace maps to itself when the folder exists here; otherwise you choose its folder, or leave it empty to link later.
- Write: everything goes into `.quantsuite-import-<time>/` beside the data root. Every stored path is rewritten there through the one `PathMap` (the JSON stores, every TEXT cell of every database), leaving full-text index tables alone because triggers keep them in step. Secrets go into a new vault in the staged root, under this machine's keystore.
- Verify: `integrity_check` on every database, rows per table against the manifest (the export now records them), every secret decrypts, every mapped folder exists, and each module's check (QuantAlgo bots table, notes, kanban, pilot, systems config). The report is saved in `migration/import-report-<time>.json` inside the staged root. A failed check never schedules the switch.
- Disarmed after the check: bots stopped, Manual execution not armed (its settings kept), and an `imported` notice on the Trading overview.
- Switch: the next start, before anything opens the data root (first line of qs-core's setup), renames the current root to `<root>.before-import-<time>`, which is kept and never deleted, and moves the staged root into place. A failure puts the old root back and records the reason. Deviation from §2.2 step 3: "replace" keeps the old root as that folder instead of writing a second bundle, so no passphrase is needed and nothing is lost.
- Move code: both machines derive the same eight characters from the bundle key. The exporting machine keeps only its hash, under `core / migration.exports`. Entered there under Settings → Data (`trading_confirm_move`), the code runs "Stop all trading" and leaves a `moved` notice; nothing is deleted, and trading resumes there only if the user turns "Stop all trading" off.
- Tests: a round trip from one temporary profile to another with a different data root and project folder. Paths are rewritten, the workspace id survives, the checks are green, the move code matches, the live root stays untouched until the next start, the old root is kept, and trading arrives disarmed with the notice. Failures: a wrong passphrase, a tampered bundle, an older target version and a missing folder each come out as a clear message, with no switch scheduled.
- Not built: offering the import on the very first start of a fresh install (it lives under Settings → Data), and QA between two isolated app profiles (the Rust round trip covers the data path).

### 2.3 Directly to the server (card 4)

Once pairing exists (Server 3/7) and client mode works (Server 5/7): Settings → Data → "Move this installation to the server". The bundle streams over the paired connection in checksummed, resumable chunks; the server imports and verifies; this PC then connects to the server as a client and keeps its local copy as an archive.

**Built (card 4, 2026-11-05).** `crates/qs-core/src/movement.rs`, `import::import_keeping`, Settings → Data → "Move this installation to the server" (`SettingsSuiteDataMove`):

1. **Export** on the PC that moves (only a PC with its own data — not a server, not a client), with the defaults of "Export everything", into `<data root>-move/`. The bundle's path is kept (`core / migration.move`), so an interrupted move continues with the same file after a restart.
2. **Send**: a connection of its own to the chosen server (a paired connection with the role Full), the server PIN (the server steps are `external`), then the bundle into the server's `transfers/` in 1 MiB chunks with a progress line. "Stop sending" — or a dropped connection — keeps what arrived; "Continue the move" sends only the rest; the whole file's SHA-256 must match on the server, else the part is discarded and the next send starts over.
3. **Check** on the server (`move_propose`): the bundle's passphrase travels inside the encrypted connection and is stored nowhere; the server proposes the path table, which you correct on the moving PC.
4. **Import** on the server (`move_import`, `import_keeping`): as in card 3, plus the server keeps its own gateway — its key (`gateway.server-identity`), `gateway.db` (the paired devices), its `server.*` settings (role, listen address) — and the moving PC's `gateway.*` secrets (its device key) never land on the server. The bundle is deleted after a green report, and the server restarts five seconds later onto the moved data (its old root kept as `<root>.before-import-<time>`, as always).
5. **Finish** on the moving PC: the report's move code confirms the move here ("Stop all trading" and the "moved" notice, card 3), the local bundle is deleted, and QuantSuite restarts using the server. The local data stays, as an archive and for the modules that stay on each PC.

Long server calls declare their own timeout in the remote manifest (`remote.timeouts` in `crates/qs-core/remote.json`: the bundle export and plan, `move_propose`, `move_import`) instead of their class's two minutes.

Found and fixed on the way: the path rewrite of card 3 broke JSON nested inside a JSON string (QuantPilot's timeline keeps a tool call's input that way, with four backslashes per separator): the backslash escaping the closing quote was taken for a separator, the moved `pilot.db` held malformed JSON, and QuantSuite stopped at start ("PluginInitialization pilot: malformed JSON"). `PathMap` now keeps the separator width the store wrote and treats shorter backslash runs as escapes; and a database cell that was JSON keeps its old text if a rewrite would still break it. Also, a resumed upload could begin beside the attempt that had just dropped and still held the part open: the server now lets one upload write a file at a time (the next waits up to 5 s) and writes each chunk through before the next.

Tests: `import::tests::a_move_onto_a_server_keeps_its_gateway_and_leaves_the_pcs_key_behind`, the nested-JSON cases in `inventory::tests::paths_remap_whatever_the_separators_and_case`, and `two_process::a_resumed_upload_whose_part_was_damaged_is_refused_and_starts_over` (resume and checksum mismatch). QA between two isolated profiles on this PC: the client's populated profile (352 MB bundle) was sent, stopped at 27 % and continued from byte 116,391,936; the server's report was green (every database's integrity and rows, the secrets, both workspaces, the module checks); it restarted onto the moved data with its listen address and paired device; the client then used it and showed its note through the gateway, and its own profile carries "Stop all trading" with the moved notice.

### 2.4 Steps the import report lists

- Log in to Claude Code / Codex on the new machine (QuantPilot threads keep their history).
- QuantMCP → Connect for each AI client (other apps' config files are not QuantSuite's to copy).
- AgentOS import (re-creates `~/.claude/rules/AGENT.md`).
- QuantCanvas browser: log in to sites again.
- Exchange API keys with an IP whitelist: add the new machine's public IP.
- Wallet keys (Hyperliquid, Solana): nothing to do — they moved in the vault.

## 3. Worked example — from this PC to the server

| Step | Where | What you do |
|---|---|---|
| 1 | PC | Stop the bots (Trading overview). Settings → Data → Export everything. Logs off, caches on, signing key off. Passphrase twice. |
| 2 | — | Copy `PC-2026-12-01.qsbundle` to the server (USB stick or network share), or use card 4's direct move. |
| 3 | Server | Install QuantSuite → "Import from a bundle" → passphrase → map `C:\Projects\QuantSuite` to `D:\Projects\QuantSuite`. |
| 4 | Server | Read the report: all green; 6 manual steps listed. |
| 5 | PC | Enter the move code → the PC no longer trades. |
| 6 | Server | Arm trading on the server. |

## 4. Cards

| Card | What | Blocked by |
|---|---|---|
| QuantSuite Migration 1/4 | Inventory as code, path portability, workspace ids | Automation 1 |
| QuantSuite Migration 2/4 | Export: the encrypted bundle | 1 |
| QuantSuite Migration 3/4 | Import: checks, path mapping, verify, disarm, old-PC marker | 2 |
| QuantSuite Migration 4/4 | Direct move to the server over the paired connection | 3, Server 5 |

## 5. Decisions taken (change them on the card if you disagree)

| Decision | Default | Why |
|---|---|---|
| Encryption | The whole bundle, passphrase + Argon2id | It holds money data and keys |
| Market caches | Included, can be unticked | Rebuilding takes hours and hits rate limits |
| Workspace `artifacts/` | Not included, can be ticked per workspace or subfolder | Over 250 GB here (2026-11-03), nearly all agent scratch that can be re-created |
| Project folders | Not included, opt-in per workspace | They are git repositories |
| Signing key | Not included unless ticked | Only needed where releases are built |
| Older target version | Refused | Old code cannot read newer schemas |
| Old PC | Disarmed, never deleted | No double trading, nothing lost |

## 6. Options — not planned unless you ask

- Scheduled backups with the same exporter (for example nightly to a second disk or a NAS) — recommended once the server runs.
- Incremental bundles.
