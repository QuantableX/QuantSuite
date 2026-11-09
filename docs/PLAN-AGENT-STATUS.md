# Agent status — progress bars for every agent

Card 84ac3068 (2026-11-09). User: "Immer wenn ich dir sage du sollst xy
implementieren und du dafür Shell verwendest … muss ich fragen 'wie ist der
stand' anstatt visuell zu sehen, wie der stand ist." One template, several bars
at once or one after another, for every agent.

## User decisions (2026-11-09)

- **Both places:** a local browser page and inside the app, showing the same
  board from one template.
- **Not a new module.** In the app it lives in the bottom drawer, under
  **Agent**. The tab's button group is `Status · Strict · Relaxed`, and
  **Status is first and the default view**.
- **Strict / Relaxed stay the approval-mode switch** exactly as before (they
  set the mode and show the tool table). While Status is shown the active mode
  stays marked (underline), and clicking Status never changes the mode.
- Status shows, per agent, its **shells** and its **bar with percent and
  elapsed / estimated time** ("1h 10m / ~1h 50m"; user: "ungefähre
  Länge/Zeit z.B. 1h/3h").
- **Both sources in one bar:** the agent reports the big steps, and the work
  itself fills the current step.
- Not built (options only): a mini indicator on the collapsed Agent tab,
  Windows taskbar progress, Windows toast on done/failed.

## How it works

```
agent ──report_progress (MCP)──┐
script ──POST /progress/report─┤                    ┌─ drawer › Agent › Status (progress_board, 1 s poll)
followed file (events.jsonl,   ├─► progress::Store ─┤
  PROGRESS 12/48 log) ─────────┤   progress.json    └─ http://127.0.0.1:<port>/progress (GET /progress/state, 1 s poll)
shell hooks ─/quantprogress/───┘
```

- **Store** — `modules/mcp/crate/src/progress.rs`, one board per suite process,
  saved to `~/.quantsuite/modules/mcp/progress.json` (batched every 2 s) so
  bars survive a restart. Lanes: `agent:<agent_id>` from reports,
  `hook:<client>:<session>` from the shell hooks.
- **Bar math** — with `steps`: (finished steps + progress of the current step)
  / steps; the current step's progress is the followed file, else
  `step_percent`. Without steps: the followed file, else `percent`. Unknown →
  an indeterminate bar. A new `step` starts empty (its text, percent and
  followed file go unless given again).
- **Time** — running: elapsed / ~total, where total is the agent's `estimate`
  while it holds, else elapsed ÷ progress once there is ≥ 3 % and ≥ 30 s.
  Finished: what it took.
- **Stale** — a running bar with no report, no running shell of its agent and
  no change of its followed file for 15 minutes turns amber
  ("no update for 20m").
- **Retention** — done bars stay grey 30 min, failed ones red 2 h; ended shells
  2 min; a foreground shell without an end 30 min; background shells until
  Claude Code's SessionEnd or 12 h; anything untouched for 24 h goes.
- **Followed files** — the last 64 KB, read on each poll. A folder means its
  `events.jsonl` (Smithery lab runs write
  `{"event":"progress","fraction":…,"message":…}`). Lines with
  `PROGRESS 12/48 text` or `PROGRESS 45% text` give a fraction too; the newest
  readable line is shown under the bar.

### Shell hooks

`HookFeature::Shells` in `memory_hook.rs` beside the memory hooks, with its own
route segment (`quantprogress`) and install snapshot (`shell-hooks`), so the
two install and uninstall independently even in one settings.json.
Installed per agent under **QuantMCP › Settings › Shells in Agent status**.

| Client | Events | Notes |
| --- | --- | --- |
| Claude Code | PreToolUse, PostToolUse, PostToolUseFailure (matcher `Bash\|PowerShell`), SessionEnd | `run_in_background` calls keep running after their PostToolUse (that is the launch) until SessionEnd |
| Codex CLI | PreToolUse, PostToolUse | no matcher; new hooks need one trust in `/hooks`; not observed live |
| Cursor | none | its shell hooks answer with a permission decision — unverified here, so Cursor agents use `report_progress` only |

The hook command is the memory hooks' curl (2 s cap, `exit 0`): a closed suite
never blocks a command.

### Joining shells to agents

A hook knows its client session and folder; an MCP call knows the agent and its
folder; there is no shared key. A hook session joins an agent lane only when
exactly **one** lane of the same client claims its folder — the same folder, or
the lane's folder holds it (most specific wins). Otherwise the shells stay a
lane of their own ("Claude Code · QuantSuite"). Agents pass `folder` = their
worktree to make the join exact. A join, once made, sticks.

### The one template

`packages/ui/src/progress/board.js` + `board.css` (+ `board.d.ts`): plain JS,
no imports. The drawer imports it; `progress.rs` embeds both files with
`include_str!` and serves them at `/progress/board.js` / `.css` beside the page.
Every value is escaped in the template.

### HTTP

All on the QuantMCP port, outside the MCP router's CORS layer; loopback `Host`
only (DNS rebinding), writes also refuse any `Origin` (no website can post).

| Route | |
| --- | --- |
| `GET /progress` | the page |
| `GET /progress/state` | the board as JSON |
| `POST /progress/report` | a script's report: the tool's fields as JSON, `agent_id` names the lane (default "script") |
| `POST /quantprogress/<client>/<event>` | the shell hooks; always an empty 200 |

## For agents

`report_progress` (AgentOS tool group) and the global AGENT.md rule
"Show your progress" (§9): report at the start (title, steps, estimate,
agent_id, folder), at every step, and done/failed at the end; `follow` the
file a long command writes.

## Verified (2026-11-09)

- `cargo test -p tauri-plugin-mcp --lib`: 159 passed (15 new progress tests,
  install tests for both hook features in one file).
- `npm run test:progress`: 7 template tests.
- `npm run check:ts`, check:modules, check:css, check:isolation,
  check:components, check:commands.
- The page rendered headless from the exact `PAGE_HTML` with a sample board.

Not verified by the agent: the running app (drawer view, live tool call, live
hooks) — the user's test command.
