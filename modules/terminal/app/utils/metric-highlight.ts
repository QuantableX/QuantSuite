import { bindings, formatValue, type Band, type Point } from '#terminal/utils/metrics'

export type HighlightMode = 'bands' | 'value' | 'sigma'
export type HighlightSide = 'both' | 'above' | 'below'

/** Every mode keeps its own fields, so switching modes never loses an earlier choice. */
export interface HighlightRule {
  mode: HighlightMode
  bands: string[]
  min: number | null
  max: number | null
  sigma: number
  side: HighlightSide
}
export interface HighlightThreshold {
  value: number
  label: string
  zone: HighlightZone
}
/** Which end of the indicator's range a matching day belongs to. */
export type HighlightZone = 'upper' | 'lower' | 'single'
export const HIGHLIGHT_ZONES: Record<HighlightZone, { label: string; color: string; tint: string }> =
  {
    upper: { label: 'Upper range', color: '#ed7884', tint: 'rgba(237, 120, 132, 0.14)' },
    single: { label: 'Range', color: '#70b5e8', tint: 'rgba(112, 181, 232, 0.11)' },
    lower: { label: 'Lower range', color: '#35b6a0', tint: 'rgba(53, 182, 160, 0.14)' },
  }
export type HighlightResult =
  | {
      ok: true
      /** Matching days and the zone each belongs to. */
      matched: Map<number, HighlightZone>
      /** Zones the rule can produce, upper first — for the legend. */
      zones: HighlightZone[]
      thresholds: HighlightThreshold[]
      summary: string
      /** Matching days inside the selected period, and its size. */
      periodMatches: number
      periodDays: number
      stats?: { mean: number; deviation: number }
    }
  | { ok: false; reason: string }
export type HighlightMatch = Extract<HighlightResult, { ok: true }>

export const HIGHLIGHT_STORAGE_KEY = 'quantview-metrics-highlight'
const MAX_SIGMA = 10

export function highlightBands(title: string): Band[] {
  return bindings[title]?.bands ?? []
}

/** Outermost named bands where a metric defines a middle; ±2σ everywhere else. */
export function defaultHighlightRule(title: string): HighlightRule {
  const bands = highlightBands(title)
  const named = bands.length >= 3
  return {
    mode: named ? 'bands' : 'sigma',
    bands: named ? [bands[0]!.label, bands.at(-1)!.label] : [],
    min: null,
    max: null,
    sigma: 2,
    side: 'both',
  }
}

const finiteOrNull = (value: unknown) =>
  typeof value === 'number' && Number.isFinite(value) ? value : null

/** Saved rules are user data: keep what is valid for this metric, default the rest. */
export function normalizeHighlightRule(title: string, raw: unknown): HighlightRule {
  const fallback = defaultHighlightRule(title)
  if (!raw || typeof raw !== 'object') return fallback
  const saved = raw as Partial<Record<keyof HighlightRule, unknown>>
  const labels = new Set(highlightBands(title).map((band) => band.label))
  const mode =
    saved.mode === 'value' || saved.mode === 'sigma' || (saved.mode === 'bands' && labels.size)
      ? saved.mode
      : fallback.mode
  const sigma = finiteOrNull(saved.sigma)
  return {
    mode,
    bands: Array.isArray(saved.bands)
      ? [...new Set(saved.bands.filter((label): label is string => labels.has(label)))]
      : fallback.bands,
    min: finiteOrNull(saved.min),
    max: finiteOrNull(saved.max),
    sigma: sigma != null && sigma > 0 && sigma <= MAX_SIGMA ? sigma : fallback.sigma,
    side: saved.side === 'above' || saved.side === 'below' ? saved.side : 'both',
  }
}

export function sameHighlightRule(a: HighlightRule, b: HighlightRule): boolean {
  return (
    a.mode === b.mode &&
    a.min === b.min &&
    a.max === b.max &&
    a.sigma === b.sigma &&
    a.side === b.side &&
    a.bands.length === b.bands.length &&
    a.bands.every((label) => b.bands.includes(label))
  )
}

const sigmaLabel = (sigma: number) => `${Number(sigma.toFixed(2))}σ`

/**
 * Days whose reading falls inside the rule. Standard deviations use the selected
 * period (the same basis as the Context percentile) and are applied to every day,
 * so panning outside the period keeps a consistent definition.
 */
export function highlightMatches(
  title: string,
  rule: HighlightRule,
  points: Point[],
  period: Point[],
): HighlightResult {
  const unit = bindings[title]?.unit ?? ''
  let test: (value: number) => HighlightZone | null
  let zones: HighlightZone[]
  let thresholds: HighlightThreshold[] = []
  let summary: string
  let stats: { mean: number; deviation: number } | undefined
  if (rule.mode === 'bands') {
    // Bands below the middle of the metric's list are its lower range, above it the upper.
    const all = highlightBands(title),
      middle = (all.length - 1) / 2
    const chosen = all.flatMap((band, i) =>
      rule.bands.includes(band.label)
        ? [{ band, zone: (i < middle ? 'lower' : i > middle ? 'upper' : 'single') as HighlightZone }]
        : [],
    )
    if (!chosen.length) return { ok: false, reason: 'Choose at least one range.' }
    test = (value) =>
      chosen.find(({ band }) => value >= band.min && value < band.max)?.zone ?? null
    zones = chosen.map(({ zone }) => zone)
    summary = chosen.map(({ band }) => band.label).join(' · ')
  } else if (rule.mode === 'value') {
    const { min, max } = rule
    if (min == null && max == null) return { ok: false, reason: 'Enter a minimum, a maximum or both.' }
    if (min != null && max != null && min > max)
      return { ok: false, reason: 'The minimum is above the maximum.' }
    // An open-ended range reads as the top (≥ min) or the bottom (≤ max) of the indicator.
    const zone: HighlightZone = max == null ? 'upper' : min == null ? 'lower' : 'single'
    test = (value) => ((min == null || value >= min) && (max == null || value <= max) ? zone : null)
    zones = [zone]
    thresholds = [
      ...(min != null ? [{ value: min, label: 'Min', zone }] : []),
      ...(max != null ? [{ value: max, label: 'Max', zone }] : []),
    ]
    summary =
      min != null && max != null
        ? `${formatValue(min, unit)} – ${formatValue(max, unit)}`
        : min != null
          ? `≥ ${formatValue(min, unit)}`
          : `≤ ${formatValue(max, unit)}`
  } else {
    const values = period.map((p) => p.value).filter(Number.isFinite)
    if (values.length < 2) return { ok: false, reason: 'The selected period has too few readings.' }
    const mean = values.reduce((sum, v) => sum + v, 0) / values.length
    const deviation = Math.sqrt(values.reduce((sum, v) => sum + (v - mean) ** 2, 0) / values.length)
    if (!(deviation > 0)) return { ok: false, reason: 'The selected period has no variation.' }
    if (!(rule.sigma > 0)) return { ok: false, reason: 'Enter a positive number of deviations.' }
    stats = { mean, deviation }
    const upper = mean + rule.sigma * deviation,
      lower = mean - rule.sigma * deviation
    const above = rule.side !== 'below',
      below = rule.side !== 'above'
    test = (value) =>
      above && value >= upper ? 'upper' : below && value <= lower ? 'lower' : null
    zones = [...(above ? ['upper' as const] : []), ...(below ? ['lower' as const] : [])]
    const label = sigmaLabel(rule.sigma)
    thresholds = [
      ...(above ? [{ value: upper, label: `+${label}`, zone: 'upper' as const }] : []),
      ...(below ? [{ value: lower, label: `−${label}`, zone: 'lower' as const }] : []),
    ]
    summary = `${rule.side === 'both' ? '±' : rule.side === 'above' ? '+' : '−'}${label}`
  }
  const matched = new Map<number, HighlightZone>()
  for (const point of points) {
    const zone = test(point.value)
    if (zone) matched.set(point.time, zone)
  }
  const order: HighlightZone[] = ['upper', 'single', 'lower']
  return {
    ok: true,
    matched,
    zones: order.filter((zone) => zones.includes(zone)),
    thresholds,
    summary,
    periodMatches: period.filter((p) => matched.has(p.time)).length,
    periodDays: period.length,
    stats,
  }
}

/** Consecutive same-zone calendar indices as inclusive [first, last, zone] runs. */
export function highlightRuns(
  calendar: number[],
  matched: Map<number, HighlightZone>,
): [number, number, HighlightZone][] {
  const runs: [number, number, HighlightZone][] = []
  calendar.forEach((time, i) => {
    const zone = matched.get(time)
    if (!zone) return
    const last = runs.at(-1)
    if (last && last[1] === i - 1 && last[2] === zone) last[1] = i
    else runs.push([i, i, zone])
  })
  return runs
}

export function loadHighlightRules(): Record<string, HighlightRule> {
  try {
    const raw = JSON.parse(globalThis.localStorage?.getItem(HIGHLIGHT_STORAGE_KEY) ?? '{}')
    if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return {}
    return Object.fromEntries(
      Object.entries(raw as Record<string, unknown>)
        .filter(([title]) => bindings[title])
        .map(([title, rule]) => [title, normalizeHighlightRule(title, rule)]),
    )
  } catch {
    return {}
  }
}

export function saveHighlightRules(rules: Record<string, HighlightRule>) {
  try {
    globalThis.localStorage?.setItem(HIGHLIGHT_STORAGE_KEY, JSON.stringify(rules))
  } catch {
    // Storage can be unavailable or full; the choice still applies for this session.
  }
}
