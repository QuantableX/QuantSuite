import { defineStore } from 'pinia'
import { useEngine, runExclusive } from '#systems/composables/useEngine'
import { useAppStore } from '#systems/stores/app'
import { useConfigStore } from '#systems/stores/config'
import type { BacktestResult, RunConfig, StrategyRun } from '#systems/types'

/** The engine's own wording when a run names a trend kind its process has
 *  not loaded — it predates the indicator (the Python sidecar stays alive
 *  across code changes). */
// A pre-fix process also exposes the bare pandas missing-volume KeyError.
// The current engine reports this as a skipped variant, so reload it once.
export const STALE_ENGINE = /restart the engine process|^(?:KeyError:\s*)?['"]volume['"]$/i

/** Every result carries its runs: an engine process from before the
 *  comparison feature returns only the scalar fields, which are the
 *  configured indicator's run. */
function withStrategies(result: BacktestResult, cfg: RunConfig): BacktestResult {
  if (result.strategies !== undefined) return result
  const { trend, emaCross, aggregate } = cfg.indicator
  const primary: StrategyRun = {
    key: trend,
    label: trend === 'ema_cross'
      ? `EMA ${emaCross.fastLength}/${emaCross.slowLength}`
      : trend === 'aggregate' ? `Aggregate (${(aggregate ?? []).length})` : trend,
    equityStrategy: result.equityStrategy,
    heldAsset: result.heldAsset,
    metricsStrategy: result.metricsStrategy,
    forcedRotations: result.forcedRotations,
  }
  return { ...result, strategies: [primary] }
}

interface BacktestState {
  result: BacktestResult | null
  isRunning: boolean
  error: string | null
  progressLabel: string
  progressValue: number
  notes: string[]
}

function emptyState(): BacktestState {
  return { result: null, isRunning: false, error: null, progressLabel: '', progressValue: 0, notes: [] }
}

export const useBacktestStore = defineStore('systems/backtest', () => {
  const engine = useEngine()
  const app = useAppStore()
  const config = useConfigStore()

  /** Restart the engine process so it loads the current Python code. */
  async function restartEngine() {
    setProgress('Restarting the engine', 0.02)
    try { await engine.stopEngine() } catch { /* already stopped */ }
    await engine.startEngine()
    await app.refreshEngineStatus()
  }

  /**
   * One backtest, with a single engine restart when the process turns out
   * to predate what the run asks for: an indicator it does not know (the
   * engine says so), or compared indicators it silently ignored (no
   * `strategies` in the result).
   */
  async function runBacktestFresh(systemId: string, cfg: RunConfig): Promise<BacktestResult> {
    try {
      const raw = await engine.runBacktest(systemId, cfg)
      if (!cfg.compareTrends.length || raw.strategies) return raw
    } catch (e) {
      if (!STALE_ENGINE.test(String(e))) throw e
    }
    await restartEngine()
    setProgress('Starting backtest', 0.03)
    return engine.runBacktest(systemId, cfg)
  }

  // Per-system backtest state, so each system keeps its own result.
  const states = reactive<Record<string, BacktestState>>({})
  // Which system currently has a backtest in flight (for progress routing).
  const runningSystemId = ref<string | null>(null)

  function stateFor(systemId: string): BacktestState {
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

  async function run(systemId: string) {
    const s = stateFor(systemId)
    if (s.isRunning) return
    s.isRunning = true
    s.error = null
    s.notes = []
    s.progressLabel = 'Queued — waiting for the current evaluation'
    s.progressValue = 0
    // Claim runningSystemId only once this run is the one the engine is actually
    // working on — a run still waiting in the queue would otherwise collect the
    // in-flight run's progress events.
    await runExclusive(async () => {
      runningSystemId.value = systemId
      s.progressLabel = 'Starting backtest'
      s.progressValue = 0.02
      try {
        const cfg = config.get(systemId)
        const raw = await runBacktestFresh(systemId, cfg)
        s.result = withStrategies(raw, cfg)
        s.notes = [...(raw.notes ?? [])]
        if (cfg.compareTrends.length && !raw.strategies) {
          s.notes.push(
            'The engine ran without indicator comparison even after a restart; only the configured indicator ran.',
          )
        }
      } catch (e) {
        s.error = String(e)
      } finally {
        s.isRunning = false
        s.progressLabel = ''
        s.progressValue = 0
        if (runningSystemId.value === systemId) runningSystemId.value = null
      }
    })
  }

  function reset(systemId: string) {
    states[systemId] = emptyState()
  }

  return { states, runningSystemId, stateFor, setProgress, run, reset }
})
