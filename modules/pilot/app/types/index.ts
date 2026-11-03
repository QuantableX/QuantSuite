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
  /** Claude Code's fast mode (round 2, card 02). */
  fast?: boolean
  /** Only in the answer that created or set up a draft: the agent wanted was not available (round 2, card 03). */
  notice?: string
  /** The user's tags (round 2, card 12). */
  tags?: string[]
  /** A side chat's main thread (round 2, card 10). */
  parentId?: string | null
  /** The agent account (round 3, card 02); "" = the default account. */
  instance?: string
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
  /** How Update would run: npm, winget, native or cli (card 19). */
  update?: string | null
  /** It can run a chat thread (card 20). */
  chat?: boolean
}

/** Another account of an agent (round 3, card 02). */
export interface Instance {
  id: string
  provider: string
  label: string
  home: string
}

export interface InstanceStatus extends Instance {
  loggedIn: boolean | null
  detail: string | null
  /** The terminal line that signs this account in (run by the user). */
  signIn: string
}

export interface PilotStatus {
  adapters: AdapterStatus[]
  instances?: InstanceStatus[]
  /** The shell a new terminal opens with, resolved. */
  shell: string
  mcpUrl: string
  generalPath: string
  dataDir: string
  relayPort: number
}

/** An agent's model and effort for a new thread (round 2, card 03). */
export interface AgentNewSetup {
  model: string
  effort: string
  /** The project pins this model. */
  modelPinned: boolean
}

/** What a new thread in a project starts with — the one resolver's answer (round 2, card 03). */
export interface NewSetup {
  provider: string
  model: string
  effort: string
  mode: Mode
  workspace: 'local' | 'worktree'
  baseBranch: string
  /** `provider`, `model`, `mode`, `workspace`: the project's own values. */
  pinned: string[]
  notice: string | null
  agents: Record<string, AgentNewSetup>
}

export interface CustomAdapter {
  id: string
  label: string
  exe: string
  args: string
  resumeArgs: string
  /** `acp`: chat threads over the Agent Client Protocol (card 20); `terminal`: V2 terminal sessions. */
  protocol?: 'acp' | 'terminal'
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
  instances?: Instance[]
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
  /** A usage-limit stop schedules the continuation for the reset (card 15). */
  limitResume?: boolean
  /** A usage-limit stop snoozes the thread until the reset (card 15). */
  limitSnooze?: boolean
  /** The start of a worktree thread's branch name. */
  worktreePrefix: string
  /** Generated commit messages follow the repository's recent subjects, or a standard form. */
  commitStyle: 'repository' | 'standard'
  /** Project actions by project (context id), in order (card 13). */
  actions: Record<string, ProjectAction[]>
  /** The agent of a new thread (card 19). */
  defaultProvider: string
  /** Per agent: extra arguments, environment, own model ids, off (card 19). */
  providers: Record<string, ProviderPrefs>
  /** Variables for every agent process. */
  env: Record<string, string>
  /** New Claude processes ignore inherited API keys, tokens and endpoints. */
  preferSavedClaudeLogin: boolean
  textSize: 'small' | 'default' | 'large' | 'larger'
  /** Panels and menus, 0–400 ms; reduced motion turns it off. */
  panelMotionMs: number
  renderMath: boolean
  /** Settings → Storage (parity 19): cleanup rules, all off by default. */
  storage?: StoragePolicy
  /** What a project changes for its new threads (card 19). */
  projects: Record<string, ProjectPrefs>
  /** GitHub through `gh` (card 23); off = no `gh` process at all. */
  github?: boolean
  /** While recording, threads with one of these tags are hidden (round 2, card 12). */
  hideTagsWhileRecording?: string[]
  /** Crews (round 2, card 07): workers at once, a worker's time limit, own worktrees. */
  crewParallel?: number
  crewTimeoutMinutes?: number
  crewWorktrees?: boolean
}

// ── GitHub (card 23) ──

export interface GhRepo {
  owner: string
  name: string
}

/** A workspace whose `origin` is on GitHub. */
export interface GhRepoRow {
  contextId: string
  name: string
  path: string
  repo: GhRepo
  url: string
}

/** A row of the composer's `#` menu. */
export interface GhRef {
  number: number
  kind: 'pr' | 'issue'
  state: 'open' | 'draft' | 'closed' | 'merged'
  title: string
  updatedAt: string
}

export interface GhChecks {
  total: number
  passed: number
  failed: number
  pending: number
  failing: string[]
}

export interface GhPull {
  number: number
  title: string
  url: string
  state: 'open' | 'draft'
  author: string
  branch: string
  review: '' | 'APPROVED' | 'CHANGES_REQUESTED' | 'REVIEW_REQUIRED'
  checks: GhChecks
  additions: number
  deletions: number
  labels: string[]
  updatedAt: string
  /** Checks pass, approved, merges cleanly. */
  ready: boolean
}

export interface GhIssue {
  number: number
  title: string
  url: string
  author: string
  labels: string[]
  comments: number
  updatedAt: string
}

export interface ProviderPrefs {
  args: string
  env: Record<string, string>
  models: string[]
  disabled: boolean
}

/** `null` / absent inherits the default. */
export interface ProjectPrefs {
  provider?: string | null
  model?: Record<string, string>
  mode?: Mode | null
  workspace?: 'local' | 'worktree' | null
  enabled?: string[] | null
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
  /** After a turn that changed files; a failure queues one repair (round 2, card 12). */
  checkAfterTurn?: boolean
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

// ── Browser tab (card 14) ──

export interface BrowserStatus {
  url: string
  title: string
  loading: boolean
  canGoBack: boolean
  canGoForward: boolean
  picking: boolean
  zoom: number
  /** The browser profile of the page (parity 18); null is temporary. */
  profile?: string | null
}

/** Cleanup rules (parity 19); null keeps a category. */
export interface StoragePolicy {
  patchDays: number | null
  unsentDays: number | null
  documentDays: number | null
  orphans: boolean
  auto: boolean
  lastRun: number | null
}
export interface StorageCategory {
  key: string
  label: string
  bytes: number
  items: number
  note: string
}
export interface StoragePlan {
  items: { category: string; target: string; bytes: number; reason: string; thread: string | null }[]
  bytes: number
  kept: { category: string; reason: string; count: number; bytes: number }[]
}
export interface StorageCleanResult {
  removed: number
  bytes: number
  failed: { target: string; error: string }[]
  skipped: number
}

/** A preview-browser profile (parity 18). */
export interface BrowserProfile {
  id: string
  name: string
  createdAt: number
}

export interface BrowserProfiles {
  profiles: BrowserProfile[]
  /** Project → profile id. */
  defaults: Record<string, string>
  /** The profile of the open page. */
  active: string | null
}

/** Another browser on this machine, and why its session cannot be imported. */
export interface ImportSource {
  browser: string
  status: 'unsupported' | 'not_found'
  reason: string
}

/** An element picked in the preview — untrusted page content. */
export interface ElementPick {
  url: string
  title: string
  selector: string
  tag: string
  text: string
  html: string
  bounds: { x: number; y: number; width: number; height: number }
}

/** A local TCP listener for the browser's start page. */
export interface Listener {
  port: number
  pid: number
  process: string
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
  /** `card`: the kanban card the thread was started from (card 21), url `card:<id>`; `issue` (card 23). */
  kind: 'pr' | 'issue' | 'card'
  url: string
  number: number
  title: string
  state: 'open' | 'draft' | 'closed' | 'merged' | 'card'
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

export type ItemKind = 'user' | 'assistant' | 'reasoning' | 'tool' | 'approval' | 'question' | 'plan' | 'todo' | 'notice' | 'error' | 'changes' | 'page'

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

// ── Usage (card 18, crate/src/usage) ──

export type UsagePeriod = '24h' | '7d' | '30d' | '90d'

/** An estimate in USD. */
export interface UsageCost {
  input: number
  cacheRead: number
  cacheWrite: number
  output: number
  /** What cache reads saved against uncached input. */
  savings: number
}

export interface UsageRow {
  /** Start of the hour (24h) or local day, Unix ms. */
  bucket: number
  provider: string
  model: string
  input: number
  cacheRead: number
  cacheWrite: number
  output: number
  /** `null`: no public price for the model. */
  cost: UsageCost | null
}

export interface UsageReport {
  period: UsagePeriod
  since: number
  bucket: 'hour' | 'day'
  rows: UsageRow[]
  prices: string
  sources: string[]
  files: number
  unreadable: number
  scanMs: number
}

export interface ProviderLimits {
  provider: string
  windows: RateWindow[]
  /** When reported, Unix ms; null = never. */
  at: number | null
  error: string | null
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
  | { type: 'goal'; goal: ThreadGoal | null }

/**
 * The thread's goal (card 16): Codex's own (`thread/goal/*`) or Claude's
 * `/goal`, whose Stop hook checks it after every stop.
 */
export interface ThreadGoal {
  objective: string
  status: 'active' | 'paused' | 'blocked' | 'usageLimited' | 'budgetLimited' | 'complete'
  /** Claude: how often the Stop hook found it not met yet, and why the last time. */
  checks?: number
  lastCheck?: string
  /** Codex: what working toward it took so far. */
  tokensUsed?: number
  timeUsedSeconds?: number
  tokenBudget?: number | null
}

/** A thread's life beyond one process (card 15). */
export interface ThreadLife {
  /** A scheduled continuation (Resume at reset), Unix ms. */
  resumeAt: number | null
  /** When the usage limit that stopped the thread lifts, Unix ms. */
  limitResetAt: number | null
  /** The agent the next message carries the conversation from. */
  handoffFrom: string | null
  /** The conversation is open in the CLI's terminal UI. */
  tui: boolean
  /** The goal the agent works toward (card 16). */
  goal?: ThreadGoal | null
}

/** A follow-up waiting for the running turn (card 4). */
export interface QueueItem {
  id: string
  threadId: string
  position: number
  text: string
  payload: { version: number; text: string; chips: ContextChip[]; attachments: NonNullable<MessageExtras['attachments']> }
  state: 'queued' | 'sending' | 'running' | 'awaiting' | 'uncertain'
  error: string | null
  revision: number
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
  /** Parity 01: `tracked` (the agent's track_files), `reported` (a provider edit event), `snapshot` (saved before). */
  source?: 'tracked' | 'reported' | 'snapshot'
  /** Why the record is partial or uncertain. */
  note?: string
  /** A rename's old path. */
  from?: string
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
  life?: ThreadLife
}

export type Answer =
  | { decision: 'allow' }
  | { decision: 'allowSession' }
  | { decision: 'deny'; message?: string | null }
  /** `files` (parity 17): question → saved attachments, sent as references the agent can open. */
  | { decision: 'answers'; answers: Record<string, string | string[]>; files?: Record<string, AnswerFile[]> }
  | { decision: 'dismiss' }
  /** An MCP form or web-page request (parity card 05). */
  | { decision: 'elicitation'; action: 'accept' | 'decline' | 'cancel'; content?: Record<string, unknown> | null }

export interface ModelInfo {
  id: string
  displayName: string
  description: string
  resolvedModel: string | null
  efforts: string[]
  /** The CLI offers its fast mode for this model (round 2, card 02). */
  fast?: boolean
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
  /** A quote's own comment (parity 14). */
  comment?: string | null
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
  /** A PDF or Office document's text, being read or read (parity 16). */
  docPending?: boolean
  doc?: DocInfo | null
}

/** A saved attachment sent with a question's answer (parity 17). */
export interface AnswerFile {
  kind: 'image' | 'file'
  name: string
  path: string
  thumb?: string | null
}

/** What reading a document attachment gave (documents.rs). */
export interface DocInfo {
  status: 'ok' | 'truncated' | 'needs_ocr' | 'encrypted' | 'unsupported' | 'failed' | 'cancelled'
  kind: string
  format: string
  title: string | null
  pages: number | null
  pagesRead: number
  unit: string
  chars: number
  textPath: string | null
  detail: string | null
  emptyPages: number[]
}

/** What a message carries besides its text. */
export interface MessageExtras {
  chips?: ContextChip[]
  attachments?: { kind: string; name: string; path: string; thumb?: string | null }[]
}

/** Shared bounded history API (parity 02); omit history for legacy agent output. */
export interface HistoryRequest {
  limit?: number
  cursor?: string
  start?: 'oldest' | 'newest'
  includeActivity?: boolean
  query?: string
}

export interface HistoryEntry {
  itemId: string
  turnId: string
  seq: number
  kind: string
  parent: string | null
  createdAt: number
  updatedAt: number
  revision: number
  preview: string
  previewOffset: number
  textChars: number
  truncated: boolean
  attachmentCount: number
}

export interface HistoryPage {
  threadId: string
  threadProvider: string
  items: HistoryEntry[]
  total: number
  nextCursor: string | null
  snapshotSeq: number | null
  contentIsUntrusted: boolean
  consistency: string
}

export interface HistoryChunk {
  threadId: string
  itemId: string
  turnId: string
  kind: string
  parent: string | null
  revision: number
  encoding: 'json'
  text: string
  offset: number
  totalChars: number
  nextCursor: string | null
  contentIsUntrusted: boolean
}

// ── Crews (round 2, card 07) ──

export type CrewTaskStatus = 'waiting' | 'running' | 'asking' | 'stalled' | 'done' | 'failed' | 'stopped'

export interface Crew {
  id: string
  orchestratorId: string
  contextId: string
  objective: string
  /** `running` | `stopped` | `done`. */
  status: string
  maxParallel: number
  allowPeers: boolean
  rev: number
  createdAt: number
  updatedAt: number
}

export interface CrewTask {
  id: string
  crewId: string
  key: string
  spec: string
  deps: string[]
  cardId: string | null
  provider: string
  model: string
  worktree: boolean
  timeoutMin: number
  status: CrewTaskStatus
  progress: number
  note: string
  workerId: string | null
  attempts: number
  summary: string
  result: string
  error: string
  startedAt: number | null
  finishedAt: number | null
  sort: number
  brief: string
}

export interface CrewMember {
  crewId: string
  threadId: string
  taskId: string | null
  role: 'orchestrator' | 'worker'
  attempt: number
  status: string
  provider: string
  joinedAt: number
}

export interface CrewMessage {
  id: string
  crewId: string
  taskId: string | null
  fromId: string
  toId: string
  kind: string
  text: string
  replyTo: string | null
  status: string
  at: number
  n: number
}

export interface CrewSnapshot {
  crew: Crew
  tasks: CrewTask[]
  members: CrewMember[]
  messages: CrewMessage[]
  messageCount: number
  /** Open questions and pending gates, whole. */
  open: CrewMessage[]
  maxParallel: number
  titles: Record<string, string>
}

// ── Schedules (round 3, card 03, crate/src/schedules.rs) ──

/** Local wall-clock times: `HH:MM`, `YYYY-MM-DDTHH:MM`; days 1 = Monday … 7 = Sunday. */
export type Recurrence =
  | { kind: 'once'; at: string }
  | { kind: 'daily'; time: string }
  | { kind: 'weekdays'; time: string }
  | { kind: 'days'; days: number[]; time: string }
  | { kind: 'hours'; every: number; from: string }

export interface ScheduleRun {
  id: string
  scheduleId: string
  slot: number
  at: number
  status: 'starting' | 'started' | 'skipped' | 'missed' | 'failed'
  threadId: string | null
  note: string
  manual: boolean
}

export interface Schedule {
  id: string
  name: string
  prompt: string
  contextId: string
  provider: string
  model: string
  effort: string
  mode: string
  instance: string
  recurrence: Recurrence
  enabled: boolean
  missed: 'skip' | 'once'
  target: 'new' | 'same'
  threadId: string | null
  lastSlot: number | null
  createdAt: number
  updatedAt: number
  nextDue: number | null
  lastRun: ScheduleRun | null
}

// ── Review guides (round 3, card 04, crate/src/guides.rs) ──

export type GuideTarget = { kind: 'thread'; id: string; scope: string; turn: string | null } | { kind: 'pr'; contextId: string; number: number }

export interface GuideFile {
  path: string
  what: string
  /** `low` | `medium` | `high`; '' when the agent did not cover the file. */
  risk: string
  check: string[]
  line: number | null
}

export interface Guide {
  summary: string
  groups: { title: string; files: GuideFile[] }[]
  questions: string[]
  missing: string[]
  cut: string[]
  key: string
  provider: string
  model: string
  createdAt: number
}

export interface GuideState {
  guide: Guide | null
  stale: boolean
  files: number
}

// ── Visual progress (round 3, card 05, crate/src/progress.rs) ──

export interface ProgressImage {
  /** `<item id>#<n>`. */
  key: string
  itemId: string
  /** 1-based; 0 when unknown. */
  turn: number
  source: 'you' | 'browser' | 'page' | 'tool'
  name: string
  path: string | null
  gone: boolean
  at: number
}

export interface Progress {
  images: ProgressImage[]
  pins: string[]
}
