/** Wire types — the crate's serde output, camelCased. */

export interface Pause {
  /** First paused day, inclusive, YYYY-MM-DD. */
  fromOn: string
  /** Last paused day, inclusive. null while the habit is still paused. */
  toOn: string | null
}

export interface Habit {
  id: string
  name: string
  /** The first trackable day — the local day the habit was created. */
  startedOn: string
  isPaused: boolean
  /** Tracked weekdays: bit 0 = Monday … bit 6 = Sunday. */
  daysMask: number
  sortIndex: number
  pauses: Pause[]
  createdAt: string
  updatedAt: string
}

export interface HabitPatch {
  name?: string
  sortIndex?: number
  daysMask?: number
}

export interface Check {
  habitId: string
  onDay: string
}

export interface HabitStats {
  id: string
  name: string
  isPaused: boolean
  startedOn: string
  eligibleDays: number
  checkedDays: number
  /** Percent, or null before the first eligible day. */
  rate: number | null
  currentStreak: number
  bestStreak: number
}

export interface StatsOut {
  year: number
  habits: HabitStats[]
  totalChecks: number
}

export type Grouping = 'week' | 'month'

export interface AppSettings {
  grouping: Grouping
  sidebarLeftOpen: boolean
  sidebarRightOpen: boolean
}

/** What one grid cell is allowed to be. */
export type DayState = 'before' | 'future' | 'paused' | 'untracked' | 'open' | 'checked'

/** Every weekday bit set — the default schedule. */
export const ALL_DAYS = 0b111_1111
