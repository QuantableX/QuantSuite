/**
 * The composer's pickers (docs/PLAN-QUANTPILOT.md §6, card 5): which models
 * a search shows, which effort levels a model offers, what each permission
 * mode means. Pure, so the rules are testable without a browser.
 */
import type { Mode, ModelInfo } from '../types'

/** A row of a picker menu (MenuList.vue). */
export interface MenuOption {
  id: string
  label: string
  description?: string
  disabled?: boolean
  /** Why a disabled row cannot be picked. */
  reason?: string
}

/** An agent in the model picker's left column. */
export interface ProviderOption {
  id: string
  label: string
  available: boolean
  reason?: string
}

/** A model row: the catalog's, or the custom id the user typed. */
export interface ModelRow {
  /** What is stored on the thread and passed to the CLI ('' = the CLI's default). */
  id: string
  label: string
  detail: string
  custom?: boolean
}

/** The catalog's `default` entry means "no override". */
export function storedModelId(catalogId: string): string {
  return catalogId === 'default' ? '' : catalogId
}

export function modelRows(models: ModelInfo[], query: string): ModelRow[] {
  const q = query.trim().toLowerCase()
  const rows: (ModelRow & { rank: number })[] = []
  for (const m of models) {
    const label = m.displayName || m.id
    const hay = [m.id, label, m.resolvedModel ?? '', m.description].map((s) => s.toLowerCase())
    let rank = 0
    if (q) {
      if (hay[0] === q || hay[1] === q) rank = 0
      else if (hay[0].startsWith(q) || hay[1].startsWith(q)) rank = 1
      else if (hay.some((h) => h.includes(q))) rank = 2
      else continue
    }
    const resolved = m.resolvedModel && m.resolvedModel !== m.id ? m.resolvedModel : ''
    rows.push({
      id: storedModelId(m.id),
      label,
      detail: [m.description, resolved].filter(Boolean).join(' · '),
      rank,
    })
  }
  rows.sort((a, b) => a.rank - b.rank)
  const out: ModelRow[] = rows.map(({ rank: _rank, ...r }) => r)
  // An id the catalog does not list can still be used (custom model).
  if (q && !models.some((m) => m.id.toLowerCase() === q) && /^[\w.:@/-]+$/.test(query.trim())) {
    out.push({ id: query.trim(), label: `Use “${query.trim()}”`, detail: 'A model id the CLI accepts', custom: true })
  }
  return out
}

/** The effort levels the thread's model reports; none means the picker is hidden. */
export function effortsFor(models: ModelInfo[], model: string): string[] {
  const id = model || 'default'
  const m = models.find((x) => x.id === id) ?? models.find((x) => x.resolvedModel === id)
  return m?.efforts ?? []
}

export interface ModeInfo {
  id: Mode
  label: string
  short: string
  description: string
}

/** The five modes, in the order T3 Code and PandaCode present them. */
export const MODES: ModeInfo[] = [
  { id: 'ask', label: 'Supervised', short: 'Supervised', description: 'Asks before commands and file changes.' },
  { id: 'edits', label: 'Accept edits', short: 'Accept edits', description: 'Edits files freely; asks before commands and other actions.' },
  { id: 'auto', label: 'Auto', short: 'Auto', description: "The agent's own reviewer approves routine actions and asks about the rest." },
  { id: 'plan', label: 'Plan', short: 'Plan', description: 'Reads, searches and plans. Makes no changes.' },
  { id: 'full', label: 'Full access', short: 'Full access', description: 'Runs commands and edits without asking.' },
]

/** The agents with an automatic reviewer (Auto): Claude Code and Codex; ACP agents and pi do not have one (card 20). */
export function modesFor(provider: string): ModeInfo[] {
  return provider === 'claude' || provider === 'codex' ? MODES : MODES.filter((m) => m.id !== 'auto')
}

/** "83k / 200k" for the context meter. */
export function compactTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(n % 1_000_000 === 0 ? 0 : 1)}M`
  if (n >= 1_000) return `${Math.round(n / 1_000)}k`
  return String(n)
}
