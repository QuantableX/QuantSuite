import { defineStore } from 'pinia'
import { useEngine, runExclusive } from '#systems/composables/useEngine'
import { useAppStore } from '#systems/stores/app'
import { STALE_ENGINE } from '#systems/stores/backtest'
import { useConfigStore } from '#systems/stores/config'
import type { LiveResult, RunConfig } from '#systems/types'

interface LiveState {
  result: LiveResult | null
  loading: boolean
  error: string | null
  progressLabel: string
  progressValue: number
}

function emptyState(): LiveState {
  return { result: null, loading: false, error: null, progressLabel: '', progressValue: 0 }
}

export const useLiveStore = defineStore('systems/live', () => {
  const engine = useEngine()
  const app = useAppStore()
  const config = useConfigStore()

  /** One evaluation, with a single engine restart when the process does not
   *  know the configured indicator (it predates the Python code). */
  async function liveEvalFresh(systemId: string, cfg: RunConfig): Promise<LiveResult> {
    try {
      return await engine.liveEval(systemId, cfg)
    } catch (e) {
      if (!STALE_ENGINE.test(String(e))) throw e
    }
    setProgress('Restarting the engine', 0.02)
    try { await engine.stopEngine() } catch { /* already stopped */ }
    await engine.startEngine()
    await app.refreshEngineStatus()
    setProgress("Building today's coin list", 0.05)
    return engine.liveEval(systemId, cfg)
  }

  // Per-system live state, so each system keeps its own evaluation result.
  const states = reactive<Record<string, LiveState>>({})
  // Which system currently has a live eval in flight (for progress routing).
  const runningSystemId = ref<string | null>(null)

  function stateFor(systemId: string): LiveState {
    if (!states[systemId]) states[systemId] = emptyState()
    return states[systemId]
  }

  function setProgress(label: string, value: number) {
    const id = runningSystemId.value
    if (!id) return
    const s = stateFor(id)
    s.progressLabel = label
    s.progressValue = value
  }

  async function refresh(systemId: string) {
    const s = stateFor(systemId)
    if (s.loading) return
    s.loading = true
    s.error = null
    s.progressLabel = 'Queued — waiting for the current evaluation'
    s.progressValue = 0
    // Claim runningSystemId only once this eval is the one the engine is actually
    // working on — an eval still waiting in the queue would otherwise collect the
    // in-flight job's progress events.
    await runExclusive(async () => {
      runningSystemId.value = systemId
      s.progressLabel = "Building today's coin list"
      s.progressValue = 0.05
      try {
        s.result = await liveEvalFresh(systemId, config.get(systemId))
      } catch (e) {
        s.error = String(e)
      } finally {
        s.loading = false
        s.progressLabel = ''
        s.progressValue = 0
        if (runningSystemId.value === systemId) runningSystemId.value = null
      }
    })
  }

  function reset(systemId: string) {
    states[systemId] = emptyState()
  }

  return { states, runningSystemId, stateFor, setProgress, refresh, reset }
})
