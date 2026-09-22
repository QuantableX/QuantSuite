# QuantMCP — Global Agent Instructions

You are connected to QuantSuite through QuantMCP. This file is the operator's
standing brief for EVERY AI agent (Claude Code, Codex, Cursor, Gemini CLI,
OpenCode, ...) in EVERY workspace. QuantMCP points you to `get_instructions`
at connection time; that tool returns this brief with the active workspace's
`AGENT.md`. Native instruction imports may also provide this brief at startup.

Your client may show the suite's tools with underscores
(`quantsuite_memory_search`); this file uses the server's names
(`quantsuite.memory.search`) — same tool.

## 1. Session start — every session, before any other work

1. `get_instructions` — the workspace model, every workspace with its index
   status, and these rules merged with the active workspace's AGENT.md.
2. `quantsuite.memory.search` with the task's keywords, then
   `quantsuite.memory.read` what matters — earlier sessions, yours and other
   agents', left their knowledge there.
3. `list_kanban_cards` — what is planned, in progress or waiting for review,
   so you never start work that is already on the board or claimed.
4. `get_kanban_approval_mode` for the target board — check before claiming
   or implementing anything. Approval gates both the start and the result.

Skip none of these because a task "looks small". Several agents and the
operator share this suite; its state lives in the tools, not in your context.

## 2. The workspace model

- A **workspace** is one registered project folder. Its code index, kanban
  board, memory vault and AGENT.md all key off the same registry entry.
- Every tool takes an optional `workspace` argument: name, entity id or folder
  path (case-insensitive). Omit it to mean the **active** workspace — the one
  open in the suite. Pass it explicitly when you work on another repository.
- `"general"` names the suite-wide General board and the general memory vault.
  It has no folder: a General card must be moved to a workspace
  (`move_kanban_card_to_workspace`) before it can be claimed.
- `list_codebases` and `quantsuite.memory.workspaces` list the workspaces.

## 3. Code — search first, then read

The index is faster and wider than grepping, and it is what the other agents
rely on too.

- `search_code` before reading or editing any file. `search_mode`:
  `structural` (keyword, always available once indexed), `semantic` (meaning;
  needs the semantic index and its embedding service) or `auto` (default).
- `lookup_symbol` for an exact function, class or variable by name.
- `list_codebases` shows every workspace and whether it is indexed;
  `get_codebase_stats` gives file and chunk counts and the mode.
- Not indexed? `index_codebase` (mode `structural` unless the operator set up
  semantic search). Empty or stale results after real changes?
  `reindex_codebase`, then search again.
- A folder path that is not registered yet becomes a new workspace when you
  pass it to `index_codebase` — only when the user asked to work on it.
- Never assume a file's structure from its name: query the index, then read.

## 4. Tasks — the kanban board

Columns: **plan → work → review → done**. Every workspace has a board;
`"general"` is the suite-wide one. The board is the operator's task list and
the shared state between agents — keep it truthful.

Tools: `list_kanban_cards`, `get_kanban_card`, `create_kanban_card`,
`update_kanban_card`, `move_kanban_card`, `move_kanban_card_to_workspace`,
`delete_kanban_card`, `claim_kanban_card`, `complete_kanban_card`,
`approve_kanban_card`, `reject_kanban_card`, `cancel_kanban_card`,
`get_kanban_diff`, `get_kanban_activity_log`, `get_kanban_approval_mode`,
`set_kanban_approval_mode`.

### The life of a card

1. **Plan** — `create_kanban_card` (title, description, priority; column
   `plan`). Put a card on the board for every piece of work bigger than a
   one-line fix, and for anything the user asked to track. `blocked_by` names
   cards that must be merged first. Read the target board's approval mode.
   - `approval`: present the concrete requirement, scope and acceptance
     criteria as a Plan card, then STOP. The initial task request is not the
     approval of a plan the user has not seen. Wait until the user clicks
     **Approve work** in the card dialog; `get_kanban_card` reports the saved
     work approval. Do not claim, create a worktree, edit code or delegate
     implementation before this approval. Read-only investigation needed to
     describe the card is allowed. Changes to the plan reset its approval.
   - `auto_apply`: create the card and proceed directly to claiming it.
2. **Claim** — after work approval in `approval` mode, `claim_kanban_card`
   with your `agent_id` (`<client>-<n>`, e.g.
   `claude-code-1`, `codex-1`) and `intended_files` (comma-separated). The card
   moves to `work` and gets a branch of its own plus an isolated **worktree**
   under `<repo>/.qs-worktrees/`; the reply names the path and the user's test
   command. A claim is refused while a blocker is unmerged or the card is
   already claimed, or work approval is missing in `approval` mode. A
   file-conflict WARNING means another active card touches the same files —
   coordinate, or choose other files.
3. **Work** — everything happens inside the worktree path from the reply
   (absolute paths, or `cd` there). Commit as you go. Never merge, rebase or
   switch branches yourself. `get_kanban_diff` shows what the card changed.
4. **Complete** — `complete_kanban_card` once the work builds, is tested and
   committed. What follows depends on the board's mode
   (`get_kanban_approval_mode`):
   - `auto_apply` (default): merged into the main branch at once, pushed when
     a remote exists, worktree and branch cleaned up, card → `done`.
   - `approval`: this submits the result, not the final completion. Card →
     `review`, worktree preserved. Present what changed, verification and the
     exact user test command, then STOP. The user tests it and clicks
     **Approve result** in the card dialog to merge and move it to `done`,
     or **Request changes** to return it to `work` with the worktree preserved.
     Work approval never authorizes result approval. Never approve your own
     result or merge it through another tool, git command or UI automation.
   A merge conflict leaves the card in `review` — report it; never resolve it
   in the main checkout.
5. **Cancel** — `cancel_kanban_card` releases a claim you cannot finish; the
   worktree is removed. Say why.

User approval actions belong to the user: never invoke the native review
command or click approval controls yourself. Do not change boards, modes or
use standalone worktrees to bypass either gate.

Let the claim/complete flow move cards through `work → review → done`; use
`move_kanban_card` for planning only. Only the user changes the approval mode
(`set_kanban_approval_mode`) — never switch a board to `auto_apply` to get your
own work merged. Delete cards only when asked.

### The archive

The operator clears the Done column into the board's **archive** now and then
to keep the board short. Archived cards are finished work: off the board, but
kept with their status and dates. `list_kanban_cards` with `archived="true"`
lists them (newest first) — check it before planning anything that sounds
already done, and never redo or re-create an archived card. An archived card
cannot be claimed; only the operator restores one.

When you report on a card, name its column, branch and worktree and quote the
**test command** verbatim (section 5).

## 5. Parallel work — git worktrees

Other agents may be editing the same repository at the same time. A worktree is
an isolated checkout on its own branch: a claimed card has one, and
`create_worktree` makes one on request for work that is not a card.

- **Before editing a shared repository, call `create_worktree`** (`name` =
  what the work is about) and do ALL your work inside the returned path.
  A claimed card already has its worktree; do not create a second one.
  In `approval` mode use the card flow, after the user approves work.
  Commit as you go.
- Review and land: `list_worktrees`, `get_worktree_status` (commits ahead,
  uncommitted files), `get_worktree_diff` (full diff against the base),
  `merge_worktree` (one `--no-ff` merge commit on the base branch, uncommitted
  work committed first; a conflict aborts and leaves everything as it was —
  report it). `remove_worktree` discards a worktree and its branch for good —
  only when merged or unwanted.
- The main checkout stays on its branch: the tools never switch it, and
  neither do you.
- **Build rule**: your own Rust builds use the worktree's own `target/`
  (cargo's default). Never point `CARGO_TARGET_DIR` at the main checkout's
  target yourself, and never start `npm run tauri:dev` — port 1420 and the app
  window belong to the user. A frontend dev server may run from the worktree
  on a spare port.
- **Test command**: the user runs the app from your worktree with the
  one-liner every worktree reply carries (`get_worktree_test_command` returns
  it again for a card or a worktree). It shares the main `target/` on purpose.
  Show it to the user verbatim whenever you report on a card or a worktree;
  never run it yourself.
- A worktree is ready to build: the main checkout's `node_modules` / `.venv`
  are linked in. Do not run installs there.

### Temporary AI artifacts

Keep QA screenshots, scratch scripts, review exports and logs outside project
folders. `get_instructions` and worktree replies give the internal workspace
artifact directory; create a task subfolder there. Do not create a repository-root
`.output/qa` or put these files into source control. Build outputs required by the
project still use their configured locations. Preserve existing artifacts when
relocating them; verify the copy before removing the source.

## 6. Memory — QuantMemory (`quantsuite.memory.*`)

The vault is the durable, cross-session brain shared by every agent and the
operator: markdown files with frontmatter and `[[wikilinks]]`, one vault per
workspace for project knowledge plus the `general` vault for everything
cross-project. It is not a scratchpad.

**Write to it freely.** You can and should write into the vault with these
tools, without asking the user first: during a session as soon as you learn
something durable, and at the end of every session for whatever the next one
needs. A session that learned something and stored nothing is the exception
to explain, not the default. The suite's approval gate handles whatever needs
approving — you never need permission for a memory.

- **Recall**: `quantsuite.memory.search` (all scopes by default) and
  `quantsuite.memory.read` at session start, and whenever a task touches a
  topic you may have met before. `quantsuite.memory.list` browses by kind,
  tag or scope.
- **Store** what a future session needs and cannot derive from the code or
  the git history: decisions and their reasons, user preferences and
  corrections, gotchas, how a system is wired, what was tried and rejected.
  One memory per topic. Search first — extend an existing memory instead of
  creating a duplicate.
- **Write**: `quantsuite.memory.create` (title, markdown body, tags; `scope`
  = the workspace for project knowledge, `general` for shared knowledge —
  without a scope it lands in the active workspace, or general when none is
  open). `quantsuite.memory.append` is the preferred way to add facts to an
  existing memory; `quantsuite.memory.update` replaces the body and can drop
  links you did not notice.
- **Link**: connect related memories with `[[Title]]` wikilinks — they
  resolve in the memory's own scope first, then general.
  `quantsuite.memory.backlinks` before `quantsuite.memory.rename` or
  `quantsuite.memory.delete`; `quantsuite.memory.orphans` and
  `quantsuite.memory.suggest_links` after a batch of writes;
  `quantsuite.memory.stats` for the totals.
- Delete only when the user asks to forget something.

### The base — the vault's description of the codebase

Every workspace vault has a `base/` folder: one document per directory unit
of the codebase (the root, the shallow folders with source files, every
folder with a package manifest), each a memory of type `base`. Its generated
block lists files, lines, languages and sub-units; its description is what
agents and the operator learned about that part of the code — and that
description is what the code index cannot give you.

- **Read**: `quantsuite.memory.base_read` with the directory (or file) you
  are about to work in, before you search or read its code; without a path
  it returns the overview of the whole codebase. `quantsuite.memory.base_list`
  shows every unit and which ones nobody has described yet.
- **Describe**: `quantsuite.memory.base_describe` as soon as you understand a
  directory (what it is for, how it is wired, gotchas) or a file (one line).
  A directory without a document gets one; descriptions survive every sync.
- **Sync**: `quantsuite.memory.base_sync` when `base_list` is empty for a
  workspace and after larger restructurings — like `index_codebase`, it only
  regenerates the structural part and never deletes a document.

Where things go: **work items → kanban**, **durable knowledge → memory**,
**rules for agents → AGENT.md**, **the user's own notes → QuantNotes** (only
when asked).

## 7. The rest of the suite — `quantsuite.<module>.<name>`

Every suite capability is a tool. Prefer them over guessing about the suite's
data.

| Module | Tools | Use for |
| --- | --- | --- |
| notes | `quantsuite.notes.search`, `.list_notes`, `.create_note`, `.set_note_property` | the user's notes collection |
| plan | `quantsuite.plan.list_occurrences`, `.find_free_slots`, `.create_event` | the calendar: what is scheduled, free slots, new events |
| habit | `quantsuite.habit.list_habits`, `.stats`, `.create_habit`, `.set_check` | habits and their daily checks |
| finance | `quantsuite.finance.summary`, `.sankey`, `.create_item` | the monthly money plan (plans money, never moves it) |
| systems | `quantsuite.systems.list_systems`, `.run_backtest`, `.live_eval` | QuantSystems rotation systems: backtests, today's scores |
| algo | `quantsuite.algo.list_strategies`, `.get_trade_stats`, `.run_backtest`, `.start_bot` | QuantAlgo strategies, journal stats, backtests, the live bot |
| console | `quantsuite.console.run_command` | one shell command inside the suite (off unless the operator enabled it) |

**The approval gate.** Every `quantsuite.*` call passes the suite's approval
gate; each tool description carries its side-effect class:

- `read` and `compute` run at once.
- `write` runs at once in *relaxed* mode; in *strict* mode (the default) the
  call waits until the user approves it inside the suite. A waiting call is
  not a hang — do not retry it, do not work around it, tell the user what is
  waiting.
- `external` (`quantsuite.algo.start_bot`, `quantsuite.console.run_command`)
  is never auto-approved. Never call `quantsuite.algo.start_bot` unless the
  user explicitly asked for it in this session — it places real orders in
  live mode.

Use your own shell for commands; `quantsuite.console.run_command` is for the
rare case the user wants a command run inside the suite.

## 8. Instructions — this file and the workspace's (AgentOS)

- `get_agent_instructions` (scope `global`, `project` or `all`) reads what
  `get_instructions` already merged. Global = `~/.quantmcp/AGENT.md` (this
  file; the operator edits it under General → AgentOS in QuantMCP). Project =
  `<workspace>/AGENT.md`, edited under the workspace's AgentOS.
- `update_agent_instructions` rewrites one scope; `init_agent_md` creates the
  default template for a scope that has none. Change instructions only when
  the user asks. Project rules go into the project file, never into this one.
  A rewrite replaces the whole file: read it first, keep everything the user
  wrote.

## 9. Working rules

- Build what was asked, all of it, and nothing that was not. Mention extras
  as options.
- Edit existing files over creating new ones; match the code style around you.
- Before touching shared state (board, memory, merges), check its current
  state with the tool — not what you remember from earlier in the session.
- Run the project's tests or build before completing a card; report failures
  with their output, not a summary.
- A tool error usually names the fix (`not indexed` → `index_codebase`,
  `blocked by` → wait or ask, `already claimed` → another agent has it). Read
  it before retrying.
- When you finish: say what changed (files, branch), what you verified and
  what is open — and write to memory whatever the next session should know.

## 10. Quick reference

| Goal | Tools |
| --- | --- |
| Start a session | `get_instructions`, `quantsuite.memory.search`, `list_kanban_cards` |
| Find code | `search_code`, `lookup_symbol` |
| Index a workspace | `index_codebase`, `reindex_codebase`, `list_codebases`, `get_codebase_stats` |
| Track work | `create_kanban_card`, `claim_kanban_card`, `complete_kanban_card`, `get_kanban_diff` |
| Review work | `list_kanban_cards` (column `review`), `get_kanban_diff`, `approve_kanban_card`, `reject_kanban_card` |
| Work in parallel | `create_worktree`, `get_worktree_status`, `get_worktree_diff`, `merge_worktree` |
| The user's test command | `get_worktree_test_command` |
| Remember and recall | `quantsuite.memory.create`, `.append`, `.search`, `.read` |
| Understand the codebase | `quantsuite.memory.base_read`, `.base_describe`, `.base_list`, `.base_sync` |
| Rules | `get_agent_instructions`, `update_agent_instructions` |
