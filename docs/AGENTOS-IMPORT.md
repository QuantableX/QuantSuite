# Import General AgentOS into agents

In QuantMCP, open **General → AgentOS → Choose agents**. Search the picker,
check recipients, click **Done**, then **Import selected**. The editor shows a
compact selection summary; the picker has **Available**, **All agents** and
**Custom** filters, with detection, destination and setup details beside the
focused agent. The native dialog supports keyboard focus and Escape, and
stacks the list and details in narrow windows.

Automatic discovery uses a registry of known integrations, checking local
configuration directories, CLI executables and Kilo's installed VS Code/Cursor
extension. It cannot infer arbitrary agents' instruction conventions. Nothing
is selected by default. **Rescan** drops selections that are no longer available.
**Select automatic** checks ready file imports; manual recipients remain
individually selectable and show their setup steps. Native instruction support
and MCP support are independent: Pi is a valid import recipient without MCP.

If there are unsaved changes, the button becomes **Save & import selected**;
a failed save stops the import. **Settings → Connect** sets up MCP connections
only. Instruction imports always require a selection in AgentOS.

The source stays `~/.quantmcp/AGENT.md`. Native instruction files receive a
managed `quantmcp:agentos` section. Existing text outside that section is
preserved, changed files have a `.bak`, and repeat imports replace the same
section. Invalid encodings, ambiguous markers and write errors are reported
without replacing the affected file. Results are independent per destination.

Start a new agent session after importing. An import does not inject text
into an already-running conversation. Re-import after editing General
AgentOS or adding an Orca account. This is an explicit import, not a file
watcher. Workspace instructions remain available through QuantMCP's
`get_instructions`.

The MCP connection sends only a short pointer to `get_instructions`. Some
clients repeat connection instructions in every tool description, so sending
the full AgentOS brief there can multiply its token cost by the number of
tools. The full brief and workspace rules are still returned by
`get_instructions` once per session.

## Automatic destinations

Only selected, available recipients are imported. Selecting a built-in agent
uses its supported global profile paths; custom targets can name another profile.

| Agent | Destination |
| --- | --- |
| Pi | `~/.pi/agent/AGENTS.md`, respecting `PI_CODING_AGENT_DIR` (absolute or `~/` path). Imports into the first existing context file instead: `AGENTS.override.md`, `AGENTS.md`, `AGENTS.MD`, `CLAUDE.md`, `CLAUDE.MD`. Even an empty override takes precedence. Pi loads instructions directly; MCP requires an extension. |
| Oh My Pi (OMP) | `~/.omp/agent/AGENTS.md`. Respects `PI_CONFIG_DIR`, `PI_CODING_AGENT_DIR`, and active `OMP_PROFILE` (fallback `PI_PROFILE`); named profiles use `<root>/profiles/<name>/agent` and take precedence over the agent-dir override. Other profiles can be added as custom targets. OMP's native MCP config is a separate `mcp.json` in the same agent directory, configured by Settings → Connect. |
| Codex | `AGENTS.md` in the default home, explicit `CODEX_HOME` / `ORCA_CODEX_HOME`, and existing Orca `codex-accounts/<account>/home` profiles. A nonempty `AGENTS.override.md` receives the section instead because it shadows `AGENTS.md`. |
| Claude Code | `CLAUDE_CONFIG_DIR/CLAUDE.md`, or `~/.claude/CLAUDE.md` |
| Gemini CLI | `~/.gemini/GEMINI.md` (default context filename) |
| OpenCode | `~/.config/opencode/AGENTS.md`, respecting `XDG_CONFIG_HOME`; also an explicit `OPENCODE_CONFIG_DIR` |
| Cline | The user's Documents directory, `Cline/Rules/quantmcp-agentos.md` |
| Roo Code | `~/.roo/rules/quantmcp-agentos.md` |
| Continue | `~/.continue/rules/quantmcp-agentos.md` |
| Amp | `~/.config/amp/AGENTS.md` |
| Kilo Code / Kilo CLI | Adds the absolute General AgentOS source path once to the shared global config's `instructions` array. Uses `~/.config/kilo`, respecting `XDG_CONFIG_HOME`. Existing config preference matches Kilo: `kilo.jsonc`, `kilo.json`, `opencode.jsonc`, `opencode.json`, `config.json`; otherwise creates `kilo.jsonc`. |
| Windsurf | A short startup directive in `~/.codeium/windsurf/memories/global_rules.md` that tells the agent to read the live General AgentOS source. The full brief exceeds Windsurf's 6,000-character global limit. |

Kilo Code and Kilo CLI share their global configuration: selecting either
enables the instruction reference for both. Kilo loads the live source in new
sessions, so later General AgentOS edits do not require another import for
Kilo. JSONC comments, existing instruction entries and other settings survive;
changed files get a `.bak`. Invalid JSONC, duplicate `instructions` properties
or an invalid instructions value are reported without writing the file.
This targets current Kilo versions using the shared global config; old
extension versions that only load legacy rules should be upgraded.

Cursor, Claude Desktop, VS Code Copilot and aider receive manual setup
instructions in the report. **Copy instructions for manual setup** copies
the General text. These rows are not counted as successful native imports.
For example, Cursor's global User Rules live in its settings UI, whereas
its documented rule files are project scoped. The importer does not rewrite
private application databases.

Custom instruction discovery settings in other clients can change what they
load; the destinations above describe the supported defaults. The report
confirms file installation, not a model's compliance with every instruction.

## Custom agents

Use **Add custom** to save an agent name and an absolute `.md` instruction-file
path. **Browse files** chooses an existing Markdown file; you can also type a
new filename in an existing directory. The folder must exist before importing.
Names and paths persist in QuantSuite's core settings (`mcp`,
`agentos.custom_recipients`). Custom rows can be edited or removed; removing a
row never deletes its instruction file or removes previously imported text.

A custom row means the operator chose a destination. It does not assert that
an application was found or that it will load that file. MCP setup remains
separate. Custom imports use the same managed blocks, backups, encoding checks,
idempotency and source-file protection as built-in imports. Empty, stale or
unknown selections fail; custom IDs must be registered, paths must be absolute
Markdown files, and duplicate IDs/paths are rejected before any writes.

## Implementation and verification

The existing `scan_clients` command reports instruction import capability
separately from MCP setup capability (`instructions_manual`, `instructions_note`,
`mcp_support`) and includes destination paths, detection evidence and `custom`.
The AgentOS UI refuses imports against an older backend lacking those fields,
since an old backend would ignore the selection and import all agents.

The existing `connect_clients` Tauri command accepts `instructionsOnly: true`
and a required nonempty `clientIds` selection for the AgentOS button. Custom
definitions are read from core settings on both scan and import; import callers
cannot smuggle an unregistered path in an ID. Unknown
IDs fail before writing; repeated IDs are deduplicated; installation is checked
again immediately before each import. That path neither reconfigures MCP connections nor
records a new connection in the client registry. Client destinations/manual
steps are in the existing `CLIENTS` table; the managed writer and Codex profile
discovery are in `clients/instructions.rs`; Kilo's comment-preserving writer
is in `clients/instructions/kilo.rs` and uses the `jsonc-parser` CST. Pi/OMP
discovery is in `clients/instructions/pi.rs`; custom selection and file import
are in `clients/instructions/custom.rs`. The picker is `AgentRecipients.vue`.
The picker previews a proposed custom list through `scan_clients` before
saving it, so invalid platform paths or duplicate file aliases cannot break
the persisted list. `PI_CONFIG_DIR` is relative to the home directory; OMP's
agent-directory override must be absolute. A native OMP `AGENTS.md` takes
precedence over user-level context it discovers from other agents.

`cargo test -p tauri-plugin-mcp --lib` covers managed-section replacement,
backups, idempotency, Unicode, refusal of damaged files, Codex overrides,
Orca profiles, full-brief installation, explicit selection, disappearance,
Kilo JSONC preservation, Pi override precedence, OMP profiles, custom validation,
unavailable custom folders and preservation of unselected files.
Frontend validation includes `npm run check:ts` and browser checks of the
compiled AgentEditor with mocked Tauri calls: exact recipient forwarding,
refresh, empty detection, old-backend refusal, save-before-import,
save/import failures, manual results and workspace-switch races. Picker checks
also cover search/filters, Pi/OMP capability details, custom create/edit/remove,
saved settings on reload, duplicate paths, file browsing, Escape and narrow
layouts. Native AI sessions are not launched by those tests.

## Client references

- [Pi context files and MCP extension model](https://github.com/badlogic/pi-mono/tree/main/packages/coding-agent)
- [OMP context files](https://github.com/can1357/oh-my-pi/blob/main/docs/context-files.md)
- [OMP profiles and config paths](https://github.com/can1357/oh-my-pi/blob/main/docs/config-usage.md)
- [OMP MCP configuration](https://github.com/can1357/oh-my-pi/blob/main/docs/mcp-config.md)
- [Codex instruction discovery](https://developers.openai.com/de-DE/docs/agent-configuration/agents-md)
- [Claude Code memory](https://code.claude.com/docs/en/memory)
- [Gemini context files](https://geminicli.com/docs/cli/gemini-md/)
- [OpenCode instructions](https://opencode.ai/v2/docs/instructions)
- [Cursor rules](https://cursor.com/docs/rules)
- [Windsurf rules and limits](https://docs.windsurf.com/windsurf/cascade/memories)
- [Cline rules](https://docs.cline.bot/customization/cline-rules)
- [Roo Code instructions](https://roocodeinc.github.io/Roo-Code/features/custom-instructions/)
- [Continue global configuration](https://docs.continue.dev/guides/configuring-models-rules-tools)
- [Amp instructions](https://ampcode.com/docs/customize/agents-md)
- [Kilo rules](https://kilo.ai/docs/customize/custom-rules)
- [Kilo global configuration implementation](https://github.com/Kilo-Org/kilocode/blob/main/packages/opencode/src/config/config.ts)
