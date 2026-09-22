<script setup lang="ts">
definePageMeta({ layout: 'algo' })

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { inActiveKeepAliveTree } from '@quantsuite/core'
import { useBotsStore } from '#algo/stores/bots'
import type { BotEquityEvent, BotTradeEvent, EquityPoint, Trade, TradingMode } from '#algo/types'

const route = useRoute()
const router = useRouter()
const botsStore = useBotsStore()
const timeframes = ['1h', '4h', '1d', '1w', '1M', 'All'] as const
type ChartTimeframe = typeof timeframes[number]

function routeTimeframe(): ChartTimeframe {
  const tf = route.query.tf
  return typeof tf === 'string' && timeframes.includes(tf as ChartTimeframe)
    ? tf as ChartTimeframe
    : '1d'
}

const selectedTimeframe = ref<ChartTimeframe>(routeTimeframe())
// What the curve shows: 'paper' | 'live' = every bot of that mode added up
// (the sum of their latest equities at each step), anything else = one bot's id.
const selectedScope = ref<string>(typeof route.query.bot === 'string' ? route.query.bot : 'paper')
const equityData = ref<EquityPoint[]>([])
const trades = ref<Trade[]>([])
const isLoading = ref(false)
const error = ref<string | null>(null)
let unlistenEquity: UnlistenFn | null = null
let unlistenTrade: UnlistenFn | null = null
let aggregateTimer: ReturnType<typeof setTimeout> | null = null
// A running bot writes a snapshot per candle; the curve follows while the page is visible.
let pollTimer: ReturnType<typeof setInterval> | null = null
const POLL_MS = 30_000

const selectedBot = computed(() => botsStore.byId(selectedScope.value))
const scopeMode = computed<TradingMode | null>(() =>
  selectedScope.value === 'paper' || selectedScope.value === 'live' ? selectedScope.value : null,
)
const scopeLabel = computed(() => {
  if (selectedBot.value) return selectedBot.value.name
  return `all ${scopeMode.value ?? 'paper'} bots`
})

async function fetchEquityCurve() {
  isLoading.value = true
  error.value = null
  try {
    equityData.value = await invoke<EquityPoint[]>('plugin:algo|get_equity_curve', {
      source: selectedBot.value?.trading_mode ?? scopeMode.value ?? 'paper',
      timeframe: selectedTimeframe.value,
      botId: scopeMode.value ? null : selectedScope.value,
    })
  } catch (err) {
    console.error('[charts] Failed to load equity curve:', err)
    error.value = String(err)
    equityData.value = []
  } finally {
    isLoading.value = false
  }
}

async function fetchTrades() {
  try {
    trades.value = await invoke<Trade[]>('plugin:algo|list_trades', {
      filters: {
        is_backtest: false,
        limit: 100,
        bot_id: selectedBot.value ? selectedScope.value : undefined,
        trading_mode: scopeMode.value ?? undefined,
      },
    })
  } catch (err) {
    console.error('[charts] Failed to load trades:', err)
  }
}

function selectTimeframe(tf: string) {
  selectedTimeframe.value = tf as ChartTimeframe
  router.replace({ path: route.path, query: { ...route.query, tf } })
}

watch(selectedTimeframe, () => {
  void fetchEquityCurve()
})

watch(selectedScope, (scope) => {
  router.replace({ path: route.path, query: { ...route.query, bot: scope === 'paper' ? undefined : scope } })
  void Promise.all([fetchEquityCurve(), fetchTrades()])
})

watch(
  () => route.query.tf,
  () => {
    const next = routeTimeframe()
    if (next !== selectedTimeframe.value) selectedTimeframe.value = next
  },
)

let isListening = false
let listenGen = 0

async function startListening() {
  if (isListening) return
  isListening = true
  const gen = ++listenGen
  await Promise.all([fetchEquityCurve(), fetchTrades()])
  const unlistenEquityFn = await listen<BotEquityEvent>('bot:equity', (event) => {
    if (selectedBot.value) {
      if (event.payload.bot_id !== selectedScope.value) return
      equityData.value = [
        ...equityData.value,
        { time: event.payload.timestamp, equity: event.payload.equity },
      ].slice(-1000)
      return
    }
    // A mode's curve is a sum across bots — one point cannot be appended;
    // refetch once the burst of snapshots settles.
    if (!scopeMode.value) return
    if (botsStore.byId(event.payload.bot_id)?.trading_mode !== scopeMode.value) return
    if (aggregateTimer) clearTimeout(aggregateTimer)
    aggregateTimer = setTimeout(() => {
      aggregateTimer = null
      if (isListening) void fetchEquityCurve()
    }, 5000)
  })
  const unlistenTradeFn = await listen<BotTradeEvent>('bot:trade', async () => {
    await fetchTrades()
  })
  if (gen !== listenGen) {
    unlistenEquityFn()
    unlistenTradeFn()
    return
  }
  unlistenEquity = unlistenEquityFn
  unlistenTrade = unlistenTradeFn
  if (pollTimer) clearInterval(pollTimer)
  pollTimer = setInterval(() => {
    if (isListening && botsStore.runningCount > 0) void fetchEquityCurve()
  }, POLL_MS)
}

function stopListening() {
  isListening = false
  listenGen++
  if (aggregateTimer) {
    clearTimeout(aggregateTimer)
    aggregateTimer = null
  }
  if (pollTimer) {
    clearInterval(pollTimer)
    pollTimer = null
  }
  unlistenEquity?.()
  unlistenEquity = null
  unlistenTrade?.()
  unlistenTrade = null
}

onMounted(() => {
  if (inActiveKeepAliveTree()) void startListening()
})
onActivated(startListening)
onDeactivated(stopListening)
onUnmounted(stopListening)
</script>

<template>
  <div class="charts">
    <div class="charts__toolbar">
      <div class="charts__left">
        <h2 class="charts__title">Equity &amp; Performance</h2>
        <select v-model="selectedScope" class="input charts__select">
          <option value="paper">All paper bots</option>
          <option value="live">All live bots</option>
          <option v-for="bot in botsStore.list" :key="bot.id" :value="bot.id">
            {{ bot.name }} · {{ bot.trading_mode }}{{ bot.status === 'running' ? ' · running' : '' }}
          </option>
        </select>
      </div>
      <div class="timeframe-bar">
        <button
          v-for="tf in timeframes"
          :key="tf"
          class="btn btn-sm"
          :class="{ 'btn-primary': selectedTimeframe === tf }"
          @click="selectTimeframe(tf)"
        >
          {{ tf }}
        </button>
      </div>
    </div>

    <div v-if="isLoading && !equityData.length" class="charts__loading">
      <p class="text-muted">Loading chart data...</p>
    </div>

    <div v-else-if="error && !equityData.length" class="charts__error card">
      <p class="text-error">Failed to load chart data</p>
      <p class="text-muted">{{ error }}</p>
      <button class="btn btn-sm" @click="fetchEquityCurve">Retry</button>
    </div>

    <div v-else-if="!equityData.length && !isLoading" class="charts__empty">
      <p class="empty-state text-muted">
        No equity data yet for {{ scopeLabel }}. Start a bot to see its curve.
      </p>
    </div>

    <template v-else>
      <div class="charts__main card">
        <div class="chart-header">
          <h3 class="chart-header__title">Equity Curve <span class="chart-header__bot">{{ scopeLabel }}</span></h3>
          <span v-if="isLoading" class="text-muted chart-header__status">Updating...</span>
        </div>
        <div class="chart-container">
          <AlgoChartsEquityCurve :data="equityData" />
        </div>
      </div>

      <div class="charts__secondary card">
        <div class="chart-header">
          <h3 class="chart-header__title">Drawdown</h3>
        </div>
        <div class="chart-container chart-container--small">
          <AlgoChartsDrawdownChart :data="equityData" />
        </div>
      </div>

      <div v-if="trades.length" class="charts__trades card">
        <div class="chart-header">
          <h3 class="chart-header__title">
            Recent Trades
            <span class="pill">{{ trades.length }}</span>
          </h3>
        </div>
        <div class="trades-summary">
          <div class="trades-summary__grid">
            <div v-for="trade in trades.slice(0, 10)" :key="trade.id" class="trade-marker-item">
              <span class="trade-marker-item__side" :class="trade.side === 'long' ? 'text-accent' : 'text-error'">
                {{ trade.side === 'long' ? 'L' : 'S' }}
              </span>
              <span class="trade-marker-item__pair">{{ trade.pair }}</span>
              <span v-if="!selectedBot" class="trade-marker-item__bot text-muted">{{ botsStore.name(trade.bot_id) }}</span>
              <span class="trade-marker-item__pnl mono" :class="(trade.pnl ?? 0) >= 0 ? 'text-success' : 'text-error'">
                {{ trade.pnl != null ? (trade.pnl >= 0 ? '+' : '') + trade.pnl.toFixed(2) : '--' }}
              </span>
            </div>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.charts {
  height: 100%;
  overflow-y: auto;
  padding: 20px;
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.charts__toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex-shrink: 0;
}

.charts__left {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
}

.charts__title {
  font-size: 16px;
  font-weight: 600;
  color: var(--qa-text);
  white-space: nowrap;
}

.charts__select {
  width: auto;
  min-width: 200px;
  padding: 5px 28px 5px 10px;
  font-size: 12px;
}

.timeframe-bar {
  display: flex;
  gap: 4px;
}

.charts__loading,
.charts__empty {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 300px;
  font-size: 14px;
}

.charts__error {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 40px;
  text-align: center;
}

.empty-state {
  font-size: 13px;
}

.charts__main {
  flex: 1;
  min-height: 300px;
  display: flex;
  flex-direction: column;
}

.charts__secondary,
.charts__trades {
  flex-shrink: 0;
}

.chart-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 12px;
}

.chart-header__title {
  font-size: 13px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--qa-text-secondary);
  display: flex;
  align-items: center;
  gap: 8px;
}

.chart-header__bot {
  text-transform: none;
  letter-spacing: 0;
  font-weight: 500;
  color: var(--qa-text);
}

.chart-header__status {
  font-size: 12px;
}

.chart-container {
  flex: 1;
  min-height: 250px;
  position: relative;
}

.chart-container--small {
  min-height: 150px;
  height: 150px;
}

.trades-summary__grid {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.trade-marker-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 6px 0;
  border-bottom: 1px solid var(--qa-border-subtle);
  font-size: 13px;
}

.trade-marker-item:last-child {
  border-bottom: none;
}

.trade-marker-item__side {
  width: 20px;
  font-weight: 700;
  font-size: 12px;
  text-align: center;
}

.trade-marker-item__pair {
  flex: 1;
  color: var(--qa-text);
}

.trade-marker-item__bot {
  font-size: 11px;
}

.trade-marker-item__pnl {
  font-weight: 600;
  font-size: 13px;
}
</style>
