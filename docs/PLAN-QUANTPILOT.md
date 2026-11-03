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
  conversation (`lifecycle::pack_history`: opening objective, recent exchanges
  and execution outcomes, at most 200 entries and 64 KiB with a lower
  context-aware budget; see parity 03 below). A notice and the composer say so; earlier turns keep
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
  uploads. Queued messages retain images, files, folders and chips as editable
  structured drafts; parity 04 below describes delivery and recovery.

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

## 12. Parity follow-up: paged history (02/23)

`read_thread` keeps its compact newest-message default, now querying messages
directly instead of filtering the newest 600 timeline items. Its footer gives
item IDs, truncation, attachment counts and an older-page cursor. `history: {}`
opts into structured chronological pages from the oldest item; `start: newest`
walks backward, `query` finds literal content and `includeActivity` includes
plans, tool inputs/results and attributed subagent items. Limits are 1–60 items;
previews are at most 2,000 Unicode characters. Search is SQLite case folding.

`read_thread_item` returns the complete saved JSON data in chunks (default 8,000,
maximum 16,000 Unicode characters). Concatenate `text`, then parse JSON to recover
text and attachment references. Schema 12 adds revision tracking so edits at the
same timestamp invalidate a body continuation. Neither API reads provider logs
or configuration. Both authenticate the connection token and enforce the same
workspace boundary, including reads of the calling thread itself.

Pages pin their upper `(seq,id)` boundary, excluding concurrent appends. Bodies
and totals remain live; interior deletions are skipped, while deleted anchors or
rewind require a fresh cursor. This is not an immutable transcript snapshot.
History is marked as untrusted quoted content. `threadProvider` means the current
provider: older items did not store historical provider identity.

Native commands `pilot_history_page`, `pilot_history_item` and
`pilot_history_around` share this storage implementation. Opening an old palette
hit fetches at most 100 nearby items into a clearly labeled history excerpt,
with Return to latest; live conversation state continues separately. It no longer
loads up to 50 pages just to reveal one old hit. Whole-thread Find improvements
remain parity 15. Rust regression tests cover large histories, Unicode/body
reconstruction, migrations, search, permissions and stale cursors. Native and
real-provider verification evidence is recorded on the card, separately from
unit tests. This follow-up does not change the 1.0.42 release or the 1.1.0 UI scope.

## 13. Parity follow-up: portable context (03/23)

Portable handoffs and non-native forks select the opening objective, recent user
request/answer, further early constraints and recent execution outcomes. Selection
is deterministic; entries retain chronological order, item/turn/thread IDs, roles,
parent attribution and a provider label where it was actually recorded. The
candidate read is bounded to the first 10 messages, last 20 messages and last 200
activity items, with a 2,000-character prefix and up to 500-character suffix per
item. It never materializes a 5,000-message transcript. Oversized entries retain
both ends and a retrievable item ID instead of stopping selection. This is an
extract, not a semantic summary: constraints buried in the omitted middle require
retrieval. The payload names both paged history tools and the source thread;
shortened entries and omitted counts are explicit. Exact saved JSON, including
attachment references, is available through parity 02.

The current request stays intact after the history wrapper; its expanded context
and images reserve space before selection. Text bytes conservatively estimate
tokens without assuming a provider tokenizer. Capacity reserves one quarter
(capped at 16,384) for instructions/tools, one quarter (capped at 8,192) for output,
8,192 per current image, 1,024 for the wrapper, and trustworthy existing usage.
History is capped at 64 KiB and 200 entries, usually less. A known explicit model's
existing session may supply capacity/usage; new destinations and default aliases
use a conservative 65,536-token capacity estimate. Model/provider/session resets
clear old telemetry, and usage from a turn begun on a different model is rejected.
Unknown capacity and provider-added instructions/media costs remain estimates,
not a guarantee of acceptance. If even retrieval references cannot fit, sending
returns an actionable error without clearing the owed history or stopping work.

Native Claude same-folder fork/resume and Codex native fork remain preferred.
Returning to Claude with portable history starts a fresh native session ID,
avoiding collision with the original Claude session; bound retries resume it.
Portable delivery remains owed in SQLite through failed starts, interruption and
app restart, clearing only after a completed turn with a usable native session.
Retrying a partially delivered turn may repeat the bounded reference context in
that same session. A send notice reports actual included/shortened/omitted counts
and fallback budgeting. Binary media, rendered HTML, native tools/background jobs
and hidden reasoning cannot be moved between providers; saved text and attachment
references are recoverable, not reattached binaries. Context-limit rejection does
not automatically stop, compact or recreate a running provider session.

## 14. Parity follow-up: structured queues (04/23)

Schema 13 stores a versioned payload with the unexpanded composer text, chips
(including skill references), and saved attachment paths and small thumbnails.
Legacy text-only rows keep their identity and order. Provider-specific context
expansion and image loading happen at delivery through the normal send pipeline.
The payload is bounded by the IPC frame limit, 100 attachments and 96 KiB per
thumbnail; full image bytes are never repeated in queue events.

Editing opens the complete draft in the normal composer. Saving keeps its queue
position; cancel/save restores the previous text, pasted text, chips and
attachments. Edit state survives a restart. If the row changes, disappears or
starts sending, the unsaved edit remains available as an explicit new queued copy.
Revision checks reject stale updates. Upload failures block sending until the
attachment is corrected or removed.

A persisted per-thread delivery claim prevents concurrent Resume, Send now and
turn-completion callbacks from consuming the same row twice. Preparation or
launch failure retains the payload and holds the queue with an actionable error.
Rows remain until their own turn completes; failed/interrupted turns retain them.
Codex steers additionally wait for the provider's asynchronous acknowledgement,
including when it arrives after turn completion. A rejected steer stays queued.
Provider capability checks reject unsupported ACP images rather than silently
dropping them. Steering retains attachment metadata in the transcript.

Restart holds every queue. An in-flight row becomes uncertain and requires the
user to check the transcript before explicitly retrying or removing it. There is
no automatic replay of an ambiguous delivery, and no claim of exactly-once remote
execution across a process crash. A completion race that prevents delivery can
leave a recoverable held message instead of silently dropping it.

Removing an unsent row never deletes saved files. The `attachment_references`
SQLite view counts distinct queue/timeline owners, including forked timelines;
future retention work can consult it. This card adds no automatic cleanup policy.
Native QA, real Claude/Codex results and focused regression evidence are recorded
on the card and in the private `quantpilot-parity-04` artifact directory. These
source changes are separate from the published 1.0.42 assets and the 1.1.0 redesign.

## 15. Parity follow-up: MCP forms and web authorization (05/23)

MCP elicitations are their own request kind (`RequestKind::Elicitation`), shown
as a question item with an `elicitation` payload instead of a generic question.
Both providers pass them through: Codex `mcpServer/elicitation/request` (modes
`form`, `openai/form`, `openaiForm`, `url`) and Claude Code `control_request`
`elicitation` (2.1.316; Claude declares `elicitation.form` and `.url` to MCP
servers). Codex's empty-schema MCP tool approvals stay ordinary approvals.

`elicitation.rs` turns `requestedSchema` into an ordered field list (the
server's order: Claude's `property_order`, for Codex read from the raw line)
with type, title, description, required, default, options with labels
(`enum`/`enumNames`, `oneOf`, multi-select `items.enum`/`anyOf`), text length
and format (email, uri, date, date-time), numeric bounds and item counts. Any
other constraint keyword, type or nested object is refused with a precise
protocol error and a notice — never a form that would accept invalid answers.
Annotations (`$comment`, `examples`, `deprecated`, `x-*`) are ignored. The
answer is checked again in the driver before it is sent; a refused answer keeps
the request waiting with the draft intact. Blank optional fields are left out,
integers go out as integers.

URL mode shows server, purpose, host and full address. Nothing opens by itself:
Open in browser (or Copy link) first, then "I've finished" sends `accept`;
Decline and Cancel send theirs. Only http(s) addresses with a host and without
embedded credentials are shown; others are refused.

Codex `requestUserInput` keeps `isSecret` (masked input) and `isOther` (no free
text when false). A secret answer goes to the agent but the saved item keeps
`{ "secret": true }` instead of the value (`runtime::redact_secrets`, also for
answers sent as a message). Diagnostics never logged answers. The providers' own
session files are outside QuantPilot's control.

Drafts (`utils/requestDrafts.ts`) are per thread and request id, in memory only:
they survive streaming, thread switches and refused submits, and are dropped
once the request no longer waits. Several waiting questions can be switched;
several forms stack with "n of m". A request the provider withdraws — Codex
`serverRequest/resolved`, Claude `control_cancel_request` (sent on Stop) —
expires at once, and a late answer is refused ("no longer waiting"), so each
request gets at most one response. A restart expires waiting requests as before.

Limits: ACP and pi have no elicitation path here (ACP/pi answers stay
approvals/questions). Codex Full access runs with approval policy `never`, under
which Codex itself auto-rejects elicitations (inferred from the protocol's
granular `mcp_elicitations` flag, not verified live). Real Codex runs were not
possible on 2026-11-02 (`codex login status`: not logged in); the Codex path is
covered by fixtures built from the v0.166.0 generated schema and the opt-in test
`live_codex_elicitation_form_and_url`. Claude was verified live
(`live_claude_elicitation_form_and_url`) and in the isolated native app (form
errors, draft across thread switch, send, web step, Stop → expired + cancel).

## 16. Parity follow-up: write attribution (01/23)

A turn's changes are captured file by file (`tracking.rs`) instead of "the
turn's snapshot diff restricted to the paths its edit tools named". Every
file gets a frozen before and after, and the diff is computed from those
(`git diff --no-index` over temporary copies — the user's index, HEAD and
hooks are never involved):

- **Tracked**: QuantMCP `quantsuite.pilot.track_files` — `begin` with the
  exact paths before a write batch (shell scripts included), `complete` with
  the `captureId` and optionally the subset actually written. Baseline at
  begin, result at complete. Changed-but-not-reported files are left out and
  said so. The thread prompt asks agents to use it for shell writes and, outside
  Git, for every edit. Side effect class `compute` (records only).
- **Reported**: Claude Edit/Write/MultiEdit/NotebookEdit, Codex fileChange,
  ACP edit/move/delete and pi write/edit events freeze the file the moment the
  event arrives; the baseline is the turn-start Git checkpoint (`git cat-file`).
  Outside Git there is no baseline: the file is listed with a note instead of an
  invented diff. Duplicate events do not duplicate files or statistics.

Uncertainty is labelled, not hidden (`note` per file, `partial` + `notes` on
the turn's changes item): another thread wrote the same file during the turn
or had it open in a capture; the file changed between this turn's own writes;
captures never completed (left out, turn marked partial); limits reached.
A file snapshot cannot see an unreported process writing the same file inside
a capture window — separate worktrees remain the dependable boundary.

Create (A), edit (M), delete (D) and rename (R, a tracked delete + create with
identical bytes, `from` = old path) are recorded; binary files keep a summary,
oversized (> 1 MiB) and unreadable files a note. Limits: 200 paths and 32 MiB
captured per turn, 64 open captures, patches 256 KiB per file / 2 MiB per turn
(existing budget), saved diff-editor sides 8 MiB per turn. Paths must stay inside
the thread's folder (parent steps and escaping links are refused).

Schema 14 adds `source`, `note`, `from_path`, `before_text`, `after_text` to
`turn_changes`; forks copy them. The turn scope of the Diff tab now reads the
saved records (`saved:<turn>`): patches and sides as frozen, so later editor
changes never alter an earlier turn, and it works outside Git. Turns saved before
schema 14 keep source `snapshot` and fall back to their Git checkpoints for the
editor sides. The after-checkpoint is still taken inside Git: revert and rewind
are unchanged. `turn_files` is no longer written (the table stays).

## 17. Parity follow-up: agent browser tools (06/23)

QuantMCP `quantsuite.pilot.browser` lets the agent use the Browser tab the
user sees: `read` (page text, cut at 8,000 characters, and up to 150 visible
interactive elements — links, buttons, fields, checkboxes, headings — as refs
`e1`…, with role, accessible name, value, checked/disabled), `click`, `fill`
(text fields, textareas, selects by value or label, contenteditable; React-safe
value setter plus input/change events), `press` (one character or a named key;
Enter submits a form field), `scroll` (to a ref or by pixels), `wait` (for text
or a ref, ≤ 15 s) and `screenshot` (WebView2 CapturePreview of the visible
viewport, saved as a thread attachment and returned as an MCP image block).

Only fixed commands from `browser_agent.js` run in the page; the agent never
passes code. Refs belong to the document that issued them: a navigation or
reload gives "the page changed since your last read", a re-rendered element
"is gone". Page text is labelled untrusted. Results are bounded (512 KiB per
answer, 3 s per page call, image inline up to 4 MiB).

Ownership: a page belongs to the thread that was on screen when it was opened
(user navigation or the agent's `open_preview`). The tool works only while that
thread is on screen, the tab shows the page, and the caller owns it — a hidden
tab, another thread on screen, a closed tab or a missing page each give an
actionable error. Guest IPC isolation is unchanged (only `main` drives the guest).

QuantMCP passes a tool's own content blocks through when its output is a JSON
object with `$mcpContent` (text and `image/*` blocks only); the log keeps a
size line instead of the image. The QuantHUD plugin now appends its WebView2
arguments to ones already in `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` instead of
replacing them — every webview of the process must share one argument set, so a
per-window argument (as an isolated QA debugging port once was) made child
webviews such as the Browser tab fail with 0x8007139F.

Verified with a real Claude Haiku thread in the isolated native app: open a local
page, read refs, fill "Your name", click a checkbox and a button, wait for the
greeting, screenshot seen by the model; plus hidden tab on thread switch,
not-on-screen caller, stale ref after reload and closed tab. Codex could not be
run (not logged in on this machine). Side-effect class `compute`: the provider's
own permission mode still asks per MCP call in Supervised.

## 18. Parity follow-up: run and preview for agents (07/23)

Two QuantMCP tools over the project actions the user configured (Settings →
Actions, the drawer's Run terminals — no second registry, nothing guessed, no
installs):

- `project_status` (read): `list` (starts nothing), `status` (the action's
  processes under its terminal's shell, their listening ports, local addresses
  from the output with who listens there, and the last 40 output lines) and
  `preview` (wait up to `timeoutMs` for the action's address — or a given local
  url — to answer over HTTP, then open it in the Browser tab for the parity 06
  browser tool; with `file`, a generated .html/.svg of the thread's folder is
  shown in the thread through the isolated page frame, no server).
- `run_project` (write — the suite's approval gate applies): `start` runs the
  action in the thread's checkout (Local or worktree cwd) in its drawer
  terminal; a running action is reported, not started twice (`restart` runs it
  again); `stop` sends Ctrl+C and, if the action ignores it, ends only that
  action's process tree. The terminal stays; nothing outside the action is
  touched — a port held by another process (the source checkout's server) is
  reported as such.

A preview that loses the screen while waiting does not open (and the frontend
ignores an open for a thread that is not active). A failing command, a
missing executable and a port collision come back with the command's own
output. The drawer reloads when an agent starts an action
(`pilot-drawer-changed`).

`qs-pty` keeps the newest 16 KiB of every session's output (`tail`) and exposes
the shell's pid. The tail is rendered like a screen (carriage returns,
backspaces, line erases and cursor moves applied), so an interactive shell's
redrawn input line reads once.

Fix of all three netstat parsers (`files::pid_in_netstat` for Servers →
Stop, `browser::parse_listeners` for the Browser tab's start page, and this
card's): they matched the state column `LISTENING`, which Windows translates
(`ABHÖREN` here), so on a non-English Windows nothing was found. A listener is
now recognised by its remote port 0 (`files::netstat_listener`).

Verified with real Claude Haiku threads in the isolated native app: list →
start (dev server with a 4 s start, path with spaces) → preview (HTTP 200,
opened) → browser read/screenshot → start again ("already running") → stop →
status; plus a failing build, a missing executable, a port held by a foreign
server and a static SVG preview. Codex not run (not logged in).

## 19. Parity follow-up: importing CLI conversations (08/23)

Sidebar → "+" menu → **Import from Claude Code or Codex…** lists the session
logs of every CLI home the Usage page reads (`CLAUDE_CONFIG_DIR`, `~/.claude`,
`~/.config/claude`; `CODEX_HOME`, `~/.codex`, their `sessions/` and
`archived_sessions/`, `.jsonl.zst` included), newest first, 600 logs at most,
searchable by title, folder or session id, 50 at a time. Each row shows title,
folder, last activity, message count and model; already imported, started by
QuantPilot itself, unavailable (folder gone, log too large) and problems
(unreadable lines, a cut last line, a compressed archive) are marked.

Import (`import.rs`) reads only — no model request, the logs are never written
— and creates a thread bound to the native session id in the conversation's own
folder, so the next message resumes it natively (Claude `--resume`, Codex
`thread/resume`). Its context is the registered workspace containing the
folder, otherwise the mapping chosen in the dialog (General by default); the
thread's folder stays the original one. Turns, roles and original timestamps
are kept; tool calls with input and output, inline images (saved as thread
attachments) and Claude resume points (`provider_ref`) come along; images a log
only names, skipped lines and entries over the 4,000-per-import limit are said
in a notice. Subagent logs and sidechains stay out.

Identity is provider + home + session id (schema 15 table `imports`): the same
session in two homes is two conversations; importing again adds only the
messages the log gained since (counted by user messages); a session QuantPilot
started itself is shown as such and not imported twice. A conversation whose
folder no longer exists is refused with the reason (it could not resume).
Stop cancels between conversations; a conversation is imported whole or not at
all (a failure deletes its partial thread).

Verified in the isolated native app: a standalone `claude -p` session in a
folder with "ü" and a space was listed, imported with its original times, and a
real Claude Haiku turn through QuantPilot resumed it ("KIWI-08"); importing
again added nothing; a deleted folder was refused with its reason; a Codex
rollout from an alternate `CODEX_HOME` (an Orca account home) was imported with
its provenance. Codex resume could not be run (not logged in on this machine).

## 20. Parity follow-up: skill library (09/23)

Settings → QuantPilot → **Skills** (`skills.rs`, `Settings/Skills.vue`). A skill
folder with a `SKILL.md` (frontmatter `name` and `description`; plain, quoted
or folded values) is imported once into `<pilot data>/skills/lib/<id>/` with its
support files (≤ 64 files, ≤ 1 MiB, no links; written to a staging folder and
swapped in). `<id>.json` records name, description, source folder, a SHA-256
library hash over all files, and every activation. A second source with the same
name is refused unless the import replaces the source.

Activation copies the library version where the CLI itself looks and records
each written file's hash:

| Provider    | User scope | Project scope (the chosen thread's checkout) |
|-------------|-----------|-----------------------------------------------|
| Claude Code | `<CLAUDE_CONFIG_DIR or ~/.claude>/skills/<id>` | `.claude/skills/<id>` |
| Codex       | `~/.agents/skills/<id>` | `.agents/skills/<id>` |
| pi, omp, OpenCode, Gemini CLI, Copilot, custom | unsupported — listed, never shown as installed | |

(Codex 0.166 `skills/list` reads user skills from `~/.agents/skills` and repo
skills from `.agents/skills` and `.codex/skills`.) A project activation adds its
folder to the checkout's `info/exclude` (via `git rev-parse --git-path`, so
worktrees work) and removes the line on deactivation.

Each cell is `active`, `outdated` (the library is newer — "Update"),
`drifted` (a written file was edited — refused unless "keep edits", which updates
the rest), `missing` (folder gone — "Restore") or `off`. Deactivation deletes only
files whose hash is still QuantPilot's; edited files stay and are named. A folder
QuantPilot did not write is never activated over, and skills installed by hand are
listed as such. A skill can leave the library only when it is off everywhere.
Activation and deactivation drop the cached Codex catalog, so the composer's `$`
list follows; a skill chip whose skill disappeared before sending is refused with
its name while the draft stays.

Not included: importing straight from a GitHub repository (import a cloned
folder), catalogs/discovery, rules (they stay in AgentOS / QuantMCP).

Verified in the isolated native app (project scope only, so the user's own skill
folders were untouched): import with a support file, activation for Claude and
Codex in a Git checkout (exclude lines written), a real Claude Haiku turn used
the skill through its Skill tool ("LIME-09"), Codex `skills/list` saw it as a repo
skill, a user edit showed drifted and was kept on deactivation, a library update
showed outdated and updated, an unsupported provider was refused, removal while
active was refused.

## 21. Parity follow-up: GitHub browse, clone and onboarding (10/23)

The GitHub view's sidebar has **Browse GitHub…** (hidden while GitHub is off
in Settings → Source control; every `gh` call checks the same switch). It lists
what the signed-in `gh` can reach — owned, collaborator and organization member
repositories, newest first, 50 a page — or searches name and description across
the account and up to ten of its organizations. Rows show private, archived,
fork and access (admin/write/read), and every registered workspace whose
`origin` is that repository (HTTPS, SSH and scp remotes normalized, compared
case-insensitively; distinct checkouts of one repository are all listed).
`gh` errors become readable sentences (rate limit, sign-in, offline) with
anything token-shaped masked.

**Clone…** asks for a parent folder (native picker) and a folder name. The
destination must not exist or be empty — a non-empty folder or another
repository's checkout is refused and left untouched; a folder that already is
this repository is registered instead of cloned. `gh repo clone … -- --progress`
reports phase and percent (`pilot-gh-clone` events); Stop ends the process
tree. A failed or stopped clone removes only what it created (the whole folder
when it was new, its contents when the folder was empty before). A successful
clone is registered through the suite's workspace service (`openWorkspacePath`,
which also makes it the active workspace), and the view selects it on its
Threads tab; if registration fails the clone stays and the message says so.

Linked PR and issue states are shared for two minutes across all threads that
link the same item (`github::shared_state`), so many threads on one PR ask
GitHub once per refresh window.

Verified in the isolated native app with the real `gh` login (read-only on
GitHub): browse (7 repositories including private and collaborator ones),
search, refusal of a non-empty folder, clone of `octocat/Hello-World` with
progress, "already cloned" for the same folder, Stop of a `microsoft/vscode`
clone (its folder gone), and a UI clone of a small repository that became a
workspace with a thread in it.

## 22. Parity follow-up: human pull-request review (11/23)

A pull request row has **Review yourself** next to the agent's thread action
(now labelled **Agent review**, unchanged). `review.rs` loads the PR at its
current head through the signed-in `gh`: title, description, author, branches,
head SHA, commits, checks, changed files with their patches (`pulls/N/files`,
paginated; renamed files keep their old path; binary or very large files say
GitHub sends no diff), existing review threads (GraphQL: resolved, outdated,
whether the viewer may resolve) and the actions GitHub allows the viewer
(comment always; approve and request changes only on an open PR the viewer did
not write).

`Github/Review.vue`: files with viewed checkboxes (stored per head SHA, so a
push resets them), the selected file's diff with old/new line numbers and the
discussions at their lines (Resolve/Reopen where allowed), click a line number
to draft a comment (Shift-click for a range from the last line). Drafts and the
summary are saved in the pilot database (schema 16, `review_drafts`) — nothing
is sent before Submit — and survive restarts. Each draft remembers the head it
was written against and its line's text: after a push, a draft whose text is
found exactly once moves with it (and says so); any other is marked stale
inline and in the review panel, and Submit refuses until it is deleted.

Submit re-reads the head, builds one `POST pulls/N/reviews` with the summary,
the event and all inline comments (`commit_id` = current head), and carries a
hidden marker derived from its content; a retry after a network failure first
looks for a review with that marker, so the review is never posted twice. On
success the drafts are deleted. A review never merges.

Provider boundary: `review::Host` has GitHub only; GitLab, Forgejo and others
are not implemented and not claimed.

Verified in the isolated native app on a public PR (`octocat/Hello-World#11683`,
read-only): load, a line draft and a summary draft, a simulated push leaving one
draft stale (inline and in the panel), Submit refused before any request, drafts
intact after an app restart, QA drafts deleted. **Not verified: an actual review
submission** — it writes to GitHub and needs the user's explicit authorization
and a designated test pull request; payload, marker and refusal rules are covered
by unit tests only.

## 23. Parity follow-up: clickable transcript attachments (12/23)

In a sent message (`Thread/UserMessage.vue`) thumbnails are buttons: click or
Enter opens `Thread/MediaViewer.vue` with the **original** image (not the
thumbnail), read only when shown through `pilot_attachment_media` — allowed
only for the thread's own attachment folder or a path one of its messages names,
32 MB at most, PNG/JPEG/GIF/WebP/SVG/BMP with real dimensions. The viewer fits
by default; + / − / Ctrl+wheel zoom, 0 fits, 1 shows actual size, ← / → move
between the message's images, Copy path, Save a copy (native save dialog,
`pilot_file_save_as`), Esc closes and focus returns to the thumbnail that opened
it. The data URL is dropped when the viewer closes.

File attachments open in the inspector's file viewer (read-only outside the
checkout, as before), folders in Explorer. Right-click or Shift+F10 opens a menu:
Copy path, Open in the default app (new `default` opener: the Windows file
association), Show in Explorer, Save a copy. On showing a message, its
attachment paths are checked (`pilot_paths_exist`); a moved or deleted file is
struck through as "unavailable", its actions are disabled and a click says where
it was. Video playback is not offered.

Verified in the isolated native app with a real Claude Haiku turn carrying two
images, a file and a folder outside the checkout (paths with spaces): full-size
originals, navigation, zoom, Esc with focus return, the file in the read-only
inspector, and a file deleted afterwards shown as unavailable with its actions
disabled.

## 24. Parity follow-up: Mermaid diagrams (13/23)

`mermaid` 12.2.0 is a dependency of the shell (added with `npm install
--workspace=@quantsuite/shell`, lockfile updated) and loads lazily the first
time a diagram appears (`utils/mermaid.ts`).

A ` ```mermaid ` fence in a reply is source while it is still streaming ("drawn
when the block is complete"), then drawn once and cached by source and theme (64
diagrams); every later repaint of the streaming message takes the drawing from
the cache, so only a newly finished diagram is ever drawn. The Markdown
sanitiser removes `<style>`, which Mermaid's SVG needs, so the drawing is put
in after sanitising through a placeholder and its own cleaning: no scripts,
foreign objects, images, links (a link becomes its text), event handlers,
external `href`/`url()` or `@import`. Mermaid runs with `securityLevel:
'strict'`, SVG labels, `maxTextSize` 20,000 and `maxEdges` 500; sources over 20k
characters or 1,000 lines stay source with the reason, a render over 8 s or a
parse error shows Mermaid's message above the source.

Each diagram has Source/Diagram, Large view (fit, + / − / Ctrl+wheel, 0 fits, Esc
closes and returns focus), Export SVG (native save dialog) and Copy (the
source). Diagrams are shown at natural size and a wide one scrolls sideways
instead of shrinking its text. The Files tab draws `.mmd`/`.mermaid` files and
Markdown files with Mermaid fences through the same renderer. Theme: the suite's
dark theme (light is keyed separately in the cache; the suite has no light mode).

Verified in the isolated native app: a real Claude Haiku reply streamed a
flowchart, a sequence, a state and a class diagram plus an invalid one (states
seen: partial → pending → ready; the invalid one showed the parse error with its
source), Source toggle, large view with zoom and Esc focus return, no links or
scripts inside the drawings, and the `.mmd` and Markdown file previews.

## 25. Parity follow-up: quoting replies (14/23)

Selecting text in an assistant message (the answer or a message inside "Worked
for …") shows **Cite in composer** under the selection (keyboard:
Ctrl+Shift+Q). It opens a small box with the quote and an optional comment;
**Add to message** puts a quote chip into the composer (`kind: "quote"`, label
`“first 40 characters…”`). Quotes are capped at 16 KB with a notice. The chip
keeps a snapshot of the selected text, and its path names the source:
`pilot-item:<thread>:<item>:<occurrence>`, where the occurrence is which
appearance of the text in that message was selected (counted without
whitespace, `utils/quote.ts`).

To the agent the quote is sent as `<quoted-reply label from="assistant" thread
item>…</quoted-reply>`, the comment separately as `<user-comment-on-quote
label>…</user-comment-on-quote>` (`context.rs`), so quoted text and the user's
own words never mix. The chip's comment travels with the message, a queued
message and "Edit from here".

A selection inside a streaming message holds the message's repaint while it
lasts, so the selection is not lost; the message catches up as soon as the
selection is gone. In the sent message the quote chip is a button: it opens the
source (another thread and older history included), unfolds "Worked for …" if
the message is folded there, scrolls to it and highlights the selected
occurrence for six seconds (CSS Custom Highlight `qp-quote`). If the source was
rewound or deleted, or its text changed, a notice says so and the quote keeps
its text. The comment shows in the message's "Attached context".

Also fixed: a chip inserted while the window has no focus (execCommand refuses)
is inserted directly.

Verified in the isolated native app with Claude Haiku: the second of two equal
phrases quoted with a comment, the agent answered with the quote and the
comment verbatim and apart; the quote chip reopened the source and highlighted
the second phrase (paragraph 9, not 3); a selection during a stream held while
the source grew by 2,361 characters and the message caught up after release; a
quote from a folded "Worked for …" message unfolded it and highlighted it; a
quote whose source does not exist showed the notice.

## 26. Parity follow-up: Find across folds, chips and old history (15/23)

Find (Ctrl+F) searches the thread in storage instead of the mounted messages:
`pilot_thread_find` (history.rs `find_in_thread`) reads the thread's user
messages, answers, commentary (folded "Worked for …" and subagents included)
and plans in one pass and returns the matching items, oldest first, with their
match counts, turn ids and a snippet, plus the total over the whole thread.
Tool output and reasoning are not searched. Over 5,000 matching items the
newest 5,000 are listed and the counter shows `+` (its tooltip says how many
older matches are not listed).

Text is compared as it reads: case folded per character (Unicode, not only
ASCII; `ß` does not become `ss`), chip marks and the Markdown emphasis
characters (`*`, `_`, `` ` ``, `~`) dropped, whitespace runs as one space — so a chip's label,
`**bold**` text and wrapped lines match like the shown text. `utils/find.ts`
applies the same rules to the rendered text of a mounted item; both sides have
the same test cases (`test-pilot-find.mjs`, `history.rs` tests). A mounted
item's own count wins (a match only inside a link target, say, is not shown and
is skipped).

Only the current match's item is brought into view: from the live view when it
is loaded, otherwise a bounded excerpt around it (`pilot_history_around`, ~100
items, "History excerpt" banner); a folded commentary's "Worked for …" opens,
and folds Find opened for earlier matches close again. A new query starts on
the newest match; Enter / Shift+Enter move with wraparound, Escape closes and
returns focus to the conversation (the fold of the current match stays open).
Typing cancels the previous search and clears its highlights at once. While
Find is open, new or streamed text and rewinds refresh the results at most
every 700 ms without moving or scrolling the current match. Searching no longer
loads older pages into the live view.

The command palette's "In messages" hits use the same reveal: the folded
commentary opens and the searched text is highlighted for six seconds.

Verified in the isolated native app on a 5,601-record thread (fixture, Claude
provider, only the newest 100 items loaded): the answer in the oldest turn
(5,597 records back), a commentary inside a folded "Worked for …" (opened, and
closed again when Find moved on), a user message with a chip
(`report.md for MARMOT` across the chip), an approved plan and `größe ärger`
(`GRÖSSE ÄRGER` finds nothing, as intended) were each found as 1/1 and
highlighted visibly; `common-term` counted 26/26 in 13 answers and Enter walked
1/26 … 26/26 with wraparound and Shift+Enter back, every step visible; the DOM
never held more than 51 items. Escape closed Find, cleared the highlights and
focused the conversation. With a live Claude Haiku thread, a streamed reply
raised the count from 4 to 35 while the current match stayed on its item, no
highlight showed wrong text, and a rewind brought it back to 4. The palette hit
for the folded commentary switched threads, opened the fold and highlighted the
text. Backend search took 55–113 ms per query on that thread (debug build).

## 27. Parity follow-up: PDF and Office document context (16/23)

Document attachments are read into text for the agent, on Windows and
everywhere else, with no system tool (`documents.rs`). Libraries, all compiled
into the binary (nothing extra to install or package, all MIT): `pdf-extract`
0.12 (with `lopdf` 0.42) for PDF text layers, `zip` 9.2 (deflate only) and
`quick-xml` 0.42 for the XML inside Office and OpenDocument files.

| Format | Status | What the agent gets |
| --- | --- | --- |
| PDF with a text layer | read | text with `[page N]` marks, title, page count; Unicode (CJK, Greek, emoji) from embedded fonts |
| PDF without a text layer (scanned, image-only) | `needs_ocr` | "needs OCR, which QuantPilot does not include" + the original's path — never an empty success |
| PDF with a password (an empty user password opens) | `encrypted` | the limitation + the path |
| Word `.docx` | read | paragraphs, `#` headings (also localized ids such as `berschrift2`), `-` list items, tables as rows of cells joined by `\|`; deleted tracked changes and field codes left out; title and page count from the properties |
| PowerPoint `.pptx` | read | `[slide N]` marks in slide order |
| OpenDocument text `.odt` | read | paragraphs, headings, lists, tables, title, page count |
| `.doc`, `.xls`, `.xlsx`, `.ppt`, `.odp`, `.ods`, `.epub`, `.pages`, `.key`, `.numbers` | `unsupported` | "not extracted" + the path |
| password-protected Office file (an OLE container) | `encrypted` | the limitation + the path |
| other files | unchanged | text inline (32 KiB) or "binary file, read it from the path" |

Limits: the attachment limit of 50 MiB, 2,000 pages or slides, 30 s, 4 MiB of
text kept, 64 MiB per decompressed XML part, 20,000 zip entries. Hitting one
gives `truncated` ("Partly read · 1,200 of 2,000 pages", the reason in the
detail), never a silent cut. PDF pages without text in an otherwise readable
PDF are named ("Pages without a text layer (scanned?): 3, 7"). A malformed
document fails with the reader's message; a reader panic is caught.

The message carries the first 48 KiB of the text inline (cut at a line end) in
`<attached-document name path format title pages|slides status note inline
full-text>`; the whole extracted text is a file in the pilot data folder
(`attachments/.documents/<sha256>-v1.txt`) whose path the block names, so the
agent reads on with its own tools. The original attachment is untouched and
still opens, previews and saves as before; nothing in a document is executed
or followed.

The composer reads a document in the background as soon as it is on the
shelf (dropped, picked, restored or put back) and shows the state next to its
size: "Reading text…", "Text · 3 pages", "Partly read · …", "Scanned · needs
OCR", "Password-protected", "Text not extracted", "Could not read" — the
tooltip has the format, title, the limitation and how much text goes along.
Removing a document while it is read stops the reading. Results are cached by
content hash and extractor version (`.json` + `.txt`), so the send (and a
queued message later) use the same result, and an edited file is read again
instead of mixing versions. The cache is not pruned yet (retention, 19/23).

Verified: unit tests build PDFs (WinAnsi Unicode, an empty page, page, time
and text limits, an image-only PDF, a locked PDF, corrupt data, cancel),
DOCX (headings, lists, a table with a two-paragraph cell, entities), PPTX
(slide order 1, 2, 10) and ODT, and the cache (same content once, a changed
file read again, a bounded block naming the full text). Real files: Chrome
printed a three-page report with German, Japanese, Greek and emoji and a
table, a 121-page manual (read in 0.7 s, debug build) and an image-only page;
a hand-built locked PDF, a cut-off PDF, a fake `.doc` and an OOXML-built DOCX.
In the isolated native app the shelf showed each state; Claude Haiku answered
all four questions about the PDF and the DOCX from the inline text without
tools, and for the 121-page PDF read the full-text file with its own tool to
find the marker on the last page and the last chapter; removing a document
while it was read stopped it; the saved copies kept their hashes. Codex is not
signed in on this machine, so no live Codex run.

## 28. Parity follow-up: images and files in question answers (17/23)

A question that takes a free answer also takes images and files: the
**Attach** button (system picker), paste into the question panel, or drop onto
it. They are saved like the composer's attachments (same limits: images
10 MiB, normalized to PNG; files 50 MiB; at most 10 per answer, 20 per
request), documents are read like the composer's (parity 16) and show their
state, and an image shows its thumbnail. Questions with only their options
(Codex `isOther: false`) and secret questions take no files — in the panel and
again in the backend. Each question of each request keeps its own draft of
picks, text and files (`utils/requestDrafts.ts`, memory only) through Back /
Next, other requests and streaming; the composer's draft is separate. While a
file is still saving or failed to save, **Answer** waits and the panel says
why; nothing partial is sent.

No agent protocol takes files in an answer (Claude Code's AskUserQuestion
answers and Codex `request_user_input` are strings), so `pilot_thread_answer`
checks the files (saved copies of this thread, existing, image or file) and
folds them into that question's answer as references the agent opens with its
own tools: "Files the user attached to this answer (saved copies; open them
from these paths): - shot.png (image): <path>", a document also with the path
of its extracted text. An answer can be only a file. Codex questions answered
by the next message carry the same text. The question keeps the files as
`replyFiles`; in the conversation "Question answered · 3 files" lists them
with the message attachment view (viewer for images, open/reveal/save for
files), also after a restart. The files live in the thread's attachment folder
and go only with the thread.

Verified in the isolated native app with Claude Haiku answering a real
AskUserQuestion of three questions: an option plus an image, only an image,
and text plus a PDF; an oversized image blocked **Answer** with "Remove the
files that could not be saved" until removed; Back to the first question kept
its pick and files; the composer draft stayed as typed. Claude read all three
paths and named the colors of both images and the PDF's code word. History
showed "Question answered · 3 files" and opened the image in the viewer, also
after restarting the app. Unit test: references, an only-file answer, and
refusal for a secret question, an options-only question, an unknown question,
a file outside the thread's attachments, a missing file, a folder and more
than 10 files. Codex is not signed in on this machine (no live Codex run; its
answer mapping takes the same folded strings).

## 29. Parity follow-up: preview-browser profiles (18/23)

The Browser tab stays a temporary (InPrivate) page by default — nothing is
kept. A **browser profile** keeps cookies, logins and site data across
restarts in its own WebView2 user-data folder, `browser-profiles/<id>/` in the
pilot data folder (`browser_profiles.rs`, list in `profiles.json`). The bar's
profile button shows the page's profile ("Temporary" or the name) and opens a
menu: Temporary, the profiles, a new profile (created and used at once) and
**Manage profiles…** — rename, **Clear data** (cookies, logins, storage and
cache go, the profile stays) and **Delete**, each with a confirmation that
names what goes, says "Attachments and history are not touched", and warns
when the page open now uses that profile. The pick belongs to the project of
the thread on screen (`defaults` per context) and is kept across restarts.

Another profile is another browser: switching closes the guest and opens the
same address in the new profile, under the browser's one-at-a-time lock, so it
waits for an agent's page action to finish. Agents keep using whatever page the
user's choice opened (open_preview opens in the tab's current profile) and have
no way to pick a profile. Clearing or deleting the profile of the open page
closes the page first, then removes only that profile's folder, waiting up to
8 s for WebView2's browser process to let go; a folder Windows still holds is
reported ("Windows still has the profile's files open … try again") and the
profile stays listed. A damaged `profiles.json` is set aside as
`profiles.json.damaged`, never overwritten, and the folders stay. At most 20
profiles, names of 1–40 characters, unique.

Importing a session: the dialog lists the other browsers on this machine and
why their sessions cannot be imported — Chrome, Edge and Brave protect cookies
with app-bound encryption only they can read (found in their `Local State`);
QuantPilot does not work around it. Firefox is listed as not supported in this
version (or not installed). Nothing is read from other browsers; signing in
once in a profile is the supported way.

Verified in the isolated native app (WebView2) with a disposable local app
that sets a one-day login cookie and logs the cookie of each page load: a new
profile "QA Work" signed in, a second profile "QA Two" saw no session, signed
in on its own, and "QA Work" still had its own; Temporary had none. After
restarting the app the project's pick ("QA Work") was back and still signed
in, "QA Two" too, Temporary not. Deleting "QA Two" while its page was open
(after a cancelled first try) closed the page in 0.5 s, removed its folder and
set the project back to Temporary, and "QA Work" kept its session; **Clear
data** on "QA Work" signed it out and kept the profile. The import list showed
Chrome and Edge as not supported (app-bound encryption), Brave and Firefox as
not installed. Unit tests: names, defaults, clear/delete touching only their
folder, a held file (Windows share mode 0) reported and the profile kept, a
damaged list set aside, and the import report from fake browser folders.
QA-harness note: the CDP port given to all WebView2 environments through
`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` can move to a profile's browser
process after another profile's process ends; the app itself kept working
(window captured with PrintWindow). Users have no remote-debugging port.

## 30. Parity follow-up: storage inventory and cleanup rules (19/23)

Settings → **Storage** (`Storage.vue`, `storage.rs`) measures what QuantPilot
keeps: messages and history (`pilot.db`), saved patches (turn changes with
their frozen sides), attachments and screenshots (with how much belongs to
deleted threads), extracted document text (parity 16), session files, browser
profiles (parity 18), the skill library and managed worktrees (counted only).

**Cleanup rules**, all off by default: saved patches of turns older than
30/90/180/365 days lose their text and frozen sides while the file list, line
counts and a note stay ("The saved patch was removed by storage cleanup on
<date>; the file list and line counts are kept." — the Diff list's `!` shows
it); attachments in no message, queued message or draft older than 7/30/90
days; document text not used for 30/90 days (read again when needed, every use
refreshes it); and the folders of deleted threads (deleting a thread kept its
attachments folder until now). **Preview cleanup** is the dry run: the items
with their reason and size, grouped, and what stays and why ("in a message or
queued message", "in a draft", "newer than 30 days", "its thread is working",
"not QuantPilot's own folder"). **Clean up now** asks once and removes only
the items the preview showed, from a plan computed the same way; the result
names what could not be removed (a lock), which is tried again next time.
**Run these rules once a day** is an explicit switch; the run starts a minute
after the app loads its settings, at most once per 23 hours.

Never removed: message text and history (handoffs and history reads depend on
them), browser profiles, the skill library, worktrees (Worktree cleanup with
its own guards), anything a message, queued message or draft names (drafts
live in the webview; the page and the daily run send their paths), anything of
a thread whose turn runs, and anything in a folder that is not a thread's.
Every target is resolved and must lie strictly inside `attachments/` or
`sessions/` of the pilot data folder; a target outside, the folder itself or a
link is left alone. Removing what is already gone counts as done, so an
interrupted run is simply repeated. Each run adds a line to
`storage-cleanup.log` (newest 200). Space freed inside `pilot.db` is reused
for new history; the file does not shrink (no VACUUM), the write-ahead log is
checkpointed after patches are cleared. Recovery: removed files and patch text
cannot be restored; checkpoints in git (rewind) are not touched.

Verified in the isolated native app on a long-history fixture (the
5,601-record thread with 330 turns of saved patches, 300 of them 200 days
old; every attachment aged 40 days, document texts 100 days; one stray
screenshot in a live thread) after deleting a thread with seven attachments
the normal way. Before: database 6.4 MB, saved patches 3.0 MB (663 rows),
attachments 1.8 MB (32 files, 575 KB of the deleted thread), document text
154 KB. With patches 90, unsent 30 and documents 30 days the preview listed
314 items (4.1 MB): the deleted thread's folder, 7 unsent files (each checked:
no reference anywhere in the database), 6 document texts and 300 old turns;
18 referenced attachments and 32 recent patch groups stayed. **Clean up now**
removed exactly those 314 (log line written); after: saved patches 284 KB
(63 recent rows untouched), attachments 672 KB (18 files, the sent ones still
there), document text 0; the old turns kept their file lists and line counts
with the note, and a second preview found nothing. Unit tests: dry run and
cleanup agree, items not shown are skipped, a repeat is harmless, every kind
of protected file stays, and targets outside the data folders, the folder
itself and `../pilot.db` are refused.

## 31. Parity follow-up: interaction polish and Windows attention (20/23)

**File references and links in replies** have a menu (`LinkMenu.vue`):
right-click, or Shift+F10 / the Menu key on a focused one (file references
are focusable now; Enter opens them). A file: **Open in the Files tab** (at its
line), **Open in VS Code / Cursor at line N** (the editors that are installed),
**Show in Explorer**, **Copy path**. A link: **Open in the system browser**,
**Open in the Browser tab**, **Copy link**. Without an open thread the
thread-bound rows are disabled and say why. The first row has the focus as the
menu appears (input works during the entrance); arrows move, Enter runs,
Escape or Tab close it and give the focus back to the reference; a click
outside or the window losing focus closes it. The exit fades in 90 ms with
`pointer-events: none`, so a closing menu never catches a click; reduced motion
turns both transitions off.

**Taskbar**: the button carries a badge with the number of threads waiting on
the user (Windows overlay icon, an amber disc with the count, "9+" above nine;
the badge count on other systems), driven by the same waiting count as the
module badge and the sound, and cleared at zero and when the module unloads.
The flash when a thread starts waiting in the background was there already;
now bringing the suite forward after such a flash opens that thread — once,
and only within 30 minutes of the flash. Toasts keep their setting and their
5-second throttle per thread.

**Bulk snooze**: the range gesture on the rows' action buttons (drag over
several threads) settles them as before; holding **Alt** at release opens
"Snooze N threads until" with 1 hour, 3 hours and Tomorrow (each with its wake
time) and Cancel, keyboard-operable. The snooze is one undo step ("Snoozed 3
threads until …" with Undo). Waking a range of snoozed threads and unsettling
a range of settled ones now have undo labels too. Snoozing never stops a
running thread (it only moves the row).

Verified in the isolated native app (1400×900): on a real Claude Haiku reply
the reference `qa13.md:3` was focusable, its menu listed the Files tab, VS
Code, Cursor, Explorer and Copy path with the first row focused; Down moved
the focus, Escape left a fading menu with `pointer-events: none`, then none,
and the focus was back on the reference; the keyboard-opened menu sat 4 px
under the reference; closing and reopening at once left exactly one menu;
**Open in the Files tab** opened `qa13.md` at line 3; the link's menu had its
three rows and a click outside closed it (external openers were not clicked).
A range over three new threads with Alt opened the snooze menu with the focus
inside; "1 hour" snoozed all three with an undo toast, and Undo woke all
three. The badge command ran; its picture was checked from the unit test's
PNG (1, 3, "9+"). Not checked natively: the overlay on a real taskbar button
and the activation after a flash — the QA window has no taskbar button and
never takes the focus (the user's main screen stays undisturbed); both are
covered by unit tests (`take_attention` once and within 30 minutes, the badge
pixels). Other window sizes and Windows scaling were not run.

QA note: the QA profile had toasts on ("unfocused") — with a never-focused QA
window, earlier runs with waiting threads could show a toast on the main
screen. The QA profile now has toasts off.

## 32. Parity follow-up: measured performance (21/23)

**Setup.** Isolated native QA app (debug Rust build, Nuxt dev frontend —
unminified, so absolute numbers are higher than a release build; before and
after use the same build mode), window 1400×900, a 32-core Windows 11
machine. Fixture `artifacts/quantpilot-parity-21/fixture.py`: "Parity21 perf"
with 400 turns and 1,200 messages (replies of ~1.6 KB with headings, lists, a
code block, a table every 7th, a Mermaid diagram every 20th; an image
attachment every 10th user message; a tool call per turn) plus nine more
threads. `perf.js` drives the real thread store: open; scroll to the top so
every older page loads; 20 s of **ten simultaneous streams** (the open thread
and nine others, 50 deltas/s each, ~17 KB reply) while typing into the
composer every 250 ms; then the worst case — ten streams for 10 s while
scrolled to the top with all 1,200 messages loaded. `switch.js` does 2 × 10
rounds of leaving the module and coming back; `gc.mjs` collects garbage and
counts heap, DOM nodes and listeners on `window`/`document`; `proc.ps1` samples
the working set and CPU of the app's process tree. Raw results: `baseline/`,
`after1/`, `after2/`, `after3/` (`perf.json`, `gc-*.json`, `idle-*.json`,
`five*.json`, screenshots). Source: baseline `db86fbc`, after = this change.

**Budgets** (set from the baseline, before optimizing): during ten streams
frames p95 ≤ 33 ms and max ≤ 100 ms, input p95 ≤ 25 ms, stream-to-paint p95
≤ 100 ms, long tasks ≤ 2 s in 20 s; scrolling through the whole history max
frame ≤ 250 ms; 20 module switches grow the heap ≤ 5 MB with a constant
listener count.

| Measure | Baseline | After | Budget |
| --- | --- | --- | --- |
| Ten streams: frames p95 / max | 75.2 / 116.8 ms | 29.3 / 70.6 ms | ≤ 33 / 100 ✓ |
| Ten streams: input p95 | 20.9 ms | 6.5 ms | ≤ 25 ✓ |
| Ten streams: stream-to-paint p50 / p95 | 108 / 172 ms | 17.6 / 62.7 ms | p95 ≤ 100 ✓ |
| Ten streams: long tasks in 20 s | 187, 11.6 s | 2, 0.12 s | ≤ 2 s ✓ |
| Deltas applied per thread in 20 s (of 1,000 sent) | 406 | 731 | — |
| Worst case (all 1,200 loaded, at the top): frames p95 / max | 66.8 / 125.2 ms ¹ | 4.4 / 38.1 ms | ✓ |
| Worst case: long tasks in 10 s | 61, 3.6 s ¹ | 0 | ✓ |
| Scroll through all pages: frames p95 / max | 65.1 / 622 ms | 41.8 / 200.3 ms | max ≤ 250 ✓, p95 over 33 |
| Scroll: long tasks | 52, 9.8 s | 33, 2.8 s | — |
| DOM nodes while streaming at the latest | 151,228 | 31,878 | — |
| JS heap after the streams | 268 MB | 164 MB | — |
| Listeners on `document` | 1,613 | 13 | constant ✓ |
| 20 module switches: heap / listeners | +2.4 MB / +1 | +2.0 MB / +1 | ✓ |
| Working set of the app's processes, idle → after streams | 1,088 → 1,781 MB | 933 → 1,293 MB | recorded |

¹ measured with the second build (after the first two fixes); the baseline
build had all 1,200 messages mounted during its 20 s run, too.

**What changed.**
- **Streaming Markdown renders only the unfinished tail** (`utils/stream.ts`,
  `renderStreaming`): the finished blocks (up to the last blank line outside a
  code fence) are parsed and sanitized once and kept; each frame parses only
  the tail. The final paint parses the whole message, so a loose list or a
  reference split by a blank line comes out exactly right.
- **One pair of document listeners for all messages**
  (`utils/markdownEvents.ts`): the quote feature (parity 14) had added a
  `selectionchange` and a `keydown` listener per rendered message — 1,600
  with 400 turns, all run on every key. Now the message holding the
  selection gets selection changes (and the one that held it before), the
  last one the keys.
- **Bounded retention**: returning to the latest messages lets go of older
  pages loaded while scrolling up (`trimOlder`, keeping ~240 items, whole
  turns) — they load again on the way up; Find and reveal (parity 15) use
  bounded excerpts and are unaffected. Not while Find is open or an excerpt
  is shown.
- **Code is coloured when it comes near the screen** (shared
  IntersectionObserver, 800 px margin) instead of every block of every
  newly loaded page at once; a repaint or unmount stops watching its blocks
  so no detached nodes are kept.
- **A streaming message far off screen is not repainted** until it comes
  within 1,200 px; the final paint always happens.

**Real agents**: five Claude Haiku threads streaming ~3–4 KB replies at once
finished in 11–18 s each; meanwhile frames p95 4.4 ms (max 96 ms) and input
p95 6.6 ms; the app's process tree used 123 % of one core and a 1,332 MB
working set (the Claude CLIs not included). Codex is not signed in on this
machine — no Codex measurement.

Functional checks after the change: code blocks on screen were coloured (3 of
236 in the loaded window, the rest when reached), a Mermaid diagram drew when
scrolled to, selecting text showed **Cite in composer** and Ctrl+Shift+Q opened
it (shared listener), Find reached a turn trimmed from the view, and the
streaming phases above had selection, scroll and typing active. Not done:
idle CPU of the baseline (the first sampler read CPU time after sleeping and
reported 0 — fixed before the after-runs: 3.4 % of one core idle, 6.9 % after
the streams finished); scrolling through all 1,200 messages still has a frame
p95 of 42 ms (pages of 60 items mounting at once) — virtualizing the timeline
was not needed for the stream budgets and is left as a follow-up.

## 33. Parity follow-up: native integration baseline and provider matrix (22/23)

**Setup.** The isolated native QA app (`artifacts/qa-native/qa.sh`: own
identifier `com.quantable.quantsuite.qa`, `QUANTSUITE_HOME` in the artifacts,
frontend :4188, WebView2 CDP :9228, window on the second monitor, never
focused, toasts off), debug build of `f8b9b81` plus this change; the real
user's Claude Code 2.1.316 login; the project "hello world qa" (a clone of
octocat/Hello-World from parity 10) for Local and Worktree runs. Scripts and
raw JSON: `artifacts/quantpilot-parity-22/` (`lib.js`, `steps.py` and one
`<step>.json` per step). The user's own app, port 1420 and sessions were not
touched.

**Fixed here** (found by codex-1 in parity 02/03 runs, recorded on the card):
- Codex's catalog offers a synthetic `default` model; it was sent literally on
  thread start, turn start and model switch ("The 'default' model is not
  supported when using Codex with a ChatGPT account"), and switching back did
  not repair the session. Now `default`/empty means no override on start and
  turn, and a switch back names the catalog's resolved default model so the
  session really returns (`model_override`, unit test). Live Codex check:
  **blocked** — Codex is not signed in on this machine.
- QuantMCP's `quantsuite.*` tools had no MCP `annotations`, so clients could
  not tell reads from writes; Claude Code's plan mode refused
  `read_thread`/`read_thread_item`. Each capability now carries
  `readOnlyHint`/`destructiveHint`/`openWorldHint` from its side-effect class
  (`read`/`compute` read-only, `external` destructive and open-world).

**Matrix** (✓ passed natively, ⚠ passed with a note, ✗ failed, ⛔ blocked):

| Path | Result | Evidence |
| --- | --- | --- |
| QuantMCP identity from a launched Claude thread, in plan mode | ✓ `list_threads` (`"you": true`) and `read_thread` completed, the reply named the thread's title | `a_identity.json` |
| Approval round-trip (Supervised; an MCP write tool) | ✓ request shown (thread "waiting"), allowed, tool ran; the rename itself refused by design (user-named thread) | `b2_approval.json` |
| Built-in file tools in Supervised mode | ⚠ no prompt: the user's global Claude settings allow Write/Edit/Bash | `b_permission.json` |
| Local mode in a git project: real edit, turn changes kept | ✓ `qa22-local.txt` created, changes item `A` | `b_permission.json` |
| Interrupt mid-stream, then continue | ✓ turn `interrupted` after 327 chars, next reply `RESUMED` | `d_interrupt_queue.json` |
| Queue while running | ✓ queued message ran by itself after the turn | `d_interrupt_queue.json` |
| Managed Worktree: real edit only in the worktree | ✓ branch `quantpilot/p22-worktree`, worktree README changed, checkout unchanged | `f_worktree.json` |
| Saved diff of the turn | ✓ `README M +2 −1`, saved patch shows the added line | `f_saved.json` |
| Rewind with files | ✓ README back to the original, worktree clean | `f_revert.json` |
| Kill the agent process mid-turn | ✓ turn `failed` with "The agent process ended (exit code -1)…", the next turn ran (`ALIVE`) | `e_after.json` |
| Restart: history and queues | ✓ item, message and queue counts identical after restart (the rewound thread empty, as rewound) | `h_before.json`, `h_after.json` |
| No agent processes left after the app stops | ⚠ 0 left — the harness ends the process tree; a graceful exit was not exercised | log |
| Questions with files, quotes, browser profiles, documents, storage | ✓ in parity 14–19 runs (same harness) | sections 25–30 |
| Browser screenshot delivered to the agent | ✓ in parity 06 (image content through QuantMCP) | section on parity 06 |
| Toast activation, Explorer drop/open, taskbar activation | ⛔ not run: they act on the user's main screen (the user plays on it); unit-tested pieces only | — |
| Fork, model/provider switch, TUI round-trip, goals, subagent/task display, Codex async questions | not re-run in this pass (ADE cards' evidence stands); left to 23/23 | — |

**Providers** (installed / version / live inference):

| Provider | Installed | Live | Status |
| --- | --- | --- | --- |
| Claude Code | 2.1.316 | ✓ all paths above | working |
| Codex | codex-cli 0.166.0 | ⛔ "Not logged in" | sign in with `codex login`; the default-model fix is unit-tested only |
| pi | 0.84.3 | ✗ external | "No API key found for the selected model" — log in with `/login` in pi |
| omp | 18.2.6 | ✗ external | its model is a local LM Studio one (`prism-ml/bonsai-27b`), not running: "Unable to connect" |
| OpenCode | 1.3.15 | ✗ external | the CLI exits 1 printing a failing SQL statement of its own token table (OpenCode's local database) |
| GitHub Copilot | VS Code shim | not run | could open VS Code on the main screen |
| Gemini CLI | not installed | — | install to use |

Every failing provider ended its turn cleanly with the provider's own message
as a notice; none of them is an adapter defect found here. Board-linked agent
start and GitHub writes were not exercised (no test authorization for writes).

## 34. Pre-1.1.0: the QuantPilot chat embedded in QuantCanvas

`PilotEmbedChat` (`components/Embed/Chat.vue`) mounts the same conversation
component as the /pilot page — timeline, composer, approvals, questions,
queue, stop — for one thread, outside the pilot layout. It loads the pilot
stores if QuantPilot was never opened, and takes live events for its thread
while its host is in front (warm cache: on activation, off on deactivation,
the thread reloaded on return). Events the /pilot layout also applies are
harmless: the store applies each sequence number once. Several embedded
chats run side by side, each bound to its own thread. A host is provided
through `utils/host.ts` (`PILOT_HOST`): the thread the chat shows and the
host's own open action; replies use the host's thread for file references
and diagram exports instead of the /pilot page's active thread, and the
reference menu offers the host's action ("Open in a File window") in place
of the Files tab and Browser tab.

QuantCanvas gets a window type `chat` (start page **Chat**, context menu **New
QuantPilot Chat**). Without a thread it offers **New thread in this
workspace** (the canvas's folder as the QuantPilot project, General for the
general canvas) and a list of existing threads (this workspace's first). The
window keeps only `chatConfig.threadId` in the canvas layout, is titled
"Chat · <thread title>", and closing it leaves the thread alone. A file
reference in the chat opens a File window beside it (relative paths against
the thread's folder). Plain wheel scrolling stays in the transcript (canvas
zoom is Ctrl+wheel, as for every window); Space and Ctrl+V typed in the chat
stay there (no canvas panning, no window paste). A window whose thread was
deleted says so and offers the picker. QuantCode and QuantConsole hosts are
follow-ups (the component is host-agnostic).

Verified in the isolated native app on the "hello world qa" canvas: a new
chat window started a thread in that workspace (project and folder right),
a Claude Haiku reply streamed into it and named the window; wheel over the
transcript left the canvas zoom unchanged; `src/app.ts:12` in a reply opened
a File window beside the chat and its menu read "Open in a File window"; a
second window attached an existing thread with no crosstalk between the two;
a message sent from the /pilot page appeared in the canvas chat after
switching back, and a turn started in the canvas (with an MCP approval shown
and allowed in the canvas chat, Supervised mode) appeared on the /pilot page;
after a restart both windows came back with their threads, and closing one
left its thread in QuantPilot.

## 35. Parity 23/23: completion matrix and delivery handoff

Source state: `main` at `69abb0e` (all 23 parity cards and the pre-1.1.0
embed card merged) plus this card's upgrade test. Version in the source:
1.0.42 — unchanged; this milestone does not bump the version and is not
1.1.0. **Merged ≠ packaged ≠ installed ≠ released:** nothing of parity 01–23
or the embed is packaged, installed or released. The published v1.0.42 and
the installed app (`%LOCALAPPDATA%\QuantSuite`, schema 11) predate all of it.

### Audit findings → delivered work

| Audit finding (report of 2026-10-30) | Card | State | Verification |
| --- | --- | --- | --- |
| 1 Saved turn diffs miss shell writes; shared-file attribution | 01 | merged | unit + native (§12–13 sections of the series; track_files tool, reported vs snapshot sources) |
| 2 Handoffs lose context; omitted history not retrievable | 02, 03 | merged | codex-1 native QA (paged history, handoff budgets); plan-mode read tools fixed in 22 |
| 3 Native integration and performance unverified | 21, 22, 23 | merged | measured before/after (§32), native matrix (§33), final regression (below) |
| 4 No agent-operated browser / run-and-preview | 06, 07 | merged | native: agent read/click/fill/capture, run_project + readiness |
| 5 MCP URL authorization and rich forms | 05 | merged | unit + native forms; live URL step needs a server that asks for one |
| Rich queued messages | 04 | merged | codex-1 native QA; queue verified again in 22/23 |
| Import of CLI conversations | 08 | merged | native import (Claude); Codex import fixture-only (not signed in) |
| Skill library | 09 | merged | native, project scope only (the user's skill folders untouched) |
| Human PR review, browse/clone | 10, 11 | merged | browse/clone native; **review submission not executed** — needs an authorized test PR |
| Media in the transcript | 12 | merged | native viewer, actions, missing state |
| Mermaid, quoting replies | 13, 14 | merged | native |
| Complete Find | 15 | merged | native on 5,601 records |
| Document extraction (Windows) | 16 | merged | native with real PDFs/DOCX |
| Files in question answers | 17 | merged | native with a real AskUserQuestion |
| Browser session persistence | 18 | merged | native profiles; imports explicitly unsupported (app-bound encryption) |
| Storage retention | 19 | merged | native dry run = cleanup on a long-history fixture |
| Native polish (menus, attention, bulk snooze) | 20 | merged | native; taskbar overlay/activation unit-tested only |
| Excluded large features, newer upstream ideas | roadmap card 91212da8 | **not decided** | separate decision card, intentionally open |
| — (user request) chat in QuantSpace modules | embed 6bfac02b | merged | native in QuantCanvas (§34) |

### Combined checks (this card, on the merged source)

`cargo clippy -p tauri-plugin-pilot -p tauri-plugin-mcp --all-targets -D
warnings` clean; `cargo test` pilot 225 passed (24 ignored: fixture/upgrade
runs), mcp 126 passed; `npm run check:ts` clean; `npm run test:pilot` 88
passed (75 at the audit); module manifests valid. **Not run:** the full
`npm test` / release checks (`npm run check`, `check:release`,
`test:distribution`, `test:release`) — recommended once before packaging.

**Upgrade** from the installed build: a read-only SQLite backup copy of the
installed app's `pilot.db` (schema 11, v1.0.42), enriched with threads,
messages with attachment paths, a three-item queue, turns and a worktree link
(`artifacts/quantpilot-parity-23/make_v11.py`), opened with the current build
(`upgrade_keeps_threads_messages_queue_and_worktrees`, run with
`QP_UPGRADE_DB`): schema 11 → 16, every thread id/title/worktree link,
message (with attachment paths) and the queue order unchanged; opening again
changes nothing. Settings files gain `storage` (rules off) with defaults.
New data appears only when used: `attachments/.documents/`,
`browser-profiles/`, `storage-cleanup.log`.

**Rollback**: keep a copy of `%USERPROFILE%\.quantsuite\modules\pilot\`
before the first start of a new build. An older build does not downgrade the
schema; going back means restoring that copy (threads created meanwhile are
then lost). Checkpoints and worktrees live in the repositories and are not
touched by an upgrade.

### Final native regression

Combined build (`main` + this card's test, debug Rust, Nuxt dev frontend) in
the isolated QA app with Claude Haiku; raw results in
`artifacts/quantpilot-parity-23/regress/` (`run.sh`, one JSON + PNG per step).

| Step | Result |
| --- | --- |
| QuantMCP identity in plan mode | ✓ list_threads + read_thread completed, own title named |
| Approval round-trip (MCP write tool, Supervised) | ✓ thread waiting, allowed, tool completed |
| Interrupt, continue, queued follow-up | ✓ `interrupted`, `RESUMED`, queued message ran by itself |
| Managed worktree edit, saved diff, rewind with files | ✓ branch `quantpilot/p23-worktree`, `README M +2 −1`, restored |
| Kill the agent mid-turn | ✓ turn `failed` with the exit notice, next turn `ALIVE` |
| Find (5,601 records: oldest, folded, chip, plan, Unicode, 26 matches) | ✓ every count right, walk with wraparound, ≤ 50 items in the DOM |
| PDF + DOCX context | ✓ HERON-42, KESTREL-9, OSPREY-7, the Japanese text |
| Files in question answers (option+image, image only, text+PDF) | ✓ all three reached Claude; composer draft untouched |
| Reply link menu | ✓ rows, focus return, one menu on rapid reopen, Files tab opened |
| QuantCanvas chat (new thread, file window, second window) | ✓ right project and folder, reply streamed, no zoom on wheel, no crosstalk |

No unexplained failures. Measured performance (§32) and the provider matrix
(§33) were not re-run; nothing in between touched their code paths.

### Remaining limitations and next user actions

- Codex: not signed in on this machine — every Codex path (and the
  default-model fix) is unit-tested only. Sign in (`codex login`) to verify.
- pi / omp / OpenCode: external setup (API key; a running LM Studio model;
  OpenCode's own database error) — see §33.
- PR review submission (11): needs an explicitly authorized test PR.
- Taskbar overlay and activation after a flash, toasts, Explorer drag/open:
  not exercised natively (they act on the user's main screen).
- Scrolling through every loaded page of a 1,200-message thread still has a
  frame p95 of ~42 ms; timeline virtualization is a possible follow-up.
- To try it: stop the app's own dev session first, then run the test command
  of this card's worktree (in the completion report), or in the main checkout
  `npm run tauri:dev` as usual. To deliver: follow docs/RELEASING.md once
  (version, checks, package) — only with the user's release go-ahead; not
  1.1.0.
- The QuantSuite 1.1.0 UI overhaul stays its own future milestone.

## 36. Round 2 · 01/12: agent logos instead of letter marks

Card 6aacbda1. The sidebar's two-letter marks (`Cl`, `Cx`, `Oc`, `G`, `π`,
`om`, else the first letter) are gone; an agent is identified by its logo
everywhere QuantPilot names one.

**One component.** `PilotAgentIcon` (`components/AgentIcon.vue`) takes an
adapter id, a size and an optional status. `utils/agentIcons.ts` (pure,
node-tested) decides the glyph: a bundled mark for a known agent, the user's
image for a custom agent that has one, else the custom agent's first letter;
an id that is still unknown (status not loaded, nothing reported) shows a
neutral `?` — never another agent's logo; a V2 terminal row without an agent
shows a terminal glyph. `mono` marks (Codex/OpenAI, OpenCode, Copilot, pi)
are drawn as a mask in the text color; the others keep their colors and
their own tiles. The status is a ring *around* the logo: working = a turning
green arc (static under reduced motion), waiting = amber, limited = dashed
amber, error = red.

**Marks.** 18 local files in `modules/pilot/app/assets/agents/` — Claude,
Codex, Gemini, OpenCode, Copilot, pi, omp and the agents of round-2 card 11
(Cursor, Kilo, Hermes, OpenClaw, Freebuff, Grok, Antigravity, Kiro, Droid,
Cline, Devin). All from Orca (MIT, commit `1d2e7f6f`): its JSX glyphs
converted to plain SVG, its bundled favicons copied; `NOTICE.md` there lists
the source of every file and the license, and says how to add one. Nothing
is fetched from the network.

**Where.** Sidebar rows (and the drag ghost), the composer's agent control
(one logo per agent of a fan-out), the model picker's agent list, the thread
header chip, Settings → Providers (each agent's header) and a project's
"Agents offered", the board (the chip beside the board and every thread chip
on a card), GitHub thread rows and the Agent review button (the agent a new
thread there starts with: the project's pin, else the default), Usage
(limits cards, the models table), both Needs-you lists, the Vitals agent,
subagent rows in the details and the inspector, a turn another agent
answered, the import dialog's sources, threads in the command palette, and
the header of a QuantPilot chat window in QuantCanvas. The New-thread menu
lists projects only; card 02 adds the agent picker there.

**Custom agents.** Settings → Providers → Your own agents: *Icon…* picks a
PNG, JPEG or WebP (at most 256 KB, 1024 px a side; decoded once to check
it), stored as `<data>/agent-icons/<id>.<ext>`; *Use the letter* removes
it. Commands `pilot_agent_icons` (id → data URL for existing custom agents),
`pilot_agent_icon_set`, `pilot_agent_icon_clear`; saving the settings
deletes the images of removed agents. Ids are the settings' slugs, so an
image can never be written outside that folder.

**Checks.** `cargo clippy -p tauri-plugin-pilot --all-targets -D warnings`
clean; `cargo test` (agent_icons: valid image stored and listed, non-image /
oversized / too large refused, path-like ids refused, a new format replaces
the old, prune on removal); `npm run check:ts`; `npm run test:pilot` 96
passed (new `scripts/test-pilot-agents.mjs`: every mark has its file and a
NOTICE row, files are small and inert SVG, aliases, the neutral glyph,
custom image/letter, slug parity with `settings.rs`, no `MARKS` left).

**Native QA** (isolated QA app, monitor 2, screenshots in
`artifacts/quantpilot-r2-01/`), at 100 % and 150 % device scale: a gallery
of all 18 marks at 12/14/16/20 px on dark and light surfaces; sidebar with a
real running Claude turn (working ring), waiting and limited rows
(client-side states for the shot) and failed rows; custom agents with an
image and with a letter (rows, picker, palette, board, settings); a refused
non-image and a refused `../` id; the image survives a reload and is pruned
when the agent is removed; composer, picker, header, Vitals, Needs you,
Usage limits and models, board chips with rings, the QuantCanvas chat
header. The suite has one theme (dark, decision 2026-08-14), so "both
themes" was checked with the light-surface gallery. Not exercised natively:
the GitHub view (the QA project has no GitHub remote — type-checked) and
the drag ghost.

## 37. Round 2 · 02/12: one agent menu — logos, install state, model + effort

Card da158b14. The composer's model picker (two panes), the New-thread
menu (projects only) and the starters on board cards and in QuantCanvas
were separate choosers that listed only the enabled agents. Now there is
one menu, `PilotAgentMenu` (`components/Agent/Menu.vue`), in the style of
Orca's agent menu.

**Rows.** Every agent with its logo (card 01). A usable agent shows the
setup a pick really starts with — the thread's own model · effort · fast,
or for a new thread the project's pinned model, else the remembered one,
with the effort only where the agent takes one (Claude Code, Codex). The
others stay visible, greyed, with the reason and a one-click fix:
*Not installed* (Set up…), *No chat on this host*, *Not signed in* (Sign
in…), *Off in Settings* (Turn on…), *Off in this project* (Project
settings…), and *Working — switch when done* while the thread's turn runs.
Fixes open Settings → Providers scrolled to that agent's card (or the
project's own settings). Footer: *Agent settings…* on the highlighted
agent. Rules in `utils/agentMenu.ts` (pure, tested).

**Submenu.** A row's chevron (or hover, Right) opens the agent's models
beside the menu: recently used first (per machine, five per agent), then
the catalog; each model row carries its effort levels as chips (`low med
high xhigh max`) and a *Fast* chip where the catalog says the model has
fast mode. A row click keeps the current effort when the model has it; a
chip picks model + effort (again = back to default). Typed model ids still
work; the CLI version and a refresh sit in the submenu.

**Fast mode.** Claude Code reports `supportsFastMode` per model; the
catalog keeps it (`ModelInfo.fast`), a thread stores `fast` (schema 17,
column added with every thread off), a Claude launch adds `--fast-mode on`
(the CLI's own option for teammates), and a change starts a fresh process
on the next turn like effort. Never remembered for new threads (it costs
more). The composer shows a *Fast* tag while it is on.

**Keys.** Typing filters agents; Up/Down; Right or the chevron into the
submenu; Enter picks (on a greyed row: its fix); in the submenu Up/Down,
Left/Right across the effort chips (Left from the model returns to the
agents), Enter, Ctrl+1…9, Shift+Enter / Shift-click for a fan-out; Escape
goes back, then closes. The menu is placed by its anchor (below, or above
for the composer, flipping when there is more room) and in a short window
it and the submenu take the full height and scroll.

**Where.** The composer's agent control (Ctrl+Shift+M; `Thread/ModelPicker.vue`
removed), New thread ▾ in the sidebar (a *New thread in* project choice on
top, *Import from Claude Code or Codex…* below; the + button and Ctrl+N stay
one click), *Start thread* ▾ on board cards, *New thread in this workspace*
▾ in a QuantCanvas chat window; fan-out lives in the composer's submenu.
`sidebar.newThread({ setup })` gives an untouched draft the picked setup
instead of making a second empty thread (the sticky setup is card 03).

**Checks.** clippy clean; pilot `cargo test` 230 passed (fast mode in the
catalog and the launch arguments, schema 16 → 17 keeps threads with fast
off); `npm run check:ts`; `npm run test:pilot` 105 passed (new
`scripts/test-pilot-agentmenu.mjs`: reasons and fixes in order, filter,
recent models, setup labels, placement incl. a short window, one menu
everywhere).

**Native QA** (isolated QA app, screenshots in
`artifacts/quantpilot-r2-02/`): composer menu with this machine's real
states (Codex not signed in, Gemini not installed); keyboard only from the
search to Sonnet 5.5 + low effort; Recent group after picks; Fast chip on
Opus 5.5 set `fast` and the composer tag, Haiku turned it off again (no
message was sent with fast mode); Sign in… on Codex opened Settings →
Providers on Codex; New thread ▾ in "hello world qa" with Sonnet + high;
Start thread ▾ on a board card with pi (prefilled, not sent); a 420 px
high window (menu above the composer, submenu full height with scroll);
fan-out of three models by Shift-click; the QuantCanvas chat window's ▾.
A live turn with fast mode was not run (it bills at a higher rate) — the
flag is covered by the launch-argument test.

## 38. Round 2 · 03/12: new threads keep the last setup (bug)

Card 81db2359. New thread, its menu, Start on a board card and the
QuantCanvas chat window started with Settings' defaults, and New thread
reopened an empty draft with whatever it was made with — never with what
the user had just picked.

**One resolver.** `Settings::new_thread_setup(project, available, label)`
(settings.rs) answers "what does a new thread here start with", value by
value: the project's pin → the user's last pick → Settings → General. It
returns agent, model and effort (both per agent), permissions, Local or
Worktree and the base branch (per project), which values the project
pins, every agent's model/effort (the agent menu's rows) and a notice
when the agent wanted is not available. `threads::create` uses it for
every way in (sidebar +, its ▾ menu, Ctrl+N, Ctrl+Alt+N, board Start,
GitHub issue/PR starters, QuantCanvas, fork and agent `start_thread` —
explicit values in a request still win); the menus read it through
`pilot_new_thread_setup`, so they show exactly what the thread gets.

**The last pick (`Settings.sticky`).** Recorded by the app itself, not the
page: `pilot_thread_update` (agent, model and effort per agent,
permissions), `pilot_thread_set_workspace` (Local/Worktree, the base per
project) and a menu pick that creates a thread (`remember`). Fast mode is
never carried over. Saving Settings keeps the app's record (the page's
copy may be older) — but a default changed in Settings → General
(agent, permissions, Work in) or a default model/effort in Providers
starts over from that value, so the change shows on the next thread
(`Settings::merged_into`). General explains it in one line.

**Drafts.** New thread opens an untouched draft (a chat nobody wrote in,
untitled, not settled, not snoozed — `isUntouchedDraft`) instead of making
another, and `pilot_thread_apply_setup` first gives it the current setup
or the one just picked, so it never shows stale choices; a thread that
has started is never changed. There is no "move a draft to another
project" in QuantPilot; a new thread in another project gets the same
last pick.

**Fallback.** When the agent wanted is not installed, turned off in
Settings, off in this project or gone, the thread starts with the
default (or the first offered agent) and its composer says so until the
first message: "pi is turned off in Settings — this thread starts with
Claude Code." (Dismiss). The project's own values carry a *project* tag in
the agent menu.

**Agents starting threads.** `quantsuite.pilot.start_thread` keeps its
rule — the caller's agent and permissions, no more — and takes the model
and effort for that agent from the resolver.

**Checks.** clippy clean; pilot `cargo test` 233 passed (new: the layered
resolver incl. per-agent model/effort and per-project base, pins named;
fallback reasons and notices; saving Settings keeps or restarts the last
pick); `npm run check:ts`; `npm run test:pilot` 106 passed (draft reuse
rules).

**Native QA matrix** (isolated QA app, `artifacts/quantpilot-r2-03/`):
thread A in "hello world qa" changed to Sonnet + high, Plan, Worktree →
sidebar +, the ▾ menu (row and pick), Ctrl+N, Ctrl+Alt+N (no project),
board Start and the QuantCanvas chat window all started with Sonnet ·
high · Plan · Worktree; + twice reopened the same untouched draft; after
picking Opus + medium in A the draft opened with Opus + medium; a project
pin (Claude · Haiku · Supervised) won and showed *project*; pi as the last
agent, then turned off → Claude with the notice in the composer; a full
app restart gave the identical setup and notice. GitHub starters use the
same `create` (the QA project has no GitHub remote) and `start_thread`
needs a running agent's token; both are covered by the shared path, not
run natively. The QA profile was set back afterwards.

## 39. Round 2 · 11/12: every agent installed here

Card f13a144b. Orca listed Cursor, Kilo, Hermes, OpenClaw and Freebuff
on this machine besides the agents QuantPilot had; T3 and PandaCode added
Grok, Antigravity, Kiro, Droid, Cline and Devin.

**One table instead of code per agent.** `adapters::EXTRAS` describes each
added agent: executable names, the ACP arguments when verified, what a
terminal session starts it with, its npm package or self-update, where to
get it and its own sign-in command. Ids avoid the editors' commands inside
the session terminal (`cursor-agent`, `kiro-cli`, `agy`). A user's own
agent with one of these ids is kept as `<id>-own` instead of vanishing.

**Protocols, checked against the installed binaries (2026-11-03).**

| Agent | Installed | Protocol | Seen |
| --- | --- | --- | --- |
| Kilo Code 7.0.37 | yes | ACP (`kilo acp`) | initialize + session/new with its model list |
| Hermes 0.21.0 | yes | ACP (`hermes acp`) | initialize; session/new asks for setup |
| OpenClaw 2026.2.9 | yes | ACP (`openclaw acp`) | initialize + session/new |
| Cursor 2026.02.13 | yes | terminal session | only a per-turn `--print` mode, no session protocol |
| Freebuff 0.0.174 | yes | terminal session | no protocol |
| Grok, Antigravity (`agy`), Kiro (`kiro-cli`), Droid, Cline, Devin | no | terminal session until verified | — |

**Menu and settings.** Every agent appears in the agent menu with its logo
(installed ones first, the rest greyed with *Not installed* and *Set up…*)
and in Settings → Providers with path, version, Update (npm, or the CLI's
own `update`) and a line saying how it runs and which command signs in.
QuantPilot starts no login probe for them (some open a browser) and never
handles credentials. An agent without a chat protocol is *Terminal
session* in the New-thread and board-Start menus: picking it makes a
terminal session that starts the agent right away (PowerShell and bash;
in cmd the user types the name) — the session row carries the agent's
logo. A chat thread cannot switch to such an agent (*Terminal session
only*).

**Errors say why.** ACP errors now carry the agent's own details
(`error.data`) instead of "Internal error".

**Native QA** (isolated QA app, `artifacts/quantpilot-r2-11/`): status of
all 18 agents; the New-thread menu (screenshot); real prompts — **none of
the three ACP agents could answer on this machine, each for a setup reason
outside QuantPilot**: Kilo accepts the session but its prompt never returns
(`kilo run` itself fails with "no providers found" — `kilo auth login`),
OpenClaw answers "gateway not connected" (its gateway is not running —
`openclaw gateway`), Hermes "No LLM provider configured. Run `hermes model`
… or `hermes setup`". Cursor and Freebuff opened their terminal sessions
and started (Cursor shows its own "Press any key to log in" there; nothing
was pressed). The test sessions were ended afterwards.

**Checks.** clippy clean; pilot `cargo test` (the table's protocols,
update plans, autostart only for a plain command name, `-own` renames,
error details); `npm run check:ts`; `npm run test:pilot` (terminal rows,
installed-first order, marks for every new id).

## 40. Round 2 · 12/12: smaller workflow gaps

Card 773d7338. Each item was checked against the current behavior first.

| # | Item | Outcome |
| --- | --- | --- |
| 1 | Marker when model/effort change mid-thread | Done: a centered divider "Model: haiku → sonnet · Effort: default → high" (also fast mode on/off) once the thread has messages (`setup_change`). |
| 2 | Thread tags + hide while recording | Done: tags in the thread menu (comma separated, ≤ 8, lowercase; schema 18), `#tag` on the row; the sidebar's **Record** switch hides threads tagged as set in Settings → General → While recording (default `private`) from the list, the palette and Needs you. |
| 3 | Pin/archive projects in the filter | Done: the filter is a menu; pinned projects first, archived ones folded under *Archived* (threads stay). Per machine. |
| 4 | Elapsed runtime on working rows | Done: "working 3m 05s", a one-second clock only while a row works. |
| 5 | Sent-message shapes | Done: images and videos above the bubble (videos as tiles), a message of only files as file cards without a bubble, long pasted text (> 14 lines or > 1,500 characters) folded as *Pasted text · N lines*, picked elements as *Snapshot*. Voice notes do not exist here — dictation (8) writes text. |
| 6 | Rich text ⇄ Markdown toggle | **Dropped**: the composer already edits Markdown source with live highlighting; there is no rich-text mode to switch to, and a WYSIWYG editor is not a small gap. |
| 7 | Usage filter by agent and model | Done: *All agents / Claude Code / Codex* and a model choice narrow totals, chart and the model table. |
| 8 | Dictation via QuantVoice | Done: a microphone in the composer uses the HUD's speech engine (`speech_start`/`speech_stop`/`speech-result`); the words land at the caret; Escape cancels; without QuantVoice set up it says so. QuantVoice's own push-to-talk takes stay its own. |
| 9 | Resume after restart | Done: the threads that were working when the app closed (recorded before the startup cleanup) are offered once, grouped by project — Resume all / the ticked ones, or Not now. |
| 10 | Thinking fold | Done: a reasoning item with text is a fold in the activity (one line closed, the whole text open). Claude Code 2.1.317 sends its thinking without text here, so it shows only where an agent sends the text. |
| 11 | Turn-end project checks, one repair | Done: a project action can be *Check after each turn*; after a turn that changed files it runs (captured, 10 min limit); a failure leaves a notice with the output and queues one repair message — never a second in a row; a repair turn that changes nothing ends the repair. |
| 12 | Mouse back/forward | Done: the mouse's back/forward buttons walk the thread history and no longer navigate the page. |

**Also fixed here:** eight synchronous Tauri commands that `check:isolation`
and `check:commands` flagged on `main` (import cancel, attention badge,
browser profiles, document cancel) now run off the main thread.

**Checks.** pilot `cargo test` 241 passed (marker text, one-repair rule and
output tail, tags clean-up, interrupted threads offered once); clippy clean;
`npm run check:ts`; `npm run test:pilot` 111 passed; `check:isolation`,
`check:commands`, module manifests green.

**Native QA** (isolated QA app, `artifacts/quantpilot-r2-12/`): a thread
with image + video, a files-only message and a 31-line paste (shapes as
described; file cards ellipsize in a narrow column); the model/effort
marker; tags and the Record switch (thread hidden from the list and the
palette, back when switched off; the switch survives a reload off); the
project filter with a pinned and an archived project; "working 5s → 7s"
on a running row; the thinking fold (an injected reasoning text — the
agents here send none); Usage filtered to Codex and one model; dictation
— without QuantVoice set up the composer says so, and a `speech-result`
event landed "summarize the last commit" after "Please" (the engine's
own recording was not used: no microphone in QA); a failing check
(`exit 1`) queued exactly one repair, the repair turn changed nothing and
nothing more was queued; the app killed during a turn and restarted →
the Resume dialog, Resume all continued the turn (then stopped);
mouse back/forward between two threads stayed on `/pilot`.

## 41. Round 2 · 04/12: a right panel that earns its space

Card 9423269a. The panel was a large face, one word, and read-only facts
that repeated the composer. `Thread/Details.vue` is now a working panel in
the same 220 px QRightPanel (shared geometry, no override):

- **Header:** a small face, the state and the live step, a ⋯ menu.
- **Agents now** (also without an open thread): every thread working or
  waiting, anywhere — who needs you first, then who works (elapsed time,
  live step), then finished results not seen yet. A pending approval is
  answered inline (Allow / Deny); a question or plan offers Answer; a
  click opens the thread. Threads hidden while recording stay out.
- **This thread:** agent and model, the context meter against the model's
  real window ("54k / 1M"), the 5-hour and weekly windows with their reset,
  cost, tokens, the last error; **Compact** (idle Claude threads),
  **Usage**, **Copy error**.
- **Changes:** every file the thread changed with summed +/−, newest first;
  a click opens its diff at the turn that changed it; **All changes**
  (branch for a worktree, else uncommitted). Before any saved change, the
  files the tools wrote.
- **Branch and PR:** branch or worktree, its pull requests and issues;
  **GitHub** opens the GitHub view (CI checks live there).
- **Tasks** (Copy as list), **Crew** (subagents → inspector; cards 07/08
  add workers), **Servers** (Open, Stop).

The ⋯ menu shows or hides each section; a section's title drags it to a
new place (pointer events; the order and hidden set are kept per machine).
Sections without content stay out. No new polling: the dev-server probe as
before, and a one-second clock only while a row works.

**Checks.** `npm run check:ts`; `npm run test:pilot` (new
`scripts/test-pilot-panel.mjs`: layout made whole, moves, drop slots,
Agents now buckets, changes summed, token pairs).

**Native QA** (isolated QA app, `artifacts/quantpilot-r2-04/`): empty
(no thread, nothing running), one running thread, five agents (three
working with elapsed times, one waiting on a Bash approval with Allow/Deny,
one on a question — states set client-side for the shot) plus a failed
Hermes thread with its error; Changes → the diff of `r212-check.txt`;
"Branch and PR" dragged above "Agents now" and kept; "This thread" hidden
and shown again. Side chats (card 10) will join as a section.

## 42. Round 2 · 10/12: side chats (/btw) beside the main thread

Card 223bc29e. Ask something on the side while the main agent keeps
working — without queuing into or steering its run.

- **Start:** `/btw <question>` in a thread's composer (a QuantPilot command)
  or **New side chat** in the panel's *Side chats* section. A side chat is a
  normal thread with `parent_id` set (schema 19): same project, agent,
  model, effort and permissions as the main thread; a worktree thread's side
  chat works in the same worktree. It has its own composer, model and
  effort controls, queue, approvals and Stop.
- **Where:** a sheet over the right edge of the conversation
  (`Layout/SideChat.vue`, the QuantCanvas embed component inside). Side
  chats are not in the sidebar or the palette; the main thread's panel lists
  them under *Side chats* (a click opens one). Agents now still shows them
  while they work.
- **Main thread context:** every side message carries a chip *Main thread
  context*: the main thread's title and state and the user and agent
  messages since the side chat last asked (an answer still being written is
  marked), newest kept, about 24 KB at most, saying how many earlier
  messages were left out (`utils/side.ts`). Which main messages a side chat
  has seen is kept per machine.
- **Actions** (on the latest answer, sheet header): *Insert into main* — a
  quote chip in the main composer (parity 14's quoting); *Open as full
  thread* — it leaves the main thread, joins the sidebar and opens; *New
  side chat*; *Close* (it stays listed). Escape returns to the main
  composer and stops nothing. `/btw` inside a side chat says to ask there
  directly.
- **Lineage:** kept across restarts; deleting the main thread makes its side
  chats threads of their own.

**Checks.** pilot `cargo test` 242 passed (the link survives a restart,
detaches, goes with the main thread); `npm run check:ts`; `npm run
test:pilot` (new `scripts/test-pilot-side.mjs`: the context block, what it
leaves out, titles).

**Native QA with real Claude Haiku** (`artifacts/quantpilot-r2-10/`): the
main thread wrote 500 numbers in words; meanwhile `/btw` asked for its
secret codeword → "PELICAN", a second side question → "500"; the main thread
kept working throughout; each side message showed the context chip;
*Insert into main* put "“500”" into the main composer as a quote; after a
full app restart both side chats were still listed under the main thread
and absent from the sidebar; *Open as full thread* moved one into the
sidebar and opened it.

## 43. Round 2 · 07/12: orchestrator and worker crews

Card 607d54dd; roadmap area 1 (architect/worker teams), adopted 2026-11-03
on the user's request (roadmap card 91212da8). An orchestrator thread splits
work into tasks and starts worker threads; they report back over one
message log. Ideas from Orca's orchestration (coordinator loop, task DAGs,
ask/reply, decision gates, retry breaker) and PandaCode's architect and
workers; no code copied.

- **Agent tools** (`quantsuite.pilot.crew_*`, `crate/src/crew.rs`, the
  caller-thread token checks of the other pilot tools):
  - orchestrator: `crew_dispatch` (tasks with key, spec, deps, agent or
    several `agents`, model, kanban `card`, `worktree`, `timeoutMinutes`;
    `maxParallel`, `peers`), `crew_reply` (answer a question or steer),
    `crew_retry` (new worker, optionally another agent or model — the swap),
    `crew_stop` (one task or the whole crew), `crew_gate` (a named decision
    for the user), `crew_wait` (the state; may wait up to 300 s for a change);
  - worker: `crew_report` (progress %, note), `crew_ask`, `crew_done`
    (summary, result, `failed`).
  Starting and steering threads are `write` (prompt in strict mode, like
  `start_thread`/`message_thread`); a worker's own bookkeeping and gates are
  `compute`, `crew_wait` is `read`.
- **No polling of agents:** after a dispatch the orchestrator ends its turn;
  QuantPilot wakes it — a message in its queue — with every question,
  result, failure, timeout and decision ("[Crew] …" with what to do next and
  the crew's state line). A worker that asks ends its turn and is woken with
  the answer. A worker whose turn ends without `crew_done` is *stalled* (or
  *failed* when its turn failed) and the orchestrator is told, with the
  worker's last words — a failed worker is never hidden.
- **Workers:** a new thread per attempt, the orchestrator's permission mode
  (never more), any installed chat agent, its own git worktree by default
  (Settings → General → Crews; the composer's fan-out path — `agents: [a, b]`
  runs one task once per agent, side by side, to compare and keep the
  winner). A task linked to a kanban card uses AGENT.md's card workflow
  instead of a second worktree; a worker without a worktree shares the
  orchestrator's checkout. Results merge through git (the orchestrator
  merges the branches, resolving conflicts per AGENT.md) or the card flow.
- **Graph and limits:** dependencies by key, checked for unknown keys and
  cycles; a task starts when its dependencies are done and a slot is free
  (Settings: 4 workers at once per crew; an atomic claim, so two settling
  workers never start one task twice); a waiting task says why (*waits for
  db*, *blocked: db did not finish*, *waits for a free slot*). Timeouts
  (Settings: 60 min; per task) stop a worker and fail its task. After three
  starts of one task only the user can retry it.
- **Rules:** workers talk to the orchestrator; `message_thread` from a
  worker to another worker or an outside thread is refused unless the crew
  allows peers (then it is logged as a peer message). A worker cannot lead
  a crew of its own.
- **The user's side:** the panel's *Crew* section (`Crew/Panel.vue`) shows
  the crew for the orchestrator and for each worker: objective, done of all,
  each task with its agent, state, progress bar and latest note or outcome;
  Retry, Swap agent (the agent menu), Stop per task; Message log, Resume,
  Stop crew (the cascade: every worker and the orchestrator's turn). Above
  the orchestrator's message box (`Crew/Bar.vue`): a decision gate with
  Approve / Reject and a note the orchestrator hears, and every worker
  waiting on an approval or question, answered right there with that
  worker's own request panel (T3 #28516). The log (`Crew/Log.vue`) lists
  every message whole — time, sender → receiver, kind — filter by task,
  Copy. Worker threads leave the sidebar (they are in the crew panel and
  the palette); the orchestrator's row says *crew 2/3* and *Decision* while
  a gate waits; gates count in the rail badge and send the "needs you"
  notification.
- **Data:** schema 20 — `crews`, `crew_tasks`, `crew_members` (the
  orchestrator and every worker attempt with its outcome), `crew_messages`
  (the log). Everything survives a restart; workers that were running when
  the app closed are *stalled* with a note, nothing restarts on its own
  (card 12's rule), the orchestrator or the user picks up (Retry, a reply,
  Resume). Deleting the orchestrator stops its workers and removes the crew
  (the workers stay as threads of their own); deleting a worker fails its
  task.
- **The `pilot-crew` event** carries a crew's snapshot after every change —
  card 08's live graph and message bus build on it.

**Checks.** pilot `cargo test` 253 passed (new: keys, the batch — duplicate
keys, unknown deps, cycles, compared agents, card tasks —, readiness and
wait notes, the turn-end rule, wake texts, the worker prompt, persistence
across a restart with the running → stalled rule, the schema 19 → 20
migration, the worker messaging rule); clippy clean; `npm run check:ts`;
`npm run test:pilot` 123 passed (new `scripts/test-pilot-crew.mjs`);
`check:isolation`, `check:commands`, `validate-modules`.

**Native QA with real agents** (isolated QA app, `artifacts/quantpilot-r2-07/`):
a Claude Haiku orchestrator in the QA git project dispatched three tasks with
one `crew_dispatch` call — greet (Claude), count (OpenCode), word (Claude),
each in its own worktree. OpenCode's process failed for real (its own
database error on this machine) → the orchestrator was woken, retried
*count* with Claude (the swap); *word* asked "Which word should word.txt
contain?" and the orchestrator answered "banana"; *greet* reported 50 %;
all three committed on their branches and reported done; the orchestrator
opened the gate *merge*. The QA app was then killed and restarted: crew,
tasks, results and the open gate were all there. Approve (with a note) in
the gate bar → the orchestrator merged the three branches (`git log` in the
QA repo: greet, count, word plus two merge commits). The log held all 16
messages in order. Controls: Stop crew stopped both workers and the
orchestrator; Resume started both again (attempt 2); Stop and Retry on one
row; the Swap agent menu opened; a 1-minute timeout failed *sleeper* and
woke the orchestrator — found here: after Stop crew the orchestrator's
queue stayed held, so the wakes waited; Resume, a user retry and a gate
decision now release it (verified: both held wakes were delivered).
Approvals: an orchestrator and worker in Supervised mode — the worker's MCP
request showed in the orchestrator's thread ("check waits for you"), was
allowed there, the worker finished and the orchestrator was woken. Only
Claude could answer on this machine: Codex is not signed in, OpenCode fails
on its own database, Gemini CLI is not installed, pi has no API key, omp
cannot reach its model server — so the demo's second agent was a real
failure, not a second success.

## 44. Round 2 · 08/12: live orchestration graph and message bus

Card 088278d3, on card 07's crews. The user's picture: the orchestrator in
the middle, workers around it with progress, messages moving between them,
a message bus underneath.

- **Crew view** (`Crew/View.vue`): a fourth main view beside Conversation,
  Board and GitHub (the thread list stays: the view follows the open
  thread). It shows the open thread's crew (as orchestrator or worker),
  else its native subagents, else the newest crew; a picker lists every
  crew and the open thread's subagents.
- **Graph** (`Crew/Graph.vue`, `utils/crewGraph.ts`): the orchestrator
  node stands out (accent ring, "orchestrator · 6 workers · leading /
  finished / stopped"); each worker shows its logo with the status ring,
  name (task key), task line, progress % and bar, state in colour
  (working, waiting, done, error, stopped, queued), elapsed time and the
  attempt when retried; hover shows its latest step; a click opens the
  worker thread. Dotted edges, one per worker. Every real crew message
  sends a particle along its edge, sender → receiver (dispatch, steer and
  reply out; progress, ask, done, failure and escalation in); the edge and
  the receiving node light up; gates and decisions light the orchestrator.
  Reduced motion: no particles, a short highlight only.
- **Layout:** radial; each ring holds up to 12 and is sized so neighbours
  never touch (√2 for the diagonals) and clears the center or the ring
  inside, then spreads to fill the stage; more workers get further rings
  and the graph scales down to fit. Wheel zooms around the pointer, drag
  pans, double-click or Fit fits again; it follows the window size.
- **Message bus** under the graph: the crew's messages live (time to the
  millisecond, sender → receiver in their node colours, the text), a
  filter per worker, newest at the bottom (kept scrolled unless the user
  scrolls up); a click opens the thread the message shows in and finds it
  there (the thread's find bar, parity 15). Footer: "7 agents on <objective>
  · 42 messages on the bus". *Full log* opens card 07's log dialog.
- **Native subagents** of a normal thread (Claude's Task, Codex's agents)
  draw the same way: a particle out when one starts, back when it ends.
- **Compact graph** in the panel's Crew section (icons, names, %), with a
  *Crew view* button.
- **Performance:** particles are small elements moved by the Web
  Animations API with `transform` — the compositor runs them, no script per
  frame; they exist only while the graph is on screen (not hidden, not a
  deactivated KeepAlive page). Highlights switch without transitions;
  progress bars scale. Driven by the `pilot-crew` event, no polling; a
  one-second clock ticks elapsed times only while a worker works. Found
  here and fixed for every view: the agent logo's *working* ring animated a
  custom property inside a conic gradient, which restyled and repainted
  every frame (~30 % of a core for nine working icons); it now turns an
  inner gradient with `transform` under a static ring mask (same look).
  Also fixed: the thread find bar overflowed a narrow conversation column.

**CPU** (renderer main thread, CDP `TaskDuration` over 10 s, QA app, six
working workers, `artifacts/quantpilot-r2-08/cpu3_*.json`): Crew view idle
0.6 % of one core (31.7 % before the ring fix); Crew view with 2 messages a
second 8.6 % (39.5 % with SVG-attribute particles); another QuantSuite
module open while messages flow 2.0 %; conversation of a plain thread with
the panel 3.1 %, without 0.4 %.

**Checks.** `npm run check:ts`; `npm run test:pilot` 129 passed (new
`scripts/test-pilot-crewgraph.mjs`: nodes and states, subagents, ring
sizes, no overlap for 1–30 workers on three stage sizes, which edge a
message travels, the bus time, jump targets, the footer); `check:isolation`,
`check:commands`.

**Native QA** (`artifacts/quantpilot-r2-08/`): a fixture crew like the
picture (6 workers porting to rust, 42 messages, "need a lock on
schema.rs") rendered as above; with 2 messages a second particles ran along
the right edges. A real crew (Claude Haiku orchestrator, two workers, one
asking "Which colour?") rendered live: 30 % → done, 7 particles for its 11
messages; the bus line "Orchestrator → beta: teal" opened beta's thread
with the find bar on "teal" (3 matches). A Haiku thread that started two
Task subagents drew them live and done. 20 workers: two rings, no overlap,
zoom and pan. Reduced motion (emulated): no particle, the edge lit for
0.6 s. Switching crews in the picker animates nothing old.

## 45. Round 2 · 09/12: QuantCanvas, a connected board

Card fa90e574. The user's reel ("a whole board like QuantCanvas with
orchestration and our kanban, visually connected"); ideas from AgentGrid and
Cline's Kanban (cards linked to agents and their worktrees), no code copied.

- **Connections** (`Canvas/Edges.vue`, `utils/edges.ts`, `utils/canvasLinks.ts`):
  a curve between two windows from the sides that face each other (it never
  cuts through either), drawn under the windows, the same width at every
  zoom, a dot at the receiving end. State: working = a dot flows along it,
  waiting = amber, error = red, done = calm green, idle = dashed. A window's
  state is its thread's (QuantPilot), its card's (claimed = working, review
  = waiting, done/merged = done, rejected = error) or a crew task's; an
  edge shows the more pressing end. Crew messages travel their edge like in
  the crew graph (card 08). Saved edges (`state.edges`, per canvas
  workspace, with the windows) are `link` (made by QuantCanvas) or
  `manual`; crew edges are derived live from the crews.
- **Ports:** every window has a port on its right edge (on hover); drag it
  to another window to connect them (pointer events; a dashed line follows
  the pointer). A click selects an edge; *Disconnect* or Delete removes a
  saved one. Closing a window removes its edges.
- **Kanban cards onto the canvas:** a card dragged out of a Kanban window
  follows the pointer as a ghost (the shared board's new `dragOut`; claimed
  cards may leave too, they still never move between columns) and,
  released over the canvas, becomes a **card node** (`Windows/CardWindow.vue`:
  title, column, priority, the agent holding it, live from the kanban bus)
  connected to its **thread's chat window**: the newest thread linked to
  the card, else a new thread linked to it with the card's prompt in its box
  (QuantPilot's board prompt: claim per AGENT.md; nothing starts until the
  user sends). Its **diff window** follows once work exists.
- **Chat windows** gain *Diff* (the thread's changes, connected — a card's
  thread shows the card's worktree against the project's branch, a worktree
  thread its branch against its base, others the uncommitted changes) and
  *Preview* (a browser, connected); diff windows gain *Preview*: card →
  thread → diff → browser. `git_diff` learned `base:<branch>` (the merge
  base against the working tree).
- **Crews** (`Canvas/CrewClusters.vue`): when a chat window's thread leads a
  crew (card 07), each worker gets a chat window, placed in a cluster right
  of the orchestrator (two rows), connected; windows the user moved stay.
  *Fold crew* on the orchestrator turns the cluster into one node — the
  crew's compact live graph, done of all, *Expand*; one edge leads to it.
  *New Crew* (canvas menu) opens an orchestrator chat whose box asks for the
  work.
- **Minimap** draws the edges in their state colours.
- Fixed on the way: chat windows on the canvas did not hear about threads
  made elsewhere (a crew's workers showed "no longer exists" until reload) —
  the canvas listens for new threads and reads the list once more before
  calling one gone.

**Checks.** `npm run check:ts`; new `npm run test:canvas` (the existing
`test-canvas-kanban.mjs`, now in a script, plus `test-canvas-edges.mjs`:
facing sides, curves that stay out of both windows, states, cluster slots);
`test:pilot` 129 passed; `check:isolation`, `check:components`,
`check:commands`; clippy for the canvas crate.

**Native QA** (`artifacts/quantpilot-r2-09/`): three new cards on the QA
board dragged out of a Kanban window with pointer events (ghost shown) →
three card nodes → three chat windows (new threads, card prompts in their
boxes) → their diff (the three share the project checkout, so one diff
window) → a preview browser: 7 edges. *New Crew* from the canvas menu; its
Claude Haiku orchestrator dispatched four workers → four chat windows in a
2×2 cluster, connected, flowing while they worked, green when done; a page
reload kept all 14 windows, the 7 saved edges and the 4 crew edges; Fold
showed one node with the compact graph (4/4 done) and one edge, Expand
brought the windows back. A port drag from a card node to the Kanban
window made a manual edge; selecting it and Disconnect removed it. 30
windows and 40 edges: panning ~112 fps (95th percentile 13 ms), dragging a
window with its edges ~220 fps. Idle CPU with the 16 card nodes and 40
edges alone: ~1 % of a core; the existing browser window (its webview
follows the canvas with a requestAnimationFrame loop, ~15 %) and embedded
chats showing the animated empty-thread face (~16 % for ten) cost far more
— not changed here.

## 46. Round 2 · 06/12: look and motion (a pre-1.1.0 Pilot pass)

Card 3a926934, run as a QuantPilot pass before 1.1.0 (the user's choice;
the 1.1.0 card 25de874a is untouched). Studied PandaCode 0.3.91 (a9be2ce;
MIT with attribution — ideas only, no code or assets): its design doc,
inset shell, action dock and screenshots, against QuantPilot in the same
states. Pilot-only; the suite header and the shared sidebars stay as they
are (1.1.0's job).

- **Inset shell:** the sidebars sit on the suite's darkest tone
  (`--qss-bg-chrome`); the main area (conversation and inspector, board,
  GitHub, crew) is an inset panel — 6 px from the frame, 12 px radius, a
  hairline edge and a soft shadow.
- **Transcript:** fades out under the panel's top edge and into the
  composer (two still gradients over the scroller — nothing repaints while
  scrolling).
- **The user's message** is a soft pill (18 px radius, a tone step, no
  outline); a steer keeps its dashed outline.
- **Composer** floats: 16 px radius, a soft shadow, a hairline edge.
- **Glass toasts:** translucent surface with blur and saturation, 12 px
  radius; arrive in 170 ms, leave in 110 ms. Reduced transparency or more
  contrast: solid.
- **Menus** (model list, thread menu, link menu, message menu) arrive from
  their trigger in 170 ms (fade, 4 px lift, a hint of scale); input works
  at once; reduced motion: a 100 ms fade. The agent menu kept its own.
- **Press feedback:** buttons give way (scale 0.97) under the pointer;
  reduced motion: none.
- **Sidebar rows:** 8 px pills on a tone step, a calm hover.
- Tokens in the module's CSS (`--qp-frame`, `--qp-panel-edge`, `--qp-glass*`,
  `--qp-ease-out`, `--qp-press`), with reduced-transparency and
  increased-contrast fallbacks. Fixed on the way: the quote highlight rule
  was the one unscoped selector in the module CSS (`check:css` failed on
  main).

**Before/after** (`artifacts/quantpilot-r2-06/shots/compare_*.png`): empty
thread, tool-heavy conversation, approval, model picker, board, streaming.
Waits for the user's approval.

**Performance** (parity 21's run, `artifacts/quantpilot-parity-21/r206-before`
and `r206-after`, same machine and build mode): ten streams frames p95 / max
29.2 / 50.1 → 25.2 / 41.7 ms, input p95 5.1 → 6.5 ms (budget 25), stream to
paint p95 59.6 → 61.3 ms, no long tasks either way; scrolling through all
1,200 messages p95 / max 33.4 / 179.6 → 29.3 / 179.5 ms; idle CPU of the
app's processes 1.6 → 2.1 % of a core (20-second samples) — within noise,
every budget met.

**Checks.** `npm run check:ts`; `npm run test:pilot` 129 passed;
`check:css`, `check:isolation`.

## 47. Round 2 · 05/12: the mascot, the same blob with more of everything

Card 11360934. The user kept the blob ("so wie jetzt, aber mit mehr allem")
instead of a new character, which settled the card's concept step.

- **Moods** (`utils/mascot.ts`): sleeping (offline), calm (idle), thinking,
  focused (working), juggling (working while three or more agents work —
  three coloured balls circle its head), tired (a run past eight minutes:
  half-lidded, a drop of sweat), calling (waiting on you: eyes on you, brows
  up, an amber ring and halo, a waving hand), worried (error: tilted brows,
  a wavy mouth, a red ring, a shiver), cheering (a turn finished: ^^ eyes,
  blush, a squash-and-stretch jump), nodding (an approval was granted).
  Eyes, brows, mouth, cheeks and body all take part.
- **Depth:** a shaded body (radial light, a dark lower edge), a highlight,
  a rim ring, cheeks and a soft shadow; it breathes.
- **Movement:** the eyes follow the pointer, otherwise glance about (a
  fresh spot and pause each time, never a loop); it blinks, sometimes
  twice; hover perks it up; a click squishes it and it says something new;
  springy transitions between expressions.
- **Your look** (Settings → Appearance → Mascot, kept on this machine):
  on/off, a name (default Pip), six skins (Night is today's look), dots /
  round / sparkle eyes, a cap, glasses, a headset or a bow, its size in
  empty threads, a tint from the thread's agent, and a **companion** — a
  small one you drag anywhere over QuantPilot (pointer events); it leans
  into the drag and stays where it is put. The section shows every mood
  live — the QA page.
- **Character:** short lines per mood in its tooltip and under the empty
  thread's question ("Pip: Ready when you are."), never in the transcript.
- **Where:** the panel head, the empty thread, the crew view's orchestrator
  node (it juggles with three busy workers), the companion.
- **Performance:** plain elements instead of SVG with a blur filter; motion
  is `transform`/`opacity`; timers, the pointer listener and the long-run
  check run only while the face is on screen (an active page, a visible
  document). Reduced motion keeps every expression and drops the
  continuous motion. Measured (CDP, renderer main thread): the empty thread
  with panel and companion 8.8 % of a core with the old face → 1.8 % with
  the new one; another module open 0.25 %; all ten moods animating at once
  in the settings gallery 9.7 %.

**Checks.** `npm run check:ts`; `npm run test:pilot` 133 passed (new
`scripts/test-pilot-mascot.mjs`: moods and their priorities, lines,
preferences, gaze); `check:css`, `check:isolation`, `check:components`.
Native QA (`artifacts/quantpilot-r2-05/`): the gallery in Night/Dots,
Mint/Round/Cap and Violet/Sparkle/Headset; the panel, the empty thread with
its line, the companion, the crew view's orchestrator node. The suite is
dark only, so there is no second theme to check.

## 48. QuantCanvas browser rework (2026-11-03)

Card 3d0f2290. The user: the canvas browser "always had problems with
visibility and layers, the window permanently in the foreground", rework it
in general. A tab is a native webview, and a native webview is drawn above
all HTML: only what the browser cut out by hand (higher windows, minimap,
sidebars, its own panels) could lie above it, so menus, dialogs, drawers and
the palette of the suite ended up under the page. A requestAnimationFrame loop
measured the DOM every frame (~15 % of a core with one browser on screen), the
URL was polled every second, scripts were injected with eval, history was an
own URL list and the progress bar a timer.

- **Engine in Rust** (`modules/canvas/crate/src/commands/browser.rs`, modeled
  on QuantPilot's browser): the crate makes each tab with `on_navigation`,
  `on_page_load`, `on_document_title_changed`, WebView2's history change
  (`pushState` in single-page apps) and `on_new_window`; every change is one
  `canvas-browser-state` event {label, url, title, loading, canGoBack,
  canGoForward}. Commands `browser_tab_open/place/close/navigate/action/
  capture` (+ `set_browser_clip_region`, `find_in_browser`) answer only the
  suite's own pages; a tab never loads the suite's origin, files or app
  schemes. Back/forward/reload/stop use the page's real history.
- **Live page or its picture** (`BrowserWindow.vue`, `utils/browserSurface.ts`):
  the live page shows only while nothing lies on top of it and the canvas is
  at rest; otherwise a picture of it (WebView2 CapturePreview, JPEG) stands in
  its place in the DOM, below every menu, dialog, drawer, popover and window.
  Higher windows (also folded ones, and their resize handles), the minimap,
  this window's own handles and what lies outside the visible canvas are cut
  out of a live page exactly (window region); anything else on top is found by
  hit-testing the page's area. Pan, zoom, drag and resize show the picture,
  the live page returns ~160 ms after the motion. Pictures are taken after a
  page loads, when the pointer comes back from the page and every 5 s while
  live.
- **Nothing per frame:** the page follows on events — canvas transform and
  window geometry, DOM changes (a MutationObserver, probed at most 5×/s),
  clicks and keys in the suite (menus open on those), visibility, KeepAlive.
- **Zoom:** page zoom = canvas zoom × the window's own page zoom (Ctrl +/-/0,
  a chip when not 100 %), so the live page and its picture look the same.
- **Tabs:** opened when first shown (a restored window with ten tabs makes
  one webview), real titles in tabs and window title, loading bar from the
  load events, stop while loading. `target="_blank"` links and `window.open`
  open a tab in the same window; a sized popup (a sign-in window) opens as the
  page asked. The suite's shell plugin puts a link handler into every webview
  that swallowed `_blank` links in web pages (they have no app access): the
  tabs take such clicks first (`NEW_TAB_LINKS`). Closing the window closes
  every tab's page, also those a reload of the suite page left running.
- **Other platforms:** the picture needs WebView2; Linux/macOS show the tab's
  title in its place. Simulated non-Windows clippy clean.

**Measured** (QA app, CDP, renderer main thread): a live browser window idle
0.15 % of a core (before ~15 %), minimized 0 %. **Checks:** check:ts, css,
isolation, components, commands, modules; `npm run test:canvas` 17 passed (new
`scripts/test-canvas-browser.mjs`); `cargo test -p tauri-plugin-canvas`.
Native QA (`artifacts/canvas-browser-rework/`): Kanban drawer, palette,
history panel and a card window over the live page; pan/zoom 75 %; module
switch; back/forward; `_blank` link and `window.open` → tab; reload of the
suite page reconnects the tabs without duplicates. Not tried: fullscreen
video, a sign-in popup, downloads.

## 49. The mascot, friendly again (2026-11-03)

Card d537d3e4. The user on the round-2 mascot (§47): "super gruselig" — the old face
was better in principle, but forward, not back. The glossy 3D ball (radial light,
a specular shine, inset shading, a ground shadow) with a permanent little smile
and cheeks read like a doll. Now the face is the old flat glyph again — a dark
body (Night darker than the panel), a soft outline, a faint glow — and at rest
just the two eyes. A mouth only for cheering, calling and worried, cheeks only
for cheering and nodding. The worried brows now lift at the inner ends (they read
angry before), the working ones barely tilt. Closed round eyes are a line in the
face's ink. Moods, gaze, blink, skins, eye styles, accessories, the companion
and the performance rules of §47 are unchanged. Before/after in
`artifacts/mascot-friendly/`. Checks: mascot tests, test:pilot, check:ts, css,
isolation, components.

## 50. The mascot: eyes only, lively (2026-11-03)

Card 8b5daac4. The user: no smile at all — "nur mit den Augen" — and more idle
animation. No mouth and no cheeks in any mood now. While calm it does a quirk
every 3–7 s, never the same twice in a row (utils/mascot.ts pickQuirk): look
around, roll its eyes, curious tilt, wink, ^^ hop, hop, wiggle, eyes pop (also
"oh, hi" when the pointer returns after 15 s), giggle > <, stretch with closed
eyes, and after 45 s without the pointer a drowsy nod-off with a startle.

**Why it never moved on this machine:** shell.css collapses every animation and
transition to 1 ms under prefers-reduced-motion, and Windows reports that
whenever its animation effects are off — the mascot did not breathe, its eyes
jumped. The mascot now has its own setting (Appearance → Movement: Lively /
Still, default Lively); while Lively it carries [data-own-motion], which the
global rule leaves alone. Measured: the empty thread with the lively face 1.2 %
of a core. Also: the inset main area got its top margin (it touched the header).
Checks: mascot tests 6, test:pilot, check:ts, css, isolation, components; QA
frames of every quirk in artifacts/mascot-friendly (quirks-sheet2.png).

Follow-up (card a92dd66e): no slight squint while working (eyes stay round, a
look down at the work; the user: "leicht zukneifen … sehr gruselig"); the
nod-off closes in one slow move instead of holding half shut. A click is one
boing (WAAPI, composite add, squash → stretch → settle, ends where it started)
with ^^ eyes for 1.2 s and no hop; a cheer hops once, so it never stops mid-air
(the old click snapped back after 0.26 s and cut its second hop).

## 51. Bob The Blob — the all-out pass (2026-11-03)

Card 7cdb47d9. The user: "all out … jegliche Aspekte", and the mascot is called
**Bob The Blob**, Bob for short (the old default name Pip becomes Bob). Kept:
flat glyph, eyes only, no half-shut holds, every animation ends where it started.

- **Moods:** thinking shows a thought bubble; working reads along (eye
  saccades left to right); needs-you gets a "!", a wave and attention hops;
  error glances nervously; long run droops (eyes lower, slow blinks, sweat) —
  no half lids any more; a cheer hops once with a sparkle burst; an approval
  nods with ^^ eyes; sleeping sways. After three quiet minutes he naps and
  wakes with an "oh, hi" when the pointer or the keyboard comes back.
- **Quirks** (utils/mascot.ts, 17): new slow "love" blink, barrel roll, side
  hop, shake-off, hiccup and a firefly he follows with his eyes (WAAPI flight,
  gaze per waypoint). Movement Lively / Calm (rarer, no big quirks) / Still.
- **Reactions:** he watches the field you type in; four quick clicks make him
  dizzy (spinning eyes, woozy sway); stroking him back and forth is petting
  (^^, contented sway); the companion is wide-eyed while held, > < when swung
  fast, and lands with a squash (WAAPI, composite add).
- **Look:** skins Ghost, Rose, Ocean; accessories Beanie, Crown, Party hat,
  Sprout; flat swatches; accessories as wrapping chips.
- **Settings:** the section is "Bob The Blob"; Tricks play every quirk on a
  preview Bob (QUIRK_LABEL, tested to cover every quirk).
- **Lines:** Bob's voice, three per mood; a calm Bob greets by the time of day.
- Also: the Usage page reads "Reading the agents' logs…" (user request).

Measured: empty thread with panel + hero Bob lively 1.4 % of a core. Checks:
mascot tests 8, test:pilot, check:ts, css, isolation, components. QA frames in
`artifacts/bob-allout/` (settings, gallery, tricks, dizzy/petting/sparkle,
companion carried and landing, skins and accessories).

## 52. Round 3: the adopted roadmap areas (2026-11-03)

The user on the roadmap (card 91212da8): "1.1.0 kommt ganz danach … implementiere
sie so gut du kannst, bei denen wo du denkst wir können das implementieren und es
lohnt sich … jedes einzelne dann wieder als karte". Adopted: 2, 7, 8, 9, 11, 12
(cards "QuantPilot round 3 NN/07"); deferred 3, 5, 6; rejected 4, 10 — reasons on
the roadmap card.

### 01/07 — a General thread moves into a project (card 29d4a825)

- Thread menu "Move to a project" (every workspace, or "Choose a folder…") and the
  header's project chip on a General thread (a button there). A new folder is
  registered without switching the suite's active workspace
  (`registerWorkspacePath` in @quantsuite/core).
- `pilot_thread_move_to_project`: refused for project threads, side chats (they
  follow their main thread), worktree threads, running turns and an open terminal
  UI. The thread's context and cwd change; transcript, title, tags stay; a warm
  process is stopped (it still has the old folder); a notice says how it continues.
- Continuity: Claude's session file is copied into
  `<config>/projects/<folder with every non-alphanumeric as ->/` so `--resume`
  continues there; Codex resumes by id; other agents start a new session and the
  next message brings the conversation along (the existing handoff of an agent
  switch). Side chats move with their main thread.
- QA (Claude Haiku): a General thread learned a code word, moved into the QA
  project, then named the word and the project folder as its working directory.
  Not tried live: the handoff path (only Claude answers on this machine) and Codex.

### 02/07 — several accounts per agent (card d8573e27)

- An account (`settings.instances`: id, agent, name, folder) is Claude Code or
  Codex pointed at its own config folder — `CLAUDE_CONFIG_DIR` / `CODEX_HOME`
  (`settings::home_var`). Other agents have no such variable and stay one account.
  The default account is today's behaviour: no variable set. Saved without a
  folder, an account gets `<pilot data>/accounts/<agent>-<id>`; the folder is
  created on save (Codex refuses a missing CODEX_HOME). QuantPilot never reads,
  copies or writes the login inside it.
- Settings → Providers, in the Claude Code and Codex cards: "Other accounts" —
  add (the id comes from the name), rename, pick a folder, remove (its threads go
  back to the default account; the folder stays). Each shows its sign-in state
  (the CLI's own status with that folder) and, when signed out, the terminal line
  that signs it in, with Copy. The user runs it; there is no thread-less terminal
  in Settings to run it in.
- Threads store `instance` (schema 21). The agent menu lists each account as a
  row of its own ("Claude Code · Work", with Not signed in / Sign in…); the
  header, the thread controls and the board panel show the account. Launch,
  resume, the warm-process key and the catalog use the thread's account.
  Switching account works like an agent switch: the process stops, the session
  is cleared (sessions never cross accounts) and the next message carries the
  conversation. A working thread keeps its account until the turn ends.
- Usage and limits per account: each account folder's logs count for
  `claude@<id>` / `codex@<id>` (own series, filter chip, model rows), Claude's
  reported windows are stored per account, Codex is asked per account. Import
  lists each account's sessions and keeps the account on the imported thread.
- Saved accounts show at once after a start; their sign-in state follows the
  status probe.
- QA (QA app, no login performed): an account added through the settings gets
  its folder and shows "not logged in" with the command; its thread runs Claude
  with that folder (the session log was written there) and answers "Not logged
  in · Please run /login"; a default thread answered and, switched to the account
  and back, still knew its code word; usage showed the account's fixture log
  apart; a Codex account without a folder got `accounts/codex-side` and its own
  status.

### Also before the next release — the canvas chat window's start screen (card be857b27)

The user: "dieser anfangsbildschirm vom chat sieht kinda shit aus". An empty Chat
window on the canvas now looks like QuantPilot's empty thread: Bob (when the
mascot is on), "Chat in <workspace>", one primary New thread button naming the
agent and model it gets (▾ opens the agent menu), then "or continue a thread" —
a search (arrows + Enter) and the threads grouped "This workspace" / "Elsewhere"
with agent icon, project and age. Side chats are not offered. A short window
drops Bob and the line (container query); a deleted thread is said above the
button.

### 03/07 — scheduled prompts (card 0d7e8a2e)

- A schedule (pilot DB schema 22: `schedules`, `schedule_runs`) is a prompt
  with a project, agent/model/effort/account, mode and a rule: once, every day,
  weekdays, chosen days or every N hours — local wall-clock time. A time the
  clocks skip at the spring change runs an hour later (02:30 → 03:30); a time
  they pass twice in autumn runs once, at the first. "Every N hours" counts real
  hours from its start. Webhooks stay deferred (they need an authenticated
  listener).
- Engine (`schedules.rs`): one task asleep until the next slot (at most a
  minute, so a wall-clock jump after sleep is noticed; a change wakes it). A run
  is recorded before it starts and (schedule, slot) is unique, so a slot runs at
  most once. A slot found more than two minutes late — the suite was closed or
  the machine asleep — follows the missed-run policy: skip (recorded as missed)
  or run once after the start; only the latest missed slot counts. A run is
  skipped while the previous run's thread still works. Each run starts a new
  thread ("<name> · Nov 4 08:00") or continues the same one; a notice in the
  thread names the schedule. New, changed and re-enabled schedules count from
  now — nothing in the past runs.
- UI: the Schedules view (header tab): the list with the next run and the last
  result, the selected schedule with Run now / Edit / Pause / Delete and its
  history (each run's thread one click away), and the form (prompt, project,
  agent menu, mode, the rule, new or same thread, missed-run policy). "Schedule
  this…" in a thread's menu prefills the form from the thread.
- QA (QA app, Claude Haiku): a one-off due a minute later ran 0 s late in the QA
  project and answered; Run now added a manual run; two one-offs whose slot
  passed while the app was closed: "run once" ran once after the start, "skip"
  was recorded as missed, and a second restart ran nothing more.

### 04/07 — review guides (card df9982c0)

- "Write guide" in the Diff tab (the book button: the guide replaces the file
  list, the diff stays below) and in a pull request's review (the right
  column's Guide tab). One read-only run of Claude Code (`-p --tools ""`, no
  session, hooks or MCP) or Codex (`exec -s read-only --ephemeral`), chosen in
  the panel's agent menu and remembered; the prompt goes in on stdin from the
  temp folder — no repository access.
- The answer is JSON: summary, groups by concern in reading order, per file
  what changed, risk (low/medium/high) and what to check plus a line to look at
  first, and questions for the author. Paths are matched to the diff's (exact,
  else the one path they end); a file the agent left out is listed under "Not
  covered by the guide", never dropped. Each patch is cut at 12,000 characters
  and the diffs at 160,000 together; cut or left-out files are named in the
  prompt and marked "cut" in the guide.
- Kept per target (`review_guides`, schema 23: a PR, or a thread's scope)
  with a key over the head/range and every patch; a changed diff shows the
  guide as stale until it is written again. Entries open their file and line
  (QDiffEditor.reveal; the PR view scrolls to the row and flashes it). Nothing
  is posted.
- QA (QA app, Claude Haiku, the QA project's uncommitted diff of five files):
  the guide came back in 12 s and named the deliberate `h > 24` slip in a
  question; its clock.js entry opened the file at its line; an edit plus
  Refresh marked it stale; Write again cleared that. Not tried live: a pull
  request's guide (no test PR) and Codex (not signed in here).

### 05/07 — visual progress (card 3614f012)

- The inspector's Progress tab: every image of the thread, turn by turn, each
  labelled with its turn and source — `you` (attachments), `browser` (a
  Browser-tab capture, saved as screenshot.png), `page` (images embedded in a
  page the agent rendered) and `tool` (an image file a tool read or wrote,
  found by its path in the tool's input; relative paths against the thread's
  folder, one tile per file). The pixels a tool returned inline are not kept by
  the timeline and cannot be shown.
- `progress.rs` scans the timeline on demand; an image is a key
  (`<item id>#<n>`), never a copy. Thumbnails (360 px box) and full images
  are read by key — which lets the media viewer open tool files and page
  images that are not attachments; a gone file shows as gone.
- Pins (schema 24, `thread_pins`) keep references first, with the thread.
  Compare picks two (the earlier is "before"): slider (drag the divider with
  pointer events, ← / →) or side by side.
- QA (QA app, Claude Haiku): a thread with two attached mockups, a capture and
  a turn where Haiku read a PNG of the project: four tiles with thumbnails
  (Turn 1 · you, Turn 2 · you / browser, Turn 3 · tool); a pin survived a
  reload; the viewer opened the tool file; both compare modes worked; deleting
  the file turned its tile "Gone". QA found and fixed a watcher that read the
  compare state before it existed (it blanked the page).

### 06/07 — design artboards in QuantCanvas (card 50d7d22a)

- Canvas window type `artboard` (canvas menu and empty-canvas start): an HTML
  mockup in a frame sandboxed with scripts but without same origin, with a CSP
  written before the mockup (`default-src 'none'`, inline style/script, data
  images) — no network. Desktop 1280 / tablet 820 / phone 390 px, scaled to the
  window; it reloads when the mockup or its comments change
  (`canvas-design-changed`). A new artboard an agent writes opens on the
  canvas by itself (an empty artboard window takes it, else a new window).
- Comments: Comment mode outlines elements in the frame; a click sends a CSS
  selector (id, else a tag path with nth-of-type) and the element's text by
  postMessage — the frame's only way out. Pins sit at the element's corner
  (the frame reports positions); open/resolved, reopen, delete. "Ask the agent"
  sends the comment, its selector and the tool names to the chat connected to
  the artboard by an edge, or opens a chat beside it, connects it and sends.
- Storage: `<workspace data>/canvas/design/<id>.html` + `<id>.comments.json`
  (name and comments) — never the project. Export copies a mockup into a
  picked folder without overwriting (`<id>-2.html`, …).
- QuantMCP (`modules/canvas/module.json`): `quantsuite.canvas.design_list`,
  `design_read` (added: an agent must read a mockup before changing it),
  `design_write`, `design_comments`, `design_resolve`; `workspace` = name,
  id or folder, default the active one.
- QA (QA app, Claude Haiku): Haiku created "QA Pricing" with design_write and it
  opened on the canvas; a real click inside the frame picked `#buy` and the
  comment was pinned; Ask the agent opened a connected chat beside it, where
  Haiku used design_read, design_write and design_resolve — the button turned
  green and the comment resolved with a note. A probe mockup could not read the
  parent, storage or cookies, could not fetch the app or QuantMCP, and its
  attempt to call an app command had no effect (Tauri's IPC object is present
  in every frame, but the frame's CSP and opaque origin leave it inert).

### 07/07 — moodboards in QuantCanvas (card d1ec7b16)

- Canvas window type `moodboard`: reference images dropped from the system,
  pasted (Ctrl+V) or added from files — PNG, JPEG, GIF, WebP only; nothing is
  downloaded. Captions; reorder by drag with pointer events (left/right half of
  a card = before/after it, a line marks the gap); remove.
- Storage: `<workspace data>/canvas/moodboards/<id>/board.json` (name, items in
  order with caption and source name) plus a copy of each image — never the
  project.
- Agents: `quantsuite.canvas.moodboard_list` and `moodboard_read` (each
  image's file path and caption; the agent opens the file with its own reading
  tool). A chat connected to the board by an edge gets "All to chat" and a
  per-image attach button: the images go into that chat's composer as
  attachments for the next message.
- QA (QA app, Claude Haiku): a board made in the window; one image pasted, two
  dropped (a text file in the same drop refused); a caption; a real mouse drag
  moved the third image to the front; order and caption survived a reload;
  "All to chat" put three image chips into the connected chat's composer; Haiku
  used moodboard_list, moodboard_read and Read and described the first image
  correctly.

### Fix — agents always use the PC's own login (card 5232de41)

The user saw a Codex chat fail with 401 at api.openai.com ("NO API … my login
from my pc"). Cause: the dev app was started from an Orca terminal, which sets
`CODEX_HOME` to an Orca account folder without a login; Codex inherited it and
fell back to the API without a key. Now the default account gets
`CODEX_HOME` / `CLAUDE_CONFIG_DIR` as Windows has them (user, then machine
environment; `qs_core::system_path::system_var`): a differing inherited value
is replaced or removed (`settings::home_fix`). A value in QuantPilot's settings
or an account still wins. It applies to chat threads, the sign-in probe,
review guides, helper runs (`providers::base_cmd`: commit messages, titles)
and the agent's terminal UI, which now also gets the thread's account; PTYs
honour `-NAME` (qs-pty). QA: the QA app started with Orca's empty
`CODEX_HOME` showed Codex "Logged in using ChatGPT" and a Codex thread answered.
