import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { ref } from 'vue'
import type {
  BacktestConfig,
  BacktestResult,
  BacktestSummary,
  BacktestProgressEvent,
  BacktestCompleteEvent,
} from '#algo/types'

export const useBacktestStore = defineStore('algo/backtest', () => {
  // ── State ──

  const backtests = ref<BacktestSummary[]>([])
  const activeBacktestId = ref<string | null>(null)
  const currentResult = ref<BacktestResult | null>(null)
  const isRunning = ref(false)
  const progress = ref<{ pct: number; message: string }>({ pct: 0, message: '' })
  const comparisonIds = ref<string[]>([])
  const comparisonResults = ref<BacktestResult[]>([])
  const isLoading = ref(false)

  const isInitialized = ref(false)
  const unlistenHandlers = ref<UnlistenFn[]>([])

  // ── Actions ──

  async function run(config: BacktestConfig): Promise<BacktestResult> {
    isRunning.value = true
    progress.value = { pct: 0, message: 'Starting backtest...' }
    currentResult.value = null

    try {
      const result = await invoke<BacktestResult>('plugin:algo|run_backtest', { config })
      currentResult.value = result
      activeBacktestId.value = result.id
      // Refresh list to include the new result
      await load()
      return result
    } catch (err) {
      console.error('[backtest store] Failed to run backtest:', err)
      throw err
    } finally {
      isRunning.value = false
      progress.value = { pct: 100, message: 'Complete' }
    }
  }

  async function load() {
    isLoading.value = true
    try {
      backtests.value = await invoke<BacktestSummary[]>('plugin:algo|list_backtests')
    } catch (err) {
      console.error('[backtest store] Failed to load backtests:', err)
    } finally {
      isLoading.value = false
    }
  }

  async function get(id: string): Promise<BacktestResult> {
    try {
      const result = await invoke<BacktestResult>('plugin:algo|get_backtest', { id })
      currentResult.value = result
      activeBacktestId.value = id
      return result
    } catch (err) {
      console.error('[backtest store] Failed to get backtest:', err)
      throw err
    }
  }

  async function save(result: BacktestResult, name: string) {
    try {
      await invoke('plugin:algo|save_backtest', { result, name })
      await load()
    } catch (err) {
      console.error('[backtest store] Failed to save backtest:', err)
      throw err
    }
  }

  async function remove(id: string) {
    try {
      await invoke('plugin:algo|delete_backtest', { id })
      if (activeBacktestId.value === id) {
        activeBacktestId.value = null
        currentResult.value = null
      }
      comparisonIds.value = comparisonIds.value.filter((cid) => cid !== id)
      comparisonResults.value = comparisonResults.value.filter((r) => r.id !== id)
      await load()
    } catch (err) {
      console.error('[backtest store] Failed to delete backtest:', err)
      throw err
    }
  }

  async function compare(ids: string[]) {
    comparisonIds.value = ids
    comparisonResults.value = []
    const results = await Promise.allSettled(
      ids.map((id) => invoke<BacktestResult>('plugin:algo|get_backtest', { id })),
    )
    for (const result of results) {
      if (result.status === 'fulfilled') {
        comparisonResults.value.push(result.value)
      } else {
        console.error('[backtest store] Failed to load comparison backtest:', result.reason)
      }
    }
  }

  // ── Event subscriptions ──

  async function init() {
    if (isInitialized.value) return
    isInitialized.value = true

    try {
      const unlistenProgress = await listen<BacktestProgressEvent>(
        'backtest:progress',
        (event) => {
          progress.value = {
            pct: event.payload.pct,
            message: event.payload.message,
          }
        },
      )

      const unlistenComplete = await listen<BacktestCompleteEvent>(
        'backtest:complete',
        (event) => {
          currentResult.value = event.payload.result
          activeBacktestId.value = event.payload.result.id
          isRunning.value = false
          progress.value = { pct: 100, message: 'Complete' }
          // Refresh list in background
          load()
        },
      )

      unlistenHandlers.value = [unlistenProgress, unlistenComplete]
    } catch (err) {
      console.error('[backtest store] Failed to initialize event listeners:', err)
    }
  }

  function dispose() {
    for (const unlisten of unlistenHandlers.value) {
      unlisten()
    }
    unlistenHandlers.value = []
    isInitialized.value = false
  }

  return {
    // State
    backtests,
    activeBacktestId,
    currentResult,
    isRunning,
    progress,
    comparisonIds,
    comparisonResults,
    isLoading,
    // Actions
    run,
    load,
    get,
    save,
    delete: remove,
    compare,
    init,
    dispose,
  }
})
