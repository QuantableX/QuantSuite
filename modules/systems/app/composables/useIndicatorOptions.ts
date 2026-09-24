import { useSystemsIndicatorsStore } from '#systems/stores/indicators'
import { findIndicatorOption, indicatorOptionRows } from '#systems/utils/indicatorOptions'
import type { Cadence, IndicatorOption, RunConfig } from '#systems/types'
import { effectiveCadence } from '#systems/utils/systemMode'

export const EMA_CROSS_OPTION: IndicatorOption = {
  value: 'ema_cross', name: 'EMA Band Cross', score: null, grade: null,
}

/** The Smithery track whose verdict applies to a cadence. LCES runs daily
 *  candles for daily/weekly/monthly rotation, so a daily signal must not
 *  inherit its hourly score; 12h has no certification track yet. */
export function scoreTrackFor(cadence: Cadence): string {
  return ['1h', '4h', '12h'].includes(cadence) ? cadence : '1d'
}

/** The display name of the aggregate for a member count. */
export function aggregateName(members: number): string {
  return `Aggregate (${members})`
}

/**
 * The rows of every indicator picker for one run configuration.
 *
 * `options`: the EMA cross first, then the Smithery catalog scored on the
 * cadence's track, best first, one row per base indicator with its versions
 * — the members an aggregate or a comparison can be built from. The configured indicator is always present, as a
 * "not loaded" row when the catalog does not know it (an engine restart
 * away). `trendOptions` adds the Aggregate row on top for the run's own
 * indicator; `compareOptions` adds it only once it has members to average.
 */
export function useIndicatorOptions(cfg: () => RunConfig) {
  const catalog = useSystemsIndicatorsStore()
  onMounted(() => { void catalog.load() })

  const scoreTrack = computed(() => scoreTrackFor(effectiveCadence(cfg())))

  const options = computed<IndicatorOption[]>(() => {
    const rows = indicatorOptionRows(catalog.indicators, scoreTrack.value)
    const trend = cfg().indicator.trend ?? 'ema_cross'
    if (trend !== 'ema_cross' && trend !== 'aggregate' && !findIndicatorOption(rows, trend)) {
      rows.unshift({ value: trend, name: trend, score: null, grade: null, tag: 'not loaded' })
    }
    return [EMA_CROSS_OPTION, ...rows]
  })

  const members = computed(() => cfg().indicator.aggregate ?? [])

  const aggregateOption = computed<IndicatorOption>(() => ({
    value: 'aggregate',
    name: aggregateName(members.value.length),
    score: null,
    grade: null,
    tag: members.value.length ? undefined : 'pick members',
  }))

  const trendOptions = computed<IndicatorOption[]>(() => [aggregateOption.value, ...options.value])

  const compareOptions = computed<IndicatorOption[]>(() =>
    members.value.length ? [aggregateOption.value, ...options.value] : options.value,
  )

  return { catalog, scoreTrack, options, trendOptions, compareOptions }
}
