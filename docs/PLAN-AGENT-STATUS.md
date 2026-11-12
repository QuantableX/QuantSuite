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
  stays marked with a small dot, and clicking Status never changes the mode.
- Status shows, per agent, its **shells** and its **bar with percent and
  elapsed / estimated time** ("1h 10m / ~1h 50m"; user: "ungefähre
  Länge/Zeit z.B. 1h/3h").
- **Both sources in one bar:** the agent reports the big steps, and the work
  itself fills the current step.
- Not built (options only): a mini indicator on the collapsed Agent tab,
  Windows taskbar progress, Windows toast on done/failed.

## How it works

Updated direction (2026-11-12, card 9c7402e4): the user prioritises readable
progress over shell commands. Technical content is now collapsed by default;
step counts, log counters and agent estimates are distinguished. The original
elapsed/estimated-time request above does not justify an extrapolated ETA.

Layout follow-up (card aecadb52): agents always stack vertically in one column,
each taking the available width, including on wide screens. Adding more agents
adds rows rather than narrowing their cards. This applies to both hosts.

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
  `hook:<client>:<session>` from the shell hooks. A report without
  `agent_id` goes to the lane its MCP session reported to before, else to the
  one lane of its client whose running bar has the same id, and only then to
  the client's name; a bar the session named differently before moves over.
  (Agents often name themselves only on the first call: the closing report
  used to open a second lane and leave the first bar stuck.)
- **Progress** — with `steps`, show the current step's fraction from the
  followed file, else `step_percent`, labelled with its source. Without an
  inner fraction show completed step counts and a segmented bar, never an
  invented overall percentage. Without steps, show the followed fraction or
  agent's `percent`; unknown progress is indeterminate. The API retains its
  equal-weight `overall` field for compatibility, but the UI does not use it.
  A running task whose counter reaches 100% still says `awaiting completion`;
  only `state=done` marks completion. A new step clears its old inner progress.
- **Time** — elapsed duration and the explicitly labelled agent estimate.
  An exceeded estimate stays exceeded; no elapsed/fraction extrapolation.
  Finished tasks show actual duration.
- **Stale** — a running task with no report or change in its own followed
  file for 15 minutes turns amber. Another shell running in the same agent
  lane cannot make an old task report current.
- **Presentation** — task cards and progress first. Commands and raw logs sit
  inside closed technical disclosures; sessions without task reports go under
  Other activity. Both hosts use `updateBoard`, preserving open details,
  internal scroll and keyboard focus across polls. Poll requests do not overlap.
- **Retention** — done bars stay grey 30 min, failed ones red 2 h; ended shells
  2 min; a foreground shell without an end 30 min; background shells until
  their agent has no running bar left (its last bar done or failed), Claude
  Code's SessionEnd, or 12 h; anything untouched for 24 h goes.
- **Followed files** — the last 64 KB, read on each poll. A folder means its
  `events.jsonl` (Smithery lab runs write
  `{"event":"progress","fraction":…,"message":…}`). Lines with
  `PROGRESS 12/48 text`, `PROGRESS 45% text`, and SystemGauntlet's native
  `[time] Variant runs: 17 of 30` give a fraction too. The newest readable
  line appears in technical details.

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
lane of their own ("Claude Code · QuantSuite"; a second such lane in the
same folder reads "Claude Code (2)"). Agents pass `folder` = their
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
- Live, in the isolated QA app built from the worktree (QA home, port 3101):
  22/22 checks — `report_progress` in tools/list with number/integer types,
  start and step reports, a bad number as a tool error, a followed run folder
  filling step 2 of 3 to 50 % with its line as the detail, hook calls exactly
  as the installed curl sends them (empty 200; background launch keeps
  running; the worktree session joined claude-code-2's lane, the main-checkout
  session stayed its own lane), a memory event refused on the shell route,
  Origin and foreign Host refused, a script report over HTTP, the page and the
  template served. The drawer showed Status by default with the active mode
  marked; Strict switched to the tool table and Status came back.

Not verified: a real Claude Code / Codex session with the shell hooks
installed (the hooks were driven with their exact JSON over HTTP), and Codex's
tool hooks at all.

## Round 2 — 2026-11-10 (card 1b7a5940)

User, on a screenshot of v1.1.2: "es ist bugged und hässlich gleichzeitig".
- Bugs: one job in two lanes (the closing report came without `agent_id`), a
  background robocopy shown as running long after the job was done, and two
  identical "Claude Code · QuantSuite" lanes. Fixed as described above.
- Look: lanes are cards in a grid (min 420 px, one height per row) instead of
  bars across the whole drawer; the agent's note in sans, a followed log line
  in mono; percent and times in sans with tabular figures; the header on one
  line; the /progress page up to 1400 px (three columns).
- Verified: 18 Rust tests (3 new), test:progress 7/7, Clippy 1.93 and 1.98,
  check:ts, and the user's case replayed against the QA app (4/4: one lane,
  bar done, robocopy over), screenshots of the drawer at 2530 and 1400 px and
  of the page.
