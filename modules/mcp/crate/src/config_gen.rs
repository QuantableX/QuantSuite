//! The catalogue of QuantMCP's own tool families — name, description and
//! parameters for `tools/list` and the dashboard. (The AI-client table that
//! used to share this file is `clients.rs`.)

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodebaseIndexToolInfo {
    pub name: String,
    pub description: String,
    pub parameters: Vec<CodebaseIndexToolParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodebaseIndexToolParam {
    pub name: String,
    pub param_type: String,
    pub description: String,
    pub required: bool,
    pub default_value: Option<String>,
}

pub fn codebase_index_tools() -> Vec<CodebaseIndexToolInfo> {
    vec![
        CodebaseIndexToolInfo {
            name: "get_instructions".into(),
            description:
                "Call this first every session. Returns usage rules, the workspace model, the workspace list, and the operator's AGENT.md instructions.".into(),
            parameters: vec![],
        },
        CodebaseIndexToolInfo {
            name: "index_codebase".into(),
            description: "Index or re-index a workspace's code. Creates one index DB per workspace. Omit `workspace` to index the active workspace; an unregistered folder path is registered as a new workspace. Use mode 'both' to create structural AND semantic indexes simultaneously.".into(),
            parameters: vec![
                CodebaseIndexToolParam {
                    name: "workspace".into(),
                    param_type: "string".into(),
                    description: "Workspace: name, entity id, or folder path (case-insensitive). Default: the active workspace.".into(),
                    required: false,
                    default_value: None,
                },
                CodebaseIndexToolParam {
                    name: "mode".into(),
                    param_type: "string".into(),
                    description: "\"structural\" (BM25), \"semantic\" (vector), or \"both\"".into(),
                    required: false,
                    default_value: Some("structural".into()),
                },
                CodebaseIndexToolParam {
                    name: "embed_provider".into(),
                    param_type: "string".into(),
                    description: "Embedding provider for semantic mode".into(),
                    required: false,
                    default_value: Some("ollama".into()),
                },
                CodebaseIndexToolParam {
                    name: "embed_model".into(),
                    param_type: "string".into(),
                    description: "Embedding model name".into(),
                    required: false,
                    default_value: Some("nomic-embed-text".into()),
                },
                CodebaseIndexToolParam {
                    name: "embed_base_url".into(),
                    param_type: "string".into(),
                    description: "Base URL of the embedding service".into(),
                    required: false,
                    default_value: Some("http://localhost:11434".into()),
                },
            ],
        },
        CodebaseIndexToolInfo {
            name: "search_code".into(),
            description:
                "Search indexed codebase for relevant code. Use search_mode to choose between structural (BM25 keyword) and semantic (vector similarity) search."
                    .into(),
            parameters: vec![
                CodebaseIndexToolParam {
                    name: "query".into(),
                    param_type: "string".into(),
                    description: "Natural language or keyword search query".into(),
                    required: true,
                    default_value: None,
                },
                CodebaseIndexToolParam {
                    name: "workspace".into(),
                    param_type: "string".into(),
                    description: "Workspace: name, entity id, or folder path (case-insensitive). Default: the active workspace.".into(),
                    required: false,
                    default_value: None,
                },
                CodebaseIndexToolParam {
                    name: "limit".into(),
                    param_type: "integer".into(),
                    description: "Maximum number of results".into(),
                    required: false,
                    default_value: Some("20".into()),
                },
                CodebaseIndexToolParam {
                    name: "search_mode".into(),
                    param_type: "string".into(),
                    description: "Search strategy: \"structural\" (BM25 keyword), \"semantic\" (vector similarity), or \"auto\" (picks best available)".into(),
                    required: false,
                    default_value: Some("auto".into()),
                },
            ],
        },
        CodebaseIndexToolInfo {
            name: "lookup_symbol".into(),
            description: "Look up an exact symbol (function, class, variable) by name. Requires structural index."
                .into(),
            parameters: vec![
                CodebaseIndexToolParam {
                    name: "symbol_name".into(),
                    param_type: "string".into(),
                    description: "Exact name of the symbol to look up".into(),
                    required: true,
                    default_value: None,
                },
                CodebaseIndexToolParam {
                    name: "workspace".into(),
                    param_type: "string".into(),
                    description: "Workspace: name, entity id, or folder path (case-insensitive). Default: the active workspace.".into(),
                    required: false,
                    default_value: None,
                },
            ],
        },
        CodebaseIndexToolInfo {
            name: "list_codebases".into(),
            description: "List every registered workspace with its code-index status (mode, indexed or not, active marker)."
                .into(),
            parameters: vec![],
        },
        CodebaseIndexToolInfo {
            name: "reindex_codebase".into(),
            description: "Force a full re-index of a workspace's code. Clears existing index data and rebuilds for the specified mode(s)."
                .into(),
            parameters: vec![
                CodebaseIndexToolParam {
                    name: "workspace".into(),
                    param_type: "string".into(),
                    description: "Workspace: name, entity id, or folder path (case-insensitive). Default: the active workspace.".into(),
                    required: false,
                    default_value: None,
                },
                CodebaseIndexToolParam {
                    name: "mode".into(),
                    param_type: "string".into(),
                    description: "Which mode(s) to re-index: \"structural\", \"semantic\", or \"both\". Defaults to all currently indexed modes.".into(),
                    required: false,
                    default_value: None,
                },
            ],
        },
        CodebaseIndexToolInfo {
            name: "get_codebase_stats".into(),
            description:
                "Get statistics about a workspace's code index including file count, chunk count, index size, and mode."
                    .into(),
            parameters: vec![CodebaseIndexToolParam {
                name: "workspace".into(),
                param_type: "string".into(),
                description: "Workspace: name, entity id, or folder path (case-insensitive). Default: the active workspace.".into(),
                required: false,
                default_value: None,
            }],
        },
    ]
}

/// Returns definitions for all AgentOS tools (agent instructions).
/// These are the unified agentic interaction toolset. Persistent memory moved
/// to QuantMemory — the `quantsuite.memory.*` capability tools — and the old
/// flat-file MEMORY.md tools are gone with it; the CONCEPT.md tools followed
/// on 2026-08-31 (AgentOS is AGENT.md only).
pub fn agentos_tools() -> Vec<CodebaseIndexToolInfo> {
    agent_instruction_tools()
}

/// Agent instruction tools (AGENT.md)
fn agent_instruction_tools() -> Vec<CodebaseIndexToolInfo> {
    vec![
        CodebaseIndexToolInfo {
            name: "get_agent_instructions".into(),
            description: "Read the merged AGENT.md instructions across all scopes (global + project). Returns the combined instructions that guide agent behavior. Use 'scope' to read a specific scope only.".into(),
            parameters: vec![
                CodebaseIndexToolParam {
                    name: "scope".into(),
                    param_type: "string".into(),
                    description: "Which scope to read: \"global\", \"project\", or \"all\" (merged). Default: \"all\"".into(),
                    required: false,
                    default_value: Some("all".into()),
                },
                CodebaseIndexToolParam {
                    name: "workspace".into(),
                    param_type: "string".into(),
                    description: "Workspace: name, entity id, or folder path (case-insensitive). Default for project scope: the active workspace.".into(),
                    required: false,
                    default_value: None,
                },
                CodebaseIndexToolParam {
                    name: "project_path".into(),
                    param_type: "string".into(),
                    description: "Deprecated — use `workspace`. Absolute path to the project root; wins over `workspace` when both are given.".into(),
                    required: false,
                    default_value: None,
                },
            ],
        },
        CodebaseIndexToolInfo {
            name: "update_agent_instructions".into(),
            description: "Update the AGENT.md instructions for a specific scope (global or project). The content will be written to the appropriate AGENT.md file.".into(),
            parameters: vec![
                CodebaseIndexToolParam {
                    name: "scope".into(),
                    param_type: "string".into(),
                    description: "Which scope to update: \"global\" or \"project\".".into(),
                    required: true,
                    default_value: None,
                },
                CodebaseIndexToolParam {
                    name: "content".into(),
                    param_type: "string".into(),
                    description: "The new AGENT.md content to write.".into(),
                    required: true,
                    default_value: None,
                },
                CodebaseIndexToolParam {
                    name: "workspace".into(),
                    param_type: "string".into(),
                    description: "Workspace: name, entity id, or folder path (case-insensitive). Default for project scope: the active workspace.".into(),
                    required: false,
                    default_value: None,
                },
                CodebaseIndexToolParam {
                    name: "project_path".into(),
                    param_type: "string".into(),
                    description: "Deprecated — use `workspace`. Absolute path to the project root; wins over `workspace` when both are given.".into(),
                    required: false,
                    default_value: None,
                },
            ],
        },
        CodebaseIndexToolInfo {
            name: "init_agent_md".into(),
            description: "Initialize AGENT.md files for a scope with a default template. Creates the file if it doesn't exist.".into(),
            parameters: vec![
                CodebaseIndexToolParam {
                    name: "scope".into(),
                    param_type: "string".into(),
                    description: "Which scope to initialize: \"global\" or \"project\".".into(),
                    required: true,
                    default_value: None,
                },
                CodebaseIndexToolParam {
                    name: "workspace".into(),
                    param_type: "string".into(),
                    description: "Workspace: name, entity id, or folder path (case-insensitive). Default for project scope: the active workspace.".into(),
                    required: false,
                    default_value: None,
                },
                CodebaseIndexToolParam {
                    name: "project_path".into(),
                    param_type: "string".into(),
                    description: "Deprecated — use `workspace`. Absolute path to the project root; wins over `workspace` when both are given.".into(),
                    required: false,
                    default_value: None,
                },
            ],
        },
    ]
}

/// Returns definitions for kanban board tools.
/// These operate directly on the QuantMCP Tauri kanban (same board visible in the UI).
pub fn kanban_tools() -> Vec<CodebaseIndexToolInfo> {
    vec![
        CodebaseIndexToolInfo {
            name: "list_kanban_cards".into(),
            description: "List all kanban cards on a board. Columns: plan, work, review, done. Optionally filter by column. Every workspace has a board; \"general\" is the suite-wide General board. The operator periodically archives the Done column to keep the board short: archived cards are finished work that is off the board but not gone — pass archived=\"true\" to list them (what was done earlier, so it is not redone).".into(),
            parameters: vec![
                CodebaseIndexToolParam { name: "workspace".into(), param_type: "string".into(), description: "Board: workspace name, entity id, or folder path (case-insensitive), or \"general\" for the General board. Default: the active workspace.".into(), required: false, default_value: None },
                CodebaseIndexToolParam { name: "column".into(), param_type: "string".into(), description: "Filter by column: \"plan\", \"work\", \"review\", or \"done\". Omit to list all.".into(), required: false, default_value: None },
                CodebaseIndexToolParam { name: "archived".into(), param_type: "string".into(), description: "\"true\" lists the board's ARCHIVE instead — cards the operator cleared off the Done column, newest archive first. Default \"false\": the live board.".into(), required: false, default_value: Some("false".into()) },
            ],
        },
        CodebaseIndexToolInfo {
            name: "get_kanban_card".into(),
            description: "Get full details of a single kanban card by its ID, including status, priority, agent, branch, worktree, the user's test commands for that worktree (PowerShell / cmd / bash), and timestamps.".into(),
            parameters: vec![
                CodebaseIndexToolParam { name: "card_id".into(), param_type: "string".into(), description: "The UUID of the card.".into(), required: true, default_value: None },
            ],
        },
        CodebaseIndexToolInfo {
            name: "create_kanban_card".into(),
            description: "Create a new kanban card on a board. Defaults to 'plan' column with 'medium' priority. In approval mode present the Plan card and STOP until the user clicks 'Approve work' in its dialog. Auto Apply allows immediate claiming. Use workspace \"general\" for the suite-wide General board (its cards cannot be claimed — no folder).".into(),
            parameters: vec![
                CodebaseIndexToolParam { name: "workspace".into(), param_type: "string".into(), description: "Board: workspace name, entity id, or folder path (case-insensitive), or \"general\" for the General board. Default: the active workspace.".into(), required: false, default_value: None },
                CodebaseIndexToolParam { name: "title".into(), param_type: "string".into(), description: "Card title.".into(), required: true, default_value: None },
                CodebaseIndexToolParam { name: "description".into(), param_type: "string".into(), description: "Card description.".into(), required: false, default_value: Some("".into()) },
                CodebaseIndexToolParam { name: "column".into(), param_type: "string".into(), description: "Column: \"plan\", \"work\", \"review\", or \"done\". Default: \"plan\".".into(), required: false, default_value: Some("plan".into()) },
                CodebaseIndexToolParam { name: "priority".into(), param_type: "string".into(), description: "Priority: \"low\", \"medium\", \"high\", or \"critical\". Default: \"medium\".".into(), required: false, default_value: Some("medium".into()) },
                CodebaseIndexToolParam { name: "blocked_by".into(), param_type: "string".into(), description: "Comma-separated card IDs that must be merged before this card can be claimed.".into(), required: false, default_value: Some("".into()) },
            ],
        },
        CodebaseIndexToolInfo {
            name: "move_kanban_card".into(),
            description: "Move a kanban card to a different column.".into(),
            parameters: vec![
                CodebaseIndexToolParam { name: "card_id".into(), param_type: "string".into(), description: "The UUID of the card to move.".into(), required: true, default_value: None },
                CodebaseIndexToolParam { name: "column".into(), param_type: "string".into(), description: "Target column: \"plan\", \"work\", \"review\", or \"done\".".into(), required: true, default_value: None },
            ],
        },
        CodebaseIndexToolInfo {
            name: "move_kanban_card_to_workspace".into(),
            description: "Re-home a kanban card onto another board — a workspace or \"general\". Keeps its column, appends it there. Only unclaimed cards move (an agent's worktree lives in the source workspace).".into(),
            parameters: vec![
                CodebaseIndexToolParam { name: "card_id".into(), param_type: "string".into(), description: "The UUID of the card to move.".into(), required: true, default_value: None },
                CodebaseIndexToolParam { name: "workspace".into(), param_type: "string".into(), description: "Target board: workspace name, entity id, or folder path (case-insensitive), or \"general\". Default: the active workspace.".into(), required: false, default_value: None },
            ],
        },
        CodebaseIndexToolInfo {
            name: "update_kanban_card".into(),
            description: "Update a kanban card's title, description, priority, or blocked_by dependencies.".into(),
            parameters: vec![
                CodebaseIndexToolParam { name: "card_id".into(), param_type: "string".into(), description: "The UUID of the card.".into(), required: true, default_value: None },
                CodebaseIndexToolParam { name: "title".into(), param_type: "string".into(), description: "New card title.".into(), required: true, default_value: None },
                CodebaseIndexToolParam { name: "description".into(), param_type: "string".into(), description: "New card description.".into(), required: false, default_value: Some("".into()) },
                CodebaseIndexToolParam { name: "priority".into(), param_type: "string".into(), description: "New priority: \"low\", \"medium\", \"high\", or \"critical\".".into(), required: false, default_value: None },
                CodebaseIndexToolParam { name: "blocked_by".into(), param_type: "string".into(), description: "Comma-separated card IDs that block this card. Empty string to clear.".into(), required: false, default_value: None },
            ],
        },
        CodebaseIndexToolInfo {
            name: "delete_kanban_card".into(),
            description: "Delete a kanban card.".into(),
            parameters: vec![
                CodebaseIndexToolParam { name: "card_id".into(), param_type: "string".into(), description: "The UUID of the card to delete.".into(), required: true, default_value: None },
            ],
        },
        CodebaseIndexToolInfo {
            name: "claim_kanban_card".into(),
            description: "Claim a kanban card to start working on it. First check get_kanban_approval_mode: in approval mode the user must approve the Plan card with 'Approve work' in the card dialog. Missing approval refuses the claim before any worktree is created. Auto Apply permits immediate claiming. Creates an isolated git worktree and branch, with the main checkout's node_modules / .venv linked in (no install). Checks blocked_by dependencies and file conflicts. Moves the card to 'work' with status 'in_progress'. The reply carries the USER's test commands for the worktree (PowerShell / cmd / bash one-liners) — show them to the user verbatim; your own Rust builds use the worktree's own target/, and you never start tauri:dev.".into(),
            parameters: vec![
                CodebaseIndexToolParam { name: "card_id".into(), param_type: "string".into(), description: "The UUID of the card to claim.".into(), required: true, default_value: None },
                CodebaseIndexToolParam { name: "agent_id".into(), param_type: "string".into(), description: "Identifier for the agent claiming the card (e.g. \"opencode-1\", \"cursor-1\").".into(), required: true, default_value: None },
                CodebaseIndexToolParam { name: "intended_files".into(), param_type: "string".into(), description: "Comma-separated file paths the agent intends to modify. Used for file conflict detection with other active cards.".into(), required: false, default_value: Some("".into()) },
            ],
        },
        CodebaseIndexToolInfo {
            name: "complete_kanban_card".into(),
            description: "Mark a claimed card as complete. In 'auto_apply' mode (default): automatically merges the branch into main, cleans up the worktree, and moves the card to 'done'. In 'approval' mode: moves to 'review' column with status 'awaiting_review' — the worktree is preserved for review, and the user must click 'Approve result' in the card dialog to merge. Present the result and STOP until that separate approval.".into(),
            parameters: vec![
                CodebaseIndexToolParam { name: "card_id".into(), param_type: "string".into(), description: "The UUID of the card to complete.".into(), required: true, default_value: None },
                CodebaseIndexToolParam { name: "agent_id".into(), param_type: "string".into(), description: "The agent completing the card (for validation).".into(), required: false, default_value: None },
            ],
        },
        CodebaseIndexToolInfo {
            name: "approve_kanban_card".into(),
            description: "Auto Apply merge retry for a card awaiting review. In approval mode this tool refuses agent self-approval: present the result and wait for the user to click 'Approve result' in the card dialog. Merges the branch into main with --no-ff, removes the worktree, deletes the branch, and moves card to 'done' with status 'merged'.".into(),
            parameters: vec![
                CodebaseIndexToolParam { name: "card_id".into(), param_type: "string".into(), description: "The UUID of the card to approve.".into(), required: true, default_value: None },
            ],
        },
        CodebaseIndexToolInfo {
            name: "reject_kanban_card".into(),
            description: "Reject a card that is awaiting review. Sends it back to 'work' with status 'in_progress'. The worktree is preserved so the agent can continue working.".into(),
            parameters: vec![
                CodebaseIndexToolParam { name: "card_id".into(), param_type: "string".into(), description: "The UUID of the card to reject.".into(), required: true, default_value: None },
                CodebaseIndexToolParam { name: "reason".into(), param_type: "string".into(), description: "Reason for rejection.".into(), required: true, default_value: None },
            ],
        },
        CodebaseIndexToolInfo {
            name: "cancel_kanban_card".into(),
            description: "Cancel a kanban card. Removes the worktree and branch if they exist, and marks the card as cancelled.".into(),
            parameters: vec![
                CodebaseIndexToolParam { name: "card_id".into(), param_type: "string".into(), description: "The UUID of the card to cancel.".into(), required: true, default_value: None },
            ],
        },
        CodebaseIndexToolInfo {
            name: "get_kanban_diff".into(),
            description: "Get the git diff for a claimed card's branch vs main. Shows what changes the agent has made.".into(),
            parameters: vec![
                CodebaseIndexToolParam { name: "card_id".into(), param_type: "string".into(), description: "The UUID of the card.".into(), required: true, default_value: None },
            ],
        },
        CodebaseIndexToolInfo {
            name: "get_kanban_activity_log".into(),
            description: "Get the activity log for kanban cards. Shows claim, complete, approve, reject, cancel events.".into(),
            parameters: vec![
                CodebaseIndexToolParam { name: "card_id".into(), param_type: "string".into(), description: "Filter by card ID. Omit to see all activity.".into(), required: false, default_value: None },
                CodebaseIndexToolParam { name: "limit".into(), param_type: "integer".into(), description: "Max entries to return. Default: 50.".into(), required: false, default_value: Some("50".into()) },
            ],
        },
        CodebaseIndexToolInfo {
            name: "get_kanban_approval_mode".into(),
            description: "Get the current approval mode for a board. Returns 'auto_apply' (completed cards are auto-merged) or 'approval' (user approval of the Plan card before work, then separate result approval before merge). On the General board the mode only governs the review step — nothing merges there.".into(),
            parameters: vec![
                CodebaseIndexToolParam { name: "workspace".into(), param_type: "string".into(), description: "Board: workspace name, entity id, or folder path (case-insensitive), or \"general\". Default: the active workspace.".into(), required: false, default_value: None },
            ],
        },
        CodebaseIndexToolInfo {
            name: "set_kanban_approval_mode".into(),
            description: "Set the approval mode for a board. 'auto_apply' (default): completed cards are automatically merged into main. 'approval': the user approves the Plan card before work and separately approves the result before merging. Only the user can change this setting through the UI; MCP calls refuse it. On the General board the mode only governs the review step — nothing merges there.".into(),
            parameters: vec![
                CodebaseIndexToolParam { name: "workspace".into(), param_type: "string".into(), description: "Board: workspace name, entity id, or folder path (case-insensitive), or \"general\". Default: the active workspace.".into(), required: false, default_value: None },
                CodebaseIndexToolParam { name: "mode".into(), param_type: "string".into(), description: "Approval mode: 'auto_apply' or 'approval'.".into(), required: true, default_value: None },
            ],
        },
    ]
}

/// Agent worktrees (docs/PLAN-WORKTREES.md): isolated checkouts for parallel
/// work in one repository, usable by any agent from any terminal. The kanban
/// tools make one per claimed card; these make one on request.
pub fn worktree_tools() -> Vec<CodebaseIndexToolInfo> {
    let workspace = || CodebaseIndexToolParam {
        name: "workspace".into(),
        param_type: "string".into(),
        description: "The repository, as a workspace: name, entity id, or folder path (case-insensitive). Default: the active workspace. Ignored when `repo` is given.".into(),
        required: false,
        default_value: None,
    };
    let repo = || CodebaseIndexToolParam {
        name: "repo".into(),
        param_type: "string".into(),
        description: "Any folder inside the git repository (e.g. your current working directory). Takes precedence over `workspace`.".into(),
        required: false,
        default_value: None,
    };
    let worktree = || CodebaseIndexToolParam {
        name: "worktree".into(),
        param_type: "string".into(),
        description: "Which worktree: its branch (`agent/fix-login-1a2b3c` or `fix-login-1a2b3c`), its folder name, or its path — as returned by create_worktree or list_worktrees.".into(),
        required: true,
        default_value: None,
    };
    vec![
        CodebaseIndexToolInfo {
            name: "create_worktree".into(),
            description: "Create an isolated git worktree for parallel work: a new branch `agent/<name>-<id>` cut from the branch the repository is on (or `base`), checked out at `<repo>/.qs-worktrees/<name>-<id>` with the main checkout's node_modules / .venv linked in (no install). Call this BEFORE editing when other agents may be working in the same repository. Then do ALL your work inside the returned path (use absolute paths, or `cd` there in shell commands), commit as you go, and never merge, rebase or switch branches yourself. Build there with the worktree's own target/ (never point CARGO_TARGET_DIR at the main checkout yourself; the reply carries the USER's test command, which shares it on purpose — show that command to the user). When the work is done, report the branch and call merge_worktree — or leave the merge to the user if they asked to review first.".into(),
            parameters: vec![
                CodebaseIndexToolParam { name: "name".into(), param_type: "string".into(), description: "What the work is about, e.g. \"fix login bug\" — becomes the branch and folder name.".into(), required: true, default_value: None },
                workspace(),
                repo(),
                CodebaseIndexToolParam { name: "base".into(), param_type: "string".into(), description: "Branch or commit to cut from. Default: the branch the repository's main checkout is on.".into(), required: false, default_value: None },
            ],
        },
        CodebaseIndexToolInfo {
            name: "list_worktrees".into(),
            description: "List every worktree of a repository — the main checkout, agent worktrees from create_worktree, and the kanban's claimed-card worktrees — with branch, path, and the base each was cut from.".into(),
            parameters: vec![workspace(), repo()],
        },
        CodebaseIndexToolInfo {
            name: "get_worktree_status".into(),
            description: "What a worktree's branch changed against its base: commits ahead, uncommitted files, lines added/removed — plus the user's test command for it.".into(),
            parameters: vec![worktree(), workspace(), repo()],
        },
        CodebaseIndexToolInfo {
            name: "get_worktree_test_command".into(),
            description: "The commands the USER runs to test a worktree's version of the app — one line each for PowerShell, cmd and bash: cd into the worktree, CARGO_TARGET_DIR on the main checkout's target/ (incremental build; the tools reset what that leaves behind when the worktree is removed), npm run tauri:dev. Every claim_kanban_card / create_worktree reply carries them already; call this to get them again for a kanban card (`card_id`) or a worktree (`worktree`). Show them to the user verbatim (the one for their shell at least) — never run them yourself (port 1420 and the app window are the user's).".into(),
            parameters: vec![
                CodebaseIndexToolParam { name: "card_id".into(), param_type: "string".into(), description: "A claimed kanban card — its stored worktree is used. Takes precedence over `worktree`.".into(), required: false, default_value: None },
                CodebaseIndexToolParam { name: "worktree".into(), param_type: "string".into(), description: "Which worktree: its branch, folder name, or path — as returned by create_worktree or list_worktrees. Required unless `card_id` is given.".into(), required: false, default_value: None },
                workspace(),
                repo(),
            ],
        },
        CodebaseIndexToolInfo {
            name: "get_worktree_diff".into(),
            description: "The full diff of a worktree's branch against its base — committed, staged, unstaged, and untracked files as new-file diffs. Use it to review another agent's work before merging.".into(),
            parameters: vec![
                worktree(),
                workspace(),
                repo(),
                CodebaseIndexToolParam { name: "max_chars".into(), param_type: "integer".into(), description: "Cut the diff after this many characters. Default 60000, at most 524288.".into(), required: false, default_value: Some("60000".into()) },
            ],
        },
        CodebaseIndexToolInfo {
            name: "merge_worktree".into(),
            description: "Merge a worktree's branch into the branch it was cut from, as one `--no-ff` merge commit in the repository's main checkout. Uncommitted work in the worktree is committed first. The main checkout must be on the base branch — this tool never switches anyone's branch. A conflict aborts the merge and leaves the worktree untouched; say so and let the user resolve it. With cleanup (default) the worktree and branch are removed afterwards.".into(),
            parameters: vec![
                worktree(),
                workspace(),
                repo(),
                CodebaseIndexToolParam { name: "cleanup".into(), param_type: "string".into(), description: "\"true\" (default) removes the worktree and branch after a successful merge; \"false\" keeps them.".into(), required: false, default_value: Some("true".into()) },
                CodebaseIndexToolParam { name: "message".into(), param_type: "string".into(), description: "Commit message for uncommitted work found in the worktree. Default: \"Work in progress on <branch>\".".into(), required: false, default_value: None },
            ],
        },
        CodebaseIndexToolInfo {
            name: "remove_worktree".into(),
            description: "Discard a worktree: delete its checkout and its branch, commits included. Not reversible — only when the work is merged already or is not wanted.".into(),
            parameters: vec![worktree(), workspace(), repo()],
        },
    ]
}
