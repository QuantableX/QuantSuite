<script setup lang="ts">
import { useMarketStore } from '#terminal/stores/market'
import { useOrderbookTick } from '#terminal/composables/useOrderbookTick'
import { formatBookPrice, formatCoins, formatUsd, groupLevels, stepDecimals, type OrderLevel, type RawLevel } from '#terminal/utils/orderbook'
import { inActiveKeepAliveTree } from '@quantsuite/core'
import { invoke } from '@tauri-apps/api/core'

const props = defineProps<{
  symbol?: string
}>()

const marketStore = useMarketStore()
const { tickSize, pairDecimals, setPair, setPrice } = useOrderbookTick()

/** The exchange step until the pair list has loaded — right for BTC/USDT, the default pair. */
const FALLBACK_STEP = 0.01
const pairSymbol = computed(() => props.symbol || marketStore.selectedSymbol)
const pairInfo = computed(() => marketStore.symbols.find((s) => s.symbol === pairSymbol.value) ?? null)
/** The coin column's header: BTC, PEPE, … */
const baseLabel = computed(() => pairInfo.value?.base ?? pairSymbol.value.split('/')[0] ?? '')
const ladderDecimals = computed(() => stepDecimals(tickSize.value))

const rawAsks = ref<RawLevel[]>([])
const rawBids = ref<RawLevel[]>([])
const asks = ref<OrderLevel[]>([])
const bids = ref<OrderLevel[]>([])
const spreadPrice = ref(0)
const spreadPercent = ref(0)
const bestAsk = ref(0)
const bestBid = ref(0)
const lastPrice = ref(0)
const prevPrice = ref(0)
const totalAskVol = ref(0)
const totalBidVol = ref(0)

const RAW_FETCH_LIMIT = 1000

// The book never scrolls: the spread stays fixed in the middle and each side
// renders exactly as many rows as fit its half (V3.1, 2026-08-15). The count
// is measured from the asks container; both halves are `flex: 1`, so they are
// always the same height.
const FALLBACK_ROW_HEIGHT = 22
const asksEl = ref<HTMLElement | null>(null)
const displayLevels = ref(12)
let resizeObserver: ResizeObserver | null = null

function measureLevels() {
  const el = asksEl.value
  if (!el) return
  const rowHeight = (el.firstElementChild as HTMLElement | null)?.offsetHeight || FALLBACK_ROW_HEIGHT
  const fit = Math.max(3, Math.floor(el.clientHeight / rowHeight))
  if (fit !== displayLevels.value) {
    displayLevels.value = fit
    rebuildLevels()
  }
}

let pollTimer: ReturnType<typeof setInterval> | null = null

// A fixed ladder of as many rows as fit each half, anchored at the best
// ask/bid; totals are USD, cumulative from the spread outwards.
function rebuildLevels() {
  asks.value = groupLevels(rawAsks.value, 'ask', bestAsk.value, tickSize.value, displayLevels.value)
  totalAskVol.value = asks.value.length > 0 ? asks.value[0]!.total : 0

  bids.value = groupLevels(rawBids.value, 'bid', bestBid.value, tickSize.value, displayLevels.value)
  totalBidVol.value = bids.value.length > 0 ? bids.value[bids.value.length - 1]!.total : 0
}

async function fetchOrderbook() {
  const sym = pairSymbol.value
  try {
    const data = await invoke<{ asks: RawLevel[]; bids: RawLevel[] }>('plugin:terminal|get_order_book', { symbol: sym, limit: RAW_FETCH_LIMIT })
    // A reply for the pair the user just left must not land in the new one.
    if (sym !== pairSymbol.value) return

    rawAsks.value = data.asks
    rawBids.value = data.bids

    // Set spread info
    const firstAsk = data.asks[0]
    const firstBid = data.bids[0]
    if (firstAsk && firstBid) {
      bestAsk.value = firstAsk.price
      bestBid.value = firstBid.price
      const mid = (bestAsk.value + bestBid.value) / 2
      prevPrice.value = lastPrice.value || mid
      lastPrice.value = mid
      spreadPrice.value = bestAsk.value - bestBid.value
      spreadPercent.value = (spreadPrice.value / mid) * 100
      setPrice(mid)
    }

    rebuildLevels()
  } catch {
    // Silently fail, will retry
  }
}

const maxTotal = computed(() => {
  const allOrders = [...asks.value, ...bids.value]
  if (allOrders.length === 0) return 1
  return Math.max(...allOrders.map(o => o.total)) || 1
})

/**
 * The price column is exactly as wide as the pair's prices (5 characters for
 * BTC at 10$ steps, 10 for PEPE); the coin, $ and Total $ columns share the
 * rest. Header and rows read the same width, so the columns line up.
 */
const priceChars = computed(() => {
  const prices = [asks.value[0]?.price, bids.value[bids.value.length - 1]?.price, bestAsk.value]
  return Math.max(5, ...prices.filter((p): p is number => !!p).map((p) => formatPrice(p).length))
})

const bidRatio = computed(() => {
  const total = totalBidVol.value + totalAskVol.value
  if (total === 0) return 50
  return (totalBidVol.value / total) * 100
})

const priceDirection = computed(() => {
  if (lastPrice.value > prevPrice.value) return 'up'
  if (lastPrice.value < prevPrice.value) return 'down'
  return 'neutral'
})

/** A ladder price, with the decimals of the chosen step. */
function formatPrice(price: number): string {
  return formatBookPrice(price, ladderDecimals.value)
}

/** The mid price and the spread, with the decimals of the exchange's step. */
function formatPairPrice(price: number): string {
  return formatBookPrice(price, pairDecimals.value)
}

// V3 warm cache: poll only while the module is visible. The start must live
// in BOTH hooks: onActivated does NOT fire for a component that mounts late
// into an already-active KeepAlive tree (async pages resolve after the
// stage's activation flush), so onMounted covers the first visit and
// onActivated covers reactivations — the timer guard makes the overlap safe.
// The mount can also land in a stage the user has already left, and there the
// timer guard turns against us: it would keep polling for a hidden module and
// make the real activation a no-op, so onMounted starts only inside an active
// tree. The ResizeObserver below watches this component's own element, not a
// global target, and stays unconditional.
function startOrderbookPolling() {
  if (pollTimer) return
  fetchOrderbook()
  pollTimer = setInterval(fetchOrderbook, 800)
}

onMounted(() => {
  // The pair list gives the coin's name and price step; it is also the menu's.
  void marketStore.loadSymbols()
  if (inActiveKeepAliveTree()) startOrderbookPolling()
  if (asksEl.value) {
    resizeObserver = new ResizeObserver(() => measureLevels())
    resizeObserver.observe(asksEl.value)
  }
  nextTick(() => measureLevels())
})

onActivated(startOrderbookPolling)

onDeactivated(() => {
  if (pollTimer) {
    clearInterval(pollTimer)
    pollTimer = null
  }
})

onBeforeUnmount(() => {
  if (pollTimer) clearInterval(pollTimer)
  resizeObserver?.disconnect()
})

// A new pair starts empty at its own step; the step also follows the pair
// list arriving after the first fetch. The first run only sets the step —
// polling starts with the module (see above).
watch([pairSymbol, () => pairInfo.value?.tickSize], ([sym, step], old) => {
  setPair(sym, step ?? FALLBACK_STEP)
  const prevSym = old?.[0]
  if (prevSym === undefined || sym === prevSym) return
  rawAsks.value = []
  rawBids.value = []
  bestAsk.value = 0
  bestBid.value = 0
  lastPrice.value = 0
  prevPrice.value = 0
  spreadPrice.value = 0
  spreadPercent.value = 0
  rebuildLevels()
  fetchOrderbook()
}, { immediate: true })

// Rebuild when tick size changes
watch(tickSize, () => {
  rebuildLevels()
})
</script>

<template>
  <div class="orderbook" :style="{ '--ob-price-chars': priceChars }">
    <!-- Column headers: coins per level, its USD value, and cumulative USD -->
    <div class="ob-header">
      <span class="ob-col-price">Price</span>
      <span class="ob-col-amount" :title="`Amount in ${baseLabel}`">{{ baseLabel }}</span>
      <span class="ob-col-value" title="Amount in USD">$</span>
      <span class="ob-col-total" title="Cumulative USD from the spread">Total $</span>
    </div>

    <!-- Asks (sells) -->
    <div ref="asksEl" class="ob-asks">
      <div
        v-for="order in asks"
        :key="'a' + order.price"
        class="ob-row ob-row-ask"
      >
        <div
          class="ob-depth-bar ob-depth-ask"
          :style="{ width: (order.total / maxTotal * 100) + '%' }"
        />
        <span class="ob-col-price ob-price-ask">{{ formatPrice(order.price) }}</span>
        <span class="ob-col-amount ob-val">{{ formatCoins(order.amount) }}</span>
        <span class="ob-col-value ob-val">{{ formatUsd(order.value) }}</span>
        <span class="ob-col-total ob-val">{{ formatUsd(order.total) }}</span>
      </div>
    </div>

    <!-- Spread / Last Price -->
    <div class="ob-spread">
      <div class="ob-spread-price">
        <span
          class="ob-last-price"
          :class="{
            'ob-price-up': priceDirection === 'up',
            'ob-price-down': priceDirection === 'down',
          }"
        >
          {{ formatPairPrice(lastPrice) }}
          <svg v-if="priceDirection === 'up'" class="ob-arrow" viewBox="0 0 10 6"><path d="M5 0L10 6H0z" fill="currentColor"/></svg>
          <svg v-else-if="priceDirection === 'down'" class="ob-arrow" viewBox="0 0 10 6"><path d="M5 6L0 0h10z" fill="currentColor"/></svg>
        </span>
      </div>
      <div class="ob-spread-info">
        <span>Spread: {{ formatPairPrice(spreadPrice) }} ({{ spreadPercent.toFixed(3) }}%)</span>
      </div>
    </div>

    <!-- Bids (buys) -->
    <div class="ob-bids">
      <div
        v-for="order in bids"
        :key="'b' + order.price"
        class="ob-row ob-row-bid"
      >
        <div
          class="ob-depth-bar ob-depth-bid"
          :style="{ width: (order.total / maxTotal * 100) + '%' }"
        />
        <span class="ob-col-price ob-price-bid">{{ formatPrice(order.price) }}</span>
        <span class="ob-col-amount ob-val">{{ formatCoins(order.amount) }}</span>
        <span class="ob-col-value ob-val">{{ formatUsd(order.value) }}</span>
        <span class="ob-col-total ob-val">{{ formatUsd(order.total) }}</span>
      </div>
    </div>

    <!-- Volume Balance Bar -->
    <div class="ob-balance">
      <div class="ob-balance-bar">
        <div class="ob-balance-bid" :style="{ width: bidRatio + '%' }" />
        <div class="ob-balance-ask" :style="{ width: (100 - bidRatio) + '%' }" />
      </div>
      <div class="ob-balance-labels">
        <span class="ob-balance-pct" style="color: var(--positive)">{{ bidRatio.toFixed(1) }}%</span>
        <span class="ob-balance-title">Buy / Sell</span>
        <span class="ob-balance-pct" style="color: var(--negative)">{{ (100 - bidRatio).toFixed(1) }}%</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.orderbook {
  display: flex;
  flex-direction: column;
  height: 100%;
  font-family: 'JetBrains Mono', monospace;
  font-size: 11px;
  user-select: none;
}

/* Header */
.ob-header {
  display: flex;
  padding: 6px 10px;
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--muted);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}

/* Columns: the price column fits the pair's prices (JetBrains Mono is
   0.6em per character at the rows' 11px), the other three share the rest. */
.ob-col-price {
  flex: 0 0 calc(var(--ob-price-chars, 8) * 6.6px + 3px);
  text-align: left;
  white-space: nowrap;
}

.ob-col-amount,
.ob-col-value,
.ob-col-total {
  flex: 1 1 0;
  min-width: 0;
  text-align: right;
  white-space: nowrap;
}

/* "Total $" must fit the narrow cells of a 10-digit price like PEPE's. */
.ob-header .ob-col-amount,
.ob-header .ob-col-value,
.ob-header .ob-col-total {
  letter-spacing: 0;
}

.ob-header .ob-col-amount {
  overflow: hidden;
  text-overflow: ellipsis;
}

/* Asks & Bids containers */
.ob-asks,
.ob-bids {
  flex: 1;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.ob-asks {
  justify-content: flex-end;
}

/* Row */
.ob-row {
  position: relative;
  display: flex;
  align-items: center;
  padding: 2px 10px;
  transition: background-color 0.1s ease;
  cursor: default;
  line-height: 1.6;
}

.ob-row:hover {
  background-color: var(--surface-2);
}

/* Depth bars */
.ob-depth-bar {
  position: absolute;
  top: 0;
  bottom: 0;
  right: 0;
  pointer-events: none;
  transition: width 0.3s ease;
}

.ob-depth-ask {
  background-color: var(--negative);
  opacity: 0.12;
}

.ob-depth-bid {
  background-color: var(--positive);
  opacity: 0.12;
}

.ob-row:hover .ob-depth-ask {
  opacity: 0.22;
}

.ob-row:hover .ob-depth-bid {
  opacity: 0.22;
}

/* Price colors */
.ob-price-ask {
  color: var(--negative);
}

.ob-price-bid {
  color: var(--positive);
}

/* Amount & total values */
.ob-val {
  color: var(--text-primary);
  opacity: 0.8;
}

.ob-row:hover .ob-val {
  opacity: 1;
}

/* Spread section */
.ob-spread {
  flex-shrink: 0;
  padding: 6px 10px;
  border-top: 1px solid var(--border);
  border-bottom: 1px solid var(--border);
  background-color: var(--surface-2);
}

.ob-spread-price {
  display: flex;
  align-items: center;
  justify-content: center;
}

.ob-last-price {
  font-size: 14px;
  font-weight: 700;
  display: flex;
  align-items: center;
  gap: 4px;
  color: var(--text-primary);
}

.ob-last-price.ob-price-up {
  color: var(--positive);
}

.ob-last-price.ob-price-down {
  color: var(--negative);
}

.ob-arrow {
  width: 8px;
  height: 6px;
}

.ob-spread-info {
  text-align: center;
  font-size: 9px;
  color: var(--muted);
  margin-top: 2px;
  letter-spacing: 0.02em;
}

/* Volume Balance */
.ob-balance {
  flex-shrink: 0;
  padding: 8px 10px;
  border-top: 1px solid var(--border);
}

.ob-balance-bar {
  display: flex;
  height: 4px;
  border-radius: 2px;
  overflow: hidden;
  gap: 1px;
}

.ob-balance-bid {
  background-color: var(--positive);
  border-radius: 2px 0 0 2px;
  transition: width 0.5s ease;
}

.ob-balance-ask {
  background-color: var(--negative);
  border-radius: 0 2px 2px 0;
  transition: width 0.5s ease;
}

.ob-balance-labels {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-top: 4px;
  font-size: 9px;
}

.ob-balance-pct {
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}

.ob-balance-title {
  color: var(--muted);
  font-size: 9px;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}
</style>
