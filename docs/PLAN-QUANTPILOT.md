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

### 4.3 Storage (`pilot.db`, schema v3)

| Table | Holds | Card |
| --- | --- | --- |
| `threads` | V2's `sessions` renamed, plus `kind` (`chat` / `terminal`), `effort`, `title_source` (`auto` / `user`), `state`, `context_tokens`, `context_window`, `last_activity_at` | 2 |
| `timeline` | the items above: id, thread, turn, seq, kind, parent, data (JSON), timestamps | 2 |
| `turns` | one row per user message: status, start, end, duration, cost, error | 2 |
| `queue` | queued follow-ups in order, with attachments | 4 |
| `checkpoints`, `turn_changes` | hidden git refs per turn, attributed changed files and saved patches | 11 |
| `attachments` | images and documents stored under the pilot data dir, never inside the project | 9 |
| `links` | kanban card, PR, issue, parent thread | 12, 21 |
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
- `result.total_cost_usd` is cumulative per process: a turn's cost is the
  difference to the previous result.
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

### 5.3 ACP and Pi (card 20)

A generic ACP v1 client (initialize, `session/new`, `session/load` or resume
when advertised, `session/prompt`, `session/update`, `session/request_permission`,
`session/set_mode` acknowledged before the prompt, `session/cancel`) serves
Gemini CLI, OpenCode, GitHub Copilot CLI and **Custom ACP agent** entries
(which replace V2's custom adapters). Pi and omp use Pi RPC mode, with a small
injected extension enforcing the permission modes.

### 5.4 Capabilities

Each adapter reports what it can do: steer, rewind, fork, compact, goals,
plan mode, effort levels, permission modes, images, subagents, usage, limits.
The UI hides or disables (with the reason) anything a provider cannot do; it
never fakes support.

## 6. Permission modes

The mode is chosen in the composer and saved on the thread. New threads use
**Full access** unless a project override says otherwise. A failed or
unsupported mode change stops the turn instead of silently running under
another policy.

| QuantPilot mode | What the agent may do | Claude Code 2.1.312 | Codex | ACP / Pi |
| --- | --- | --- | --- | --- |
| **Supervised** | Asks before commands and file changes | `--permission-mode manual` | `workspace-write` sandbox, approval `on-request` | the agent's manual/default mode |
| **Accept edits** | Edits files freely, asks for the rest | `acceptEdits` | as Supervised, edit escalations approved | the agent's accept-edits mode where offered |
| **Auto** | The provider's own reviewer approves routine actions | `auto` | agent-reviewed approvals | not offered unless the agent has a reviewer |
| **Plan** | Reads, searches and plans; no edits | `plan` | `read-only` sandbox | the agent's plan mode |
| **Full access** | Commands and edits without prompts | `bypassPermissions` | `danger-full-access`, approval `never` | the agent's unattended mode; remaining requests answered "once", never as a standing grant |

Organization policies and the agent's own deny rules still apply in every
mode. Full access never answers *questions* for the user.

## 7. Default keybindings

`mod` is Ctrl on Windows. Rules live in the pilot data dir as JSON
(`key`, `command`, optional `when`); the last matching rule wins; the
terminal keeps its own keys. `when` contexts: `terminalFocus`,
`terminalOpen`, `previewFocus`, `composerFocus`, `editableFocus`,
`turnRunning`, `modelPickerOpen`, `usagePageOpen`. Most defaults carry
`!terminalFocus`. QuantPilot already binds Ctrl+B, Ctrl+Shift+B and Ctrl+,
in `layouts/pilot.vue`; they keep their meaning. No Vim mode or Vim keys.

| Key | Command |
| --- | --- |
| Ctrl+N / Ctrl+Alt+N | New thread / new thread without a project |
| Ctrl+K | Command palette (`>` = actions only) |
| Ctrl+B / Ctrl+Shift+B / Ctrl+Alt+B | Sidebar / thread panel / inspector |
| Ctrl+J | Terminal drawer (Ctrl+N new, Ctrl+D split, Ctrl+W close while focused) |
| Ctrl+D / Ctrl+Shift+J | Diff tab / browser tab |
| Ctrl+P / Ctrl+Shift+F | File picker / project search |
| Ctrl+F | Find in thread (find in diff when the diff has focus) |
| Ctrl+U | Usage page (C / T / L tabs, Ctrl+Shift+1..4 periods) |
| Ctrl+, | Settings |
| Enter / Shift+Enter | Send / new line (send shortcut is a setting) |
| Ctrl+Enter | While a turn runs: the opposite of Queue/Steer; in a new thread: start in the background |
| Ctrl+Alt+Enter | Send and open a new thread |
| Ctrl+Shift+Enter | Send the oldest queued message as a steer |
| Alt+Up | Edit the latest queued message |
| ArrowUp / ArrowDown in an empty composer | Prompt history |
| Ctrl+S | Stash the prompt / restore a stash |
| Ctrl+Shift+M / E / A / X / G / L | Model / effort / permission mode / workspace / branch / previous worktree |
| Ctrl+1..9 | Open the first nine threads (pick entries while a picker is open) |
| Ctrl+Shift+[ / ] | Previous / next thread |
| Ctrl+[ / ] | Back / forward |
| Ctrl+Shift+S / Ctrl+Shift+P | Settle / pin thread |
| Ctrl+Shift+C | Copy thread reference (PR link when linked) |
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

## 11. Open questions

None blocking. Decisions taken without asking, open to change:

- The terminal-first V2 model is replaced, not kept as a second mode; the
  TUI remains reachable per thread (Open in TUI) and in the terminal drawer.
- Threads without a project get their own folder under the pilot data dir,
  like today's General vault context.
- GitHub (card 23) is low priority because the QuantSuite repo has no remote
  locally; the user may delete or postpone it.
