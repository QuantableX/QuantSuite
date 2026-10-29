/**
 * QuantPilot's types — the Rust side's shapes (sessions, adapters, status,
 * settings) and the one signal vocabulary every adapter speaks
 * (crate/src/signals.rs).
 */

/** Supervised, Accept edits, Auto, Plan, Full access (docs/PLAN-QUANTPILOT.md §6). */
export type Mode = 'ask' | 'edits' | 'auto' | 'plan' | 'full'
export type LiveState = 'offline' | 'idle' | 'thinking' | 'working' | 'waiting' | 'error'
/** How the pilot learns what an agent does: Claude's hooks, a tailed session file, or output activity alone. */
export type SignalKind = 'hooks' | 'transcript' | 'activity'

export interface SessionMeta {
  id: string
  /** Adapter id (`claude`, `codex`, `pi`, `omp`, `opencode`, `gemini`, custom) once one was started in the terminal; empty before. */
  provider: string
  providerSessionId: string | null
  title: string
  contextId: string
  contextName: string
  cwd: string
  model: string
  mode: Mode
  createdAt: number
  updatedAt: number
  costUsd: number
  inputTokens: number
  outputTokens: number
  lastError: string | null
  /** `chat` = a V3 thread; `terminal` = a V2 session row (continues as a chat on its first message). */
  kind: 'chat' | 'terminal'
  effort: string
  titleSource: 'auto' | 'user'
  /** The last known state, saved: `idle` | `running` | `waiting` | `error` | `interrupted` | `limited`. */
  state: string
  contextTokens: number | null
  contextWindow: number | null
  lastActivityAt: number
  /** Kept above the active threads. */
  pinned: boolean
  /** The user's order in Pinned and Active: larger is higher. */
  sortKey: number
  /** On the Settled shelf since then. */
  settledAt: number | null
  /** On the Snoozed shelf until then, unless it needs you first. */
  snoozedUntil: number | null
  /** `false` keeps it out of automatic settlement. */
  autoSettle: boolean
  /** `local` (the project's checkout) or `worktree` (its own). */
  workspaceMode: 'local' | 'worktree'
  baseBranch: string
  /** The worktree's branch (planned until the first message). */
  branch: string
  worktreePath: string | null
}

export interface SessionDetail {
  meta: SessionMeta
  ptyId: string | null
  alive: boolean
  display: string | null
}

export interface LaunchResult {
  ptyId: string
  resumed: boolean
  display: string
  attached: boolean
}

export interface RuntimeInfo {
  ptyId: string | null
  alive: boolean
}

export interface PilotContext {
  id: string
  name: string
  path: string
  kind: 'general' | 'workspace'
}

export interface AdapterStatus {
  id: string
  label: string
  custom: boolean
  signals: SignalKind | null
  found: boolean
  path: string | null
  version: string | null
  loggedIn: boolean | null
  detail: string | null
}

export interface PilotStatus {
  adapters: AdapterStatus[]
  /** The shell a new terminal opens with, resolved. */
  shell: string
  mcpUrl: string
  generalPath: string
  dataDir: string
  relayPort: number
}

export interface CustomAdapter {
  id: string
  label: string
  exe: string
  args: string
  resumeArgs: string
}

export interface PilotSettings {
  version: number
  /** Shell a session's terminal runs; empty = pwsh → Windows PowerShell → cmd. */
  shell: string
  defaultMode: Mode
  /** Adapter id → executable path override. */
  exe: Record<string, string>
  /** Adapter id → model to launch with. */
  model: Record<string, string>
  effort: string
  attachQuantmcp: boolean
  includeGeneralVault: boolean
  custom: CustomAdapter[]
  /** `enter` | `modEnterMultiline` | `modEnter` */
  sendShortcut: SendShortcut
  /** What a message sent while the agent works does. */
  followUp: FollowUp
  /** A Windows toast when a thread needs you. */
  notify: 'off' | 'unfocused' | 'always'
  /** A short sound when a thread starts waiting on you. */
  notifySound: boolean
  /** Settle a thread nobody touched for this many days (0 = never). */
  autoSettleDays: number
  /** Fold running threads into the sidebar's Working section. */
  workingSection: boolean
  /** Where new threads work. */
  newThreadWorkspace: 'local' | 'worktree'
  /** The start of a worktree thread's branch name. */
  worktreePrefix: string
  /** Generated commit messages follow the repository's recent subjects, or a standard form. */
  commitStyle: 'repository' | 'standard'
  /** Project actions by project (context id), in order (card 13). */
  actions: Record<string, ProjectAction[]>
}

// ── Files tab, terminal drawer, project actions (card 13) ──

export type ActionIcon = 'play' | 'test' | 'lint' | 'build' | 'configure' | 'debug'

/** A command run in a Run terminal of the thread's checkout. */
export interface ProjectAction {
  id: string
  name: string
  command: string
  icon: ActionIcon
  /** e.g. `Ctrl+Shift+R`; empty = none. */
  key: string
  runOnWorktreeCreate: boolean
  runOnSettle: boolean
}

export interface DirEntry {
  name: string
  /** Relative to the checkout, forward slashes. */
  rel: string
  dir: boolean
  size: number
}

export interface FileContent {
  path: string
  rel: string | null
  kind: 'text' | 'image' | 'pdf' | 'binary'
  text: string | null
  base64: string | null
  mime: string
  size: number
  truncated: boolean
  readOnly: boolean
}

export interface LineMatch {
  line: number
  column: number
  text: string
}

export interface FileMatches {
  rel: string
  matches: LineMatch[]
}

export interface TextSearchResult {
  files: FileMatches[]
  total: number
  truncated: boolean
}

/** A shell or Run terminal in the drawer. */
export interface DrawerTerm {
  ptyId: string
  threadId: string
  label: string
  action: string | null
  cwd: string
  createdAt: number
}

export interface PortOwner {
  pid: number
  name: string
}

// ── Branch toolbar (card 10) ──

export interface RepoInfo {
  root: string
  branch: string
  defaultBranch: string
  dirty: number
}

/** What a thread's checkout can commit, push, pull or merge (card 12). */
export interface GitStatus {
  /** Empty on a detached HEAD. */
  branch: string
  defaultBranch: string
  /** What Merge and a pull request target. */
  base: string
  upstream: string | null
  ahead: number
  behind: number
  aheadOfBase: number
  files: FileChange[]
  remote: string | null
  /** owner/repo when the remote is on GitHub. */
  github: string | null
  /** Why pull requests are unavailable; null = available. */
  prBlocked: string | null
  worktree: boolean
  /** Commits the next push or merge carries that credit an AI. */
  aiCredits: string[]
}

export interface GitGenerated {
  title: string
  body: string
}

export interface GitDone {
  message: string
  /** Half done: committed, but the push failed. */
  error: string | null
}

/** A pull request (or later a board card) linked to a thread. */
export interface ThreadLink {
  threadId: string
  kind: 'pr'
  url: string
  number: number
  title: string
  state: 'open' | 'draft' | 'closed' | 'merged'
  checkedAt: number
  createdAt: number
}

export interface GitInfo {
  repo: RepoInfo | null
  mode: 'local' | 'worktree'
  baseBranch: string
  branch: string
  worktreePath: string | null
  worktreeMissing: boolean
  started: boolean
}

export interface BranchInfo {
  name: string
  current: boolean
  updatedAt: number
  elsewhere: boolean
}

export interface CleanupCheck {
  exists: boolean
  dirty: string[]
  merged: boolean
  ahead: number
  branch: string
  base: string
  suggestRemove: boolean
  suggestDeleteBranch: boolean
}

export type SendShortcut = 'enter' | 'modEnterMultiline' | 'modEnter'
export type FollowUp = 'queue' | 'steer'

// ── Signals (crate/src/signals.rs) ──

export type PilotSignal =
  | { type: 'launched'; adapter: string }
  | { type: 'bound'; providerSessionId: string; model: string | null }
  | { type: 'state'; state: LiveState; detail: string | null }
  | { type: 'tool'; name: string; title: string; done: boolean; isError: boolean }
  | { type: 'file'; path: string }
  | { type: 'turn'; status: 'started' | 'completed' }
  | {
      type: 'vitals'
      model: string | null
      costUsd: number | null
      contextPct: number | null
      linesAdded: number | null
      linesRemoved: number | null
      inputTokens: number | null
      outputTokens: number | null
    }
  | { type: 'title'; title: string }
  | { type: 'notice'; text: string }
  | { type: 'exited' }

export interface PilotEnvelope {
  sessionId: string
  event: PilotSignal
}

export interface Vitals {
  model: string | null
  costUsd: number | null
  contextPct: number | null
  linesAdded: number | null
  linesRemoved: number | null
  inputTokens: number | null
  outputTokens: number | null
}

/** What the UI holds per session beyond the row. */
export interface SessionLive {
  state: LiveState
  detail: string | null
  /** The console PTY the pane attaches to; `null` until launched. */
  ptyId: string | null
  alive: boolean
  launching: boolean
  /** The command line the CLI was started with. */
  display: string | null
  /** A launch that failed, shown over the stage until the next try. */
  error: string | null
  /** The running tool, one line. */
  tool: string | null
  /** Bumps when a turn completes — the face goes ^^. */
  cheer: number
  turnRunning: boolean
  /** Files the agent wrote, most recent first. */
  files: string[]
  vitals: Vitals
  notice: string | null
  signals: SignalKind
  lastOutputAt: number
  /** The terminal bell rang and nothing has happened since. */
  bell: boolean
}

// ── V3 threads (crate/src/events.rs) ──

export type ItemKind = 'user' | 'assistant' | 'reasoning' | 'tool' | 'approval' | 'question' | 'plan' | 'todo' | 'notice' | 'error' | 'changes'

export interface ThreadItem {
  id: string
  threadId: string
  turnId: string
  seq: number
  kind: ItemKind
  /** The tool call that started the subagent this item belongs to. */
  parent: string | null
  data: Record<string, any>
  createdAt: number
  updatedAt: number
}

export type ThreadState = 'idle' | 'starting' | 'running' | 'waiting' | 'error' | 'interrupted' | 'limited'

export interface RateWindow {
  kind: string
  usedPct: number | null
  resetsAt: number | null
  status: string | null
}

export type ThreadEvent =
  | { type: 'item'; item: ThreadItem }
  | { type: 'delta'; itemId: string; text: string }
  | { type: 'turn'; turnId: string; status: 'started' | 'completed' | 'failed' | 'interrupted' | 'limited'; durationMs: number | null; costUsd: number | null; error: string | null }
  | { type: 'state'; state: ThreadState; detail: string | null }
  | { type: 'session'; provider: string; providerSessionId: string; model: string | null }
  | { type: 'usage'; contextTokens: number | null; contextWindow: number | null; costUsd: number | null; inputTokens: number | null; outputTokens: number | null }
  | { type: 'rateLimit'; windows: RateWindow[] }
  | { type: 'title'; title: string }
  | { type: 'queue'; items: QueueItem[]; held: boolean }

/** A follow-up waiting for the running turn (card 4). */
export interface QueueItem {
  id: string
  threadId: string
  position: number
  text: string
  createdAt: number
  updatedAt: number
}

/** One file a turn or a scope changed (crate/src/checkpoints.rs). */
export interface FileChange {
  path: string
  /** A added, M modified, D deleted, R renamed. */
  status: 'A' | 'M' | 'D' | 'R'
  added: number
  removed: number
  binary: boolean
}

/** The Diff tab's file list (pilot_diff_list). */
export interface DiffList {
  files: FileChange[]
  staged: string[]
  from: string
  to: string
  /** Turns with changes, newest first: [turn id, label]. */
  turns: [string, string][]
}

export interface DiffSides {
  original: string
  modified: string
}

export interface QueueState {
  items: QueueItem[]
  held: boolean
}

export interface ThreadEnvelope {
  threadId: string
  seq: number
  event: ThreadEvent
}

export interface ThreadSnapshot {
  thread: SessionMeta
  items: ThreadItem[]
  seq: number
  hasMore: boolean
  turnId: string | null
  processAlive: boolean
  queue: QueueState
  canSteer: boolean
}

export type Answer =
  | { decision: 'allow' }
  | { decision: 'allowSession' }
  | { decision: 'deny'; message?: string | null }
  | { decision: 'answers'; answers: Record<string, string | string[]> }
  | { decision: 'dismiss' }

export interface ModelInfo {
  id: string
  displayName: string
  description: string
  resolvedModel: string | null
  efforts: string[]
}

export interface CommandInfo {
  name: string
  description: string
  argumentHint: string
}

export interface ProviderCatalog {
  provider: string
  version: string | null
  models: ModelInfo[]
  commands: CommandInfo[]
  skills?: SkillInfo[]
}

// ── Composer context (card 9) ──

export interface SkillInfo {
  name: string
  description: string
  /** `user`, `project`, a plugin's name, or Codex's scope. */
  source: string
  path: string
}

export interface FileHit {
  /** Relative to the checkout; folders end with `/`. */
  path: string
  dir: boolean
  /** What `@` inserts: `@path`, quoted with spaces. */
  insert: string
}

/** A chip in the composer's text: what its label stands for. */
export interface ContextChip {
  /** `file` | `thread` | `skill` | `terminal` | `comment` | `element` | `image` */
  kind: string
  label: string
  /** What the agent gets (a skill: its name). */
  content: string
  path?: string | null
}

/** An attachment saved next to the thread, waiting in the composer. */
export interface ComposerAttachment {
  /** Local id while it uploads. */
  key: string
  kind: 'image' | 'file' | 'folder'
  name: string
  path: string
  size: number
  thumb?: string | null
  /** Still being saved, or why it failed. */
  pending?: boolean
  error?: string | null
}

/** What a message carries besides its text. */
export interface MessageExtras {
  chips?: ContextChip[]
  attachments?: { kind: string; name: string; path: string; thumb?: string | null }[]
}
