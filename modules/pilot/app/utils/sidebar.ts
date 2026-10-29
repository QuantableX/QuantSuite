/**
 * The thread sidebar's rules (card 7), free of Vue so node tests can run
 * them: which section a thread is in, the order inside each, what a drop
 * does, the undo list and the snooze times. T3 Code's
 * docs/user/thread-sidebar.md is the reference.
 */

export type Section = 'pinned' | 'active' | 'working' | 'settled' | 'snoozed'
/** What a row shows: `waiting` = needs you. */
export type RowStatus = 'working' | 'waiting' | 'error' | 'limited' | 'idle'

export interface SidebarThread {
  id: string
  contextId: string
  pinned: boolean
  /** Larger is higher, in Pinned and Active. */
  sortKey: number
  settledAt: number | null
  snoozedUntil: number | null
  status: RowStatus
  /** When the user last sent work — the Working section's order. */
  sentAt: number
}

export interface Sections {
  pinned: SidebarThread[]
  active: SidebarThread[]
  working: SidebarThread[]
  settled: SidebarThread[]
  snoozed: SidebarThread[]
}

/** A saved thread state (`state`) as the row's status. */
export function rowStatus(state: string | null | undefined): RowStatus {
  switch (state) {
    case 'running':
    case 'starting':
      return 'working'
    case 'waiting':
      return 'waiting'
    case 'error':
      return 'error'
    case 'limited':
      return 'limited'
    default:
      return 'idle'
  }
}

/**
 * A snoozed thread stays hidden until its wake time unless it needs you; a
 * settled one stays on the shelf; pinned threads stay pinned even while
 * they work; the Working fold takes running threads when it is on.
 */
export function sectionOf(t: SidebarThread, now: number, workingSection: boolean): Section {
  if (t.snoozedUntil && t.snoozedUntil > now && t.status !== 'waiting') return 'snoozed'
  if (t.settledAt) return 'settled'
  if (t.pinned) return 'pinned'
  if (workingSection && t.status === 'working') return 'working'
  return 'active'
}

export function sections(threads: SidebarThread[], opts: { now: number; workingSection: boolean; contextId?: string | null }): Sections {
  const out: Sections = { pinned: [], active: [], working: [], settled: [], snoozed: [] }
  for (const t of threads) {
    if (opts.contextId && t.contextId !== opts.contextId) continue
    out[sectionOf(t, opts.now, opts.workingSection)].push(t)
  }
  const bySortKey = (a: SidebarThread, b: SidebarThread) => b.sortKey - a.sortKey || a.id.localeCompare(b.id)
  out.pinned.sort(bySortKey)
  out.active.sort(bySortKey)
  out.working.sort((a, b) => b.sentAt - a.sentAt || a.id.localeCompare(b.id))
  out.settled.sort((a, b) => (b.settledAt ?? 0) - (a.settledAt ?? 0) || a.id.localeCompare(b.id))
  out.snoozed.sort((a, b) => (a.snoozedUntil ?? 0) - (b.snoozedUntil ?? 0) || a.id.localeCompare(b.id))
  return out
}

/** The rows in screen order, with the shelves only when they are open. */
export function visibleOrder(s: Sections, open: { working: boolean; settled: boolean; snoozed: boolean }): SidebarThread[] {
  return [
    ...s.pinned,
    ...s.active,
    ...(open.working ? s.working : []),
    ...(open.settled ? s.settled : []),
    ...(open.snoozed ? s.snoozed : []),
  ]
}

// ── Dragging ──

/** Where a drop lands: a spot in Pinned or Active, or the Settled header. */
export type DropZone = { section: 'pinned' | 'active'; index: number } | { section: 'settled' }
export type DropAction = 'Pin' | 'Unpin' | 'Settle' | 'Un-settle' | 'Wake'

/** What the sidebar sends `pilot_thread_arrange`; absent = unchanged, `snoozedUntil: null` wakes. */
export interface Arrangement {
  pinned?: boolean
  sortKey?: number
  settled?: boolean
  snoozedUntil?: number | null
  autoSettle?: boolean
}

/** The sort key that puts a thread at `index` of `list` (ordered high to low, without the dragged row). */
export function keyAt(list: SidebarThread[], index: number, fallback: number): number {
  if (!list.length) return fallback
  if (index <= 0) return list[0].sortKey + 1
  if (index >= list.length) return list[list.length - 1].sortKey - 1
  return (list[index - 1].sortKey + list[index].sortKey) / 2
}

/** The badge the dragged row shows; `null` = a plain reorder, or no drop at all. */
export function dropAction(from: Section, zone: DropZone): DropAction | null {
  if (zone.section === 'settled') return from === 'settled' ? null : 'Settle'
  if (from === 'snoozed') return 'Wake'
  if (from === 'settled') return 'Un-settle'
  if (zone.section === 'pinned') return from === 'pinned' ? null : 'Pin'
  return from === 'pinned' ? 'Unpin' : null
}

/**
 * Whether a drop is allowed at all: nothing goes into the Snoozed shelf
 * (snoozing needs a wake time), and a working thread only changes section
 * — inside the active list it would land straight back in Working.
 */
export function canDrop(from: Section, zone: DropZone, workingSection: boolean): boolean {
  if (from === 'settled' && zone.section === 'settled') return false
  if (from === 'working' && workingSection && zone.section === 'active') return false
  return true
}

/** The change a drop makes. `list` is the target section without the dragged row. */
export function arrangementFor(t: SidebarThread, from: Section, zone: DropZone, list: SidebarThread[]): Arrangement {
  if (zone.section === 'settled') return { settled: true }
  const a: Arrangement = { pinned: zone.section === 'pinned', sortKey: keyAt(list, zone.index, t.sortKey) }
  if (from === 'settled') a.settled = false
  if (from === 'snoozed') a.snoozedUntil = null
  return a
}

/** The arrangement that undoes `a` on a thread that looked like `before`. */
export function inverse(before: SidebarThread, a: Arrangement): Arrangement {
  const back: Arrangement = {}
  if (a.pinned !== undefined || a.settled !== undefined) back.pinned = before.pinned
  if (a.sortKey !== undefined || a.settled !== undefined) back.sortKey = before.sortKey
  if (a.settled !== undefined) back.settled = before.settledAt !== null
  if (a.snoozedUntil !== undefined) back.snoozedUntil = before.snoozedUntil
  return back
}

/**
 * The midpoint keys of a long-arranged list run out of float precision at
 * some point; then the rows get fresh, evenly spaced keys (high to low).
 */
export function needsRespacing(list: SidebarThread[]): boolean {
  for (let i = 1; i < list.length; i++) {
    if (list[i - 1].sortKey - list[i].sortKey < 1e-6) return true
  }
  return false
}

/** Fresh keys one apart, keeping the order and the top key. */
export function respaced(list: SidebarThread[]): { id: string; sortKey: number }[] {
  const top = list[0]?.sortKey ?? 0
  return list.map((t, i) => ({ id: t.id, sortKey: top - i }))
}

// ── Undo (5 s, Ctrl+Z takes the newest) ──

export const UNDO_MS = 5000

export interface UndoEntry {
  id: number
  label: string
  at: number
}

export function pushUndo<T extends UndoEntry>(list: T[], entry: T, max = 10): T[] {
  return [...list, entry].slice(-max)
}

/** The newest entry still inside its five seconds. */
export function latestUndo<T extends UndoEntry>(list: T[], now: number): T | null {
  for (let i = list.length - 1; i >= 0; i--) {
    if (now - list[i].at < UNDO_MS) return list[i]
  }
  return null
}

export function dropExpired<T extends UndoEntry>(list: T[], now: number): T[] {
  return list.filter((e) => now - e.at < UNDO_MS)
}

// ── Snooze ──

const HOUR = 3_600_000

export function inHours(now: number, hours: number): number {
  return now + hours * HOUR
}

/** 9:00 local time on the next day. */
export function tomorrowAt(now: number, hour = 9): number {
  const d = new Date(now)
  d.setDate(d.getDate() + 1)
  d.setHours(hour, 0, 0, 0)
  return d.getTime()
}

/** `90m`, `2h`, `1.5 h`, `3d` (a day is 24 hours) → milliseconds; null when unreadable. */
export function parseDuration(text: string): number | null {
  const m = /^\s*(\d+(?:\.\d+)?)\s*(m|min|mins|minutes?|h|hrs?|hours?|d|days?)\s*$/i.exec(text)
  if (!m) return null
  const n = Number(m[1])
  if (!(n > 0)) return null
  const unit = m[2].toLowerCase()[0]
  return Math.round(n * (unit === 'm' ? 60_000 : unit === 'h' ? HOUR : 24 * HOUR))
}

/** A wake time for the menu: "15:00" today, "Tomorrow 09:00", "Mon, Nov 3 09:00". */
export function wakeLabel(at: number, now: number): string {
  const d = new Date(at)
  const time = d.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit', hour12: false })
  const today = new Date(now)
  if (d.toDateString() === today.toDateString()) return time
  const tomorrow = new Date(now)
  tomorrow.setDate(tomorrow.getDate() + 1)
  if (d.toDateString() === tomorrow.toDateString()) return `Tomorrow ${time}`
  return `${d.toLocaleDateString('en-US', { weekday: 'short', month: 'short', day: 'numeric' })} ${time}`
}
