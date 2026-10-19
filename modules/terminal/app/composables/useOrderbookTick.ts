import { defaultTick, stepDecimals, tickOptions } from '#terminal/utils/orderbook'

/**
 * The order book's price grouping, shared by the header menu and the book.
 * The steps follow the pair: from the exchange's own step in 1-5-10 steps up
 * to 1 % of the price. A new pair starts at its default (10$ for BTC) once the
 * book's first price is known; the price is kept from then on, so the menu
 * does not change under the cursor.
 */
const pair = ref('')
const minTick = ref(0.01)
const refPrice = ref(0)
const chosen = ref<number | null>(null)

export function useOrderbookTick() {
  const options = computed(() => tickOptions(minTick.value, refPrice.value))
  const tickSize = computed(() => chosen.value ?? minTick.value)
  /** Decimals of the exchange step: the last price and the spread. */
  const pairDecimals = computed(() => stepDecimals(minTick.value))

  function selectTick(t: number) {
    chosen.value = t
  }

  /** A pair (or its exchange step) changed: start over at its own step. */
  function setPair(symbol: string, step: number) {
    if (symbol === pair.value && step === minTick.value) return
    pair.value = symbol
    minTick.value = step
    refPrice.value = 0
    chosen.value = null
  }

  /** The book's mid price; the first one of a pair picks the default step. */
  function setPrice(mid: number) {
    if (refPrice.value > 0 || !(mid > 0)) return
    refPrice.value = mid
    chosen.value ??= defaultTick(tickOptions(minTick.value, mid), mid)
  }

  function formatTick(t: number): string {
    return `${t.toFixed(stepDecimals(t))}$`
  }

  return {
    options,
    tickSize,
    pairDecimals,
    selectTick,
    setPair,
    setPrice,
    formatTick,
  }
}
