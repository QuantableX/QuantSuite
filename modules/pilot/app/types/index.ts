/**
 * QuantPilot's types — the Rust side's shapes (sessions, adapters, status,
 * settings) and the one signal vocabulary every adapter speaks
 * (crate/src/signals.rs).
 */

export type Mode = 'ask' | 'auto' | 'full'
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
}

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
