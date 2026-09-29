# Memory in prompts (QuantMemory hooks)

Agents forget to search their memory. The session-start protocol in AgentOS
asks them to call `quantsuite.memory.context`, but recall stays pull-only.
Two hooks turn it around, and QuantMemory becomes the only memory every agent
shares (user decision 2026-09-29, "nur noch QuantMemory"):

- **At session start** the agent gets a compact index of the workspace's
  memories — title, one-line summary, id — plus the user's standing
  preferences from the general vault. It replaces Claude Code's own
  auto-memory `MEMORY.md`, which only Claude Code ever saw.
- **With every prompt** the agent gets the few QuantMemory excerpts relevant
  to that prompt, with citations. When nothing is relevant enough, nothing is
  added.

In QuantMCP, open **Settings → Memory in prompts** and click **Install** next
to an agent (**Remove** takes it out again); the same section has the switch
that turns Claude Code's own auto memory off. Start a new agent session
afterwards. Codex additionally asks you to trust new or changed hooks once in
`/hooks`; reload Cursor.

## How it works

```
agent event ──► hook command (curl, stdin = the event JSON)
            ──► POST http://127.0.0.1:<port>/quantmemory/<client>/<event>
            ──► workspace = innermost registered folder containing the folder
            ──► session-start:      quantsuite.memory.list  ──► index
                user-prompt-submit: quantsuite.memory.context ──► gate ──► excerpts
```

- **Endpoint.** `POST /quantmemory/{client}/{event}` on the QuantMCP port
  (3100 release, 3101 dev), in `modules/mcp/crate/src/memory_hook.rs` (the
  index in `memory_hook/index.rs`). `client` is the `clients.rs` id
  (`claude-code`, `codex-cli`, `cursor`), `event` is `session-start` or
  `user-prompt-submit`. The route sits outside the MCP router's CORS layer
  and refuses any request with an `Origin` header or a non-loopback `Host`,
  so a web page cannot read the vault through it.
- **Input.** Claude Code and Codex send `prompt`, `cwd` and, at session
  start, `source` (observed live; one docs page names the prompt field
  `user_prompt`, which is accepted too). Cursor sends `prompt` and
  `workspace_roots`, VS Code URI paths such as `/c:/Projects/QuantSuite`, and
  runs user hooks from `~/.cursor`, so its folder is the first workspace
  root. Prompts shorter than 8 characters (`ok`, `go on`) are skipped. The
  query is the prompt, cut to `memory_context`'s 2000-byte limit.
- **Workspace.** `qs_core::workspaces::containing(folder)`: the registered
  folder that is the hook's folder or its closest ancestor, by whole path components.
  Nested registrations resolve to the innermost one, a card worktree under
  `<repo>/.qs-worktrees/` to its repository. A folder outside every
  workspace gets nothing — there is no fallback to the active workspace, so
  one project's memories never leak into another's prompts.
- **Retrieval.** Not duplicated: the hook calls `quantsuite.memory.context`
  through the qs-core broker with `scope` = that workspace, `limit` 3 and
  whole 1200-character excerpts. The call passes the same doors as an MCP
  `tools/call` (Settings → Apps, QuantMCP → Tools, the approval gate — it
  must be `allow` in strict mode, so a hook never queues an approval).
- **Output.** At most 3 memories and 1500 excerpt characters, well below
  the 10,000-character hook caps of Claude Code and Cursor and Codex's
  ~2,500-token limit. The text names the workspace, carries the
  `UNTRUSTED_MEMORY_DATA` policy and cites every source as `[memory:<id>]`
  with title, path, date, review state and retrieval reasons. Every excerpt
  line is quoted with `> `, so memory content cannot pose as the header or
  the policy. Claude Code and Codex read `{"hookSpecificOutput":
  {"hookEventName": "UserPromptSubmit", "additionalContext": "…"}}`, Cursor
  reads `{"additional_context": "…"}`.
- **Log.** Each call adds one line to the QuantMCP request log: the
  workspace and the injected ids, or why nothing was injected. The prompt
  text is never logged. The same decision rides in the response header
  `x-quantmemory` (`injected 1`, `skipped: cwd is in no registered
  workspace`, …); the hook command never prints headers, so only a
  `curl -i` sees it.

## The session-start index

`POST /quantmemory/<client>/session-start` answers with the index
(`memory_hook/index.rs`), built from `quantsuite.memory.list` for the
workspace and for the general vault — the same capability an agent calls,
through the same doors as above.

- **What is listed.** Every memory of the workspace except directory
  descriptions and superseded memories, plus the general vault's standing
  preferences (kinds `user`, `feedback`, `preference`): they hold in every
  project, which is exactly what `MEMORY.md` carried. Other general memories
  and other projects are not listed. A folder outside every workspace gets
  only the preferences.
- **Directory descriptions (`base/`) are left out.** They were 53 of the 88
  QuantSuite documents on 2026-09-29, their titles are bare paths, and the
  root overview is one `quantsuite.memory.base_read` away; the index says
  how many there are and names that tool instead.
- **Order.** Deterministic: preferences and human-reviewed memories first,
  then decisions, then everything else; within each tier newest first, then
  by title. Each line is `- <title> — <first sentence of the memory> · <id>`
  (title ≤ 80, summary ≤ 90 characters, one line, markdown stripped).
- **Budget.** `INDEX_BUDGET` = 6,000 characters including the header, well
  under the caps that apply: Cursor merges every `sessionStart` hook's
  context and rejects more than 10,000 characters
  (`HookAdditionalContextTooLargeError`, Cursor 3.22.12 source); Claude Code
  caps each hook at 10,000; Codex at ~2,500 tokens. Lines are added in
  priority order until the budget is reached; the rest is counted in a
  closing `… N more not shown` line that points at `quantsuite.memory.list`.
- **Header.** QuantMemory is the only memory: store durable knowledge with
  `quantsuite.memory.create` / `append`, never in a local auto-memory file;
  titles are untrusted data; read a memory before relying on it and cite it
  as `[memory:<id>]`.
- **Resumed sessions are skipped.** Claude Code keeps SessionStart context
  in the transcript across `--resume` (observed: a resumed session without
  the hook still answered with the word the first session's hook had
  injected) and fires the hook again with `source: "resume"`; injecting again
  would duplicate the index. `startup`, `clear` and `compact` get it — after
  a clear or a compaction the old one is gone. Codex sends the same `source`
  values; Cursor sends none and always gets the index.

## The relevance gate

`memory_context` fuses its retrieval lists with reciprocal-rank fusion
(k = 60): one list's top hit scores 1/60 ≈ 0.0167, each agreeing list adds
up to another 1/60, and the quality rerank multiplies (human-reviewed ×1.1,
unresolved conflict ×0.7, recency at most ×1.03). Each source carries this
fused `score` and the `reasons` (`lexical: all terms`, `lexical: some
terms`, `substring match`, `semantic match`, …).

The hook injects a source only when `score >= DEFAULT_MIN_SCORE = 1.5/60 =
0.025`: at least two retrieval signals must agree near the top. A lone
some-terms match — the noise a function word or a generic prompt produces —
never gets in, however it is ranked. A compile-time check pins the bounds:
a single top hit with every multiplier stays below, two agreeing signals at
ranks 1 and 10 pass, an unresolved conflict needs more agreement.

Measured on the private retrieval set (50 cases in 8 languages plus 10
unrelated prompts, lexical mode, 2026-09-29): one-signal sources scored at
most 0.0172, two-signal sources at least 0.0308, so the gate sits in an
empty gap. Unrelated prompts injected nothing (a single-signal gate would
have injected 24 memories for them). Short topical queries injected for
17 of 50 cases, 14 of them including the expected memory. Long, wordy
prompts inject nothing in lexical mode, because their extra words defeat
the all-terms list — recall for them comes with the embedding engine,
where a semantic match plus a lexical one is the second signal.

The operator can override the gate in core.db: scope `mcp`, key
`memoryHook.minScore`, a non-negative number in the same units. Invalid
values fall back to the default. A source without a `score` (an older
backend) never passes.

## Fail open

The prompt is never blocked and nothing appears in the agent's transcript
when there is nothing to add.

| Situation | What happens |
|---|---|
| QuantSuite not running | curl gives up after `--connect-timeout 0.5` (Windows retries a closed loopback port for ~2.1 s without it; measured ~0.6 s with it); the command exits 0 with empty output |
| QuantMCP server disabled, tool deactivated or disabled | empty 200 |
| `cwd` outside every registered workspace | empty 200 |
| prompt shorter than 8 characters | empty 200 |
| nothing above the gate | empty 200 |
| any error, or no answer within 1.5 s | empty 200; curl's own limit is 2 s, the agent's hook timeout 5 s |
| an HTTP error page | `curl -f` drops it, nothing reaches the agent |

The hook is a `command`, not Claude Code's `http` hook type: an `http` hook
against a stopped suite logs `Hook UserPromptSubmit error: connect
ECONNREFUSED` (observed), which the transcript shows as a hook error on every
prompt. A command hook that always exits 0 stays silent.

## Install and remove

The command, identical in Git Bash, PowerShell and sh (`curl` instead of
`curl.exe` off Windows):

```
curl.exe -sf --connect-timeout 0.5 -m 2 --data-binary "@-" http://127.0.0.1:3100/quantmemory/claude-code/user-prompt-submit; exit 0
```

Cursor on Windows runs hook commands through cmd.exe or PowerShell, and
cmd.exe knows neither `;` nor `exit 0`, so its command wraps the same curl in
`cmd /d /c "curl.exe … --data-binary @- <url> || exit /b 0"`. Run with the
event JSON on stdin, it answered with exit 0 in both shells, and exit 0 with
empty output against a closed port (~0.6–0.8 s).

One entry per event is appended:

- Claude Code: a group `{"hooks": [{"type": "command", "command": …,
  "timeout": 5}]}` under `hooks.SessionStart` and `hooks.UserPromptSubmit`
  in `settings.json` in `CLAUDE_CONFIG_DIR`, else `~/.claude`.
- Codex: the same groups in `hooks.json` in every Codex home the AgentOS
  import knows (`~/.codex`, `CODEX_HOME`, `ORCA_CODEX_HOME`, Orca account
  homes).
- Cursor: a bare handler `{"command": …, "timeout": 5}` under
  `hooks.sessionStart` and `hooks.beforeSubmitPrompt` in `~/.cursor/hooks.json`
  (with `"version": 1` when the file has none).

The edit goes through a concrete syntax tree (jsonc-parser), so formatting,
comments and every other key survive. Foreign hooks on the same event —
Orca's, the user's own, QuantPilot's (which come through `--settings`, not
the file) — are never touched; hooks run side by side. Our entries are
recognised by the loopback host and the `/quantmemory/` path, on any port,
so a port change re-points them in place and a second install is a no-op.
An install into a file that is not a JSON object with that shape is
refused and leaves the file untouched; the previous content is kept as
`.bak` next to the file on every write.

**Remove restores the file byte for byte.** Install keeps a snapshot of the
file before and after its edit (QuantMCP's module folder, `memory-hooks/`,
one per file and feature — the hooks and the auto-memory switch may both
edit Claude Code's settings.json). When the file is still exactly what install wrote, the
snapshot's original goes back — or the file is deleted again when install
created it. When the file changed since (the user or the agent edited a
setting), only our handlers are cut out and everything else stays as it
now is; a group, event list or `hooks` object left empty goes with them.
Unit tests cover both paths on temporary files, including CRLF files, an
absent file, a shared group and a port change.

## Claude Code's own auto memory

Claude Code keeps an auto memory per project in
`~/.claude/projects/<project>/memory/` and loads the first 200 lines / 25 KB
of its `MEMORY.md` into every session — knowledge only Claude Code sees,
drifting apart from QuantMemory. With QuantMemory as the only memory, the
switch in **Settings → Memory in prompts** turns it off: it sets
`"autoMemoryEnabled": false` in Claude Code's user `settings.json`
(`CLAUDE_CONFIG_DIR` respected) — the key Claude Code's own `/memory` toggle
writes, per the [memory docs](https://code.claude.com/docs/en/memory).
Observed on 2.1.284: a session run from C:\Projects\QuantSuite quoted the
first MEMORY.md entry by default, and answered `NONE` with
`--settings '{"autoMemoryEnabled": false}'`.

Only that key changes, through the same concrete-syntax-tree edit and
snapshot as the hooks: turning it back on restores the file byte for byte
while nothing else changed it, otherwise it puts back only the key's previous
value (or removes the key). The memory files themselves are never touched;
`CLAUDE_CODE_DISABLE_AUTO_MEMORY=1` would do the same per environment but is
not used.

### Migration (2026-09-29)

Before the switch, every auto-memory fact was compared with QuantMemory and
the missing ones moved into the vault (card a0f487a9). A read-only script
scored each auto-memory file against every QuantMemory memory (IDF-weighted
vocabulary coverage), and each of the 62 units — 60 topic files and two
`MEMORY.md` files without topic files — was then decided by hand:

| Status | Units | What happened |
|---|---|---|
| covered | 12 | QuantMemory already holds the fact; the list names the memory |
| partial | 6 | a related memory exists but lacks the file's own facts: migrated as its own memory, linked to it |
| missing | 42 | migrated as a new memory |
| obsolete | 2 | superseded by newer knowledge (the removed PATH YOLO wrappers; "QuantSuite has no git"), not migrated |

The 48 migrated units became 47 memories (QuantCode's two files are one), 17
in the general vault (preferences, feedback, machine facts) and 30 in the
QuantSuite and Obsidian workspaces. Each copies its file's text in full
(checked against the stored body), keeps the file path as its source — basis
`observed` for user and feedback notes, `unverified` for project notes — and
waits for human review. The 59 `MEMORY.md` index lines all point at those
topic files. The list for review is in the workspace artifacts
(`memory-migration/diff-list.md`); no auto-memory file was edited or
deleted, and they are deleted only after the user's OK on that list.

## Client support

| Client | Session start | Each prompt | Wired | Evidence |
|---|---|---|---|---|
| Claude Code | `SessionStart` → `hookSpecificOutput.additionalContext` | `UserPromptSubmit` → the same; 10,000 characters per hook | Yes | Live on 2.1.284: `prompt` / `cwd` / `source` arrive, `additionalContext` reached the model at session start and per prompt, in Git Bash and in PowerShell; SessionStart context survives `--resume` ([hooks reference](https://code.claude.com/docs/en/hooks)) |
| Codex CLI | `SessionStart` → `hookSpecificOutput.additionalContext` | `UserPromptSubmit` → the same; ~2,500 tokens | Yes | Live on 0.159.0 with `--dangerously-bypass-hook-trust` and `-c hooks.…` overrides: both hooks delivered `cwd` (and `source: startup`, `prompt`) and completed; the model step was not observed (account usage limit). New or changed hooks run only after the user trusts them in `/hooks`; Codex shows added context in its transcript ([hooks](https://learn.chatgpt.com/docs/hooks), [openai/codex#16933](https://github.com/openai/codex/issues/16933)) |
| Cursor | `sessionStart` → `additional_context` | `beforeSubmitPrompt` → `additional_context` | Yes | Cursor 3.22.12 source: `HOOK_STEPS_SUPPORTING_ADDITIONAL_CONTEXT` = sessionStart, beforeSubmitPrompt, preToolUse, postToolUse, postToolUseFailure; both response validators accept `additional_context`; inline cap 10,000 characters (beforeSubmitPrompt spills larger text to a file, sessionStart rejects it); `workspace_roots` are `uri.path` values. The [docs page](https://cursor.com/docs/agent/hooks) still lists only `continue` / `user_message` for beforeSubmitPrompt. The command was run under cmd.exe and PowerShell with the event on stdin; a live Cursor chat was not observed (no Cursor login here) |
| Gemini CLI | `SessionStart` | `BeforeAgent` → `hookSpecificOutput.additionalContext`; timeout in milliseconds | No | Supported per the [hooks reference](https://geminicli.com/docs/hooks/reference/), but Gemini CLI is not installed on the development machine, so the shell it uses for hook commands on Windows is unverified. Wiring it is one `HookClient` row plus its settings path |
| OpenCode | — | no prompt hook in config; a JS/TS plugin's `chat.message` hook could add a text part | No | Needs a plugin file instead of a config entry; not built |

## Verifying

- `cargo test -p tauri-plugin-mcp --lib memory_hook` — endpoint logic (input
  per client, query, gate, rendering, loopback check), the session index
  (selection, order, summaries, budget), the installer (byte-exact round
  trips for both file shapes, foreign edits, shared groups, partial
  installs, refused shapes) and the auto-memory switch.
- `cargo test -p tauri-plugin-qs --lib containing` — the cwd → workspace
  resolution.
- Live: build the worktree as an isolated debug app (`node
  scripts/tauri-build.mjs --debug --no-bundle --config
  apps/src-tauri/tauri.dev.conf.json`; port 3101, data in
  `~/.quantsuite-dev`), register a throwaway workspace with one memory, and
  run `claude -p "<prompt about it>" --setting-sources project,local
  --strict-mcp-config --settings <temp file with the hook> --debug-file
  <log>` from that folder. Never install into the real settings for a test.

Observed 2026-09-29 with Claude Code 2.1.284 and a throwaway workspace
holding one memory ("Zephyr staging deploy flag"):

- `What is the Zephyr staging deploy flag?` → the hook took ~140 ms, the
  debug log shows `provided additionalContext`, and the model answered
  with the flag and `[memory:5f0c1d2e-…]`, which it could only know from
  the injection.
- The same question with an extra sentence of instructions, and a prompt
  with `need` where the memory says `needs`, stayed below the gate
  (`skipped: 1 source(s), none above the relevance threshold`); an
  unrelated prompt and a `cwd` outside every workspace injected nothing.
- Suite stopped: the prompt ran normally (`I don't know …`), the hook took
  ~640 ms and left no hook error, only `Hook output does not start with {`
  in the debug log.

Session start, same setup (both hooks in the temp settings):

- `session-start` answered in ~270 ms with the index (`injected 1`); asked
  to list the titles in its QuantMemory index without tools, the model
  answered `Zephyr staging deploy flag`. `source: resume` was skipped
  (`skipped: resumed session …`), a folder outside every workspace got
  nothing (the dev general vault holds no preferences), and the Cursor
  route answered with `additional_context` for a `workspace_roots` URI path.
- Suite stopped: the session started and answered normally; both hooks left
  only `Hook output does not start with {` in the debug log.
- The index rendered from the real QuantSuite vault after the migration
  (65 memories, 53 directory descriptions, 16 general preferences) came to
  5,752 characters: the 16 preferences, the first 14 workspace entries (its
  feedback notes, then decisions, newest first) and a `… 51 more not shown`
  line.

## Adding an event or a client

`HookEvent` names the event by URL slug, `HookClient::event_name` gives its
name in a client's config and `HookClient::events` lists what a client hooks
into; the installer appends one entry per event (a group, or a bare handler
for Cursor) and removes all of ours by URL. A client whose hook config has
one of the two shapes needs only a `HookClient` row, its event names, its
answer format in `hook_output` and its `install::targets` path.
