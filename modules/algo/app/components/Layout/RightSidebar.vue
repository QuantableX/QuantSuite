<script setup lang="ts">
import { useBotsStore } from '#algo/stores/bots'
import { useBacktestStore } from '#algo/stores/backtest'
import { useExchangeStore } from '#algo/stores/exchange'
import { useJournalStore } from '#algo/stores/journal'
import { useStrategiesStore } from '#algo/stores/strategies'
import { formatCurrency, formatDate, formatPnl } from '#algo/utils/format'

const route = useRoute()
const router = useRouter()
const botsStore = useBotsStore()
const backtests = useBacktestStore()
const exchangeStore = useExchangeStore()
const journalStore = useJournalStore()
const strategies = useStrategiesStore()

// ── Context section title ──

const sectionTitle = computed(() => {
  switch (route.path) {
    case '/algo': return 'Overview'
    case '/algo/bots': return 'Bots'
    case '/algo/strategies': return 'Strategies'
    case '/algo/indicators': return 'The Forge'
    case '/algo/backtest': return 'Backtest Results'
    case '/algo/terminal': return 'Terminal'
    case '/algo/journal': return 'Journal Filters'
    case '/algo/charts': return 'Chart Options'
    case '/algo/exchange': return 'Exchanges'
    case '/algo/settings': return 'Context'
    default: return 'Context'
  }
})

// ── Terminal log filter state ──

// ── Journal filter state ──

const journalDateFrom = ref('')
const journalDateTo = ref('')
const journalBot = ref<string>('')
const journalMode = ref<string>('paper')
const journalStrategy = ref<string>('')
const journalExchange = ref<string>('')
const journalPair = ref('')
const journalSide = ref<string>('')
const journalMinPnl = ref<number | null>(null)

// The exchange and pair filters offer what the journal holds — never free
// text (PLAN-QUANTALGO §5). Exchange values are connected-exchange ids on bot
// trades and provider names on old backtest rows; both get a readable label.
const journalExchangeOptions = computed(() =>
  journalStore.facets.exchanges.map((value) => ({ value, label: exchangeStore.label(value) })),
)
const journalPairOptions = computed(() => journalStore.facets.pairs)

watch(
  () => route.path,
  (path) => {
    if (path === '/algo/journal') void journalStore.loadFacets()
  },
  { immediate: true },
)

// ── Charts state ──

const chartTimeframes = ['1h', '4h', '1d', '1w', '1M', 'All'] as const
const selectedTimeframe = computed(() => {
  const tf = route.query.tf
  return typeof tf === 'string' && chartTimeframes.includes(tf as typeof chartTimeframes[number])
    ? tf
    : '1d'
})

const savedBacktests = computed(() => backtests.backtests.slice(0, 8))
const activeExchanges = computed(() => exchangeStore.exchanges)

watch(
  () => journalStore.filters,
  (filters) => {
    journalDateFrom.value = filters.from_date ?? ''
    journalDateTo.value = filters.to_date ?? ''
    journalBot.value = filters.bot_id ?? ''
    journalMode.value = filters.trading_mode ?? 'paper'
    journalStrategy.value = filters.strategy_id ?? ''
    journalExchange.value = filters.exchange ?? ''
    journalPair.value = filters.pair ?? ''
    journalSide.value = filters.side ?? ''
    journalMinPnl.value = filters.min_pnl ?? null
  },
  { deep: true, immediate: true },
)

watchDebounced(
  [journalDateFrom, journalDateTo, journalMode, journalBot, journalStrategy, journalExchange, journalPair, journalSide, journalMinPnl],
  async () => {
    if (route.path !== '/algo/journal')
      return

    await journalStore.updateFilters({
      from_date: journalDateFrom.value || undefined,
      to_date: journalDateTo.value || undefined,
      bot_id: journalBot.value || undefined,
      trading_mode: (journalMode.value === 'live' ? 'live' : 'paper') as 'paper' | 'live',
      strategy_id: journalStrategy.value || undefined,
      exchange: journalExchange.value || undefined,
      pair: journalPair.value || undefined,
      side: (journalSide.value || undefined) as 'long' | 'short' | undefined,
      min_pnl: journalMinPnl.value ?? undefined,
    })
  },
  { debounce: 300 },
)

async function loadBacktest(id: string) {
  try {
    await backtests.get(id)
  } catch (err) {
    console.error('Failed to load backtest from sidebar:', err)
  }
}

async function selectExchange(exchangeId: string) {
  exchangeStore.setActive(exchangeId)
  try {
    await exchangeStore.refreshBalances(exchangeId)
  } catch (err) {
    console.error('Failed to refresh exchange balances:', err)
  }
}

function setChartTimeframe(tf: string) {
  router.replace({
    path: route.path,
    query: {
      ...route.query,
      tf,
    },
  })
}
</script>

<template>
  <aside class="right-sidebar">
    <div class="right-content">
      <div class="section-title">{{ sectionTitle }}</div>

      <!-- Dashboard context: every bot at once -->
      <template v-if="route.path === '/algo'">
        <div class="context-section">
          <div class="context-sublabel">Across all bots</div>
          <div class="context-row">
            <span class="row-label">Running</span>
            <span class="row-value mono">{{ botsStore.runningCount }} / {{ botsStore.list.length }}</span>
          </div>
          <div class="context-row">
            <span class="row-label">Equity · paper</span>
            <span class="row-value mono">{{ formatCurrency(botsStore.equityByMode.paper) }}</span>
          </div>
          <div class="context-row">
            <span class="row-label">Equity · live</span>
            <span class="row-value mono" :class="botsStore.equityByMode.live ? 'text-error' : ''">{{ formatCurrency(botsStore.equityByMode.live) }}</span>
          </div>
          <div class="context-row">
            <span class="row-label">Today PnL · paper</span>
            <span class="row-value mono" :class="botsStore.pnlPaper.today >= 0 ? 'text-success' : 'text-error'">
              {{ formatPnl(botsStore.pnlPaper.today).text }}
            </span>
          </div>
          <div class="context-row">
            <span class="row-label">Today PnL · live</span>
            <span class="row-value mono" :class="botsStore.pnlLive.today >= 0 ? 'text-success' : 'text-error'">
              {{ formatPnl(botsStore.pnlLive.today).text }}
            </span>
          </div>
          <div class="context-row">
            <span class="row-label">Open · paper / live</span>
            <span class="row-value mono">{{ botsStore.openByMode.paper }} / {{ botsStore.openByMode.live }}</span>
          </div>
        </div>
        <div class="context-section">
          <div class="context-sublabel">Running bots</div>
          <div v-if="!botsStore.running.length" class="context-value text-muted">None</div>
          <div v-for="b in botsStore.running" :key="b.id" class="context-row">
            <span class="row-label bot-label">{{ b.name }}</span>
            <span class="row-value mono" :class="b.trading_mode === 'live' ? 'text-error' : ''">{{ b.pair }}</span>
          </div>
        </div>
      </template>

      <!-- Bots context -->
      <template v-else-if="route.path === '/algo/bots'">
        <div class="context-section">
          <div class="context-sublabel">Summary</div>
          <div class="context-row">
            <span class="row-label">Running</span>
            <span class="row-value mono">{{ botsStore.runningCount }}</span>
          </div>
          <div class="context-row">
            <span class="row-label">Live</span>
            <span class="row-value mono" :class="botsStore.liveRunningCount ? 'text-error' : ''">{{ botsStore.liveRunningCount }}</span>
          </div>
          <div class="context-row">
            <span class="row-label">Equity · paper</span>
            <span class="row-value mono">{{ formatCurrency(botsStore.equityByMode.paper) }}</span>
          </div>
          <div class="context-row">
            <span class="row-label">Equity · live</span>
            <span class="row-value mono" :class="botsStore.equityByMode.live ? 'text-error' : ''">{{ formatCurrency(botsStore.equityByMode.live) }}</span>
          </div>
          <div class="context-row">
            <span class="row-label">Open · paper / live</span>
            <span class="row-value mono">{{ botsStore.openByMode.paper }} / {{ botsStore.openByMode.live }}</span>
          </div>
          <div class="context-row">
            <span class="row-label">PnL all time · paper</span>
            <span class="row-value mono" :class="botsStore.pnlPaper.all >= 0 ? 'text-success' : 'text-error'">{{ formatPnl(botsStore.pnlPaper.all).text }}</span>
          </div>
          <div class="context-row">
            <span class="row-label">PnL all time · live</span>
            <span class="row-value mono" :class="botsStore.pnlLive.all >= 0 ? 'text-success' : 'text-error'">{{ formatPnl(botsStore.pnlLive.all).text }}</span>
          </div>
        </div>
        <div class="context-section">
          <div class="context-sublabel">Model</div>
          <div class="context-value text-muted">
            One runner process per bot, its own budget, journal and equity curve. Paper bots fill on the exchange's live candles.
          </div>
        </div>
      </template>

      <!-- Strategies context -->
      <template v-else-if="route.path === '/algo/strategies'">
        <div v-if="strategies.strategies.length === 0" class="context-empty">
          No strategies saved
        </div>
        <div v-else class="strategy-list">
          <button
            v-for="strat in strategies.strategies"
            :key="strat.id"
            class="strategy-item"
            :class="{ active: strategies.activeStrategyId === strat.id }"
            @click="strategies.select(strat.id)"
          >
            <span class="strategy-name">{{ strat.name }}</span>
          </button>
        </div>
      </template>

      <!-- Backtest context -->
      <template v-else-if="route.path === '/algo/backtest'">
        <div v-if="!savedBacktests.length" class="context-empty">
          No saved results
        </div>
        <div v-else class="strategy-list">
          <button
            v-for="backtest in savedBacktests"
            :key="backtest.id"
            class="strategy-item"
            :class="{ active: backtests.activeBacktestId === backtest.id }"
            @click="loadBacktest(backtest.id)"
          >
            <span class="strategy-name">{{ backtest.name }}</span>
          </button>
        </div>
      </template>

      <!-- Terminal context -->
      <template v-else-if="route.path === '/algo/terminal'">
        <div class="context-section">
          <div class="context-sublabel">Log Stream</div>
          <div class="context-value">
            Every bot's lines, or one bot's — pick it in the toolbar. Lines of all bots carry the bot's name.
          </div>
        </div>
        <div class="context-section">
          <div class="context-sublabel">Running</div>
          <div v-if="!botsStore.running.length" class="context-value text-muted">No bot running</div>
          <div v-for="b in botsStore.running" :key="b.id" class="context-row">
            <span class="row-label bot-label">{{ b.name }}</span>
            <span class="row-value mono">{{ b.pair }}</span>
          </div>
        </div>
      </template>

      <!-- Journal context -->
      <template v-else-if="route.path === '/algo/journal'">
        <div class="context-section">
          <div class="context-sublabel">Date Range</div>
          <div class="filter-row">
            <input
              v-model="journalDateFrom"
              type="date"
              class="input input-sm"
              placeholder="From"
            />
            <input
              v-model="journalDateTo"
              type="date"
              class="input input-sm"
              placeholder="To"
            />
          </div>
        </div>
        <div class="context-section">
          <div class="context-sublabel">Mode</div>
          <select v-model="journalMode" class="input input-sm">
            <option value="paper">Paper</option>
            <option value="live">Live</option>
          </select>
        </div>
        <div class="context-section">
          <div class="context-sublabel">Bot</div>
          <select v-model="journalBot" class="input input-sm">
            <option value="">All bots</option>
            <option v-for="b in botsStore.list" :key="b.id" :value="b.id">{{ b.name }}</option>
          </select>
        </div>
        <div class="context-section">
          <div class="context-sublabel">Strategy</div>
          <select v-model="journalStrategy" class="input input-sm">
            <option value="">All strategies</option>
            <option
              v-for="strat in strategies.strategies"
              :key="strat.id"
              :value="strat.id"
            >
              {{ strat.name }}
            </option>
          </select>
        </div>
        <div class="context-section">
          <div class="context-sublabel">Exchange</div>
          <select v-model="journalExchange" class="input input-sm">
            <option value="">All exchanges</option>
            <option
              v-for="option in journalExchangeOptions"
              :key="option.value"
              :value="option.value"
            >
              {{ option.label }}
            </option>
          </select>
        </div>
        <div class="context-section">
          <div class="context-sublabel">Pair</div>
          <select v-model="journalPair" class="input input-sm">
            <option value="">All pairs</option>
            <option v-for="pair in journalPairOptions" :key="pair" :value="pair">
              {{ pair }}
            </option>
          </select>
        </div>
        <div class="context-section">
          <div class="context-sublabel">Side</div>
          <select v-model="journalSide" class="input input-sm">
            <option value="">All</option>
            <option value="long">Long</option>
            <option value="short">Short</option>
          </select>
        </div>
        <div class="context-section">
          <div class="context-sublabel">Minimum PnL</div>
          <input
            v-model.number="journalMinPnl"
            type="number"
            class="input input-sm"
            placeholder="0"
          />
        </div>
      </template>

      <!-- Charts context -->
      <template v-else-if="route.path === '/algo/charts'">
        <div class="context-section">
          <div class="context-sublabel">Range</div>
          <div class="timeframe-grid">
            <button
              v-for="tf in chartTimeframes"
              :key="tf"
              class="btn btn-sm tf-btn"
              :class="{ active: selectedTimeframe === tf }"
              @click="setChartTimeframe(tf)"
            >
              {{ tf }}
            </button>
          </div>
        </div>
      </template>

      <!-- Exchange context -->
      <template v-else-if="route.path === '/algo/exchange'">
        <div v-if="!activeExchanges.length" class="context-empty">
          No exchanges configured
        </div>
        <div v-else class="strategy-list">
          <button
            v-for="exchange in activeExchanges"
            :key="exchange.id"
            class="strategy-item"
            :class="{ active: exchangeStore.activeExchangeId === exchange.id }"
            @click="selectExchange(exchange.id)"
          >
            <span class="strategy-name">{{ exchange.name }}</span>
            <span class="item-meta">{{ formatDate(exchange.updated_at) }}</span>
          </button>
        </div>
      </template>

    </div>
  </aside>
</template>

<style scoped>
.right-sidebar {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.right-content {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  padding-top: 4px;
}

.section-title {
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--qa-text-muted);
  padding: 16px 16px 8px;
}

.context-section {
  padding: 0 16px;
  margin-bottom: 16px;
}

.context-sublabel {
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--qa-text-muted);
  margin-bottom: 8px;
}

.context-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 5px 0;
  font-size: 12px;
}

.context-row:hover {
  background: var(--qa-bg-hover);
  border-radius: 4px;
}

.row-label {
  color: var(--qa-text-secondary);
}

.bot-label {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  margin-right: 8px;
}

.row-value {
  color: var(--qa-text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  flex-shrink: 0;
  max-width: 50%;
  text-align: right;
}

.context-value {
  font-size: 12px;
  color: var(--qa-text);
}

.context-empty {
  font-size: 12px;
  color: var(--qa-text-muted);
  text-align: center;
  padding: 24px 16px;
}

.mono {
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Fira Code', Menlo, Consolas, monospace;
}

/* ── Strategy list ── */

.strategy-list {
  display: flex;
  flex-direction: column;
  gap: 1px;
  padding: 0 8px;
}

.strategy-item {
  display: flex;
  align-items: center;
  padding: 7px 10px;
  border-radius: 6px;
  background: none;
  border: none;
  cursor: pointer;
  text-align: left;
  color: var(--qa-text-secondary);
  font-size: 12px;
  transition: background var(--qa-transition), color var(--qa-transition);
}

.strategy-item:hover {
  background: var(--qa-bg-hover);
  color: var(--qa-text);
}

.strategy-item.active {
  background: var(--qa-bg-hover);
  color: var(--qa-text);
}

.strategy-name {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.item-meta {
  margin-left: auto;
  font-size: 11px;
  color: var(--qa-text-muted);
}

/* ── Filters ── */

.filter-checks {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.check-label {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: var(--qa-text-secondary);
  cursor: pointer;
  transition: color var(--qa-transition);
}

.check-label:hover {
  color: var(--qa-text);
}

.check-label input[type="checkbox"] {
  accent-color: var(--qa-accent);
  width: 13px;
  height: 13px;
}

.filter-row {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.input-sm {
  padding: 5px 10px;
  font-size: 11px;
  width: 100%;
  max-width: 100%;
  box-sizing: border-box;
}

/* ── Timeframe grid ── */

.timeframe-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 4px;
}

.tf-btn {
  font-size: 11px;
  padding: 5px 4px;
}

.tf-btn.active {
  background: var(--qa-accent);
  color: var(--qa-bg);
  border-color: var(--qa-accent);
}
</style>
