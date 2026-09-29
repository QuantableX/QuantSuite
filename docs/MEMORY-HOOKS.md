# Memory in prompts (QuantMemory prompt hooks)

Agents forget to search their memory. The session-start protocol in AgentOS
asks them to call `quantsuite.memory.context`, but recall stays pull-only. A
prompt hook turns it around: every prompt an agent sends from a registered
workspace goes to QuantMCP first, and the most relevant QuantMemory excerpts
come back as extra context, with citations. When nothing is relevant enough,
nothing is added.

In QuantMCP, open **Settings → Memory in prompts** and click **Install** next
to an agent (**Remove** takes it out again). Start a new agent session
afterwards. Codex additionally asks you to trust a new hook once in `/hooks`.

## How it works

```
agent prompt ──► hook command (curl, stdin = the event JSON)
             ──► POST http://127.0.0.1:<port>/quantmemory/<client>/<event>
             ──► workspace = innermost registered folder containing `cwd`
             ──► quantsuite.memory.context (same capability agents call)
             ──► relevance gate ──► cited excerpts as additionalContext
```

- **Endpoint.** `POST /quantmemory/{client}/{event}` on the QuantMCP port
  (3100 release, 3101 dev), in `modules/mcp/crate/src/memory_hook.rs`.
  `client` is the `clients.rs` id (`claude-code`, `codex-cli`), `event` is
  `user-prompt-submit`. The route sits outside the MCP router's CORS layer
  and refuses any request with an `Origin` header or a non-loopback `Host`,
  so a web page cannot read the vault through it.
- **Input.** Claude Code and Codex both send `prompt` and `cwd` (observed
  live; one docs page names the field `user_prompt`, which is accepted too).
  Prompts shorter than 8 characters (`ok`, `go on`) are skipped. The query
  is the prompt, cut to `memory_context`'s 2000-byte limit.
- **Workspace.** `qs_core::workspaces::containing(cwd)`: the registered
  folder that is `cwd` or its closest ancestor, by whole path components.
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
  the policy. The answer is `{"hookSpecificOutput": {"hookEventName":
  "UserPromptSubmit", "additionalContext": "…"}}`, which both clients read.
- **Log.** Each call adds one line to the QuantMCP request log: the
  workspace and the injected ids, or why nothing was injected. The prompt
  text is never logged.

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

It is written as one appended group, `{"hooks": [{"type": "command",
"command": …, "timeout": 5}]}`, under `hooks.UserPromptSubmit` in:

- Claude Code: `settings.json` in `CLAUDE_CONFIG_DIR`, else `~/.claude`.
- Codex: `hooks.json` in every Codex home the AgentOS import knows
  (`~/.codex`, `CODEX_HOME`, `ORCA_CODEX_HOME`, Orca account homes).

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
file before and after its edit (QuantMCP's module folder,
`memory-hooks/`). When the file is still exactly what install wrote, the
snapshot's original goes back — or the file is deleted again when install
created it. When the file changed since (the user or the agent edited a
setting), only our handlers are cut out and everything else stays as it
now is; a group, event list or `hooks` object left empty goes with them.
Unit tests cover both paths on temporary files, including CRLF files, an
absent file, a shared group and a port change.

## Client support

| Client | Prompt-time context | Wired | Evidence |
|---|---|---|---|
| Claude Code | `UserPromptSubmit` → `hookSpecificOutput.additionalContext` (or plain stdout), 10,000-character cap per hook | Yes | Live canary on 2.1.284: the hook received `prompt` + `cwd`, the model answered with the canary word from `additionalContext`, in Git Bash and in PowerShell ([hooks reference](https://code.claude.com/docs/en/hooks)) |
| Codex CLI | `UserPromptSubmit` → `hookSpecificOutput.additionalContext`, ~2,500-token limit; new hooks run only after the user trusts them in `/hooks` | Yes | Live on 0.159.0 with `--dangerously-bypass-hook-trust` and a `-c hooks.…` override: the same command delivered `prompt` + `cwd` and completed; the model step was not observed (account usage limit). Codex shows the added context in its transcript ([hooks](https://learn.chatgpt.com/docs/hooks), [openai/codex#16933](https://github.com/openai/codex/issues/16933)) |
| Cursor | `beforeSubmitPrompt` can only continue or block (`continue`, `user_message`); only `sessionStart` accepts `additional_context`, all hooks' context merged and capped at 10,000 characters | No | [Cursor hooks](https://cursor.com/docs/agent/hooks); cap observed in the Cursor 3.21.18 source (AgentOS import, card 88712416) |
| Gemini CLI | `BeforeAgent` → `hookSpecificOutput.additionalContext`, appended to the turn's prompt; timeout in milliseconds | No | Supported per the [hooks reference](https://geminicli.com/docs/hooks/reference/), but Gemini CLI is not installed on the development machine, so the shell it uses for hook commands on Windows is unverified. Wiring it is one `HookClient` row plus its settings path |
| OpenCode | No prompt hook in config; a JS/TS plugin's `chat.message` hook could add a text part | No | Needs a plugin file instead of a config entry; not built |

## Verifying

- `cargo test -p tauri-plugin-mcp --lib memory_hook` — endpoint logic (input,
  query, gate, rendering, loopback check) and the installer (byte-exact
  round trips, foreign edits, shared groups, refused shapes).
- `cargo test -p tauri-plugin-qs --lib containing` — the cwd → workspace
  resolution.
- Live: run a debug build (port 3101, `~/.quantsuite-dev`), register a test
  workspace with a memory, and start `claude -p "<prompt about it>"
  --settings <temp file with the hook>` from that folder; `--debug-file`
  shows `Hook UserPromptSubmit (…) provided additionalContext`. Stop the
  build and run the prompt again: it answers normally, the hook exits 0.

## Adding an event or a client

`HookEvent` names the event (URL slug and the name both clients use) and
`HookClient::events` lists what a client hooks into; the installer appends
one group per event and removes all of ours by URL. A client whose hook
config has the same `hooks.<Event>[].hooks[]` shape needs only a
`HookClient` row and its `install::targets` path.
