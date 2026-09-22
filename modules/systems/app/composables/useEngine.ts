import { invoke } from '@tauri-apps/api/core'
import type {
  AppSettings,
  BacktestResult,
  CacheStats,
  EngineStatus,
  LiveResult,
  RunConfig,
  SystemMeta,
} from '#systems/types'

export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

let engineQueue: Promise<unknown> = Promise.resolve()

/**
 * Run an evaluation with the engine all to itself.
 *
 * The Python sidecar answers one request at a time and its `eval:progress`
 * notifications carry no job id, so overlapping runs cannot be told apart —
 * whoever routes the events would paint one job's ticks onto the other's bar.
 * Chaining every run through here keeps exactly one job in flight.
 */
export function runExclusive<T>(job: () => Promise<T>): Promise<T> {
  const next = engineQueue.then(job, job)
  engineQueue = next.catch(() => undefined)
  return next
}

/**
 * Thin typed wrappers around the Rust `invoke()` commands. Everything that
 * touches the Python engine flows through here.
 */
export function useEngine() {
  return {
    isTauri,

    // Systems & settings
    listSystems: () => invoke<SystemMeta[]>('plugin:systems|list_systems'),
    getSystemConfig: (systemId: string) => invoke<RunConfig>('plugin:systems|get_system_config', { systemId }),
    saveSystemConfig: (systemId: string, config: RunConfig) =>
      invoke<RunConfig>('plugin:systems|save_system_config', { systemId, config }),
    getAppSettings: () => invoke<AppSettings>('plugin:systems|get_app_settings'),
    updateAppSettings: (settings: AppSettings) =>
      invoke<AppSettings>('plugin:systems|update_app_settings', { settings }),

    // Engine lifecycle
    startEngine: () => invoke<EngineStatus>('plugin:systems|start_engine'),
    stopEngine: () => invoke<EngineStatus>('plugin:systems|stop_engine'),
    engineStatus: () => invoke<EngineStatus>('plugin:systems|engine_status'),

    // Evaluation
    liveEval: (systemId: string, config: RunConfig) =>
      invoke<LiveResult>('plugin:systems|live_eval', { systemId, config }),
    runBacktest: (systemId: string, config: RunConfig) =>
      invoke<BacktestResult>('plugin:systems|run_backtest', { systemId, config }),

    // Cache
    cacheStats: () => invoke<CacheStats>('plugin:systems|cache_stats'),
    clearCache: (scope: 'rankings' | 'ohlcv' | 'all') =>
      invoke<{ ok: boolean; scope: string }>('plugin:systems|clear_cache', { scope }),
  }
}
