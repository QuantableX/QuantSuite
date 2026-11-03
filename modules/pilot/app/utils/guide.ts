/**
 * Review guides (round 3, card 04): small helpers for the guide panel. The
 * guide itself is written by one read-only agent run in the crate
 * (crate/src/guides.rs).
 */
import type { Guide } from '#pilot/types'

/** How many files of each risk a guide has — the header line. */
export function riskCounts(g: Guide): { high: number; medium: number; low: number; other: number } {
  const out = { high: 0, medium: 0, low: 0, other: 0 }
  for (const group of g.groups) {
    for (const f of group.files) {
      if (f.risk === 'high' || f.risk === 'medium' || f.risk === 'low') out[f.risk]++
      else out.other++
    }
  }
  return out
}

/** "3 files · 1 high risk · 2 medium" */
export function riskLine(g: Guide): string {
  const c = riskCounts(g)
  const total = c.high + c.medium + c.low + c.other
  const parts = [`${total} file${total === 1 ? '' : 's'}`]
  if (c.high) parts.push(`${c.high} high risk`)
  if (c.medium) parts.push(`${c.medium} medium`)
  if (c.low) parts.push(`${c.low} low`)
  if (c.other) parts.push(`${c.other} not covered`)
  return parts.join(' · ')
}

export interface GuideAgent {
  provider: string
  model: string
  instance: string
}

const KEY = 'qp-guide-agent'

/** The agent the last guide was written with; only Claude Code and Codex write guides. */
export function loadGuideAgent(fallback: GuideAgent): GuideAgent {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? 'null') as Partial<GuideAgent> | null
    if (v && (v.provider === 'claude' || v.provider === 'codex')) return { provider: v.provider, model: v.model ?? '', instance: v.instance ?? '' }
  } catch {
    /* no saved choice */
  }
  return fallback
}

export function saveGuideAgent(a: GuideAgent) {
  try {
    localStorage.setItem(KEY, JSON.stringify(a))
  } catch {
    /* private mode */
  }
}
