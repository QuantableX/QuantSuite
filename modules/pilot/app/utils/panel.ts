/**
 * The thread panel's sections (round 2, card 04): which ones show, in the
 * user's order, and what goes into "Agents now" and "Changes". Pure, so the
 * node tests import it.
 */

export const PANEL_SECTIONS = ['agents', 'thread', 'changes', 'branch', 'tasks', 'crew', 'servers'] as const
export type PanelSection = (typeof PANEL_SECTIONS)[number]

export const SECTION_LABELS: Record<PanelSection, string> = {
  agents: 'Agents now',
  thread: 'This thread',
  changes: 'Changes',
  branch: 'Branch and PR',
  tasks: 'Tasks',
  crew: 'Crew',
  servers: 'Servers',
}

export interface PanelLayout {
  order: PanelSection[]
  hidden: PanelSection[]
}

/** A saved layout made whole: unknown names dropped, new sections added at the end. */
export function normalizeLayout(saved: Partial<{ order: string[]; hidden: string[] }> | null | undefined): PanelLayout {
  const known = (list: string[] | undefined) => (list ?? []).filter((s): s is PanelSection => (PANEL_SECTIONS as readonly string[]).includes(s))
  const order = [...new Set(known(saved?.order))]
  for (const s of PANEL_SECTIONS) if (!order.includes(s)) order.push(s)
  return { order, hidden: [...new Set(known(saved?.hidden))] }
}

/** `section` moved to `to` (its new index), the others keep their order. */
export function moveSection(order: PanelSection[], section: PanelSection, to: number): PanelSection[] {
  const rest = order.filter((s) => s !== section)
  const at = Math.max(0, Math.min(rest.length, to))
  return [...rest.slice(0, at), section, ...rest.slice(at)]
}

/** Where a dragged section lands: the slot whose middle the pointer passed. */
export function dropIndex(mids: number[], y: number): number {
  let i = 0
  while (i < mids.length && y > mids[i]!) i++
  return i
}

export interface AgentNowInput {
  id: string
  title: string
  status: 'working' | 'waiting' | 'error' | 'limited' | 'idle'
  /** The working turn's start, or the last activity. */
  since: number
  unread: boolean
}

/**
 * Agents now: who needs you first, then who works (longest first), then
 * finished results not seen yet (newest first) — at most `max`.
 */
export function agentsNow<T extends AgentNowInput>(threads: T[], max = 8): (T & { bucket: 'waiting' | 'working' | 'done' })[] {
  const waiting = threads.filter((t) => t.status === 'waiting').sort((a, b) => a.since - b.since)
  const working = threads.filter((t) => t.status === 'working').sort((a, b) => a.since - b.since)
  const done = threads.filter((t) => t.unread && t.status !== 'working' && t.status !== 'waiting').sort((a, b) => b.since - a.since)
  return [
    ...waiting.map((t) => ({ ...t, bucket: 'waiting' as const })),
    ...working.map((t) => ({ ...t, bucket: 'working' as const })),
    ...done.map((t) => ({ ...t, bucket: 'done' as const })),
  ].slice(0, max)
}

export interface ChangedFile {
  path: string
  added: number
  removed: number
  status: string
  /** The newest turn that changed it (its diff opens there). */
  turnId: string
}

/** Every file the thread's turns changed, newest first, with the lines summed. */
export function aggregateChanges(items: { kind: string; turnId: string; data: Record<string, unknown> }[]): ChangedFile[] {
  const by = new Map<string, ChangedFile>()
  for (const it of items) {
    if (it.kind !== 'changes' || !Array.isArray(it.data.files)) continue
    for (const f of it.data.files as { path: string; added?: number; removed?: number; status?: string }[]) {
      const prev = by.get(f.path)
      by.delete(f.path)
      by.set(f.path, {
        path: f.path,
        added: (prev?.added ?? 0) + (f.added ?? 0),
        removed: (prev?.removed ?? 0) + (f.removed ?? 0),
        status: f.status ?? prev?.status ?? 'M',
        turnId: it.turnId,
      })
    }
  }
  return [...by.values()].reverse()
}

/** "83k / 200k" for the context meter. */
export function tokenPair(used: number, limit: number): string {
  const k = (n: number) => (n >= 1_000_000 ? `${(n / 1_000_000).toFixed(n % 1_000_000 ? 1 : 0)}M` : n >= 1000 ? `${Math.round(n / 1000)}k` : String(n))
  return `${k(used)} / ${k(limit)}`
}
