# QuantPilot V3 — the suite's Agentic Development Environment

QuantPilot (`modules/pilot`, app QuantAgent) becomes a native ADE in the
spirit of T3 Code and PandaCode: threads with a streaming conversation,
a composer, approvals, diffs, worktrees, a terminal, a browser preview and
usage, all driving the coding agents the user already has installed.

The user's brief (2026-10-29): *"update QuantPilot to be like T3 Code or
PandaCode … i dont need fancy backgrounds but the functionality of both are
crazy good and the native feel as well … no half baked stuff, fully fledged
ADE (Agentic Development Environment)"*.

This document is authoritative for the 24 cards "QuantPilot ADE n/24" on
the QuantSuite board. Each card refers here; a card that changes a decision
updates this file in the same commit.

## 1. References

Both projects are cloned read-only in the user's folder for pulled projects.
They are requirements sources only: no code or assets are copied.

| Project | Path | Commit | Why it matters |
| --- | --- | --- | --- |
| T3 Code (pingdotgg) | `C:\pulled_projects\t3code` | `dac8b11d` (2026-10-28) | The most complete feature set and keyboard model. Electron + Node server, Claude via the Agent SDK. |
| PandaCode (aicodeking) | `C:\pulled_projects\pandacode` | `af25de0` (2026-10-29) | **Same stack as QuantSuite**: Tauri 2, Rust core, SQLite, no Node server. The architecture blueprint. |

Most useful files:

- T3 Code user docs, `docs/user/`: `composer.md`, `thread-sidebar.md`,
  `keybindings.md`, `permission-modes.md`, `source-control.md`,
  `project-settings.md`, `usage.md`, `terminal.md`, `keyboard-focus.md`,
  `activity-log.md`, `html-renders.md`, `providers-claude.md`,
  `providers-codex.md`.
- T3 Code internals: `docs/internals/overview.md` (event log, checkpoints,
  settlement), `docs/internals/providers.md` (adapter boundary, protocol
  traps), `packages/contracts/src/keybindings.ts` (default bindings),
  `apps/web/src/components/chat/` (timeline, composer, pickers, banners).
- PandaCode docs, `docs/`: `providers.md` (process model, streaming,
  subagents, Claude and Codex protocols), `approvals.md` (mode mapping),
  `turn-changes.md`, `worktrees.md`, `slash-commands.md`,
  `file-mentions.md`, `attachments.md`, `browser.md`, `run-project.md`,
  `github.md`, `agent-usage.md`, `agent-tools.md`, `design.md`,
  `task-board.md`, `rewind.md`, `chats.md`, `conversation-presentation.md`.
- PandaCode Rust: `src-tauri/src/providers.rs`, `turn_changes.rs`,
  `workspace.rs`, `terminal.rs`, `slash_commands.rs`.

The research summary lives in QuantMemory: *QuantPilot ADE: T3 Code +
PandaCode reference research (2026-10-29)*.

## 2. What changes from V2

**V2 (2026-09-02):** a session is a terminal. `claude`, `codex`, `pi`,
`omp`, `opencode` and `gemini` are wrapper commands typed into a PTY; the
face learns what happens from Claude hooks, tailed session files or output
activity.

**V3:** a **thread** is a structured protocol session rendered natively.
QuantPilot starts the user's installed CLI in its machine protocol and draws
the conversation, tools, approvals and diffs itself:

| Agent | Protocol |
| --- | --- |
| Claude Code | `claude -p` with stream-json input and output, the bidirectional control protocol the Agent SDK uses |
| Codex | `codex app-server` (JSON-RPC over stdio) |
| Gemini CLI, OpenCode, GitHub Copilot CLI, custom agents | ACP v1 |
| Pi, omp | Pi RPC mode |

What stays true from V2:

- It is still the user's own binary, login and configuration — not a copy,
  not an API key in QuantPilot. Skills, plugins, hooks, CLAUDE.md/AGENTS.md
  and MCP servers keep working because the CLI loads them itself.
- QuantMCP is attached to every run (setting `attachQuantmcp`), so agents
  have the kanban, memory, worktree and suite tools.
- Full access is the default permission mode (the user's V2 decision).
- The real TUI is one click away: **Open in TUI** runs `claude --resume <id>`
  (or `codex resume <id>`) in the thread's terminal drawer (card 15).
- The QuantPilot face stays, in the thread details panel.

## 3. Layout

One page inside the module; the main area never scrolls, each list scrolls
inside its own pane.

```
┌ QModuleHeader: Conversation | Board | GitHub · usage · settings ───────────────┐
├ QSidebar 220 ┬ conversation column ─────────────┬ inspector ──────┬ QRightPanel 220 ┤
│ project      │ timeline (scrolls)               │ Diff·Files·     │ face + state     │
│ filter       │   user / assistant messages      │ Browser tabs    │ model, context % │
│ Pinned       │   "Worked for 1m 12s" folds      │ (resizable      │ cost, tokens     │
│ Active       │   turn-change cards              │  split inside   │ branch/worktree  │
│ Working      │ approval bar / question panel    │  the main area) │ links, servers   │
│ Settled      │ context bar: project·Local·branch│                 │ agents, tasks    │
│ Snoozed      │ composer + controls row          │                 │ files touched    │
│              ├ terminal drawer (Ctrl+J) ───────────────────────────┤                  │
└──────────────┴─────────────────────────────────────────────────────┴──────────────────┘
```

- QSidebar and QRightPanel keep the suite's 220 px width with **no width
  override**, so they line up with the header. Wide content (diffs, files,
  browser) goes into the inspector split inside the main area.
- Ctrl+B toggles the sidebar, Ctrl+Shift+B the thread panel, Ctrl+Alt+B the
  inspector, Ctrl+J the terminal drawer.
- The sidebar (card 7; rules in `app/utils/sidebar.ts`, T3 Code's
  `thread-sidebar.md`): Pinned and Active keep the user's order; with the
  Working fold on (default), running threads leave Active and come back on
  top when they finish, fail or need you; a snoozed thread stays hidden
  unless it needs you (failing or needing you wakes it for good). Pointer
  drag reorders, pins, settles (onto the Settled header), un-settles and
  wakes; rows slide with transforms only, the dragged slot empties, the
  Pinned / Active boundary is an overlay, Escape cancels. The hover action
  dragged across rows settles / un-settles / wakes the range. Unpin, settle,
  snooze and delete show a 5 s Undo toast (Ctrl+Z outside fields); a delete
  runs after the window closes. Explorer files dropped on a row open the
  thread and attach (text files as file chips until card 9). Titles come
  from the first prompt (filler openers dropped), a manual rename wins, and
  "Regenerate title" asks a separate `claude -p --model haiku` run.
- The Board view (card 21) fills the main area without the thread sidebar.
  Usage (card 18) and Settings are pages inside QuantPilot, never OS windows.

## 4. Rust architecture (`modules/pilot/crate`)

### 4.1 Runtime

- **Thread → turn → run.** A thread owns a provider session id. A turn is
  one user message and everything the agent does in response. A run is one
  provider process serving one or more native turns (a Codex goal can chain
  several).
- **One process per active thread, kept warm briefly.** A turn needs a
  process; after the turn it stays up for follow-ups — at most two idle
  processes, each for ten minutes (PandaCode's pre-start rule) — then stdin
  closes and the CLI exits. The next turn resumes the saved session id. A
  changed model, effort, mode or QuantMCP setting starts a fresh process.
  Codex goals keep their connection open while the goal is active.
- **Stop** sends the provider's interrupt (Claude control `interrupt`,
  Codex `turn/interrupt`, ACP `session/cancel`), then kills the process group
  after a short grace period. Closing the app stops every group.
- **Bounds:** stdout frames up to 4 MiB, stderr tail of 16 KiB, each stdout
  byte scanned once for newlines. Tool previews are capped at 4 KiB input and
  8 KiB output; subagent previews at 32 KiB response and the last 16 tools.
- **Admission:** no global concurrency limit; a second start for a thread
  that already runs is refused before a user message is saved.
- **Detection without side effects:** version and login checks never send a
  prompt, start MCP servers, run hooks or open a login browser.

### 4.2 Adapters and the event vocabulary

Every provider sits behind one adapter trait (`crate/src/driver.rs`).
Provider-specific code lives only in the driver; the store, the UI and the
MCP tools see normalized events. A driver turns stdout lines into `Emit`s
(message start/delta/done, reasoning, tool start/input/result, file changed,
request, todos, usage, rate limit, notice, turn end); the runtime's projector
turns those into timeline **items** and envelopes.

The wire format (`crate/src/events.rs`) is deliberately small: the timeline
is a list of items upserted whole when they change, plus deltas for
streaming text. Each envelope carries `threadId`, a per-thread monotonically
increasing `seq` and one event:

| Event | Payload |
| --- | --- |
| `item` | one timeline item, created or replaced |
| `delta` | `itemId`, text appended to a streaming assistant or reasoning item |
| `turn` | `turnId`, status `started` / `completed` / `failed` / `interrupted` / `limited`, duration, cost, error |
| `state` | `idle` / `running` / `waiting` / `error` / `interrupted` / `limited` |
| `session` | provider, provider session id, model |
| `usage` | context tokens, context window, thread totals (cost, input, output tokens) |
| `rateLimit` | windows with % used, reset time, status |
| `title` | the thread's new title |

Item kinds and their `data`: `user` (text, attachments), `assistant` (text,
phase, streaming), `reasoning` (text, estimated tokens, streaming), `tool`
(call id, name, category, title, bounded input/output, status `running` /
`completed` / `failed` / `denied` / `interrupted`, path for edits),
`approval` (request id, tool, category, title, input, status `pending` /
`allowed` / `allowedSession` / `denied` / `expired`, reply), `question`
(request id, questions, status, answers), `plan` (request id, plan, status
`pending` / `approved` / `rejected` / `expired`), `todo` (todos), `notice` and
`error` (text). A subagent's items carry `parent` = the tool call that
started it. Goals and the subagent panel extend this in card 16.

Envelopes ride the Tauri event `pilot-thread-event`. The webview loads with
`pilot_thread_snapshot(id, beforeSeq?, limit?)` — the newest 100 items (or
the page before `beforeSeq`) and the `seq` they reflect, read while the
thread's projector is held so no event falls in between — and drops live
events with a lower or equal seq. Streaming text is persisted at least once a
second and at every message end, so the persisted history is the truth.

### 4.3 Storage (`pilot.db`, schema v5)

| Table | Holds | Card |
| --- | --- | --- |
| `threads` | V2's `sessions` renamed, plus `kind` (`chat` / `terminal`), `effort`, `title_source` (`auto` / `user`), `state`, `context_tokens`, `context_window`, `last_activity_at` | 2 |
| `timeline` | the items above: id, thread, turn, seq, kind, parent, data (JSON), timestamps | 2 |
| `turns` | one row per user message: status, start, end, duration, cost, error | 2 |
| `queue` (+ `threads.queue_held`) | queued follow-ups in order (schema v4); a completed turn sends the next one, a stopped, failed or limited turn — or an app restart — holds the queue until Resume or the next sent message | 4 |
| `threads` sidebar columns (schema v5) | `pinned`, `sort_key` (the user's order in Pinned and Active, larger is higher; new threads take their creation time), `settled_at`, `snoozed_until`, `auto_settle` (per-thread opt-out), `unsettled_at` (a manual Un-settle keeps automatic settlement away until the user writes again). A user message clears `settled_at`, `snoozed_until` and `unsettled_at`. Listing the threads settles those idle for `autoSettleDays` (default 3) unless they run, wait on the user, have queued follow-ups or opted out; pinning does not prevent it and settling unpins | 7 |
| `checkpoints`, `turn_files`, `turn_changes` (schema v7) | per turn: the before/after snapshot commits (also kept as `refs/quantpilot/checkpoints/<thread>/<turn>-before|after`), the paths the agent's tools reported, and the attributed changes with their patches (2 MiB per turn). `turns.provider_ref` holds the provider's id for the turn (Claude: the last top-level assistant uuid; Codex: the turn id); `threads.rewind` the pending "Edit from here" for the next launch | 11 |
| `attachments` | images and documents stored under the pilot data dir, never inside the project | 9 |
| (files, no table) | card 9 keeps attachments as files under `attachments/<thread>/` (images normalized to PNG ≤ 2048 px, files copied); the sent user item keeps their names, paths, PNG thumbnails and the chips' content in its `attachments` JSON | 9 |
| `threads.handoff`, `resume_at`, `limit_reset_at` (schema v9) | history owed to a new agent or a fork (JSON), the scheduled continuation, when a usage limit lifts | 15 |
| `links` (schema v8) | a thread's pull requests (card 12): url, number, title, state (`open` / `draft` / `closed` / `merged`), when GitHub was last asked; card 21 adds board cards | 12, 21 |
| `stash`, `prompt_history`, `keybindings` | composer and keyboard state | 4, 17 |

Later cards add their tables and columns as new schema versions
(`PRAGMA user_version`). On open, whatever was live when the app ended is
settled: pending requests expire, running tools and started turns become
interrupted.

The V2 `sessions` rows migrate in place into `threads` (`kind = 'terminal'`),
keeping the provider session id: a Claude row continues as a chat on its
first message (`claude --resume` works across the TUI and stream-json).

## 5. Providers

### 5.1 Claude Code (card 2)

```
claude -p --input-format stream-json --output-format stream-json --verbose
       --include-partial-messages --permission-prompt-tool stdio
       --session-id <thread id> | --resume <session id>
       --permission-mode <mode> [--model <id>] [--effort <level>]
       [--mcp-config <quantmcp.json>] [--add-dir <general vault>]
       --append-system-prompt-file <context.txt>
```

Verified on the installed **Claude Code 2.1.312** with recorded turns
(`crate/tests/fixtures/`): `--permission-mode` accepts `acceptEdits`,
`auto`, `bypassPermissions`, `manual`, `dontAsk` and `plan`, and still takes
`default` (= manual). Permission prompts reach the host **only with
`--permission-prompt-tool stdio`**: with `--permission-prompts host` alone a
prompt is denied as "not granted yet". A new thread starts with
`--session-id <thread id>`, later turns `--resume` it. `--fork-session` forks
on resume. The user's own Claude settings still apply — with `Write` and
`Bash` in their allow list, Supervised asks for nothing those rules allow.
Discovery runs `initialize` only, with `--no-session-persistence
--strict-mcp-config --settings {"disableAllHooks":true}` (0.7 s, no inference).

- Wait for the `initialize` response: models (`value`, `displayName`,
  `description`, `resolvedModel`), slash commands, `supportedEffortLevels`.
  Cached for five minutes.
- Text deltas start a message at `message_start`; the final snapshot only
  appends a missing suffix, never a duplicate.
- `tool_use` and `tool_result` join by id. `parent_tool_use_id` routes a
  subagent's frames to that child.
- `can_use_tool` becomes an `approval` item; the reply is allow
  (`updatedInput` = the original input), allow for the session
  (`updatedPermissions`: the request's suggestions, else a session rule for
  the tool), or deny with the user's message. AskUserQuestion becomes a
  `question` item (answers go back in `updatedInput.answers`); ExitPlanMode a
  `plan` item. Hook and SDK-MCP callbacks are answered with an error.
  A deny with the user's reply is sent as "The user declined this and
  replied: …" — a bare text reads to the agent like a tool error, which it
  may distrust and ignore (live test, card 6).
- `result.total_cost_usd` is cumulative per process: a turn's cost is the
  difference to the previous result.
- **Steering works**: a user message written to stdin while a turn runs
  joins that turn — one `result`, and the agent answers "per your mid-turn
  message" (verified 2026-10-29). With `--replay-user-messages` the CLI
  echoes each accepted message with `isReplay: true`. Follow-ups therefore
  have two meanings (card 4): **Queue** keeps the message in `pilot.db`
  until the turn ends, **Steer** writes it into the running turn now.
- Context % = the latest main-agent message's input + cache read + cache
  creation + output tokens, over the window from the result. Subagent and
  cumulative billing totals are excluded.

### 5.2 Codex (card 8)

`codex app-server`, verified against
`codex app-server generate-json-schema --experimental` of the installed
**codex-cli 0.165.0**: `initialize` (opting out of `turn/diff/updated` —
diffs come from checkpoints), `thread/start` then `thread/resume`,
`turn/start` with model, effort, sandbox, approval policy and reviewer,
`turn/interrupt`, item notifications, approvals, async questions (persisted,
answered with a new user message), `model/list`, `skills/list`,
`thread/compact/start`, `review/start`, `account/rateLimits/read` and
`thread/goal/*`. Child-thread notifications are filtered so a subagent never
completes the parent turn.

Shipped with card 8 (`crate/src/codex.rs`, fixture
`tests/fixtures/codex_approval_turn.jsonl`, ignored live tests
`live_codex_*`):
- One `codex app-server` per active thread; the thread opens inside the
  protocol (`thread/start`, or `thread/resume` by the saved id — a failed
  resume says so and starts fresh). The suite's context goes in as
  `developerInstructions`, QuantMCP as the per-run override
  `config["mcp_servers.QuantMCP"]` (same name as the Connect snippet, so a
  configured QuantMCP is updated, not doubled); the user's `config.toml` is
  never written.
- Model, effort and the mode's policy go with every `turn/start`; a live
  mode or model change is `thread/settings/update`. A message during a turn
  is `turn/steer` with `expectedTurnId`.
- `commandExecution` (the parsed `commandActions`, not the PowerShell
  wrapper, is the title; read/list/search actions show as read/search),
  `fileChange`, `mcpToolCall`, `dynamicToolCall`, `webSearch`, `imageView`
  and `collabAgentToolCall` become tool rows; agent messages keep their
  phase; empty reasoning items stay out; `turn/plan/updated` is the task
  list; `contextCompaction` is a notice.
- Approvals: command (`availableDecisions` decide whether "Allow for this
  session" is shown; without `decline` a denial is `cancel`, which ends the
  turn — the timeline says so), file change (with its diff), permission
  escalation (turn or session scope), MCP tool calls
  (`mcpServer/elicitation/request` with an empty form; `persist: session`
  for the session grant), MCP forms as questions, URL elicitations declined.
- Questions: `item/tool/requestUserInput` answered by question id;
  async questions (agent messages with `delivery: "async"`) are saved with
  `byMessage`, survive the turn, the process and restarts, and are answered
  by a new user message ("question → answer" lines).
- Usage: `last.totalTokens` is the context in use, `total` deltas count
  tokens (a resumed thread counts from its first new call); the 5-hour and
  weekly windows come from `account/rateLimits/*`. A usage or rate limit ends
  the turn as Limited.
- The catalog (`model/list` with a synthetic "Default", `skills/list`) comes
  from a short discovery run or a live thread. Another agent can take a
  thread only before its first message; carrying a conversation over is
  card 15.

### 5.3 ACP and Pi (card 20)

Shipped (`crate/src/acp.rs`, `crate/src/pi.rs`, `pi_modes.ts`):

- ACP v1 client for Gemini CLI (`gemini --experimental-acp`), OpenCode
  (`opencode acp`), GitHub Copilot CLI (`copilot --acp`, a new built-in) and
  the user's own agents (Settings → Providers → Your own agents, "Speaks:
  ACP"; V2 entries stay "Terminal" and keep running in terminal sessions).
  `initialize` offers no file system and no terminal (requests for them are
  refused); `session/new`, or `session/load` when advertised (its replayed
  history is not shown again; a lost session starts a new one with a
  notice); QuantMCP as an HTTP MCP server named `quantsuite-<thread>` when
  the agent takes HTTP MCP; the suite's context ahead of a new session's
  first prompt (ACP has no system prompt); images when advertised.
  Streaming: message chunks (split at a tool call or thought when there is
  no `messageId`), thoughts, tool calls and updates (diffs → changed files),
  plans → task list, `available_commands_update` → the slash menu, models
  from the session → the picker (`session/set_model`), `usage_update`.
- Modes: the thread's mode maps onto the agent's modes (`acp::mode_for`:
  plan / yolo-bypass-full / autoEdit-acceptEdits / default-ask-build) with
  `session/set_mode` acknowledged before the prompt; a refused switch or a
  missing mode fails the turn. Permission replies carry the policy too:
  Full access answers `allow_once` itself, Accept edits for edits, Plan
  declines anything but reading; Supervised asks (Allow → allow_once,
  Allow for the session → allow_always, Deny → reject_once). Stop answers
  waiting requests "cancelled", then `session/cancel`.
- pi and omp in RPC mode: `--mode rpc -e <thread>/qp-modes.ts
  [--session <file>] [--model provider/id] --append-system-prompt
  <context.txt>`; events → messages, thinking, tools (writes/edits →
  changed files), usage and cost; `prompt` with `streamingBehavior: steer`
  while a turn runs; `abort`; resume by session file (`get_state`). The
  extension stops what the mode does not allow and asks through an RPC
  `select` titled `QP_APPROVAL {json}` (→ an approval; other extensions'
  selects/confirms → questions); `/qp-mode <mode>` switches it. omp speaks
  the same protocol but announces commands itself and does not echo ids.
- Neither has an automatic reviewer: Auto is not offered for them (the
  composer's mode list per agent) and fails the turn if it is set.
- Verified: fixture tests for both drivers; live (no inference) pi and omp
  load the mode extension. Not verified live: Gemini CLI and Copilot CLI are
  not installed on the development machine (the `copilot` on its PATH is a
  stub that says so), OpenCode 1.3.15 fails at startup there with its own
  database migration error, and pi/omp have no model with a key there.

### 5.4 Capabilities

Each adapter reports what it can do: steer, rewind, fork, compact, goals,
plan mode, effort levels, permission modes, images, subagents, usage, limits.
The UI hides or disables (with the reason) anything a provider cannot do; it
never fakes support.

### 5.6 Branch toolbar and worktrees (card 10)

- Above the composer: project · Local / Worktree (switchable until the
  first message; Ctrl+Shift+X) · branch (Ctrl+Shift+G) · copy path. A Local
  thread switches or creates branches in the project's checkout — refused
  while files are changed, with the files named. A new Worktree thread
  shows its base and the planned branch (`quantpilot/<title>`, prefix in
  Settings; an auto name never takes an existing branch, a typed existing
  one is checked out in the worktree). The first message creates the
  worktree with QuantMCP's own code (`qs_mod_mcp::worktree::create_on_branch`
  — same `.qs-worktrees/` layout and dependency junctions), and the thread's
  `cwd` moves into it. Ctrl+Shift+L reuses the newest worktree of the
  project; a thread's menu offers "New thread in this worktree". A missing
  worktree blocks sending and shows Recreate / Settle.
- Settling a worktree thread offers a guarded cleanup: a clean worktree is
  removed by default, its branch only when merged into the base; unmerged
  commits keep the branch unless ticked (`git branch -D` then); uncommitted
  changes keep everything; a worktree another open thread uses is kept.
- Shift-click in a new thread's model picker selects several models: the
  first message starts one thread and worktree per model in the background.
  Ctrl+Enter in a new thread (send key Enter) starts it in the background
  and opens the next draft with the same project, mode and workspace.
- Threads outside git show only the project. Schema v6 adds
  `workspace_mode`, `base_branch`, `branch`, `worktree_path`.

### 5.7 Turn changes, the diff inspector and Edit from here (card 11)

- Every turn in a git checkout is bracketed by two snapshot commits made
  with a temporary index (`crate/src/checkpoints.rs`: `read-tree HEAD`,
  `add -A`, `write-tree`, `commit-tree` with a QuantPilot author) — the
  user's index, HEAD and branch are never touched. After the turn, the
  changes between the two are restricted to the files the agent's tools
  reported, so edits the user made meanwhile are not attributed to it; the
  turn gets a "changes" item under its answer (files, +/−; each file opens
  its diff).
- The inspector is a split beside the conversation (pointer-dragged edge,
  320 px minimum; open state and width per device). Its Diff tab compares
  This turn (between a turn's checkpoints; the picker lists "Turn N ·
  prompt"), Uncommitted (HEAD against the working tree, untracked included;
  Stage / Unstage / Discard with an inline confirmation, refused while the
  agent works) or Branch vs base (HEAD against its merge base). Monaco's
  diff editor shows the file — split or unified, wrapped or not; it goes
  inline by itself below 900 px. A review comment on selected lines lands
  in the composer as a chip (`path:a–b`) with the lines and the comment.
- "Edit from here" on a user message drops that turn and every later one
  from the thread and puts the message — text, chips and attachments — back
  into the box. The next launch resumes before it: Claude with
  `--resume <session> --resume-session-at <previous turn's last assistant
  uuid> --fork-session` (a new session id; the first message starts a new
  session), Codex with `thread/revert` to before the turn after resuming.
  "Rewind with files" also restores the checkout to the turn's before
  checkpoint — only in a worktree thread, and refused while another live
  thread works in the same folder.

### 5.8 Git actions and pull requests (card 12)

- The thread header carries the git control (`components/Git/Actions.vue`,
  logic in `utils/gitActions.ts`): one button for the next sensible step —
  Commit (& push with a remote), Pull when behind, Push & create PR on a
  GitHub branch, Merge into the base in a clean worktree thread, Push — and
  a menu with every action; unavailable ones say why. A repository without
  a remote shows local actions only; outside git the control is absent.
- Commit dialog: the branch, the changed files (untick to leave out —
  committed with `--pathspec-from-file`, so other staged files stay out),
  the message. Generate runs as the dialog opens: a short helper run of the
  thread's own provider (Claude: `-p --model haiku --tools "" --no-session-
  persistence --strict-mcp-config`, Codex: `exec --ephemeral -s read-only
  -c model_reasoning_effort=low`), prompt on stdin, the selection's stat
  and patch, and — style "Repository conventions" (Settings) — the last 15
  subjects plus the commit lines of AGENTS.md / AGENT.md / CLAUDE.md.
- No message from the suite carries an AI's co-author or "generated with"
  line: generated text is cleaned, a typed message loses them too
  (`gitops::strip_ai_credits`; human co-authors stay), and commits that
  already carry one are listed in the menu and the merge confirmation,
  never rewritten.
- Merge into the base is QuantMCP's own `--no-ff` merge
  (`qs_mod_mcp::worktree::merge`, no cleanup — settling cleans up): refused
  with uncommitted changes or while an agent works in the worktree or the
  project checkout; a conflict aborts and names the files.
- Pull is `--ff-only`; Discard all is `reset --hard` + `clean -fd` (ignored
  files stay), both refused while an agent works in the folder.
- Pull requests need a GitHub remote and a signed-in `gh` (checked at most
  every five minutes): the dialog writes title and description from the
  commits, the diff and the thread; creating pushes first when needed,
  runs `gh pr create --body-file`, and links the PR. An agent's own
  `gh pr create` in a turn links it at turn end. Chips (`PR #45 · open`)
  sit in the header, the sidebar row and the context bar; open ones refresh
  on focus at most every five minutes. Issues ("Closes #n") come with card 23.
- Every git/gh call: argv only, null stdin, prompts off, 30 s (local) or
  120 s (hooks, network) timeouts, output capped at 64 KiB.

### 5.9 Files tab, terminal drawer, project actions (card 13)

- Inspector tabs (Diff, Files; Browser with card 14) close with Ctrl+W and
  come back with Ctrl+Shift+T; the split's width is kept per thread (the
  last one set is the default for others).
- Files tab: a lazy tree of the checkout (`files::list_dir`, gitignore
  honoured, `.git` / `.qs-worktrees` / `node_modules` left out) or the
  project search (Ctrl+Shift+F: text or regex, case switch, 1,000 matches,
  binary and >1 MB files skipped, 4 s budget) beside the open file. Text
  opens in Monaco read-only — Edit, Ctrl+S saves through the crate (only
  inside the checkout; outside files and texts cut at 4 MB stay
  read-only). Markdown, HTML (an `allow-scripts` sandbox without
  same-origin, so it cannot reach the app), SVG and CSV/TSV render with a
  Raw switch; images and PDFs (≤ 25 MB) show. Copy contents, Save as (the
  buffer, or a copy of the file). Ctrl+P picks a file by name (the
  composer's `@` index). `path:line` in backticks or a link to a path in a
  message opens here at the line; code names like `store.load` do not.
- Terminal drawer (Ctrl+J) under the conversation: tabs of shells in the
  thread's checkout (the user's shell setting), each splittable beside or
  below; PTYs in the console registry under owner `pilot-drawer`, tracked
  per thread by the crate; tab layout per device. Selected output becomes
  a "Terminal lines a–b" chip in the composer. When a thread settles,
  shells idle at their prompt close; running ones and Run terminals stay.
- Project actions (Settings → Project actions, per project: name,
  command, icon, own shortcut, run when a worktree is created, run when a
  thread settles; drag to reorder): the header shows the first as a
  button and all in its menu; Run types the command into the action's own
  terminal (a live one gets Ctrl+C and the command again). The crate runs
  the hooks itself (first message of a Worktree thread; manual or
  automatic settle).
- Dev servers: `localhost:PORT` URLs in drawer output and in the agent's
  command output, listed under Servers in the thread panel while the port
  answers (checked every 8 s while there are candidates). Stop sends
  Ctrl+C to a drawer terminal; for an agent's server it names the process
  holding the port (netstat + tasklist) and ends that tree only after the
  user confirms, and only if the port still belongs to that pid. Open
  shows it in the Browser tab (card 14).

### 5.10 Browser tab (card 14)

- One native child webview (`pilot-browser`, `crate/src/browser.rs`) in
  the main window, made on the first navigation and closed with the tab —
  incognito, no devtools, downloads and new windows refused. Only the
  `main` webview may call the browser commands, and the guest never loads
  the app's own origin (in dev `localhost:1420`, which the capabilities
  would treat as local): `normalize_url` refuses it and `on_navigation`
  blocks it, along with anything but http(s) without credentials.
- A native view is always above the app's page, so the tab hides it
  whenever a modal is open (`aria-modal`, settings, palette, our veils) or
  a menu, listbox, dialog or open bottom drawer overlaps its area; another
  inspector tab, a closed inspector or another module hide it too. Its
  bounds follow the tab body (ResizeObserver, window resize, DOM changes;
  one update per frame, only when changed).
- Toolbar: back/forward (WebView2 history), reload/stop, start page,
  address (URL, host → https, `localhost:3000` → http, bare port →
  `http://localhost:port`; Ctrl+L), zoom (Ctrl+= / Ctrl+- / Ctrl+0,
  remembered), pick an element, screenshot, open in the system browser.
  Ctrl+R reloads while the tab is in front. The last page is kept per
  thread; Ctrl+Shift+J toggles the tab.
- Start page: the thread's detected servers, then the machine's TCP
  listeners above port 1024 with process names (`netstat -ano` +
  `tasklist`), asked on open and Refresh only.
- Element picker: a page script (`browser_picker.js`) highlights on hover
  and takes the next click; the result waits in a page global the crate
  reads every 250 ms only while armed (one minute at most), then checks it
  (http(s) origin, finite bounds) and bounds it — selector, tag, visible
  text (no form values, editable or hidden text), a markup summary with
  safe attributes only, sanitized again in Rust (no script/style bodies,
  handlers, values, `javascript:`). It becomes an "element" chip labelled
  as untrusted page content.
- Screenshot: WebView2 CapturePreview of the visible viewport → the same
  PNG normalization as pasted images, saved in the thread's attachments
  (5 MiB at most, four per message); capturing sends nothing.

### 5.11 Thread lifecycle (card 15)

- Restart: threads keep their provider session id; a turn cut by an app
  exit shows Interrupted, the queue is held. Resume (the empty composer's
  button) sends a fixed continuation the timeline shows as "↻ Resumed".
- Fork (a user message's Fork, the thread menu, `/fork`): a new thread with
  the turns up to that message — items, turns, checkpoints and changes
  copied under new ids — in this checkout or a new worktree branching from
  this thread's branch (committed work only). Claude continues natively in
  the same folder (`--resume … --resume-session-at <turn's last uuid>
  --fork-session`); Codex with `thread/fork {threadId, lastTurnId, cwd}`
  (verified live on 0.165); otherwise the first message carries the history.
- Another agent on an idle conversation: the model picker allows it; the
  thread starts fresh with the new agent and its next message carries the
  conversation (`lifecycle::pack_history`: user/assistant text only, the
  newest 200 messages and 64 KiB, truncation disclosed, wrapped as
  reference data). A notice and the composer say so; earlier turns keep
  their agent's label (`data.provider` on user items). The owed history
  (`threads.handoff`) counts as delivered only after a completed turn with
  a native session — a failed start or an app exit keeps it.
- Compaction: `/compact` is Claude's own; for Codex the driver turns it
  into `thread/compact/start` (a turn of its own, verified live). After an
  hour's pause on a Claude conversation over 200k tokens the composer
  offers Compact first (on) or Full for the next message: `/compact` runs
  and the message waits in the queue.
- Limits: a Limited stop remembers when the allowance returns (the
  exhausted rate window's reset, `threads.limit_reset_at`). Resume at
  reset schedules the continuation (`threads.resume_at`, a minute after
  the reset; cancellable; a message, a stop or settling cancels it; a
  background task wakes for the next one and runs overdue ones after a
  restart); Snooze until reset hides the thread until then. Settings →
  Threads makes either the default.
- Restart agent session (thread menu, `/restart`): the provider process
  ends; the next message starts it fresh on the same conversation.
- Open in the agent's terminal UI (thread menu, `/tui`): the CLI itself in
  the drawer (`claude --resume <id>` / `codex resume <id>`, same
  permission mode) as the PTY's own program. While it runs the thread does
  not send ("Open in the terminal — close it there…", Take it back closes
  it); when it ends, what it added comes into the timeline from the
  CLI's transcript (Claude: the session file after the last known uuid;
  Codex: the rollout beyond the thread's user messages), display only.

### 5.12 Plan mode, task lists, goals, subagents (card 16)

- Plans: Claude's ExitPlanMode waits for review (Implement — accept edits
  or full access, live on the process; Keep planning with what should
  change). Codex's finished `plan` item is a proposal (`Emit::Proposal`,
  a plan item with status `proposed`): on the thread's newest turn while
  nothing runs, Implement switches the mode and sends `PLEASE IMPLEMENT
  THIS PLAN:` plus the plan (T3 Code's wording); Keep planning sends what
  should change. Every plan has Copy. Claude in plan mode reads without
  asking (verified live).
- Task lists: one row outside the fold per turn ("Tasks 2/5 · Running the
  tests", opens into the checklist) and the current list under Tasks in
  the thread details. Sources: TodoWrite, Codex's `turn/plan/updated`, and
  Claude Code 2.1.x's TaskCreate/TaskUpdate — headless Claude has those
  only with `CLAUDE_CODE_ENABLE_TODO_TOOLS=1` (verified live 2026-10-30).
  The driver keeps that list itself ("Task #N created" gives the id) and
  a fresh process gets the thread's last list (`Driver::restore_tasks`).
- Goals (`/goal <objective>`, `/goal` shows it, `/goal clear`; Codex also
  `pause`/`resume`), shown above the box with what it took. Claude: the
  CLI answers `/goal` with synthetic replies ("Goal set: …", "Goal active:
  … (N turns)", "No goal set"); its Stop hook's "Stop hook feedback" counts
  a check with the reason; a run that ends with model work while the goal
  is set met it — the CLI drops it silently (verified live) — so the driver
  reports it complete. Codex (`thread/goal/*`, experimental API, verified
  live on 0.165): a new goal is set paused, its first turn starts with the
  thread's settings, then it turns active; an active goal on an idle
  thread makes Codex start turns by itself, so Resume only activates and
  one run spans Codex's goal turns until the goal stops (complete, paused,
  limits). Stop pauses the goal. A met goal shows until the next message
  (`threads.goal`, schema 10).
- Subagents: Agents in the thread details (newest first, kind, tool
  count, the running one's newest step) and a subagent call's "Open what
  it did" open its transcript read-only in the inspector's Agent tab: what
  it was asked, its text, reasoning and newest 16 tool calls (nested
  agents count in), its report; previews cut at 32 KiB.

### 5.13 Usage and limits (card 18)

- A page in the main area (`store.page = 'usage'`; Ctrl+U, the sidebar's
  Usage row, the palette): Cost, Tokens, Limits (C / T / L), 24 hours /
  7 / 30 / 90 days (Ctrl+Shift+1..4; days are today and the days before,
  whole local days), Escape back. The thread view stays mounted behind it;
  the Browser tab's webview hides while it is open.
- Sources (`crate/src/usage/scan.rs`): Claude Code `projects/**/*.jsonl`
  under `$CLAUDE_CONFIG_DIR`, `~/.claude`, `~/.config/claude` (subagent logs
  included; a request counts once by message id + request id across files —
  streams and resumed sessions repeat it); Codex rollouts under
  `$CODEX_HOME` and `~/.codex` (`sessions/`, `archived_sessions/`, older
  ones `.jsonl.zst`, read with the pure-Rust `ruzstd`; a rollout counts once
  by name, the uncompressed copy first; a `token_count` whose running total
  did not move is a repeat; OpenAI's input includes cached tokens). Only
  token counts are kept. An in-memory ledger re-reads a file only when its
  size or time changed, an appended log from where it stopped (complete
  lines). On this machine: 481 files, 3.2 GB, first scan 2.3 s release /
  16 s debug, rescan 15 ms. Scans run on open / Refresh only, off the UI
  thread.
- Estimated API cost (`usage/pricing.rs`), labelled as not the bill:
  LiteLLM's public price list, Anthropic's and OpenAI's own chat listings,
  standard rates (input, cache read, 5-minute and 1-hour cache writes,
  output, above-200k / above-272k tiers per request size); fetched at most
  daily (hourly on Refresh) into `usage-prices.json`, else the saved copy,
  else the built-in snapshot (`usage/prices.json`, 127 models). Dated and
  suffixed names find their listing; others are unpriced (tokens count, the
  headline shows their share). Cache savings = cache reads × (input − cache
  read rate). Checked against `ccusage`: Oct 26–29, 2026 identical to the
  cent and token.
- The page: tiles (estimate, savings, tokens, unpriced share / tokens by
  kind), a stacked bar chart per hour or day (agents for cost, token kinds
  for tokens; validated categorical slots on the dark surface; hover/arrow
  keys show every series, a Table view lists the numbers), the breakdown by
  model (cost, share, tokens, cache hit rate, cost per 1M) and a model's
  trend.
- Limits (`pilot_usage_limits`): Codex answers `account/rateLimits/read` in
  a discovery run (`runtime::probe_limits`, live 0.8 s), asked at most every
  five minutes (Refresh: now); Claude's windows come from its stream's rate
  limit events — every reported set is kept per provider (`usage_limits`,
  schema 11) with its time. Bars show % used, a reset countdown and clock;
  ≥ 90% "almost used up", 100% "used up"; a window without a reading says
  "not reported", never 0%.
- `/usage` in the composer shows the thread agent's windows above the box
  (the last reading, no agent run) until dismissed or the next message.

### 5.14 Settings and providers (card 19)

- Sections: General (new threads: agent, permissions, workspace, branch
  prefix, shell; composer; notifications; threads; the suite as context),
  Providers, Keybindings, Project actions, Source control, Appearance,
  Advanced. Each section root carries `data-module="pilot"` so the
  module's own styles apply inside the suite's settings modal.
- Settings v3 (`settings.rs`; a V2 file keeps everything): `defaultProvider`,
  `providers` (per agent: extra `args`, `env`, own `models`, `disabled`),
  global `env`, `preferSavedClaudeLogin`, `textSize`, `panelMotionMs`,
  `renderMath`, `projects` (per context: `provider`, `model` per agent,
  `mode`, `workspace`, `enabled` agents).
- Layers: default → project (`Settings::defaults_for`, `model_in`,
  `enabled_in`). `pilot_thread_create` fills what the request leaves open
  from them (the frontend no longer sends a mode); a disabled default agent
  hands over to the next one. Settings → General → "Settings for" shows a
  project's rows as inherited (muted layers mark) or set here (accent,
  Reset).
- Launch: `launch_env` = global variables, then the agent's own; with
  "Prefer saved Claude login" new Claude processes drop inherited
  credential variables (`CLAUDE_CREDENTIAL_VARS`: API key, auth/OAuth
  tokens, base URLs and headers, Bedrock/Vertex/Foundry switches, model
  overrides) unless the settings set them — `-NAME` env entries remove a
  variable in `spawn_process`. Extra arguments go after Claude's own and
  before Codex's `app-server` (global `-c` options). Both are part of the
  warm-process key, discovery runs and the login probe use them too. Own
  model ids join the picker's catalog. Agents switched off leave the model
  picker (the thread's own stays).
- Login state: `claude auth status --json` and `codex login status`, no
  prompt, nothing started. Update (`providers::update_plan`): the installer
  that put the CLI there — WinGet package → `winget upgrade --id <id>`, npm
  global → `npm install -g <pkg>@latest`, Claude's native install
  (`~/.local/bin/claude.exe`) → `claude update` — else the CLI's own
  `update`; the page shows the command first (dry run), runs it on
  confirmation and reads the version again. Live on this machine: Claude
  native → `claude update`, Codex (`.cli-bin`) → `codex update`.
- Source control: `gh auth status` with the exact `gh auth login` hint;
  commit message style. Appearance: conversation text size (zoom on the
  timeline), panel motion 0–400 ms (`--qp-motion`, none under reduced
  motion), math on/off (`setMathEnabled`). Advanced: the data folder,
  diagnostics without secrets (variable names only), Export logs to
  Downloads (with the newest 512 KB of the suite log), shown in Explorer.

### 5.15 Board view (card 21)

- The header's Conversation | Board switch (`app.view`, remembered with the
  chosen board in localStorage; also `board.toggle` in the palette). Board
  fills the main area: no thread list, no thread panel, the conversation
  unmounted (the V2 terminals stay mounted under it).
- The board is the suite's one board, `QDrawerKanban` bound to a board id
  (the remembered one, else the open thread's workspace, a forgotten
  workspace falls back to General): its columns, pointer drag and drop for
  unclaimed cards, the board's approval mode select and fold rule, the
  `QKanbanCardModal` editor (double click), refreshes on kanban bus events
  only. Its host hooks: the `board` slot (QuantPilot's picker: General and
  every workspace), `card-extra` (a row on every card, no drag starts
  there), `select` on a plain click, `selectedCardId`.
- QuantPilot's row (`Board/CardExtra.vue`): priority, the branch (worktree in
  its tooltip; the agent is the board's own badge), the threads started
  from the card with their status, Start thread (not on Done cards).
- Start thread: `pilot_thread_create` in the card's workspace with its
  defaults and the card's title, the draft prefilled (`cardPrompt`: the
  card, then the workflow — claim and worktree for a workspace card, the
  General columns for a General card; nothing is sent by itself), and
  `pilot_card_link`: a `links` row kind `card`, url `card:<id>`, beside the
  pull requests (their consumers filter `kind === 'pr'`).
- A click on a card with threads opens its latest beside the board
  (`Board/Panel.vue`: the conversation and composer of the open thread,
  resizable, Open in conversation, Close). The thread's header names its
  card; a click shows the card selected on its board with the thread
  beside it.

### 5.16 QuantMCP pilot tools (card 22)

- `quantsuite.pilot.*` capabilities in `modules/pilot/module.json`, served by
  QuantMCP's bridge catalogue and dispatched through the suite's approval
  gate like every suite tool: `set_title`, `render_page`, `start_thread`,
  `message_thread` (write); `open_file`, `open_preview`, `list_threads`,
  `read_thread` (read). Commands in `crate/src/agent_tools.rs`.
- The calling thread: every V3 thread's agent (Claude, Codex, ACP) gets
  QuantMCP as `…/mcp?pilot=<token>` — 32 hex digits per thread, minted per
  app run (`PilotState.mcp_tokens`). QuantMCP's POST and SSE handlers read
  the token and put it into each `quantsuite.pilot.*` call as `pilotToken`,
  dropping a value the agent passed (`with_pilot_token`); the command maps
  it back to the thread. Other threads are reachable by id only inside the
  caller's workspace.
- `set_title` keeps a title the user gave (`title_source = user`); the
  agent's is `agent`. `open_file` / `open_preview` work only while the
  thread is on screen with its inspector (`pilot_ui_visible`: the frontend
  reports the open thread in the conversation view, none on the board,
  the terminal or another module) and emit `pilot-agent-open`.
  `open_preview` takes what the Browser tab takes (http/https).
- `render_page`: a timeline item kind `page` (`{ title?, html }`, ≤ 1 MB,
  local `src` images embedded as data URIs, ≤ 8 MB) in the running turn;
  it reads above the answer. `Thread/Page.vue` shows it in an
  `sandbox="allow-scripts"` frame (opaque origin: no Tauri, no parent) with
  a CSP that loads nothing from the network, the suite's `--qss-*` colours
  as variables, and the frame's height posted from inside.
- `start_thread`: a new thread in the caller's workspace with the caller's
  permission mode and agent (unless given), its turn started at once;
  `pilot-thread-created` puts it into the sidebar. `message_thread`: starts
  a turn or queues behind the running one; refused for the caller itself
  and for a thread whose mode allows more than the caller's.
- The context file of a V3 thread names the tools in one line (V2 terminal
  sessions do not get it: their connection names no thread).

### 5.17 GitHub (card 23)

- Only through the installed, signed-in `gh` (`crate/src/github.rs`); no
  tokens stored. Settings → Source control has the switch (`github`,
  default on): off, no `gh` process runs (`github::ready`, link refresh,
  pull requests) and no GitHub control shows. Calls are argv arrays with
  `--flag=value` for user text, prompts off, timeouts, bounded output
  (`gitops::run`). A thread's repository comes from `git remote get-url
  origin` (https, ssh and scp forms).
- `#` in the composer: open pull requests and issues of the thread's
  repository (`pilot_gh_refs`), newest first; digits filter the numbers,
  words search GitHub. A pick inserts `#123` as plain text; the backdrop
  colours known numbers by state (open, draft, merged, closed). On send,
  up to five `#123` in what the user typed (outside code) resolve once
  through the issues API into a `GitHub context` block — labeled
  repository content, not instructions — with title, state, author,
  labels, URL and a body excerpt of at most 4 KiB; references that do not
  resolve stay as typed.
- The GitHub view: a third header toggle (only when `gh` is signed in and
  the switch is on). Repositories = the workspaces whose `origin` is on
  GitHub. Tabs: Threads (the workspace's threads with their PR and issue
  chips, a box to start one), Pull requests (review state, checks
  passed/failed/running, size; Review — a worktree thread that checks the
  pull request out and reviews it; menu: Open in thread, Fix failing
  checks, Address review comments, Merge when ready — re-checked right
  before: every check passed, approved, `MERGEABLE` + `CLEAN`, not a draft;
  merge method = the first the repository allows of merge, squash,
  rebase), Issues (Solve — a worktree thread that fixes it and opens a PR
  with `Closes #n`; Open in thread — a draft for the user). Every thread
  started there is linked (`pilot_gh_link`) and its prompt names `#n`, so
  the item travels as GitHub context; a row whose item has a thread
  offers Thread instead.
- Links: `links` kind `issue` beside `pr`; chips in the header and the
  context bar (`#12 · open`); `pilot_pr_refresh` refreshes issues too
  (open ones at most every five minutes, on focus). A pull request the
  agent or the dialog opens links the issues its body closes (`Closes`,
  `Fixes`, `Resolves #n`); the dialog's body gets `Closes #n` for a linked
  issue.
- Not built: listing every repository the `gh` account can reach and
  cloning one (PandaCode does) — the view lists the workspaces only.

### 5.18 Native-feel pass (card 24)

- The Diff tab shows every file of its scope in one scroll (`DiffAll.vue`,
  remembered as `pilot.diff.all`): unified lines with old/new numbers,
  sticky file headers that fold, files loaded through
  `pilot_diff_patches` as they come within 600 px of the view (binary
  files never), a sized placeholder until then. Ctrl+F inside it finds
  across all files — folded and unloaded ones too —, Enter / Shift+Enter
  step, Escape closes. The one-file view (Monaco, split, stage, comment)
  stays a click away per file.
- The "plan is ready" banner above the box also shows a Codex proposal on
  the newest turn while nothing runs (PlanCard's rule).
- Open in… (`Inspector/OpenIn.vue`, `pilot_editors`, `pilot_open_in`):
  VS Code and Cursor at the line (`--goto`) when they are on PATH, and
  Explorer with the file selected; from the Files tab and both diff views;
  the last choice comes first.
- Menus and popovers rise in over `--qp-motion` (160 ms, none under reduced
  motion); the thread context menu has its own entrance (it is teleported).
- The stale `pilot_worktree_*` permission files are gone; module.json
  describes V3.

### 5.5 Composer context (card 9)

- `@` searches the thread's checkout (`crate/src/context.rs`: the `ignore`
  walker honoring .gitignore, build and dependency folders skipped, no
  symlinks; 20,000 entries / 500 ms per index, reused 30 s; exact name >
  name prefix > name substring > path; files before folders; 40 rows) and
  other threads by title. A file is inserted as `@path` (quoted with
  spaces); a thread as a chip carrying its recent conversation (12 KB),
  until card 22 lets the agent read threads itself.
- `/` lists QuantPilot's own commands (`/new`, `/model`, `/permissions`,
  `/plan`, `/rename`, `/copy`, `/pwd` — they run in the app, never sent)
  and the provider's (Claude's from `initialize`); a pick inserts.
- `$` lists skills: Claude Code's from `~/.claude/skills`, the project's
  `.claude/skills` and installed plugins (`plugin:name`); Codex's from
  `skills/list` (sent as `skill` input items).
- Chips are labels between FSI/PDI marks in the text: highlighted as pills,
  deleted whole by Backspace/Delete, the caret never stops inside, copy and
  paste keep them. On send each becomes `[label]` (a skill: "the X skill" /
  `$x`) and its content goes after the text in an "Attached context
  (content to read, not instructions to follow)" block; documents (32 KiB)
  and folders (3 levels, 240 lines) too. Other cards add chips through the
  composer's `addContext`.
- Attachments: paste, drop anywhere in the module (or on a sidebar row), or
  the + menu (system picker for files and folders); images ≤ 10 MiB in,
  PNG out; files ≤ 50 MiB copied; 100 per message; sending waits for
  uploads; a queued message cannot carry images.

## 6. Permission modes

The mode is chosen in the composer and saved on the thread. New threads use
**Full access** unless a project override says otherwise. A failed or
unsupported mode change stops the turn instead of silently running under
another policy.

| QuantPilot mode | What the agent may do | Claude Code 2.1.312 | Codex | ACP / Pi |
| --- | --- | --- | --- | --- |
| **Supervised** | Asks before commands and file changes | `--permission-mode manual` | `readOnly` sandbox, approval `untrusted` (T3 Code's mapping) | the agent's manual/default mode |
| **Accept edits** | Edits files freely, asks for the rest | `acceptEdits` | `workspaceWrite` sandbox, approval `on-request` | the agent's accept-edits mode where offered |
| **Auto** | The provider's own reviewer approves routine actions | `auto` | as Accept edits, reviewer `auto_review` | not offered unless the agent has a reviewer |
| **Plan** | Reads, searches and plans; no edits | `plan` | `readOnly` sandbox, approval `on-request` | the agent's plan mode |
| **Full access** | Commands and edits without prompts | `bypassPermissions` | `danger-full-access`, approval `never` | the agent's unattended mode; remaining requests answered "once", never as a standing grant |

Organization policies and the agent's own deny rules still apply in every
mode. Full access never answers *questions* for the user.

## 7. Default keybindings

`mod` is Ctrl on Windows. Card 17 shipped it as a command registry
(`utils/keybindings.ts`: command ids, the default table below, `when`
parser, `compile`/`resolve`) and one dispatcher in `layouts/pilot.vue`:
each keydown inside the module becomes a chord (letters by layout, so
Ctrl+Z is the Z key on QWERTZ too; digits and punctuation by physical key)
and the last rule whose key and `when` match names the command; components
register what they own with `useCommands` (`utils/commands.ts`, warm-cache
aware; a handler returning `false` lets the key go on). Handlers closer to
the key (menus, find bar, the terminal drawer, Monaco) claim it first with
preventDefault; the suite's own keys (Ctrl+K, Ctrl+Shift+N) are taken in
the capture phase. The user's rules live in `keybindings.json` in the
pilot data dir (`pilot_keybindings_get/set`), after the defaults; a rule
`{ "key", "command": "-id" }` removes that command's earlier rules on that
key; invalid rules are skipped, named in Settings → Keybindings and kept
in the file. `when` contexts: `terminalFocus`, `terminalOpen`,
`previewFocus`, `previewOpen`, `inspectorFocus`, `inspectorOpen`,
`composerFocus`, `editableFocus`, `turnRunning`, `modelPickerOpen`,
`paletteOpen`, `usagePageOpen` (card 18); unknown ones are false; `!`,
`&&`, `||`, parentheses. Settings → Keybindings: search, record (Escape
cancels), Remove, Reset, conflict warnings (other commands, the suite's
keys, project actions). Tooltips name the key a command has now. The
command palette (`components/Palette.vue`, ranking in `utils/palette.ts`):
threads of every project by title, branch, PR or project — frecency
(opened often and lately) and the last pick for the same query boosted for
three weeks; "In messages" (`pilot_messages_search`: top-level user
messages, answers and plans of every thread, from three letters, opening
at the message via `threads.reveal`); projects (New thread in…); actions
with their keys. The open palette keeps the keyboard. Back/forward walks
the visited threads (`pilot.history`). The composer's send keys (Enter
variants, Alt+Up, history) stay the composer's own. No Vim mode or Vim keys.

| Key | Command |
| --- | --- |
| Ctrl+N / Ctrl+Alt+N | New thread / new thread without a project |
| Ctrl+K | Command palette (`>` = actions only; Ctrl+1..9 pick; Escape back to the message box) |
| Ctrl+B / Ctrl+Shift+B / Ctrl+Alt+B | Sidebar / thread panel / inspector |
| Ctrl+J | Terminal drawer (Ctrl+N new, Ctrl+D split beside, Ctrl+Shift+D split below, Ctrl+W close the pane — while the drawer has focus) |
| Ctrl+D / Ctrl+Shift+J | Diff tab / browser tab |
| Ctrl+P / Ctrl+Shift+F | File picker / project search |
| Ctrl+F | Find in thread (find in diff when the diff has focus) |
| Ctrl+U | Usage page (C / T / L tabs, Ctrl+Shift+1..4 periods) — card 18 |
| Ctrl+, | Settings |
| Enter / Shift+Enter | Send / new line (send shortcut is a setting) |
| Ctrl+Enter | While a turn runs: the opposite of Queue/Steer; in a new thread: start in the background |
| Ctrl+Alt+Enter | Send and open a new thread |
| Ctrl+Shift+Enter | Send the oldest queued message as a steer |
| Alt+Up | Edit the latest queued message |
| ArrowUp / ArrowDown in an empty composer | Prompt history |
| Ctrl+S | Stash the prompt / restore a stash |
| Ctrl+Shift+M / E / A / X / G / L | Model / effort / permission mode / workspace / branch / previous worktree |
| Ctrl+1..9 | Open the first nine threads (pick entries while the palette or model picker is open) |
| Ctrl+Shift+[ / ] | Previous / next thread |
| Ctrl+[ / ] | Back / forward |
| Ctrl+Shift+S / Ctrl+Shift+P | Settle / pin thread |
| Ctrl+Shift+C | Copy thread reference (PR link when linked; otherwise `quantpilot://thread/<id>?title=…`) |
| Ctrl+Z | Undo the last sidebar action (5 s toast) outside editable fields |
| Ctrl+W / Ctrl+Shift+T | Close inspector tab / reopen the last closed one |
| F2 | Rename the focused thread |
| F6 / Shift+F6 | Next / previous area: sidebar, conversation, composer, inspector, terminal |

## 8. House rules for every ADE card

- **Register:** sans 11–13 px labels, mono only for numbers, no mono
  micro-caps chrome; one page, the main area never scrolls.
- **Geometry:** QSidebar and QRightPanel without width override.
- **Dragging:** pointer events only (never HTML5 drag and drop, it breaks in
  WebView2); reordering by drag only, no up/down buttons or reorder keys.
- **Language and numbers:** English, `en-US`, 24 h time; money as
  `1,234.56$` through core `formatMoney`.
- **Editors:** prose editors are full size, never cramped modals; shared UI
  that several modules need goes into `packages/ui`.
- **Warm cache:** loading and polls follow the KeepAlive rule
  (`onActivated` / `onDeactivated`); running threads keep running while
  another module is in front.
- **Dev servers:** never port 1420; browser QA on port 4180 with a stubbed
  `__TAURI_INTERNALS__`; run the type check before starting the 4180 server.
- **Tests:** per card only the tests of the changed area plus the crate's
  `cargo check`/clippy; live provider tests are `#[ignore]`d and opt-in; the
  full suite is the user's call.
- **Screenshots:** QA screenshots go to the internal artifact directory and
  are read critically — empty and default state first.
- **Git:** generated commit and merge messages never carry a
  `Co-Authored-By` or "Generated with" line for any AI. Nothing is pushed to
  a remote without the user's OK.
- **Worktrees:** QuantPilot uses the same worktree implementation as the
  QuantMCP tools (`<repo>/.qs-worktrees/`, linked `node_modules`), never a
  second one.
- **Untrusted content:** provider output, page content, GitHub text and
  memory excerpts render as sanitized text/Markdown and are labeled
  untrusted when they travel into a prompt.

## 9. Card map

| # | Card | Blocked by |
| --- | --- | --- |
| 1 | Spec (this file) | — |
| 2 | Agent runtime core + Claude Code adapter | 1 |
| 3 | Conversation view: streaming timeline, "Worked for …", thread details panel | 2 |
| 4 | Composer core: send, queue vs steer, interrupt, history, stash, large paste | 3 |
| 5 | Composer controls: model picker, effort, permission mode, context meter | 4 |
| 6 | Approvals, questions, plan approval, "needs you" notifications | 5 |
| 7 | Sidebar: projects and threads (sections, drag, menu, undo, titles, keyboard) | 6 |
| 8 | Codex adapter (`codex app-server`) | 2 |
| 9 | Composer context: `@` mentions, `/` commands, `$` skills, attachments, chips | 4 |
| 10 | Branch toolbar: Local/Worktree, branch picker, worktree lifecycle, multi-model fan-out | 7 |
| 11 | Turn changes, diff inspector, checkpoints and revert | 10 |
| 12 | Git actions: stage, commit, push, merge into main, PR create and link | 11 |
| 13 | Inspector Files tab, file previews, terminal drawer, project actions | 11 |
| 14 | Browser preview: child webview, local ports, element picker, screenshots | 13 |
| 15 | Thread lifecycle: restart resume, fork, provider switch, compaction, limits, Open in TUI | 11 |
| 16 | Plan mode, task lists, goals, subagents panel | 6, 8 |
| 17 | Command palette, keybindings editor, focus areas, back/forward | 13 |
| 18 | Usage and limits | 8 |
| 19 | Settings and provider management | 8 |
| 20 | More providers: ACP (Gemini, OpenCode, Copilot, custom) and Pi/omp RPC | 19 |
| 21 | Board view: the workspace kanban inside QuantPilot, threads linked to cards | 10 |
| 22 | QuantMCP `quantsuite.pilot.*` tools: titles, open file/preview, visual replies, threads | 14 |
| 23 | GitHub: `#` mentions, PR and issue lists, review/fix/solve threads (low priority) | 12 |
| 24 | Native-feel pass, V2 cleanup, end-to-end QA, docs and memory | 9, 15–18, 20–22 |

Cards are worked one at a time in this order. Packaging happens once at the
end of a session (or on request), never per card.

## 10. Out of scope

Seen in T3 Code or PandaCode, deliberately not planned. Each one is only
built if the user asks for it:

- Mobile apps, remote access and relays (T3 Connect, PandaCode Connect)
- Cloud sandboxes, local VMs, iOS/Android simulators
- Chat bots (Telegram, Slack, Discord, WhatsApp)
- Deploy buttons (Vercel, Cloudflare, …)
- Wallpapers, theme galleries, VS Code theme import ("no fancy backgrounds")
- PandaCode's Jev review add-on, model routing, agent teams (architect + workers)
- Scheduled and webhook automations
- Several accounts per provider, CLIProxyAPI hubs
- T3 review guides, GitHub stacks, mobile PR review
- Mockup sketcher, moodboards, token-saver response packs
- A global quick-launch overlay (the suite has QuantHUD)

## 12. Shipped state and known gaps (card 24, 2026-10-30)

All 24 cards are merged. Each card's evidence (tests, browser QA shots
under the internal artifact dir `quantpilot-ade-<n>`) is in its card on the
board. What is known to be open:

- **Not run in the real app by the agent.** Every card was verified with
  unit tests, `cargo test`/`clippy`, `test:pilot` and a browser QA against a
  stubbed backend; the live CLI tests of cards 2, 8 and 20 ran against the
  real CLIs. An end-to-end pass in the dev app with real Claude and Codex
  threads (Local + Worktree, approvals, diff, revert, queue, fork, board
  start), card 16's plan/subagent/goal flows, card 22's tools from a real
  thread and card 23 against a real GitHub repository are the user's test
  run — `npm run tauri:dev` belongs to the user.
- **Performance not measured.** The timeline is paged (100 items, older
  pages on scroll), not virtualized; RSS idle vs five active threads and
  ten parallel turns were not measured.
- **Native feel, partly.** Menus animate in, not out; context menus exist
  on thread rows, not on every file link or image (no right-click copy
  path / save on media); Open in… is not offered on file links inside
  replies; no taskbar flash (toasts and the module badge exist).
- **V2 kept where it is still used.** The thread's Terminal (header) still
  runs the agent CLIs through the wrapper commands, and its face reads the
  hooks relay and tailed session files. Removing them removes that
  terminal session; that is the user's call. Open in TUI (card 15) uses a
  plain `--resume` launch.
- **GitHub.** The view lists the workspaces with a GitHub remote, not every
  repository the account can reach, and has no clone; linked items refresh
  per thread, not in one batched GraphQL call.

## 11. Open questions

None blocking. Decisions taken without asking, open to change:

- The terminal-first V2 model is replaced as the main mode; the TUI remains
  reachable per thread (Open in TUI), in the terminal drawer, and in the
  thread's Terminal session, which still uses the V2 wrappers (§12).
- Threads without a project get their own folder under the pilot data dir,
  like today's General vault context.
- GitHub (card 23) is low priority because the QuantSuite repo has no remote
  locally; the user may delete or postpone it.
