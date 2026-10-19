import { invoke } from '@tauri-apps/api/core'
import { defineStore } from 'pinia'

export interface OHLCVCandle {
  time: number
  open: number
  high: number
  low: number
  close: number
  volume: number
}

/** A pair of the order book's symbol menu (plugin:terminal|get_symbols). */
export interface SymbolInfo {
  /** `BTC/USDT` */
  symbol: string
  base: string
  quote: string
  /** The exchange's price step: 0.01 for BTC, 0.00000001 for PEPE. */
  tickSize: number
}

const TIMEFRAME_MAP: Record<string, string> = {
  '1m': '1m',
  '5m': '5m',
  '15m': '15m',
  '1H': '1h',
  '4H': '4h',
  '1D': '1d',
  '1W': '1w',
}

export const useMarketStore = defineStore('terminal/market', () => {
  const selectedSymbol = ref('BTC/USDT')
  const selectedExchange = ref('binance')
  const timeframe = ref('1D')
  const ohlcv = ref<OHLCVCandle[]>([])
  const ohlcvLoading = ref(false)
  /** Every tradable Binance USDT pair, busiest first. */
  const symbols = ref<SymbolInfo[]>([])
  const symbolsLoading = ref(false)
  const symbolsError = ref<string | null>(null)
  const selectedInfo = computed(() => symbols.value.find((s) => s.symbol === selectedSymbol.value) ?? null)

  let ohlcvRequest = 0

  async function fetchOHLCV(symbol?: string, tf?: string) {
    const sym = symbol || selectedSymbol.value
    const ccxtTf = TIMEFRAME_MAP[tf || timeframe.value] || '1d'

    // Only the newest request may write — a slower earlier one would otherwise
    // leave `ohlcv` holding the previously selected symbol's candles.
    const request = ++ohlcvRequest

    ohlcvLoading.value = true
    try {
      const data = await invoke<OHLCVCandle[]>('plugin:terminal|get_ohlcv', { symbol: sym, timeframe: ccxtTf, limit: 1000 })
      if (request !== ohlcvRequest) return
      ohlcv.value = data
    } catch (error) {
      console.error('Failed to fetch OHLCV:', error)
    } finally {
      if (request === ohlcvRequest) ohlcvLoading.value = false
    }
  }

  // Nothing renders `ohlcv` yet — the chart is a TradingView embed with its own
  // feed — so a symbol/timeframe pick only records the selection; no candles
  // are fetched and there is no live poll to gate. fetchOHLCV() stays exported
  // for the first real consumer, which then also owns its refresh strategy.
  function setTimeframe(tf: string) {
    timeframe.value = tf
  }

  function setSymbol(symbol: string) {
    selectedSymbol.value = symbol
  }

  /** Read once per session; the backend caches the list for an hour. */
  async function loadSymbols() {
    if (symbols.value.length || symbolsLoading.value) return
    symbolsLoading.value = true
    symbolsError.value = null
    try {
      symbols.value = await invoke<SymbolInfo[]>('plugin:terminal|get_symbols')
    } catch (error) {
      symbolsError.value = String(error)
    } finally {
      symbolsLoading.value = false
    }
  }

  return {
    selectedSymbol,
    selectedExchange,
    timeframe,
    ohlcv,
    ohlcvLoading,
    symbols,
    symbolsLoading,
    symbolsError,
    selectedInfo,
    fetchOHLCV,
    setTimeframe,
    setSymbol,
    loadSymbols,
  }
})
