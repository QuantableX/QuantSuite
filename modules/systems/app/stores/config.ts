import { defineStore } from 'pinia'
import { useEngine } from '#systems/composables/useEngine'
import type { RunConfig } from '#systems/types'

function localIso(d: Date): string {
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  return `${y}-${m}-${day}`
}

function isoDaysAgo(days: number): string {
  const d = new Date()
  d.setDate(d.getDate() - days)
  return localIso(d)
}

export function isoToday(): string {
  return localIso(new Date())
}

export function defaultRunConfig(systemId?: string): RunConfig {
  const sces = systemId === 'sces'
  return {
    topN: 100,
    excludeTopN: sces ? 5 : 0,
    cadence: 'daily',
    startDate: isoDaysAgo(365 * 3),
    endDate: isoToday(),
    rankingSource: 'auto',
    excludeStablecoins: true,
    excludeWrapped: true,
    includeUsd: true,
    feeRate: 0.001,
    slippageRate: 0.0,
    minRequestInterval: 1.2,
    indicator: { trend: 'ema_cross', emaCross: { src: 'close', fastLength: 12, slowLength: 21 }, aggregate: [] },
    compareTrends: [],
    marketFilter: false,
    marketIndicator: null,
  }
}

export const useConfigStore = defineStore('systems/config', () => {
  const engine = useEngine()
  const configBySystem = ref<Record<string, RunConfig>>({})
  // Systems whose config was already loaded this app run. On the first load
  // after app start, endDate resets to today (ignoring the persisted value);
  // afterwards the session value wins so a user-picked end date sticks.
  const loadedThisSession = new Set<string>()

  function get(systemId: string): RunConfig {
    return configBySystem.value[systemId] ?? defaultRunConfig(systemId)
  }

  async function load(systemId: string) {
    const firstLoad = !loadedThisSession.has(systemId)
    try {
      const cfg = await engine.getSystemConfig(systemId)
      const defaults = defaultRunConfig(systemId)
      configBySystem.value[systemId] = {
        ...defaults,
        ...cfg,
        endDate: firstLoad
          ? isoToday()
          : configBySystem.value[systemId]?.endDate ?? isoToday(),
        // deep-merge: configs saved before the trend selector existed
        // have an indicator object without `trend`
        indicator: { ...defaults.indicator, ...cfg.indicator },
      }
      loadedThisSession.add(systemId)
    } catch {
      if (!configBySystem.value[systemId]) {
        configBySystem.value[systemId] = defaultRunConfig(systemId)
      }
    }
  }

  function update(systemId: string, patch: Partial<RunConfig>) {
    const current = get(systemId)
    configBySystem.value[systemId] = { ...current, ...patch }
  }

  async function save(systemId: string) {
    const cfg = get(systemId)
    try {
      configBySystem.value[systemId] = await engine.saveSystemConfig(systemId, cfg)
    } catch {
      /* not in Tauri — keep local state */
    }
  }

  return { configBySystem, get, load, update, save }
})
