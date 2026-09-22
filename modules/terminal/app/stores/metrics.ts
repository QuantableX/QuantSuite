import { defineStore } from 'pinia'
import { computed, onScopeDispose, ref, shallowReactive } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { bindings, type FeedSource, type History, type Point } from '#terminal/utils/metrics'
import { affectedMetrics } from '#terminal/utils/metrics-calculation'
import { createMetricsProcessor } from '#terminal/utils/metrics-processing'

export interface FeedState {
  data: History | null
  loading: boolean
  error: string | null
  attemptedAt: number | null
}
const TTL = 5 * 60 * 1000
const MAX_CONCURRENT_LOADS = 3
const REFRESH_COOLDOWN = 5_000

export const useMetricsStore = defineStore('terminal/metrics', () => {
  // Histories are immutable snapshots: proxying every daily row makes rendering expensive.
  const feeds = shallowReactive<Record<string, FeedState>>({})
  const series = shallowReactive<Record<string, Point[]>>({})
  const processor = createMetricsProcessor()
  const pending = new Map<string, Promise<void>>()
  const queue: Array<() => Promise<void>> = []
  let activeLoads = 0
  let disposed = false
  let refreshJob: Promise<PromiseSettledResult<void>[]> | null = null
  let cooldownTimer: ReturnType<typeof setTimeout> | null = null
  const refreshing = ref(false)
  const coolingDown = ref(false)
  const refreshCompleted = ref(0)
  const refreshTotal = ref(0)
  const now = ref(Date.now() / 1000)
  const selectedTitle = ref('MVRV Ratio')
  const comparisonAsset = ref('BTC')
  const rangeDays = ref(365)
  const feedKey = (source: FeedSource, asset = 'BTC') =>
    source === 'price' ? `price:${asset}` : source

  function state(source: FeedSource, asset = 'BTC'): FeedState {
    return (
      feeds[feedKey(source, asset)] ?? {
        data: null,
        loading: false,
        error: null,
        attemptedAt: null,
      }
    )
  }

  function metricState(title: string): FeedState {
    const binding = bindings[title]!
    const primary = state(binding.source)
    const inputs = [primary, ...(binding.joins ?? []).map((join) => state(join.source))]
    return {
      ...primary,
      loading: inputs.some((input) => input.loading),
      error:
        inputs
          .map((input) => input.error)
          .filter(Boolean)
          .join(' · ') || null,
      data: primary.data
        ? {
            ...primary.data,
            fetchedAt: Math.min(
              ...inputs.filter((input) => input.data).map((input) => input.data!.fetchedAt),
            ),
          }
        : null,
    }
  }

  function pump() {
    while (activeLoads < MAX_CONCURRENT_LOADS && queue.length) {
      activeLoads++
      // Let the page paint its loading state before starting work, including cache hits.
      const job = queue.shift()!
      setTimeout(() => void job(), 0)
    }
  }

  function load(source: FeedSource, asset = 'BTC', force = false): Promise<void> {
    if (disposed) return Promise.resolve()
    const key = feedKey(source, asset)
    if (pending.has(key)) return pending.get(key)!
    const previous = state(source, asset)
    if (!force && previous.attemptedAt && Date.now() - previous.attemptedAt < TTL)
      return Promise.resolve()
    feeds[key] = { ...previous, loading: true, error: null }
    let finish!: () => void
    const job = new Promise<void>((resolve) => {
      finish = resolve
    })
    // Register queued work too, so refresh/activation/asset changes cannot duplicate it.
    pending.set(key, job)
    queue.push(async () => {
      try {
        if (disposed) return
        const data = await invoke<History>('plugin:terminal|get_metric_history', { source, asset })
        if (disposed) return
        if (!data?.rows?.length || !Number.isFinite(data.fetchedAt))
          throw new Error('Provider returned no usable history')
        feeds[key] = { ...previous, data, loading: true, error: null }
        if (source !== 'price') {
          // Send only the feeds needed by the changed metrics, without reactive proxies.
          const sources = new Set(
            affectedMetrics(source).flatMap(([, binding]) => [
              binding.source,
              ...(binding.joins ?? []).map((join) => join.source),
            ]),
          )
          const histories = Object.fromEntries(
            [...sources].flatMap((input) =>
              feeds[input]?.data ? [[input, feeds[input]!.data]] : [],
            ),
          )
          const updated = await processor.calculate({
            source,
            histories,
            now: Date.now() / 1000,
          })
          if (disposed) return
          Object.assign(series, updated)
        }
        feeds[key] = {
          data,
          loading: false,
          error: null,
          attemptedAt: Date.now(),
        }
      } catch (error) {
        if (disposed) return
        const message = error instanceof Error ? error.message : String(error)
        feeds[key] = {
          ...previous,
          loading: false,
          attemptedAt: Date.now(),
          error: /not found|__TAURI|reading 'invoke'/i.test(message)
            ? 'History service unavailable. Start the updated QuantSuite desktop app.'
            : message,
        }
      } finally {
        if (disposed) feeds[key] = { ...previous, loading: false }
        pending.delete(key)
        now.value = Date.now() / 1000
        activeLoads--
        finish()
        pump()
      }
    })
    pump()
    return job
  }

  const loading = computed(() => refreshing.value || Object.values(feeds).some((f) => f.loading))
  const canRefresh = computed(() => !loading.value && !coolingDown.value)
  const availableCount = computed(() => Object.values(series).filter((p) => p.length).length)
  function refresh(force = false) {
    // Lock the whole refresh, including feeds that finish before slower providers.
    if (refreshJob) return refreshJob
    if (disposed || (force && coolingDown.value)) return Promise.resolve([])
    refreshing.value = true
    now.value = Date.now() / 1000
    const selected = bindings[selectedTitle.value]
    const sources = [
      ...new Set([
        // The visible chart and its BTC comparison take priority over the catalogue.
        ...(selected
          ? [selected.source, ...(selected.joins ?? []).map((join) => join.source)]
          : []),
        'network' as FeedSource,
        ...Object.values(bindings).flatMap((binding) => [
          binding.source,
          ...(binding.joins ?? []).map((join) => join.source),
        ]),
      ]),
    ]
    refreshCompleted.value = 0
    refreshTotal.value = sources.length + (comparisonAsset.value === 'BTC' ? 0 : 1)
    refreshJob = Promise.resolve()
      .then(() => {
        const jobs =
          comparisonAsset.value === 'BTC'
            ? []
            : [
                load(
                  'price',
                  comparisonAsset.value === 'BTC-SPOT' ? 'BTC' : comparisonAsset.value,
                  force,
                ),
              ]
        jobs.push(...sources.map((source) => load(source, 'BTC', force)))
        return Promise.allSettled(
          jobs.map((job) =>
            job.finally(() => {
              refreshCompleted.value++
            }),
          ),
        )
      })
      .finally(() => {
        refreshJob = null
        refreshing.value = false
        if (force && !disposed) {
          coolingDown.value = true
          cooldownTimer = setTimeout(() => {
            coolingDown.value = false
          }, REFRESH_COOLDOWN)
        }
      })
    return refreshJob
  }

  onScopeDispose(() => {
    disposed = true
    processor.dispose()
    if (cooldownTimer) clearTimeout(cooldownTimer)
  })

  return {
    feeds,
    now,
    selectedTitle,
    comparisonAsset,
    rangeDays,
    series,
    loading,
    canRefresh,
    refreshing,
    refreshCompleted,
    refreshTotal,
    availableCount,
    state,
    metricState,
    load,
    refresh,
  }
})
