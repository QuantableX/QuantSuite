/**
 * Connections on the canvas (round 2, card 09): where an edge leaves one
 * window and enters the other, its curve, the state it shows, and where a
 * crew's workers go. Pure, so the node tests import it.
 */

export interface Rect {
  x: number
  y: number
  width: number
  height: number
}

export interface Pt {
  x: number
  y: number
}

/** What an edge shows: working = flowing, waiting = amber, error = red, done = calm. */
export type EdgeState = 'working' | 'waiting' | 'error' | 'done' | 'idle'

/** A minimized window is its header only. */
export const HEADER_H = 32

export function rectOf(w: { position: Rect; minimized?: boolean }): Rect {
  return { x: w.position.x, y: w.position.y, width: w.position.width, height: w.minimized ? HEADER_H : w.position.height }
}

const center = (r: Rect): Pt => ({ x: r.x + r.width / 2, y: r.y + r.height / 2 })

/**
 * The sides that face each other: left/right when the windows sit more
 * beside than above each other, else top/bottom. The curve leaves and
 * enters perpendicular to those sides, so it never cuts through either
 * window.
 */
export function anchors(a: Rect, b: Rect): { p0: Pt; p3: Pt; n0: Pt; n3: Pt } {
  const ca = center(a)
  const cb = center(b)
  const dx = cb.x - ca.x
  const dy = cb.y - ca.y
  // Compare the gaps between the boxes, not the centers: wide windows side by side stay horizontal.
  const gapX = Math.max(b.x - (a.x + a.width), a.x - (b.x + b.width))
  const gapY = Math.max(b.y - (a.y + a.height), a.y - (b.y + b.height))
  if (gapX >= gapY) {
    const right = dx >= 0
    return {
      p0: { x: right ? a.x + a.width : a.x, y: ca.y },
      p3: { x: right ? b.x : b.x + b.width, y: cb.y },
      n0: { x: right ? 1 : -1, y: 0 },
      n3: { x: right ? -1 : 1, y: 0 },
    }
  }
  const down = dy >= 0
  return {
    p0: { x: ca.x, y: down ? a.y + a.height : a.y },
    p3: { x: cb.x, y: down ? b.y : b.y + b.height },
    n0: { x: 0, y: down ? 1 : -1 },
    n3: { x: 0, y: down ? -1 : 1 },
  }
}

export function cubic(p0: Pt, p1: Pt, p2: Pt, p3: Pt, t: number): Pt {
  const u = 1 - t
  const a = u * u * u
  const b = 3 * u * u * t
  const c = 3 * u * t * t
  const d = t * t * t
  return { x: a * p0.x + b * p1.x + c * p2.x + d * p3.x, y: a * p0.y + b * p1.y + c * p2.y + d * p3.y }
}

export interface EdgeGeometry {
  d: string
  /** Points along the curve, for a particle's keyframes. */
  points: Pt[]
  mid: Pt
  end: Pt
}

export function edgeGeometry(a: Rect, b: Rect, samples = 10): EdgeGeometry {
  const { p0, p3, n0, n3 } = anchors(a, b)
  const dist = Math.hypot(p3.x - p0.x, p3.y - p0.y)
  const k = Math.max(40, Math.min(240, dist * 0.45))
  const p1 = { x: p0.x + n0.x * k, y: p0.y + n0.y * k }
  const p2 = { x: p3.x + n3.x * k, y: p3.y + n3.y * k }
  const r = (n: number) => Math.round(n * 10) / 10
  const points = Array.from({ length: samples + 1 }, (_, i) => cubic(p0, p1, p2, p3, i / samples))
  return {
    d: `M ${r(p0.x)} ${r(p0.y)} C ${r(p1.x)} ${r(p1.y)}, ${r(p2.x)} ${r(p2.y)}, ${r(p3.x)} ${r(p3.y)}`,
    points,
    mid: cubic(p0, p1, p2, p3, 0.5),
    end: p3,
  }
}

// ── State ──

/** A QuantPilot thread's row status (sidebar `statusOf`). */
export function threadState(status: string | null | undefined): EdgeState {
  switch (status) {
    case 'working':
      return 'working'
    case 'waiting':
      return 'waiting'
    case 'error':
    case 'limited':
      return 'error'
    default:
      return 'idle'
  }
}

/** A kanban card: claimed = working, review = waiting, done or merged = done. */
export function cardState(card: { column?: string; status?: string; agent_id?: string | null } | null | undefined): EdgeState {
  if (!card) return 'idle'
  if (card.status === 'rejected' || card.status === 'cancelled') return 'error'
  if (card.column === 'done' || card.status === 'merged' || card.status === 'approved') return 'done'
  if (card.column === 'review' || card.status === 'awaiting_review') return 'waiting'
  if (card.status === 'in_progress' || card.agent_id || card.column === 'work') return 'working'
  return 'idle'
}

/** A crew task (card 07). */
export function taskState(status: string): EdgeState {
  switch (status) {
    case 'running':
      return 'working'
    case 'asking':
    case 'stalled':
      return 'waiting'
    case 'failed':
      return 'error'
    case 'done':
      return 'done'
    default:
      return 'idle'
  }
}

/** The more pressing of two states (an edge between two windows shows the one that needs attention). */
export function worse(a: EdgeState, b: EdgeState): EdgeState {
  const rank: Record<EdgeState, number> = { error: 4, waiting: 3, working: 2, done: 1, idle: 0 }
  return rank[a] >= rank[b] ? a : b
}

// ── Crew clusters ──

/**
 * Where a crew's worker windows go: a grid to the right of the
 * orchestrator, two rows tall, columns added as needed.
 */
export function clusterSlots(origin: Rect, count: number, size = { width: 460, height: 520 }, gap = 40): Rect[] {
  const rows = count <= 1 ? 1 : 2
  return Array.from({ length: count }, (_, i) => {
    const col = Math.floor(i / rows)
    const row = i % rows
    const top = origin.y + origin.height / 2 - (rows * size.height + (rows - 1) * gap) / 2
    return { x: origin.x + origin.width + 120 + col * (size.width + gap), y: Math.round(top + row * (size.height + gap)), width: size.width, height: size.height }
  })
}

/** The box around some windows — a collapsed cluster sits at its top-left. */
export function boundsOf(rects: Rect[]): Rect | null {
  if (!rects.length) return null
  const x = Math.min(...rects.map((r) => r.x))
  const y = Math.min(...rects.map((r) => r.y))
  const right = Math.max(...rects.map((r) => r.x + r.width))
  const bottom = Math.max(...rects.map((r) => r.y + r.height))
  return { x, y, width: right - x, height: bottom - y }
}

/** A free spot right of a window for a window it opens (a card's chat, a chat's diff). */
export function besideOf(r: Rect, size: { width: number; height: number }, gap = 80): Rect {
  return { x: r.x + r.width + gap, y: r.y, width: size.width, height: size.height }
}
