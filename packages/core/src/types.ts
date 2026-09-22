/** Shared types for the suite. Mirrors the Rust structs in `crates/qs-core`. */

export interface QsEvent<T = unknown> {
  id: string
  /** `<domain>.<entity>.<verb>` */
  topic: string
  /** Module id, or `core`. */
  source: string
  /** Unix millis. */
  ts: number
  payload: T
  correlation_id?: string | null
}

export interface Entity {
  /** `<module>:<kind>:<ulid>` */
  id: string
  module: string
  kind: string
  title: string
  subtitle?: string | null
  /** Route that opens this thing. */
  route: string
  icon?: string | null
  updated_at: number
  payload?: unknown
}

export interface Link {
  src: string
  dst: string
  /** `references` | `derived_from` | `annotates` | ... */
  rel: string
}

/**
 * What a module last reported about one of its processes. Mirrors the Rust
 * `ProcessState` (internally tagged on `state`).
 */
export type ProcessState =
  | { state: 'stopped' }
  /** `pid` is null for things with no OS process id of ours — a Docker container. */
  | { state: 'running'; pid: number | null; since: number }
  | { state: 'failed'; message: string }

/**
 * One process a module has announced to `qs-core`'s register (ARCHITECTURE.md §6).
 *
 * The register owns nothing: the module keeps its own process — only it knows
 * the protocol on those pipes — and names the commands the shell may invoke.
 * Those follow one signature so the process page can call them blind:
 * start/stop take no arguments, logs takes `{ limit }` and returns `string[]`.
 * A `null` command means the module cannot offer that action; the page hides
 * the button rather than inventing one.
 */
export interface ProcessInfo {
  /** Module-prefixed and stable: `systems.engine`, `algo.bot`. */
  id: string
  module: string
  label: string
  status: ProcessState
  /** `plugin:<module>|<command>`, or null. */
  startCommand: string | null
  stopCommand: string | null
  logsCommand: string | null
  /** Set only by modules that learn of an exit by asking. The process page
   *  invokes it before each poll so a process that died unobserved still
   *  turns up as stopped. */
  refreshCommand: string | null
}

/** One agent-callable capability from the MCP bridge catalogue (§7). */
export interface AgentCapability {
  /** `quantsuite.<module>.<name>` */
  tool: string
  module: string
  name: string
  /** `plugin:<module>|<command>` */
  command: string
  description: string
  sideEffects: 'read' | 'compute' | 'write' | 'external'
}

/** What the approval gate would do with a call. */
export type AgentDecision =
  | { decision: 'allow' }
  | { decision: 'prompt'; reason: string }
  | { decision: 'deny'; reason: string }

/**
 * An agent call waiting on the shell (E4): `allow` mid-dispatch, `prompt`
 * sitting in the approval queue. Denied calls never become pending.
 */
export interface PendingAgentCall {
  callId: string
  tool: string
  /** `plugin:<module>|<command>` — what the shell invokes on approval. */
  command: string
  args: Record<string, unknown>
  decision: 'allow' | 'prompt'
  reason: string | null
  requestedAt: number
}

export type ModuleStatus = 'planned' | 'in_progress' | 'migrated' | 'stub'

export interface ModuleInfo {
  id: string
  title: string
  description: string
  status: ModuleStatus
  /** Migration phase from docs/MIGRATION.md. */
  phase: number | null
  /** App this module belongs to (modules/apps.json); null only for own-window modules. */
  app: string | null
  /** Position inside its app's module selector. */
  appOrder: number | null
  route: string
  order: number
  plugin: string | null
  /**
   * Set only for a module that cannot render inside the suite window —
   * QuantHUD's always-on-top overlay. Selecting it invokes this command
   * instead of navigating.
   */
  ownWindow: { command: string } | null
}

/** A suite app — one vertical entry on the rail (V3). */
export interface AppInfo {
  id: string
  title: string
  order: number
  /** Initial availability until the user saves an explicit app switch. */
  defaultEnabled: boolean
  /** Only the dashboard routes directly; module apps route via their modules. */
  route: string | null
  /** Key into the shared logo map (packages/ui `logoFor`). */
  logo: string
  /** Member module ids, sorted by appOrder. */
  modules: string[]
}

/**
 * One QuantConsole terminal session (docs/PLAN-CONSOLE.md). The PTY lives in
 * Rust; the frontend only ever holds this id.
 */
export interface ConsoleSession {
  id: string
  shell: string
  cwd: string
  /** Unix millis. */
  startedAt: number
  /** The child has exited; the output stays readable until the user closes it. */
  exited: boolean
}

/**
 * What re-attaching to a session returns: everything the child printed while
 * nobody listened, verbatim — xterm replays it and the screen is whole again.
 */
export interface ConsoleAttachResult {
  raw: string
}

/** A shell this machine actually has, as reported by the console plugin. */
export interface ConsoleShell {
  id: string
  label: string
  path: string
  isDefault: boolean
}

export interface SuitePaths {
  root: string
  coreDb: string
  logs: string
  secrets: string
}

// ── files and git (see `qs.files` in commands.ts) ────────────────────────────

/**
 * One node of a directory tree. `children` is `null` for a file, and for a
 * directory it is the whole subtree — the walk is done in one call rather than
 * one per expansion, because a hundred round trips to draw a tree is slower than
 * one walk that already skipped everything `.gitignore` names.
 */
export interface FileEntry {
  name: string
  path: string
  isDirectory: boolean
  children: FileEntry[] | null
}

/** A changed file. `status` is one of new/modified/deleted/renamed/typechange/conflicted/unknown. */
export interface GitFileEntry {
  path: string
  status: string
  /** In the index. A file with both staged and unstaged changes appears twice. */
  staged: boolean
}

export interface GitStatusResult {
  branch: string
  files: GitFileEntry[]
  ahead: number
  behind: number
}

/** One commit from `git_log` — newest first, reachable from HEAD. */
export interface GitCommitEntry {
  /** Full OID — what `gitCommitFiles` wants back. */
  oid: string
  short: string
  summary: string
  author: string
  /** Unix seconds. */
  time: number
}

/** A file one commit touched. `status` is added/modified/deleted/renamed/typechange/unknown. */
export interface GitCommitFile {
  path: string
  status: string
}

/** One local save snapshot of a file (QuantCode's Timeline). */
export interface TimelineEntry {
  /** Opaque id — hand it back to `timelineRead`. */
  id: string
  /** Unix milliseconds of the save. */
  savedAt: number
  bytes: number
}
