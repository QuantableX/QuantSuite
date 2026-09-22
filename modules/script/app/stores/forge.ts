/**
 * The forge inside QuantScript (docs/PLAN-QUANTSCRIPT.md §2, moved from
 * QuantAlgo on 2026-09-12): the registry's roster (`plugin:algo|list_indicators`),
 * the vault listing (`plugin:algo|smithery_info`), the session's jobs
 * (`plugin:algo|smithery_jobs`, polled once a second while one runs and the
 * page is visible), reports, and the strategy QuantAlgo makes from an
 * indicator. The runner itself stays in QuantAlgo's crate; this store is
 * its one client.
 */
import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { computed, ref } from 'vue'
import { bus } from '@quantsuite/core'
import type {
  AlgoStrategy,
  ForgeJob,
  ForgeRequest,
  IndicatorInfo,
  IndicatorRegistry,
  SmitheryInfo,
  SmitheryReportMeta,
  SmitheryShelfSeries,
} from '#script/types'

const POLL_MS = 1000

export type ForgeTab = 'roster' | 'forge' | 'reports' | 'shelf'

export const useForgeStore = defineStore('script/forge', () => {
  // ── The roster ──
  const registry = ref<IndicatorRegistry | null>(null)
  const registryLoading = ref(false)
  const registryError = ref<string | null>(null)
  const indicators = computed<IndicatorInfo[]>(() => registry.value?.indicators ?? [])
  const certified = computed(() => indicators.value.filter((i) => i.certification?.certified))

  // ── Vault, shelf, reports ──
  const info = ref<SmitheryInfo | null>(null)
  const infoLoading = ref(false)
  const infoError = ref<string | null>(null)

  // ── Jobs ──
  const jobs = ref<ForgeJob[]>([])
  const detailJobId = ref<string | null>(null)
  const starting = ref(false)
  const runError = ref<string | null>(null)

  // ── Reports ──
  const reportMarkdown = ref<Record<string, string>>({})
  const reportLoading = ref<string | null>(null)
  const reportError = ref<string | null>(null)
  const selectedReport = ref<string | null>(null)

  // ── The page ──
  const tab = ref<ForgeTab>('roster')
  /** A request the Roster or the Shelf handed over — the Forge tab starts from it. */
  const draft = ref<ForgeRequest | null>(null)
  const creating = ref<string | null>(null)
  const createError = ref<string | null>(null)

  const shelf = computed<SmitheryShelfSeries[]>(() => info.value?.shelf ?? [])
  const reports = computed<SmitheryReportMeta[]>(() => info.value?.reports ?? [])
  const runningJob = computed(() => jobs.value.find((j) => j.status === 'running') ?? null)
  const isRunning = computed(() => runningJob.value !== null)
  const detailJob = computed<ForgeJob | null>(
    () =>
      runningJob.value ??
      jobs.value.find((j) => j.id === detailJobId.value) ??
      (jobs.value.length ? jobs.value[jobs.value.length - 1]! : null),
  )

  function latestReport(indicatorName: string): SmitheryReportMeta | null {
    return reports.value.find((r) => r.indicator === indicatorName) ?? null
  }

  async function loadRegistry(refresh = false) {
    registryLoading.value = true
    registryError.value = null
    try {
      registry.value = await invoke<IndicatorRegistry>('plugin:algo|list_indicators', { refresh })
    } catch (err) {
      registryError.value = String(err)
      console.error('[forge] Failed to load the registry:', err)
    } finally {
      registryLoading.value = false
    }
  }

  async function loadInfo(refresh = false) {
    infoLoading.value = true
    infoError.value = null
    try {
      info.value = await invoke<SmitheryInfo>('plugin:algo|smithery_info', { refresh })
    } catch (err) {
      infoError.value = String(err)
      console.error('[forge] Failed to read the vault:', err)
    } finally {
      infoLoading.value = false
    }
  }

  async function pollJobs() {
    try {
      jobs.value = await invoke<ForgeJob[]>('plugin:algo|smithery_jobs', { detail: detailJobId.value })
    } catch (err) {
      console.error('[forge] Failed to poll the forge:', err)
    }
  }

  async function run(request: ForgeRequest): Promise<ForgeJob | null> {
    starting.value = true
    runError.value = null
    try {
      const job = await invoke<ForgeJob>('plugin:algo|smithery_run', { request })
      detailJobId.value = job.id
      jobs.value = [...jobs.value.filter((j) => j.id !== job.id), job]
      startPolling()
      return job
    } catch (err) {
      runError.value = String(err)
      console.error('[forge] Failed to start a forge job:', err)
      return null
    } finally {
      starting.value = false
    }
  }

  async function cancel() {
    try {
      await invoke<ForgeJob | null>('plugin:algo|smithery_cancel')
    } catch (err) {
      runError.value = String(err)
      console.error('[forge] Failed to cancel the forge job:', err)
    }
    await pollJobs()
  }

  function showJob(id: string) {
    detailJobId.value = id
    void pollJobs()
  }

  async function loadReport(name: string) {
    selectedReport.value = name
    if (reportMarkdown.value[name]) return
    reportLoading.value = name
    reportError.value = null
    try {
      const doc = await invoke<{ name: string; file: string; markdown: string }>('plugin:algo|smithery_report', { name })
      reportMarkdown.value = { ...reportMarkdown.value, [name]: doc.markdown }
    } catch (err) {
      reportError.value = String(err)
      console.error('[forge] Failed to read a report:', err)
    } finally {
      reportLoading.value = null
    }
  }

  /** A report a job just wrote is new — drop what the cache had under that name. */
  function forgetReport(name: string) {
    if (name in reportMarkdown.value) {
      const next = { ...reportMarkdown.value }
      delete next[name]
      reportMarkdown.value = next
    }
  }

  /** The Roster / Shelf hand a request to the Forge tab. */
  function openForge(request: ForgeRequest) {
    draft.value = request
    tab.value = 'forge'
  }

  function openReport(name: string) {
    void loadReport(name)
    tab.value = 'reports'
  }

  /**
   * A RegimeTrend strategy in QuantAlgo for one indicator — QuantAlgo makes
   * it; the strategies page opens it via `?select=`.
   */
  async function createStrategy(key: string): Promise<AlgoStrategy | null> {
    creating.value = key
    createError.value = null
    try {
      const strategy = await invoke<AlgoStrategy>('plugin:algo|create_strategy_from_indicator', { indicator: key, name: null })
      window.dispatchEvent(new CustomEvent('qss:navigate', { detail: { route: `/algo/strategies?select=${encodeURIComponent(strategy.id)}` } }))
      return strategy
    } catch (err) {
      createError.value = String(err)
      console.error('[forge] Failed to create a strategy:', err)
      return null
    } finally {
      creating.value = null
    }
  }

  // ── Polling (V3 warm cache: only while the page is visible) ──
  let timer: ReturnType<typeof setInterval> | null = null
  let visible = false

  async function tick() {
    const wasRunning = isRunning.value
    await pollJobs()
    if (wasRunning && !isRunning.value) {
      // The run changed the vault (a report, the ledger, the shelf); the
      // backend reloaded its listing before it marked the job finished.
      for (const e of detailJob.value?.summary ?? []) {
        if (typeof e.report_name === 'string') forgetReport(e.report_name)
      }
      await loadInfo(false)
      await loadRegistry(true)
    }
    if (!isRunning.value || !visible) stopPolling()
  }

  function startPolling() {
    if (timer) return
    timer = setInterval(() => void tick(), POLL_MS)
  }

  function stopPolling() {
    if (timer) {
      clearInterval(timer)
      timer = null
    }
  }

  /** The page is on screen: catch up once, keep polling while a job runs. */
  async function resume() {
    visible = true
    await pollJobs()
    if (isRunning.value) startPolling()
  }

  function pause() {
    visible = false
    stopPolling()
  }

  // A script saved, created or deleted in the editor changes the registry
  // the roster shows.
  for (const topic of ['script.file.saved', 'script.file.created', 'script.file.deleted']) {
    bus.on(topic, () => {
      if (registry.value) void loadRegistry(true)
    })
  }

  return {
    registry,
    registryLoading,
    registryError,
    indicators,
    certified,
    info,
    infoLoading,
    infoError,
    jobs,
    detailJobId,
    detailJob,
    runningJob,
    isRunning,
    starting,
    runError,
    reportMarkdown,
    reportLoading,
    reportError,
    selectedReport,
    tab,
    draft,
    creating,
    createError,
    shelf,
    reports,
    latestReport,
    loadRegistry,
    loadInfo,
    pollJobs,
    run,
    cancel,
    showJob,
    loadReport,
    forgetReport,
    openForge,
    openReport,
    createStrategy,
    resume,
    pause,
  }
})
