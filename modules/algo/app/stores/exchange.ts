import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { ref, computed } from 'vue'
import type { Exchange, ExchangeConfig, ConnectionResult, Balance } from '#algo/types'
import { capitalize } from '#algo/utils/format'

/** A command that never answers (a panic in the backend) must surface as an
 * error, not as a spinner that never ends. */
const PAIRS_TIMEOUT_MS = 45_000

function withTimeout<T>(promise: Promise<T>, ms: number, message: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(message)), ms)
    promise.then(
      (value) => { clearTimeout(timer); resolve(value) },
      (err) => { clearTimeout(timer); reject(err) },
    )
  })
}

export const useExchangeStore = defineStore('algo/exchange', () => {
  // ── State ──

  const exchanges = ref<Exchange[]>([])
  const activeExchangeId = ref<string | null>(null)
  const balances = ref<Balance[]>([])
  /** The pairs of the exchange loaded last — what `loadPairs` returned. */
  const pairs = ref<string[]>([])
  /**
   * Pairs per connected exchange, as the exchange's API listed them. Every
   * pair dropdown in the module reads from here (PLAN-QUANTALGO §5); the
   * backend keeps its own 15-minute cache per provider, this one only saves
   * the round trip while the same exchange stays selected.
   */
  const pairsByExchange = ref<Record<string, string[]>>({})
  const isLoading = ref(false)

  // ── Getters ──

  const activeExchange = computed(() => {
    if (!activeExchangeId.value) return null
    return exchanges.value.find((e) => e.id === activeExchangeId.value) ?? null
  })

  function byId(id: string | null | undefined): Exchange | null {
    if (!id) return null
    return exchanges.value.find((e) => e.id === id) ?? null
  }

  /**
   * What a `trades.exchange` value means to the user: the connected exchange's
   * name when it is an id, else the provider name the old backtest rows carry.
   */
  function label(value: string | null | undefined): string {
    if (!value) return '--'
    const exchange = byId(value)
    if (exchange) return `${exchange.name} (${capitalize(exchange.provider)})`
    return capitalize(value)
  }

  // ── Actions ──

  async function load() {
    isLoading.value = true
    try {
      exchanges.value = await invoke<Exchange[]>('plugin:algo|list_exchanges')
    } catch (err) {
      console.error('[exchange store] Failed to load exchanges:', err)
    } finally {
      isLoading.value = false
    }
  }

  async function add(config: ExchangeConfig): Promise<Exchange> {
    try {
      const exchange = await invoke<Exchange>('plugin:algo|add_exchange', { config })
      await load()
      return exchange
    } catch (err) {
      console.error('[exchange store] Failed to add exchange:', err)
      throw err
    }
  }

  async function update(id: string, config: Partial<ExchangeConfig>) {
    try {
      await invoke('plugin:algo|update_exchange', { id, config })
      delete pairsByExchange.value[id]
      await load()
    } catch (err) {
      console.error('[exchange store] Failed to update exchange:', err)
      throw err
    }
  }

  async function remove(id: string) {
    try {
      await invoke('plugin:algo|delete_exchange', { id })
      if (activeExchangeId.value === id) {
        activeExchangeId.value = null
        balances.value = []
        pairs.value = []
      }
      delete pairsByExchange.value[id]
      await load()
    } catch (err) {
      console.error('[exchange store] Failed to delete exchange:', err)
      throw err
    }
  }

  /** Test the credentials stored for a connected exchange. */
  async function testConnection(id: string): Promise<ConnectionResult> {
    try {
      return await invoke<ConnectionResult>('plugin:algo|test_exchange_connection', { id })
    } catch (err) {
      console.error('[exchange store] Failed to test connection:', err)
      throw err
    }
  }

  /** Test credentials that are still in the form — nothing is saved. */
  async function testCredentials(config: ExchangeConfig): Promise<ConnectionResult> {
    try {
      return await invoke<ConnectionResult>('plugin:algo|test_exchange_credentials', { config })
    } catch (err) {
      console.error('[exchange store] Failed to test credentials:', err)
      throw err
    }
  }

  async function refreshBalances(exchangeId: string) {
    try {
      balances.value = await invoke<Balance[]>('plugin:algo|get_balances', { exchangeId })
    } catch (err) {
      console.error('[exchange store] Failed to refresh balances:', err)
      throw err
    }
  }

  /**
   * The pairs a connected exchange lists, from its API. Cached per exchange
   * for the session; `force` refetches.
   */
  async function loadPairs(exchangeId: string, force = false): Promise<string[]> {
    const cached = pairsByExchange.value[exchangeId]
    if (cached && !force) {
      pairs.value = cached
      return cached
    }
    try {
      const result = await withTimeout(
        invoke<string[]>('plugin:algo|get_exchange_pairs', { exchangeId }),
        PAIRS_TIMEOUT_MS,
        'The exchange did not answer the pair request in time',
      )
      pairsByExchange.value = { ...pairsByExchange.value, [exchangeId]: result }
      pairs.value = result
      return result
    } catch (err) {
      console.error('[exchange store] Failed to load pairs:', err)
      throw err
    }
  }

  function setActive(id: string | null) {
    activeExchangeId.value = id
    if (!id) {
      balances.value = []
      pairs.value = []
    }
  }

  return {
    // State
    exchanges,
    activeExchangeId,
    balances,
    pairs,
    pairsByExchange,
    isLoading,
    // Getters
    activeExchange,
    byId,
    label,
    // Actions
    load,
    add,
    update,
    delete: remove,
    testConnection,
    testCredentials,
    refreshBalances,
    loadPairs,
    setActive,
  }
})
