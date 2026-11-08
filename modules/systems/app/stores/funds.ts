import { defineStore } from 'pinia'
import { useEngine, runExclusive } from '#systems/composables/useEngine'
import { useAppStore } from '#systems/stores/app'
import type { Fund, FundResult } from '#systems/types'

/** Manual funds: their list and, per fund and page, the last result. A result stays until the next run. */
export type FundMode = 'backtest' | 'live'

interface FundRunState {
  result: FundResult | null
  isRunning: boolean
  error: string | null
  progressLabel: string
  progressValue: number
}

/** An engine process from before funds exists answers this way; one restart loads the current code. */
const STALE_ENGINE = /Unknown method: fund|restart the engine process/i

export function emptyFund(): Fund {
  return {
    id: '',
    name: '',
    amount: 500,
    members: [],
    method: 'equal',
    lookbackDays: 180,
    rebalance: 'monthly',
    capPct: 50,
    kellyFraction: 0.25,
    fixedPct: {},
    liveStartDate: new Date().toISOString().slice(0, 10),
  }
}

export const useFundsStore = defineStore('systems/funds', () => {
  const engine = useEngine()
  const app = useAppStore()
  const funds = ref<Fund[]>([])
  const loaded = ref(false)
  const error = ref<string | null>(null)
  const states = reactive<Record<string, FundRunState>>({})
  const runningKey = ref<string | null>(null)

  function byId(id: string) {
    return funds.value.find(f => f.id === id)
  }

  async function load(force = false) {
    if (loaded.value && !force) return
    try {
      funds.value = await engine.listFunds()
      loaded.value = true
      error.value = null
    } catch (e) {
      error.value = String(e)
    }
  }

  async function save(fund: Fund) {
    const saved = await engine.saveFund(fund)
    const at = funds.value.findIndex(f => f.id === saved.id)
    if (at >= 0) funds.value[at] = saved
    else funds.value.push(saved)
    return saved
  }

  async function remove(id: string) {
    await engine.deleteFund(id)
    funds.value = funds.value.filter(f => f.id !== id)
    for (const mode of ['backtest', 'live'] as const) delete states[`${id}:${mode}`]
  }

  function stateFor(id: string, mode: FundMode): FundRunState {
    const key = `${id}:${mode}`
    if (!states[key]) states[key] = { result: null, isRunning: false, error: null, progressLabel: '', progressValue: 0 }
    return states[key]
  }

  function setProgress(label: string, value: number) {
    const key = runningKey.value
    if (!key || !states[key]) return
    states[key].progressLabel = label
    states[key].progressValue = value
  }

  async function runFresh(id: string, mode: FundMode) {
    try {
      return await engine.runFund(id, mode === 'live')
    } catch (e) {
      if (!STALE_ENGINE.test(String(e))) throw e
    }
    setProgress('Restarting the engine', 0.02)
    try { await engine.stopEngine() } catch { /* already stopped */ }
    await engine.startEngine()
    await app.refreshEngineStatus()
    return engine.runFund(id, mode === 'live')
  }

  async function run(id: string, mode: FundMode) {
    const state = stateFor(id, mode)
    if (state.isRunning) return
    state.isRunning = true
    state.error = null
    state.progressLabel = 'Queued — waiting for the current evaluation'
    state.progressValue = 0
    await runExclusive(async () => {
      runningKey.value = `${id}:${mode}`
      state.progressLabel = 'Starting'
      state.progressValue = 0.02
      try {
        state.result = await runFresh(id, mode)
      } catch (e) {
        state.error = String(e)
      } finally {
        state.isRunning = false
        state.progressLabel = ''
        state.progressValue = 0
        runningKey.value = null
      }
    })
  }

  return { funds, loaded, error, states, runningKey, byId, load, save, remove, stateFor, setProgress, run }
})
