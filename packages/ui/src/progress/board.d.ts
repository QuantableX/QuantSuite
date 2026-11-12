/** Types of the shared progress-board template (board.js). */

export interface ProgressShell {
  id: string
  command: string
  cwd?: string | null
  background: boolean
  startedAt: number
  endedAt?: number | null
  failed: boolean
}

export interface ProgressTask {
  id: string
  title: string
  step?: number | null
  steps?: number | null
  stepText?: string | null
  stepFraction?: number | null
  percent?: number | null
  estimateSecs?: number | null
  follow?: string | null
  followFraction?: number | null
  detail?: string | null
  followChangedAt?: number | null
  state: 'running' | 'done' | 'failed'
  note?: string | null
  startedAt: number
  updatedAt: number
  finishedAt?: number | null
  /** The whole bar, 0–1; null while nothing says how far it is. */
  overall?: number | null
  totalSecs?: number | null
  stale: boolean
}

export interface ProgressLane {
  key: string
  agent: string
  client?: string | null
  folder?: string | null
  place?: string | null
  tasks: ProgressTask[]
  shells: ProgressShell[]
  updatedAt: number
}

export interface ProgressBoard {
  now: number
  lanes: ProgressLane[]
  pageUrl?: string | null
}

export function renderBoard(view: ProgressBoard | null | undefined, options?: { empty?: string }): string
export function updateBoard(root: HTMLElement, view: ProgressBoard | null | undefined, options?: { empty?: string }): void
export function formatDuration(ms: number): string
export function percentText(fraction: number): string
export function timeLine(task: ProgressTask, now: number): string
