/**
 * The Usage page's arithmetic (card 18): a report's rows — one per bucket,
 * provider and model, priced in the crate — become totals, a gap-free
 * series of buckets for the charts, and the per-model breakdown. Pure
 * functions: the components only render this.
 */
import type { UsageReport, UsageRow } from '../types'

export interface UsageTotals {
  input: number
  cacheRead: number
  cacheWrite: number
  output: number
  tokens: number
  /** Estimated cost of the priced models. */
  cost: number
  costParts: { input: number; cacheRead: number; cacheWrite: number; output: number }
  savings: number
  /** Tokens of models without a public price. */
  unpricedTokens: number
}

export const tokensOf = (r: Pick<UsageRow, 'input' | 'cacheRead' | 'cacheWrite' | 'output'>) => r.input + r.cacheRead + r.cacheWrite + r.output
export const costOf = (r: UsageRow) => (r.cost ? r.cost.input + r.cost.cacheRead + r.cost.cacheWrite + r.cost.output : 0)

export function totals(rows: UsageRow[]): UsageTotals {
  const t: UsageTotals = { input: 0, cacheRead: 0, cacheWrite: 0, output: 0, tokens: 0, cost: 0, costParts: { input: 0, cacheRead: 0, cacheWrite: 0, output: 0 }, savings: 0, unpricedTokens: 0 }
  for (const r of rows) {
    t.input += r.input
    t.cacheRead += r.cacheRead
    t.cacheWrite += r.cacheWrite
    t.output += r.output
    if (r.cost) {
      t.costParts.input += r.cost.input
      t.costParts.cacheRead += r.cost.cacheRead
      t.costParts.cacheWrite += r.cost.cacheWrite
      t.costParts.output += r.cost.output
      t.savings += r.cost.savings
    } else t.unpricedTokens += tokensOf(r)
  }
  t.tokens = t.input + t.cacheRead + t.cacheWrite + t.output
  t.cost = t.costParts.input + t.costParts.cacheRead + t.costParts.cacheWrite + t.costParts.output
  return t
}

export interface Bucket {
  at: number
  /** Per series id: providers for cost, token kinds for tokens. */
  values: Record<string, number>
}

/** Every hour or local day of the period, empty ones too, oldest first. */
export function bucketStarts(report: Pick<UsageReport, 'since' | 'bucket'>, now = Date.now()): number[] {
  const out: number[] = []
  if (report.bucket === 'hour') {
    for (let t = report.since; t <= now; t += 3_600_000) out.push(t)
    return out
  }
  const d = new Date(report.since)
  d.setHours(0, 0, 0, 0)
  // Calendar steps, so a daylight-saving day is still one day.
  for (; d.getTime() <= now; d.setDate(d.getDate() + 1)) out.push(d.getTime())
  return out
}

export type SeriesBy = 'provider' | 'kind'

/** Cost per provider, or tokens per kind, for each bucket. */
export function series(report: UsageReport, by: SeriesBy, model: string | null = null, now = Date.now()): Bucket[] {
  const index = new Map(bucketStarts(report, now).map((at, i) => [at, i]))
  const out: Bucket[] = [...index.keys()].map((at) => ({ at, values: {} }))
  for (const r of report.rows) {
    if (model && r.model !== model) continue
    const i = index.get(r.bucket)
    if (i === undefined) continue
    const v = out[i].values
    if (by === 'provider') v[r.provider] = (v[r.provider] ?? 0) + costOf(r)
    else {
      v.input = (v.input ?? 0) + r.input
      v.cacheRead = (v.cacheRead ?? 0) + r.cacheRead
      v.cacheWrite = (v.cacheWrite ?? 0) + r.cacheWrite
      v.output = (v.output ?? 0) + r.output
    }
  }
  return out
}

export interface ModelUsage {
  provider: string
  model: string
  tokens: number
  input: number
  cacheRead: number
  cacheWrite: number
  output: number
  /** `null`: no public price. */
  cost: number | null
  /** Of the period's estimated cost, 0–1. */
  share: number
  /** Cache reads of all prompt tokens, 0–1. */
  cacheHitRate: number
  /** Estimated USD per million tokens. */
  costPerMillion: number | null
}

/** One row per model, the costliest first, then by tokens. */
export function byModel(rows: UsageRow[]): ModelUsage[] {
  const map = new Map<string, ModelUsage>()
  let total = 0
  for (const r of rows) {
    const key = `${r.provider}\u0000${r.model}`
    const m = map.get(key) ?? { provider: r.provider, model: r.model, tokens: 0, input: 0, cacheRead: 0, cacheWrite: 0, output: 0, cost: null, share: 0, cacheHitRate: 0, costPerMillion: null }
    m.input += r.input
    m.cacheRead += r.cacheRead
    m.cacheWrite += r.cacheWrite
    m.output += r.output
    m.tokens += tokensOf(r)
    if (r.cost) {
      m.cost = (m.cost ?? 0) + costOf(r)
      total += costOf(r)
    }
    map.set(key, m)
  }
  const out = [...map.values()]
  for (const m of out) {
    const prompt = m.input + m.cacheRead + m.cacheWrite
    m.cacheHitRate = prompt ? m.cacheRead / prompt : 0
    m.share = total && m.cost ? m.cost / total : 0
    m.costPerMillion = m.cost !== null && m.tokens ? m.cost / (m.tokens / 1e6) : null
  }
  return out.sort((a, b) => (b.cost ?? -1) - (a.cost ?? -1) || b.tokens - a.tokens)
}

/** "12.3M", "845k", "912". */
export function compact(n: number): string {
  if (n >= 1e9) return `${(n / 1e9).toFixed(n >= 1e10 ? 0 : 1)}B`
  if (n >= 1e6) return `${(n / 1e6).toFixed(n >= 1e7 ? 0 : 1)}M`
  if (n >= 1e3) return `${(n / 1e3).toFixed(n >= 1e4 ? 0 : 1)}k`
  return String(Math.round(n))
}

/** Reset countdown: "2h 14m", "3d 4h", "now". */
export function countdown(at: number, now = Date.now()): string {
  const m = Math.max(0, Math.round((at - now) / 60_000))
  if (m <= 0) return 'now'
  if (m < 60) return `${m}m`
  const h = Math.floor(m / 60)
  if (h < 24) return `${h}h ${m % 60}m`
  return `${Math.floor(h / 24)}d ${h % 24}h`
}

const WINDOW_NAMES: Record<string, string> = {
  five_hour: '5-hour',
  seven_day: 'Weekly',
  seven_day_opus: 'Weekly · Opus',
  seven_day_sonnet: 'Weekly · Sonnet',
}

export function windowName(kind: string): string {
  return WINDOW_NAMES[kind] ?? kind.replace(/_/g, ' ').replace(/^\w/, (c) => c.toUpperCase())
}
