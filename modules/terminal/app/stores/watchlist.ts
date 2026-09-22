import { defineStore } from 'pinia'

/** One row of the market cap ladder, as `plugin:terminal|get_marketcap` returns it. */
export interface MarketCapCoin {
  rank: number
  symbol: string
  name: string
  price: number
  marketCap: number
  change: number
}

/**
 * A favorited coin. It carries the coin's last ladder snapshot, so the
 * watchlist can render (price, change, rank) even when the ladder page the
 * coin lives on is not the one currently loaded.
 */
export interface WatchlistCoin extends MarketCapCoin {
  /** Epoch ms of the snapshot the price/change columns come from. */
  seenAt: number
}

/** The two views of the terminal's left sidebar. */
export type SidebarView = 'ladder' | 'watchlist'

const STORAGE_KEY = 'quantterminal-watchlist'
const VIEW_STORAGE_KEY = 'quantterminal-sidebar-view'

/**
 * QuantTerminal's watchlist (2026-09-03): the coins the user favorited from
 * the market cap ladder (right-click a row → "Add to watchlist"), plus which
 * of the two sidebar views — Market Ladder or Watchlist — is showing.
 *
 * Both persist in localStorage, like the TradingView settings store. Coins are
 * keyed by symbol and kept in the order they were added.
 */
export const useWatchlistStore = defineStore('terminal/watchlist', () => {
  const coins = ref<WatchlistCoin[]>([])
  const view = ref<SidebarView>('ladder')

  const count = computed(() => coins.value.length)
  const symbols = computed(() => new Set(coins.value.map((c) => c.symbol)))

  function has(symbol: string): boolean {
    return symbols.value.has(symbol)
  }

  function add(coin: MarketCapCoin) {
    if (has(coin.symbol)) return
    coins.value.push({ ...coin, seenAt: Date.now() })
    save()
  }

  function remove(symbol: string) {
    const idx = coins.value.findIndex((c) => c.symbol === symbol)
    if (idx < 0) return
    coins.value.splice(idx, 1)
    save()
  }

  function toggle(coin: MarketCapCoin) {
    if (has(coin.symbol)) remove(coin.symbol)
    else add(coin)
  }

  /**
   * Refresh the snapshot of every favorite present in a freshly fetched ladder
   * page. The ladder calls this after each fetch, so favorites on a loaded page
   * are always exactly as fresh as the ladder itself.
   */
  function updateSnapshots(fresh: MarketCapCoin[]) {
    if (coins.value.length === 0) return
    const bySymbol = new Map(fresh.map((c) => [c.symbol, c]))
    let changed = false
    for (const fav of coins.value) {
      const next = bySymbol.get(fav.symbol)
      if (!next) continue
      Object.assign(fav, next, { seenAt: Date.now() })
      changed = true
    }
    if (changed) save()
  }

  function setView(next: SidebarView) {
    view.value = next
    try {
      localStorage.setItem(VIEW_STORAGE_KEY, next)
    } catch {
      // Storage unavailable — the view still switches for this session.
    }
  }

  function load() {
    try {
      const raw = localStorage.getItem(STORAGE_KEY)
      if (raw) {
        const saved: unknown = JSON.parse(raw)
        if (Array.isArray(saved)) {
          coins.value = saved.filter(
            (c): c is WatchlistCoin =>
              typeof c === 'object' && c !== null && typeof (c as WatchlistCoin).symbol === 'string',
          )
        }
      }
      const savedView = localStorage.getItem(VIEW_STORAGE_KEY)
      if (savedView === 'ladder' || savedView === 'watchlist') view.value = savedView
    } catch {
      // Ignore corrupt storage — start with an empty watchlist.
    }
  }

  function save() {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(coins.value))
    } catch {
      // Storage full/unavailable — keep the in-memory list.
    }
  }

  // Init
  if (import.meta.client) {
    load()
  }

  return {
    coins,
    view,
    count,
    has,
    add,
    remove,
    toggle,
    updateSnapshots,
    setView,
    load,
  }
})
