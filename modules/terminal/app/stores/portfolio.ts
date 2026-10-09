import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import type { PortfolioAsset, PortfolioDocument, PortfolioTrade } from '../types/portfolio'
import type { MarketCapCoin } from './watchlist'
import { calculateHoldings, emptyPortfolio, portfolioHistory, portfolioTotals, recordPortfolioSnapshot, validatePortfolio } from '../utils/portfolio.ts'

const storageKey = 'quantterminal-portfolio-v1'
const native = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
const copy = <T>(value: T): T => JSON.parse(JSON.stringify(value))
const id = () => crypto.randomUUID()

export const usePortfolioStore = defineStore('terminal/portfolio', () => {
  const document = ref<PortfolioDocument>(emptyPortfolio())
  const loaded = ref(false)
  const loading = ref(false)
  const busy = ref(false)
  const error = ref('')
  const quoteError = ref('')
  const refreshing = ref(false)
  const marketCoins = ref<(MarketCapCoin & { page: number })[]>([])
  const marketLoading = ref(false)
  const holdings = computed(() => calculateHoldings(document.value))
  const totals = computed(() => portfolioTotals(holdings.value))
  const history = computed(() => portfolioHistory(document.value))
  let loadingPromise: Promise<void> | null = null

  async function read(): Promise<PortfolioDocument> {
    const raw: unknown = native()
      ? await invoke('plugin:qs|get_setting', { scope: 'terminal', key: 'portfolio.v1' })
      : JSON.parse(localStorage.getItem(storageKey) ?? 'null')
    const doc = raw ?? emptyPortfolio()
    validatePortfolio(doc)
    return doc
  }

  async function load() {
    if (busy.value) return
    if (loadingPromise) return loadingPromise
    loading.value = true
    loadingPromise = (async () => {
      try { document.value = await read(); loaded.value = true; error.value = '' }
      catch (e) { error.value = `Could not load portfolio: ${String(e)}` }
      finally { loading.value = false; loadingPromise = null }
    })()
    return loadingPromise
  }

  async function commit(change: (doc: PortfolioDocument) => void, expected = document.value.revision) {
    if (!loaded.value || busy.value || loading.value) return false
    busy.value = true
    error.value = ''
    const write = async () => {
      const latest = await read()
      if (latest.revision !== expected) {
        document.value = latest
        throw new Error('The portfolio changed in another window. Close this editor, review the latest data and reopen your edit.')
      }
      const next = copy(latest)
      change(next)
      validatePortfolio(next)
      recordPortfolioSnapshot(next)
      next.revision = id()
      if (native()) await invoke('plugin:qs|set_setting', { scope: 'terminal', key: 'portfolio.v1', value: next })
      else localStorage.setItem(storageKey, JSON.stringify(next))
      document.value = next
      return true
    }
    try {
      // Serialize read/check/write across app webviews on the same origin.
      return typeof navigator !== 'undefined' && navigator.locks
        ? await navigator.locks.request('quantterminal-portfolio-v1', write) : await write()
    } catch (e) { error.value = String(e); return false }
    finally { busy.value = false }
  }

  function saveAsset(asset: PortfolioAsset, opening?: { quantity: number; price: number; date: string }, expected?: string) {
    return commit((doc) => {
      const at = doc.assets.findIndex((a) => a.id === asset.id)
      if (at < 0) doc.assets.push(asset)
      else {
        const previous = doc.assets[at]!
        if (previous.symbol !== asset.symbol || previous.marketName !== asset.marketName || previous.source !== asset.source) delete doc.quotes[asset.id]
        doc.assets[at] = asset
      }
      if (at < 0 && opening && opening.quantity > 0) doc.trades.push({
        id: id(), assetId: asset.id, kind: 'opening', quantity: opening.quantity, price: opening.price,
        date: opening.date, fees: 0, notes: 'Opening holding', order: nextOrder(doc),
      })
    }, expected)
  }
  function nextOrder(doc: PortfolioDocument) { return Math.max(0, ...doc.trades.map((t) => t.order)) + 1 }
  function saveTrade(trade: Omit<PortfolioTrade, 'order'>, expected?: string) {
    return commit((doc) => {
      const at = doc.trades.findIndex((t) => t.id === trade.id)
      if (at < 0) doc.trades.push({ ...trade, order: nextOrder(doc) })
      else doc.trades[at] = { ...trade, order: doc.trades[at]!.order }
    }, expected)
  }
  function removeTrade(tradeId: string) { return commit((doc) => { doc.trades = doc.trades.filter((t) => t.id !== tradeId) }) }
  function removeAsset(assetId: string) {
    return commit((doc) => {
      doc.assets = doc.assets.filter((a) => a.id !== assetId)
      doc.trades = doc.trades.filter((t) => t.assetId !== assetId)
      delete doc.quotes[assetId]
    })
  }

  async function loadMarket(page = 1) {
    if (marketLoading.value) return
    marketLoading.value = true
    quoteError.value = ''
    try {
      const rows = await invoke<MarketCapCoin[]>('plugin:terminal|get_marketcap', { page, provider: 'coingecko' })
      marketCoins.value = rows.filter((c) => Number.isFinite(c.price) && c.price > 0).map((c) => ({ ...c, page }))
    } catch (e) { quoteError.value = `Market prices unavailable: ${String(e)}` }
    finally { marketLoading.value = false }
  }

  async function refreshPrices() {
    if (!loaded.value || refreshing.value || busy.value || loading.value) return
    const assets = document.value.assets.filter((a) => a.source === 'market')
    if (!assets.length) return
    refreshing.value = true
    quoteError.value = ''
    const revision = document.value.revision
    const pages = [...new Set(assets.map((a) => a.marketPage))]
    const quotes: PortfolioDocument['quotes'] = {}
    const missing: string[] = []
    try {
      for (const page of pages) {
        const rows = await invoke<MarketCapCoin[]>('plugin:terminal|get_marketcap', { page, provider: 'coingecko' })
        for (const asset of assets.filter((a) => a.marketPage === page)) {
          // Symbol alone is ambiguous (unrelated coins can share a ticker).
          const matches = rows.filter((c) => c.symbol === asset.symbol && c.name === asset.marketName && Number.isFinite(c.price) && c.price > 0)
          if (matches.length === 1) quotes[asset.id] = { price: matches[0]!.price, checkedAt: new Date().toISOString() }
          else missing.push(asset.symbol)
        }
      }
      // A user edit while quotes were downloading takes precedence.
      if (document.value.revision !== revision) return
      if (Object.keys(quotes).length) await commit((doc) => { Object.assign(doc.quotes, quotes) }, revision)
      if (missing.length) quoteError.value = `No matching quote for ${missing.join(', ')} on its saved market page. Previous quotes are retained; reselect the market asset or enter a manual price.`
    } catch (e) { quoteError.value = `Market refresh failed. Previous quotes are retained. ${String(e)}` }
    finally { refreshing.value = false }
  }

  return { document, loaded, loading, busy, error, quoteError, refreshing, marketCoins, marketLoading,
    holdings, totals, history, load, saveAsset, saveTrade, removeTrade, removeAsset, loadMarket, refreshPrices }
})
