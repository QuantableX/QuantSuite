/**
 * The Smithery catalog as indicator-picker rows — pure, so the tests can run
 * it without the store (scripts/test-systems-queue.mjs).
 */
import type { SmitheryCatalogEntry, SmitheryVerdict, SmitheryVersionRole } from '@quantsuite/core'
import type { IndicatorOption, IndicatorVersionOption, TrendKind } from '#systems/types'

// The version slots of every indicator (smithery/variants.py).
const VERSION_SLOTS: { role: SmitheryVersionRole; short: string; label: string; track: string | null }[] = [
  { role: 'standard', short: 'S', label: 'Standard', track: null },
  { role: 'optimized', short: 'G', label: 'General', track: null },
  { role: 'optimized_1h', short: '1H', label: 'Optimized 1H', track: '1h' },
  { role: 'optimized_4h', short: '4H', label: 'Optimized 4H', track: '4h' },
  { role: 'optimized_1d', short: '1D', label: 'Optimized 1D', track: '1d' },
]

/**
 * The Smithery catalog as picker rows scored on one track: one row per base
 * indicator with its versions (the version keys stay the values — a config
 * stores `dcl_opt`, never a base plus a role), research subversions and
 * legacy keys as rows of their own. Best first.
 */
export function indicatorOptionRows(catalog: SmitheryCatalogEntry[], track: string): IndicatorOption[] {
  const byKey = new Map(catalog.map(ind => [ind.key, ind]))
  const scored = (verdict: SmitheryVerdict | null | undefined) => ({
    score: verdict ? +verdict.score.toFixed(1) : null,
    grade: verdict?.grade ?? null,
    tag: verdict?.certified ? `${track} ${verdict.source === 'historical' ? 'reference' : 'certified'}` : 'research',
  })
  const rows: IndicatorOption[] = []
  for (const ind of catalog) {
    if (ind.variant) {
      if (ind.role || ind.variant.role) continue
      const base = byKey.get(ind.variant.base_key)?.name ?? ind.variant.base_key
      rows.push({ value: ind.key as TrendKind, name: `${base} › ${ind.variant.label}`, ...scored(ind.timeframes?.[track]) })
      continue
    }
    const versions: IndicatorVersionOption[] = VERSION_SLOTS.flatMap((slot) => {
      const key = ind.versions?.[slot.role] ?? (slot.role === 'standard' ? ind.key : null)
      const entry = key ? byKey.get(key) : undefined
      if (!key || !entry) return []
      const verdict = entry.timeframes?.[track]
      // A timeframe version is only measured on its own track.
      return [{ value: key as TrendKind, role: slot.role, short: slot.short, label: slot.label, matches: slot.track === track,
        ...scored(verdict), ...(verdict ? {} : { tag: `no ${track} verdict` }) }]
    })
    const preferred = versions.find(v => v.matches) ?? versions.find(v => v.role === 'optimized') ?? versions[0]
    if (!preferred) continue
    rows.push({
      value: preferred.value, name: ind.name, score: preferred.score, grade: preferred.grade, tag: preferred.tag,
      ...(versions.length > 1 ? { versions } : {}),
    })
  }
  return rows.sort((a, b) => (b.score ?? -1) - (a.score ?? -1))
}

/** The row a value belongs to — its own row, or the base row of a version. */
export function findIndicatorOption(options: IndicatorOption[], value: string): IndicatorOption | undefined {
  return options.find(o => o.value === value && !o.versions) ?? options.find(o => o.versions?.some(v => v.value === value))
}
