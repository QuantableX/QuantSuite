<script setup lang="ts">
import { useMarketStore } from '#terminal/stores/market'
import { useOrderbookTick } from '#terminal/composables/useOrderbookTick'
import { inActiveKeepAliveTree } from '@quantsuite/core'
import { invoke } from '@tauri-apps/api/core'
interface OrderLevel {
  price: number
  amount: number
  total: number
}

interface RawLevel {
  price: number
  amount: number
}

const props = defineProps<{
  symbol?: string
}>()

const marketStore = useMarketStore()
const { tickSize } = useOrderbookTick()

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

function groupLevels(raw: RawLevel[], side: 'ask' | 'bid', anchor: number): OrderLevel[] {
  if (raw.length === 0 || anchor === 0) return []

  const tick = tickSize.value

  // Aggregate raw orders into tick-sized buckets
  const grouped = new Map<number, number>()
  for (const level of raw) {
    const bucket = side === 'ask'
      ? Math.ceil(level.price / tick) * tick
      : Math.floor(level.price / tick) * tick
    const key = Math.round(bucket * 1e8) / 1e8
    grouped.set(key, (grouped.get(key) || 0) + level.amount)
  }

  // Generate a fixed price ladder of as many rows as fit the half,
  // anchored at the best ask/bid, extending outward
  const startBucket = side === 'ask'
    ? Math.round(Math.ceil(anchor / tick) * tick * 1e8) / 1e8
    : Math.round(Math.floor(anchor / tick) * tick * 1e8) / 1e8

  const levels: { price: number; amount: number }[] = []
  for (let i = 0; i < displayLevels.value; i++) {
    const price = side === 'ask'
      ? Math.round((startBucket + i * tick) * 1e8) / 1e8
      : Math.round((startBucket - i * tick) * 1e8) / 1e8
    levels.push({ price, amount: grouped.get(price) || 0 })
  }

  // Build cumulative totals
  let cumTotal = 0
  const result = levels.map(l => {
    cumTotal += l.amount
    return { price: l.price, amount: l.amount, total: cumTotal }
  })

  // Asks display reversed (highest at top, lowest near spread)
  return side === 'ask' ? result.reverse() : result
}

function rebuildLevels() {
  asks.value = groupLevels(rawAsks.value, 'ask', bestAsk.value)
  totalAskVol.value = asks.value.length > 0 ? asks.value[0]!.total : 0

  bids.value = groupLevels(rawBids.value, 'bid', bestBid.value)
  totalBidVol.value = bids.value.length > 0 ? bids.value[bids.value.length - 1]!.total : 0
}

async function fetchOrderbook() {
  const sym = props.symbol || marketStore.selectedSymbol
  try {
    const data = await invoke<{ asks: RawLevel[]; bids: RawLevel[] }>('plugin:terminal|get_order_book', { symbol: sym, limit: RAW_FETCH_LIMIT })

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
    }

    rebuildLevels()
  } catch {
    // Silently fail, will retry
  }
}

const maxTotal = computed(() => {
  const allOrders = [...asks.value, ...bids.value]
  if (allOrders.length === 0) return 1
  return Math.max(...allOrders.map(o => o.total))
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

function formatPrice(price: number): string {
  if (price >= 1000) return price.toFixed(2)
  if (price >= 1) return price.toFixed(4)
  return price.toFixed(6)
}

function formatAmount(amount: number): string {
  if (amount >= 100) return amount.toFixed(2)
  if (amount >= 1) return amount.toFixed(4)
  return amount.toFixed(6)
}

function formatVolume(vol: number): string {
  if (vol >= 1000) return (vol / 1000).toFixed(1) + 'K'
  return vol.toFixed(2)
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

watch(() => props.symbol || marketStore.selectedSymbol, () => {
  fetchOrderbook()
})

// Rebuild when tick size changes
watch(tickSize, () => {
  rebuildLevels()
})
</script>

<template>
  <div class="orderbook">
    <!-- Column headers -->
    <div class="ob-header">
      <span class="ob-col-price">Price</span>
      <span class="ob-col-amount">Amount</span>
      <span class="ob-col-total">Total</span>
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
        <span class="ob-col-amount ob-val">{{ formatAmount(order.amount) }}</span>
        <span class="ob-col-total ob-val">{{ formatAmount(order.total) }}</span>
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
          {{ formatPrice(lastPrice) }}
          <svg v-if="priceDirection === 'up'" class="ob-arrow" viewBox="0 0 10 6"><path d="M5 0L10 6H0z" fill="currentColor"/></svg>
          <svg v-else-if="priceDirection === 'down'" class="ob-arrow" viewBox="0 0 10 6"><path d="M5 6L0 0h10z" fill="currentColor"/></svg>
        </span>
      </div>
      <div class="ob-spread-info">
        <span>Spread: {{ formatPrice(spreadPrice) }} ({{ spreadPercent.toFixed(3) }}%)</span>
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
        <span class="ob-col-amount ob-val">{{ formatAmount(order.amount) }}</span>
        <span class="ob-col-total ob-val">{{ formatAmount(order.total) }}</span>
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

/* Columns */
.ob-col-price {
  flex: 1.2;
  text-align: left;
}

.ob-col-amount {
  flex: 1;
  text-align: right;
}

.ob-col-total {
  flex: 1;
  text-align: right;
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
