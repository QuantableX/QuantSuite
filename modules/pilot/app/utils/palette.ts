/**
 * The command palette's ranking (card 17; T3 Code's palette): a thread
 * matches when every word of the query is in its title, branch, pull
 * request or project; title starts beat title words beat the other fields.
 * Threads opened often and lately rank higher (frecency), and the thread
 * picked last for the same query is boosted for three weeks.
 */

export interface PaletteThread {
  id: string
  title: string
  branch: string
  project: string
  /** "#42 Faster rows" for a linked pull request. */
  pr: string
  lastActivityAt: number
}

/** Per thread: how often it was opened from anywhere, and when last. */
export type Frecency = Record<string, { n: number; at: number }>
/** Per query: the thread picked for it, and when. */
export type Picks = Record<string, { id: string; at: number }>

const DAY = 86_400_000
export const PICK_BOOST_MS = 21 * DAY

export function matchScore(query: string, t: PaletteThread): number | null {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean)
  const title = (t.title || 'New thread').toLowerCase()
  const rest = [t.branch, t.pr, t.project].join(' ').toLowerCase()
  let score = 0
  for (const w of words) {
    if (title.startsWith(w)) score += 3
    else if (title.includes(` ${w}`) || title.includes(`-${w}`) || title.includes(`/${w}`)) score += 2
    else if (title.includes(w)) score += 1.5
    else if (rest.includes(w)) score += 1
    else return null
  }
  return score
}

export function frecencyBoost(f: { n: number; at: number } | undefined, now: number): number {
  if (!f) return 0
  const days = (now - f.at) / DAY
  const recency = days < 1 ? 1 : days < 7 ? 0.7 : days < 30 ? 0.4 : 0.2
  return Math.log2(1 + f.n) * recency
}

export function normalizeQuery(q: string): string {
  return q.trim().toLowerCase().replace(/\s+/g, ' ')
}

/** Threads for the query, best first; without a query the most used and recent ones. */
export function rankThreads(threads: PaletteThread[], query: string, frecency: Frecency, picks: Picks, now: number, limit = 8): PaletteThread[] {
  const q = normalizeQuery(query)
  const picked = q && picks[q] && now - picks[q].at < PICK_BOOST_MS ? picks[q].id : null
  const scored: { t: PaletteThread; s: number }[] = []
  for (const t of threads) {
    const m = q ? matchScore(q, t) : 0
    if (m === null) continue
    const recent = q ? 0 : Math.max(0, 1 - (now - t.lastActivityAt) / (14 * DAY))
    scored.push({ t, s: m + frecencyBoost(frecency[t.id], now) + recent + (t.id === picked ? 5 : 0) })
  }
  scored.sort((a, b) => b.s - a.s || b.t.lastActivityAt - a.t.lastActivityAt)
  return scored.slice(0, limit).map((x) => x.t)
}

/** Every word of the query is in the text. */
export function matchesWords(query: string, text: string): boolean {
  const hay = text.toLowerCase()
  return normalizeQuery(query)
    .split(' ')
    .filter(Boolean)
    .every((w) => hay.includes(w))
}
