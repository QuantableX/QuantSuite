import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { ref, computed } from 'vue'
import type { Trade, TradeStats, TradeFilters, TradeFacets, TradeStatsBreakdown, DailyPnl, TradingMode } from '#algo/types'

/** The heatmaps' window: the last year of local days. */
const HEATMAP_DAYS = 365

function yearAgoBound(): string {
  const d = new Date()
  d.setDate(d.getDate() - HEATMAP_DAYS)
  d.setHours(0, 0, 0, 0)
  return `${d.toISOString().slice(0, 19)}+00:00`
}

export const useJournalStore = defineStore('algo/journal', () => {
  // ── State ──

  const trades = ref<Trade[]>([])
  const stats = ref<TradeStats | null>(null)
  /** The same statistics split by mode and by bot, under the same filters. */
  const breakdown = ref<TradeStatsBreakdown | null>(null)
  /** A year of realised PnL per local day, paper and live apart (the heatmaps). */
  const dailyPnl = ref<{ paper: DailyPnl[]; live: DailyPnl[] }>({ paper: [], live: [] })
  /** The exchange and pair values that occur in the journal — the filter dropdowns' options. */
  const facets = ref<TradeFacets>({ exchanges: [], pairs: [] })
  // The page always shows one mode (the Paper | Live switch); never both mixed.
  const filters = ref<TradeFilters>({
    limit: 50,
    offset: 0,
    trading_mode: 'paper',
  })
  const mode = computed<TradingMode>(() => (filters.value.trading_mode === 'live' ? 'live' : 'paper'))
  const isLoading = ref(false)
  const totalCount = ref(0)

  // Two filter changes inside one slow list_trades would otherwise let the
  // first response win because it resolved last — the table would show the
  // previous filter's rows. Each load carries a token and only writes while it
  // is still the newest one.
  let tradesToken = 0
  let statsToken = 0
  let breakdownToken = 0
  let dailyToken = 0

  // ── Actions ──

  async function loadTrades() {
    const token = ++tradesToken
    isLoading.value = true
    try {
      const result = await invoke<Trade[]>('plugin:algo|list_trades', { filters: filters.value })
      if (token !== tradesToken) return
      trades.value = result
    } catch (err) {
      console.error('[journal store] Failed to load trades:', err)
    } finally {
      // A superseded load must not clear the flag the newer one set.
      if (token === tradesToken) isLoading.value = false
    }
  }

  async function loadStats() {
    const token = ++statsToken
    try {
      const result = await invoke<TradeStats>('plugin:algo|get_trade_stats', { filters: filters.value })
      if (token !== statsToken) return
      stats.value = result
    } catch (err) {
      console.error('[journal store] Failed to load trade stats:', err)
    }
  }

  async function loadBreakdown() {
    const token = ++breakdownToken
    try {
      // The bot table shows every bot whatever the switch says — a bot has one mode anyway.
      const { trading_mode: _mode, ...rest } = filters.value
      const result = await invoke<TradeStatsBreakdown>('plugin:algo|get_trade_stats_breakdown', { filters: rest })
      if (token !== breakdownToken) return
      breakdown.value = result
    } catch (err) {
      console.error('[journal store] Failed to load the statistics breakdown:', err)
    }
  }

  // The heatmaps keep the bot / exchange / pair / strategy / side filters but
  // always show the last year, one calendar per mode.
  async function loadDailyPnl() {
    const token = ++dailyToken
    const { limit: _limit, offset: _offset, trading_mode: _mode, from_date: _from, to_date: _to, exited_from: _ef, exited_to: _et, ...rest } = filters.value
    const tzOffsetMinutes = -new Date().getTimezoneOffset()
    const load = (mode: 'paper' | 'live') =>
      invoke<DailyPnl[]>('plugin:algo|get_daily_pnl', {
        filters: { ...rest, is_backtest: false, trading_mode: mode, exited_from: yearAgoBound() },
        tzOffsetMinutes,
      })
    try {
      const [paper, live] = await Promise.all([load('paper'), load('live')])
      if (token !== dailyToken) return
      dailyPnl.value = { paper, live }
    } catch (err) {
      console.error('[journal store] Failed to load the daily PnL:', err)
    }
  }

  async function loadFacets() {
    try {
      facets.value = await invoke<TradeFacets>('plugin:algo|list_trade_facets')
    } catch (err) {
      console.error('[journal store] Failed to load trade facets:', err)
    }
  }

  function setMode(next: TradingMode) {
    if (mode.value !== next) void updateFilters({ trading_mode: next })
  }

  async function updateFilters(partial: Partial<TradeFilters>) {
    // Reset offset when filters change (unless offset itself is being set)
    if (partial.offset === undefined) {
      filters.value = { ...filters.value, ...partial, offset: 0 }
    } else {
      filters.value = { ...filters.value, ...partial }
    }
    await Promise.all([loadTrades(), loadStats(), loadBreakdown(), loadDailyPnl()])
  }

  async function updateNotes(id: string, notes: string) {
    try {
      await invoke('plugin:algo|update_trade_notes', { id, notes })
      // Update the local trade record
      const trade = trades.value.find((t) => t.id === id)
      if (trade) {
        trade.notes = notes
      }
    } catch (err) {
      console.error('[journal store] Failed to update trade notes:', err)
      throw err
    }
  }

  async function refresh() {
    await Promise.all([loadTrades(), loadStats(), loadBreakdown(), loadDailyPnl(), loadFacets()])
  }

  async function resetFilters() {
    filters.value = { limit: 50, offset: 0, trading_mode: mode.value }
    // Replacing the filters is not enough — nothing watches them any more.
    await Promise.all([loadTrades(), loadStats(), loadBreakdown(), loadDailyPnl()])
  }

  return {
    // State
    trades,
    stats,
    breakdown,
    dailyPnl,
    facets,
    filters,
    mode,
    isLoading,
    totalCount,
    // Actions
    loadTrades,
    loadStats,
    loadBreakdown,
    loadDailyPnl,
    loadFacets,
    setMode,
    updateFilters,
    updateNotes,
    refresh,
    resetFilters,
  }
})
