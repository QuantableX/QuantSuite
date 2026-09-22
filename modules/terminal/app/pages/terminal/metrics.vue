<script setup lang="ts">
import {
  ArrowDown,
  ChevronDown,
  Gauge,
  Maximize2,
  Minimize2,
  PanelRight,
  RefreshCw,
  Search,
  Star,
  Unplug,
} from 'lucide-vue-next'
import { indicators as definitions, type Indicator } from '#terminal/data/metrics-catalogue'
import { useMetricsFilters } from '#terminal/composables/useMetricsFilters'
import { useMetricsStore } from '#terminal/stores/metrics'
import { bindings, DAY, formatValue, metricTone, type Point } from '#terminal/utils/metrics'
import HistoryChart from '#terminal/components/metrics/HistoryChart.vue'
import ScoreContext from '#terminal/components/metrics/ScoreContext.vue'
import CardVisual from '#terminal/components/metrics/CardVisual.vue'
import { cardReading } from '#terminal/utils/metric-card'
import { historicalMetrics, calculatedMetrics } from '#terminal/data/historical-metrics'

definePageMeta({ layout: 'terminal' })
const store = useMetricsStore()
const { filters, isFavorite, toggleFavorite, activeFilterCount, resetFilters } = useMetricsFilters()
const originalCatalogue: Indicator[] = [
  {
    title: 'Fear & Greed Index',
    symbol: 'BTC',
    category: 'Sentiment',
    provider: 'Alternative.me',
    status: 'public',
    unit: 'Score',
    formula: 'Provider sentiment index',
    reads: 'Bitcoin market sentiment from extreme fear to extreme greed.',
    sourcePath: 'https://alternative.me/crypto/fear-and-greed-index/',
    thresholds: [],
    icon: Gauge,
  },
  ...definitions,
]
const referenceDefinition = (
  title: string,
  unit: string,
  category: string,
  reads: string,
): Indicator => ({
  title,
  unit,
  category,
  reads,
  symbol: 'BTC',
  icon: Gauge,
  thresholds: [],
  status: 'public',
  provider: bindings[title]!.provider,
  formula: bindings[title]!.formula,
  sourcePath: bindings[title]!.url,
})
const catalogue = [
  ...new Map(
    [
      ...originalCatalogue,
      ...historicalMetrics.map(([title, , , unit, category, reads]) =>
        referenceDefinition(title, unit, category, reads),
      ),
      ...calculatedMetrics.map(([title, unit, category, reads]) =>
        referenceDefinition(title, unit, category, reads),
      ),
    ].map((definition) => [definition.title, definition]),
  ).values(),
]
const selected = computed(
  () => catalogue.find((i) => i.title === store.selectedTitle) ?? catalogue[0]!,
)
const selectedBinding = computed(() => bindings[store.selectedTitle]!)
const selectedFeed = computed(() => store.metricState(store.selectedTitle))
const referencePrice = computed(() => store.comparisonAsset === 'BTC')
const comparisonAsset = computed(() =>
  store.comparisonAsset === 'BTC-SPOT' ? 'BTC' : store.comparisonAsset,
)
const quote = computed(() => (referencePrice.value ? 'USD' : 'USDT'))
const priceProvider = computed(() =>
  referencePrice.value ? 'Coin Metrics · daily USD reference' : 'Binance · daily close',
)
const priceFeed = computed(() =>
  referencePrice.value ? store.state('network') : store.state('price', comparisonAsset.value),
)
const priceData = computed(() => priceFeed.value.data)
const priceRows = computed(() =>
  referencePrice.value
    ? (priceData.value?.rows ?? [])
        .filter((r) => r.values.PriceUSD! > 0)
        .map((r) => ({ ...r, values: { close: r.values.PriceUSD! } }))
    : (priceData.value?.rows ?? []),
)
const prices = computed<Point[]>(() =>
  priceRows.value
    .filter((p) => Number.isFinite(p.values.close))
    .map((p) => ({ time: p.time, value: p.values.close! })),
)
const networkSpot = computed<Point[]>(() =>
  (store.state('network').data?.rows ?? [])
    .filter((row) => Number.isFinite(row.values.PriceUSD) && row.values.PriceUSD! > 0)
    .map((row) => ({ time: row.time, value: row.values.PriceUSD! })),
)
const cardReadings = computed(() =>
  Object.fromEntries(
    Object.keys(bindings).map((title) => [
      title,
      cardReading(title, store.series[title] ?? [], networkSpot.value),
    ]),
  ),
)
const includeEarly = ref(false)
const rawHistory = computed(() => store.series[store.selectedTitle] ?? [])
const hiddenEarly = computed(
  () => rawHistory.value.filter((p) => p.time < (selectedBinding.value.analysisStart ?? 0)).length,
)
const history = computed(() =>
  includeEarly.value
    ? rawHistory.value
    : rawHistory.value.filter((p) => p.time >= (selectedBinding.value.analysisStart ?? 0)),
)
const windowEnd = computed(() =>
  Math.max(history.value.at(-1)?.time ?? 0, prices.value.at(-1)?.time ?? 0),
)
const windowStart = computed(() =>
  store.rangeDays === 0
    ? Math.max(history.value[0]?.time ?? 0, prices.value[0]?.time ?? 0)
    : windowEnd.value - (store.rangeDays - 1) * DAY,
)
const visibleHistory = computed(() => history.value.filter((p) => p.time >= windowStart.value))
const missingDays = computed(() => {
  const points = visibleHistory.value
  return points.length > 1
    ? Math.round((points.at(-1)!.time - points[0]!.time) / DAY) + 1 - points.length
    : 0
})
const lab = ref<HTMLElement | null>(null)
const pageRoot = ref<HTMLElement | null>(null)
const librarySection = ref<HTMLElement | null>(null)
const expanded = ref(false)
const showContext = ref(true)
function escapeFocus(event: KeyboardEvent) {
  if (event.key === 'Escape') expanded.value = false
}
const libraryMode = ref<'connected' | 'all' | 'unavailable'>('connected')
const periods = [
  { label: '30D', days: 30 },
  { label: '90D', days: 90 },
  { label: '1Y', days: 365 },
  { label: '3Y', days: 1095 },
  { label: '5Y', days: 1826 },
  { label: 'Max', days: 0 },
]
const connectedCount = Object.keys(bindings).length
const errors = computed(() =>
  Object.entries(store.feeds).filter(
    ([key, feed]) =>
      feed.error &&
      (!key.startsWith('price:') ||
        (!referencePrice.value && key === `price:${comparisonAsset.value}`)),
  ),
)
const lastChecked = computed(() =>
  Math.max(0, ...Object.values(store.feeds).map((f) => f.attemptedAt ?? 0)),
)
const checkedLabel = computed(() =>
  lastChecked.value
    ? new Date(lastChecked.value).toLocaleTimeString([], {
        hour: '2-digit',
        minute: '2-digit',
      })
    : 'Not checked',
)
const library = computed(() =>
  catalogue.filter((indicator) => {
    const binding = bindings[indicator.title],
      point = store.series[indicator.title]?.at(-1)
    if (libraryMode.value === 'connected' && !binding) return false
    if (libraryMode.value === 'unavailable' && binding) return false
    const source = binding
      ? binding.derived || binding.calculate || indicator.title === 'Puell Multiple'
        ? 'derived'
        : binding.provider.startsWith('Binance')
          ? 'exchange'
          : 'public'
      : indicator.status
    if (filters.value.source !== 'all' && source !== filters.value.source) return false
    if (filters.value.favorite === 'favorites' && !isFavorite(indicator.title)) return false
    if (filters.value.favorite === 'non-favorites' && isFavorite(indicator.title)) return false
    if (
      filters.value.tone !== 'all' &&
      (!point || metricTone(indicator.title, point.value) !== filters.value.tone)
    )
      return false
    if (filters.value.symbol !== 'all' && indicator.symbol !== filters.value.symbol) return false
    if (filters.value.category !== 'all' && indicator.category !== filters.value.category)
      return false
    const query = filters.value.search.trim().toLowerCase()
    return (
      !query ||
      [
        indicator.title,
        indicator.symbol,
        indicator.category,
        binding?.provider,
        binding?.formula,
        indicator.reads,
        indicator.formula,
      ]
        .join(' ')
        .toLowerCase()
        .includes(query)
    )
  }),
)

function latest(title: string) {
  return store.series[title]?.at(-1)
}
function connectionHint(indicator: Indicator) {
  if (indicator.title === 'Perps DEX Volume') return 'DefiLlama paid API required'
  if (/Heatmap|Max Pain|HODL Waves/.test(indicator.title))
    return 'Historical feed + dedicated view needed'
  if (/Liquidation/.test(indicator.title)) return 'Recorded liquidation history needed'
  if (
    /SOPR|Realized Profit|Realized Loss|Supply in Profit|Spent|CDD|Coin Days|Dormancy|Liveliness/.test(
      indicator.title,
    )
  )
    return 'Spent-output / cost-basis history needed'
  if (/Holder|Miner|Exchange|Whale|Illiquid|Liquid Supply/.test(indicator.title))
    return 'Wallet labels + historical balances needed'
  if (/Options|Put\/Call|Implied Volatility/.test(indicator.title))
    return 'Historical options feed needed'
  if (/Yield|Borrow/.test(indicator.title)) return 'Protocol or pool feed needed'
  return 'Additional data or model integration needed'
}
function date(time?: number) {
  return time ? new Date(time * 1000).toISOString().slice(0, 10) : 'No observation'
}
function quality(title: string) {
  const binding = bindings[title]
  if (!binding) return 'No feed connected'
  const state = store.metricState(title),
    point = latest(title)
  if (state.loading && !point) return 'Loading history'
  if (state.error) return point ? 'Cached · refresh failed' : 'Feed unavailable'
  if (!point) return 'No observations'
  if (store.now - point.time > 3 * DAY) return 'Data delayed'
  if (binding.source.startsWith('utxo-')) return 'Daily · on-chain model'
  return point.provisional ? 'Daily · provisional' : 'Daily · sourced'
}
function delta(title: string) {
  const points = store.series[title] ?? [],
    last = points.at(-1)
  if (!last) return '—'
  const previous = points.find((p) => p.time === last.time - 7 * DAY)
  if (!previous) return '7D change unavailable'
  const value = last.value - previous.value
  return `${value > 0 ? '+' : ''}${formatValue(value, bindings[title]?.unit)} · 7D`
}
async function choose(title: string, scroll = true) {
  if (!bindings[title]) return
  store.selectedTitle = title
  if (scroll) {
    await nextTick()
    pageRoot.value?.scrollTo({ top: 0, behavior: 'instant' })
    lab.value?.querySelector<HTMLSelectElement>('select')?.focus({ preventScroll: true })
  }
}
let poll: ReturnType<typeof setInterval> | null = null
function start() {
  void store.refresh()
  if (!poll)
    poll = setInterval(
      () => {
        if (!document.hidden) void store.refresh()
      },
      5 * 60 * 1000,
    )
}
function stop() {
  if (poll) clearInterval(poll)
  poll = null
}
watch(
  () => store.comparisonAsset,
  (asset) => {
    if (asset !== 'BTC') void store.load('price', asset === 'BTC-SPOT' ? 'BTC' : asset)
  },
)
onMounted(() => {
  start()
  document.addEventListener('keydown', escapeFocus)
})
onActivated(start)
onDeactivated(stop)
onBeforeUnmount(() => {
  stop()
  document.removeEventListener('keydown', escapeFocus)
})
</script>

<template>
  <div ref="pageRoot" class="metrics-page">
    <div class="analysis-screen">
      <header class="metrics-topbar">
        <div class="brand">
          <h1>Market metrics</h1>
          <span class="brand-meta">Checked {{ checkedLabel }}</span>
        </div>
        <div class="topbar-actions">
          <div class="feed-counter">
            <i :class="{ warning: errors.length }" /><strong>{{ store.availableCount }}</strong
            ><span>histories available</span>
          </div>
          <button class="refresh-button" :disabled="!store.canRefresh" @click="store.refresh(true)">
            <RefreshCw :size="13" :class="{ spinning: store.loading }" />{{
              store.loading ? 'Loading…' : 'Refresh'
            }}
            <span v-if="store.refreshing">{{ store.refreshCompleted }}/{{ store.refreshTotal }}</span>
          </button>
        </div>
      </header>

      <details v-if="errors.length" class="feed-errors">
        <summary>
          {{ errors.length }}
          {{ errors.length === 1 ? 'feed needs' : 'feeds need' }} attention
          <ChevronDown :size="13" />
        </summary>
        <div v-for="[key, feed] in errors" :key="key">
          <strong>{{ key }}</strong> {{ feed.error }}
        </div>
      </details>

      <Teleport to="body" :disabled="!expanded">
        <section
          ref="lab"
          data-module="terminal"
          class="analysis-lab"
          :class="{ expanded }"
          aria-label="Metric analysis workspace"
        >
          <header class="analysis-controls">
            <label class="metric-select"
              ><span>Indicator</span
              ><select v-model="store.selectedTitle" aria-label="Analysis indicator">
                <option v-for="(_, title) in bindings" :key="title" :value="title">
                  {{ title }}
                </option>
              </select></label
            >
            <span class="versus">vs.</span>
            <label class="asset-select"
              ><span>Price</span
              ><select v-model="store.comparisonAsset" aria-label="Comparison asset">
                <option
                  v-for="asset in ['BTC', 'BTC-SPOT', 'ETH', 'SOL', 'BNB', 'XRP', 'AVAX']"
                  :key="asset"
                  :value="asset"
                >
                  {{
                    asset === 'BTC'
                      ? 'BTC/USD · full history'
                      : asset === 'BTC-SPOT'
                        ? 'BTC/USDT · Binance'
                        : `${asset}/USDT`
                  }}
                </option>
              </select></label
            >
            <div class="periods" aria-label="Chart period">
              <button
                v-for="period in periods"
                :key="period.days"
                :class="{ active: store.rangeDays === period.days }"
                :aria-pressed="store.rangeDays === period.days"
                :title="period.days === 0 ? 'All shared indicator and price history' : period.label"
                @click="store.rangeDays = period.days"
              >
                {{ period.label }}
              </button>
            </div>
            <div class="workspace-actions">
              <button
                class="focus-button"
                :aria-pressed="showContext"
                @click="showContext = !showContext"
              >
                <PanelRight :size="15" />Context
              </button>
              <button class="focus-button" :aria-pressed="expanded" @click="expanded = !expanded">
                <Minimize2 v-if="expanded" :size="15" /><Maximize2 v-else :size="15" />{{
                  expanded ? 'Exit focus' : 'Focus'
                }}
              </button>
            </div>
          </header>
          <div class="coverage-line">
            <span
              ><i
                :class="{
                  warning:
                    selectedFeed.error ||
                    !history.length ||
                    quality(store.selectedTitle) === 'Data delayed',
                }"
              />{{ quality(store.selectedTitle) }}</span
            ><span
              >{{ visibleHistory.length }} observations · {{ date(visibleHistory[0]?.time) }} →
              {{ date(visibleHistory.at(-1)?.time) }} UTC</span
            ><span v-if="missingDays" class="coverage-gaps">{{ missingDays }} missing days</span
            ><button
              v-if="hiddenEarly && store.rangeDays === 0"
              class="early-toggle"
              :aria-pressed="includeEarly"
              title="Early price discovery and cost-basis initialization; retained as raw provider observations."
              @click="includeEarly = !includeEarly"
            >
              {{
                includeEarly ? 'Hide pre-2011 data' : `Include pre-2011 data (${hiddenEarly})`
              }}</button
            ><span v-if="selectedFeed.data"
              >Fetched {{ new Date(selectedFeed.data.fetchedAt * 1000).toLocaleTimeString() }}</span
            >
          </div>
          <div v-if="selectedFeed.loading || priceFeed.loading" class="loading-line" role="status">
            <RefreshCw :size="12" class="spinning" /> Loading
            {{ selectedFeed.loading ? 'indicator history' : `${store.comparisonAsset} prices` }}…
          </div>
          <div v-if="selectedFeed.error || priceFeed.error" class="analysis-error" role="alert">
            {{ selectedFeed.error ? `Indicator: ${selectedFeed.error}` : '' }}
            {{ priceFeed.error ? `Price: ${priceFeed.error}` : ''
            }}<button :disabled="!store.canRefresh" @click="store.refresh(true)">Retry feeds</button>
          </div>
          <div class="analysis-body" :class="{ 'without-context': !showContext }">
            <HistoryChart
              :title="store.selectedTitle"
              :unit="selectedBinding.unit"
              :points="history"
              :prices="priceRows"
              :asset="comparisonAsset"
              :quote="quote"
              :price-provider="priceProvider"
              :range-days="store.rangeDays"
            /><ScoreContext
              v-if="showContext"
              :title="store.selectedTitle"
              :points="visibleHistory"
              :prices="prices"
              :asset="comparisonAsset"
              :quote="quote"
            />
          </div>
          <footer class="formula-strip">
            <span>FORMULA</span><code>{{ selectedBinding.formula }}</code
            ><span class="scope-label"
              >Indicator: {{ selected.symbol }} · Price: {{ comparisonAsset }}/{{ quote }}</span
            >
          </footer>
        </section>
      </Teleport>

      <button
        class="library-jump"
        @click="librarySection?.scrollIntoView({ behavior: 'smooth', block: 'start' })"
      >
        <ArrowDown :size="14" /> Browse metric library
        <span>{{ connectedCount }} connected</span>
      </button>
    </div>
    <section ref="librarySection" class="metric-library" aria-label="Metric library">
      <div class="library-heading">
        <div>
          <h2>
            Metric library <span>{{ library.length }}</span>
          </h2>
        </div>
        <div class="library-tabs">
          <button
            :class="{ active: libraryMode === 'connected' }"
            @click="libraryMode = 'connected'"
          >
            Connected <span>{{ connectedCount }}</span></button
          ><button
            :class="{ active: libraryMode === 'unavailable' }"
            @click="libraryMode = 'unavailable'"
          >
            No feed <span>{{ catalogue.length - connectedCount }}</span></button
          ><button :class="{ active: libraryMode === 'all' }" @click="libraryMode = 'all'">
            All
          </button>
        </div>
      </div>
      <div v-if="libraryMode !== 'connected'" class="catalogue-note">
        <Unplug :size="14" /> These definitions need additional datasets or models. Some require
        paid API access, wallet labels or recorded trading history; their requirements appear below
        each title.
      </div>
      <div class="inline-search">
        <Search :size="13" /><input
          v-model="filters.search"
          aria-label="Search metrics"
          placeholder="Find a metric, source or formula…"
        /><button v-if="activeFilterCount" @click="resetFilters">
          Reset filters ({{ activeFilterCount }})
        </button>
      </div>
      <div v-if="!library.length" class="empty-library">
        No metrics match these filters.<button @click="resetFilters">Reset filters</button>
      </div>
      <div class="metrics-grid">
        <article
          v-for="indicator in library"
          :key="indicator.title"
          class="metric-card"
          :style="{ '--metric-accent': cardReadings[indicator.title]?.color }"
          :class="{
            'is-selected': store.selectedTitle === indicator.title,
            'is-unavailable': !bindings[indicator.title],
          }"
        >
          <button
            v-if="bindings[indicator.title]"
            type="button"
            class="card-hit-target"
            :aria-label="`Analyze ${indicator.title}. ${cardReadings[indicator.title]?.label ?? ''}. ${cardReadings[indicator.title]?.reference ?? ''}`"
            :aria-pressed="store.selectedTitle === indicator.title"
            :title="indicator.reads"
            @click="choose(indicator.title)"
          />
          <header class="metric-card-head">
            <span class="metric-icon"><component :is="indicator.icon" :size="15" /></span>
            <div>
              <span class="metric-category">{{ indicator.symbol }} · {{ indicator.category }}</span>
              <h3>{{ indicator.title }}</h3>
              <span class="metric-source">{{
                bindings[indicator.title]?.provider || connectionHint(indicator)
              }}</span>
            </div>
            <button
              class="favorite-button"
              :class="{ favorited: isFavorite(indicator.title) }"
              :aria-label="`${isFavorite(indicator.title) ? 'Remove' : 'Add'} ${indicator.title} ${isFavorite(indicator.title) ? 'from' : 'to'} favorites`"
              :aria-pressed="isFavorite(indicator.title)"
              @click.stop="toggleFavorite(indicator.title)"
            >
              <Star :size="13" :fill="isFavorite(indicator.title) ? 'currentColor' : 'none'" />
            </button>
          </header>
          <template v-if="bindings[indicator.title]">
            <div class="card-reading">
              <div>
                <strong
                  >{{ formatValue(latest(indicator.title)?.value, bindings[indicator.title]?.unit)
                  }}<small v-if="bindings[indicator.title]?.unit !== '%'" class="card-unit">{{
                    bindings[indicator.title]?.unit
                  }}</small></strong
                ><span>{{ delta(indicator.title) }}</span>
              </div>
              <CardVisual
                :title="indicator.title"
                :points="store.series[indicator.title] ?? []"
                :reading="cardReadings[indicator.title]!"
                :spot="networkSpot"
              />
            </div>
            <div class="card-interpretation">
              <strong>{{ cardReadings[indicator.title]?.label }}</strong>
              <span>{{ cardReadings[indicator.title]?.reference }}</span>
            </div>
            <div class="card-status">
              <span>{{ quality(indicator.title) }}</span
              ><time>{{ date(latest(indicator.title)?.time) }} UTC</time>
            </div></template
          >
          <div v-else class="unavailable-reading">
            <span>—</span><span><Unplug :size="12" /> No feed connected</span>
          </div>
          <p class="metric-description" :title="indicator.reads">
            {{ indicator.reads }}
          </p>
        </article>
      </div>
    </section>
    <footer class="page-footer">
      <ArrowDown :size="11" /><span
        >Values update when the source publishes a new observation. Refreshing checks the source; it
        does not generate a new score.</span
      >
    </footer>
  </div>
</template>

<style scoped>
.metrics-page,
.analysis-lab {
  --muted: color-mix(in srgb, var(--text-primary) 72%, var(--surface-1));
}
.metrics-page {
  height: 100%;
  overflow: auto;
  padding: var(--qss-surface-padding, 24px);
  min-width: 0;
  color: var(--text-primary);
  container: metrics / inline-size;
}
.analysis-screen {
  height: 100%;
  min-height: 500px;
  display: flex;
  flex-direction: column;
}
.library-jump {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  min-height: 34px;
  padding: 9px 0 0;
  border: 0;
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
}
.library-jump span {
  color: var(--muted);
  font-size: 11px;
}
.library-jump:hover {
  color: var(--text-primary);
}
.metrics-topbar,
.topbar-actions,
.feed-counter,
.refresh-button {
  display: flex;
  align-items: center;
}
.metrics-topbar {
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 22px;
  flex-shrink: 0;
}
.brand {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 4px;
}
h1 {
  margin: 0;
  font-size: 27px;
  font-weight: 500;
  letter-spacing: -0.7px;
  line-height: 36px;
}
h1 span {
  color: var(--text-secondary);
}
.brand-meta {
  color: var(--muted);
  font-size: 11px;
}
.topbar-actions {
  gap: 16px;
}
.feed-counter {
  gap: 6px;
  font-size: 12px;
  color: var(--text-secondary);
}
.feed-counter strong {
  font-family: var(--qss-font-mono);
  color: var(--text-primary);
  font-size: 14px;
}
.feed-counter i,
.coverage-line i {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--positive);
  display: inline-block;
}
.feed-counter i.warning,
.coverage-line i.warning {
  background: var(--warning);
}
button {
  cursor: pointer;
}
.refresh-button,
.focus-button {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 7px 10px;
  border: 1px solid var(--border);
  border-radius: 5px;
  font-size: 12px;
  background: var(--surface-1);
  color: var(--text-primary);
  white-space: nowrap;
}
.refresh-button:hover,
.focus-button:hover {
  background: var(--surface-2);
}
.focus-button[aria-pressed='true'] {
  background: var(--surface-2);
  border-color: var(--muted);
}
button:disabled {
  opacity: 0.5;
  cursor: wait;
}
.feed-errors {
  margin-bottom: 12px;
  border: 1px solid color-mix(in srgb, var(--warning) 30%, var(--border));
  border-radius: 5px;
  padding: 10px 12px;
  font-size: 12px;
  color: var(--warning);
}
.feed-errors summary {
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 8px;
}
.feed-errors > div {
  color: var(--text-secondary);
  margin-top: 8px;
  word-break: break-word;
  font-size: 12px;
}
.feed-errors strong {
  margin-right: 10px;
}
.analysis-lab {
  container: analysis / inline-size;
  flex: 1;
  min-height: 0;
  border: 1px solid var(--border);
  border-radius: 7px;
  background: var(--surface-1);
  overflow: hidden;
  scroll-margin-top: 8px;
  display: flex;
  flex-direction: column;
}
.analysis-lab.expanded {
  position: fixed;
  inset: 42px 14px 40px;
  z-index: 200;
  overflow: auto;
  box-shadow: 0 0 0 100vmax #0009;
}
.analysis-controls {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  border-bottom: 1px solid var(--border);
  flex-wrap: wrap;
  flex-shrink: 0;
}
.analysis-controls label {
  display: flex;
  align-items: center;
  gap: 7px;
}
.analysis-controls label > span {
  color: var(--muted);
  font-size: 11px;
}
.analysis-controls select {
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 7px 24px 7px 8px;
  background: var(--surface-0);
  color: var(--text-primary);
  font-size: 12px;
  max-width: 240px;
}
.versus {
  color: var(--muted);
  font-size: 12px;
}
.periods {
  display: flex;
  gap: 2px;
  margin-left: auto;
}
.periods button {
  background: transparent;
  border: 1px solid transparent;
  border-radius: 4px;
  color: var(--text-secondary);
  font-size: 12px;
  padding: 7px 8px;
}
.periods button.active {
  color: var(--text-primary);
  background: var(--surface-2);
  border-color: var(--border);
}
.periods button:hover {
  color: var(--text-primary);
}
.workspace-actions {
  display: flex;
  gap: 6px;
}
.coverage-line {
  display: flex;
  align-items: center;
  gap: 14px;
  flex-wrap: wrap;
  padding: 8px 12px;
  border-bottom: 1px solid var(--border);
  color: var(--muted);
  font-size: 11px;
  flex-shrink: 0;
}
.coverage-line > span:first-child {
  display: flex;
  align-items: center;
  gap: 6px;
}
.coverage-line > span:last-child {
  margin-left: auto;
}
.early-toggle {
  background: transparent;
  border: 1px solid var(--border);
  border-radius: 4px;
  color: var(--text-secondary);
  padding: 3px 6px;
  font-size: 11px;
  cursor: pointer;
}
.early-toggle[aria-pressed='true'] {
  color: #70b5e8;
  border-color: #70b5e8;
}
.coverage-gaps {
  color: #e7ad53;
}
.analysis-body {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 300px;
  flex: 1;
  min-height: 0;
  overflow: auto;
}
.analysis-body.without-context {
  grid-template-columns: minmax(0, 1fr);
}
.expanded .analysis-body {
  flex: 1;
  height: auto;
  min-height: 0;
}
.formula-strip {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
  padding: 8px 12px;
  border-top: 1px solid var(--border);
  color: var(--muted);
  font-size: 11px;
  flex-shrink: 0;
}
.formula-strip > span:first-child {
  font-size: 10px;
  letter-spacing: 0.05em;
}
code {
  color: var(--text-secondary);
  font-size: 11px;
  font-family: var(--qss-font-mono);
}
.scope-label {
  margin-left: auto;
}
.loading-line {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 12px;
  color: var(--text-secondary);
  font-size: 12px;
  flex-shrink: 0;
}
.analysis-error {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  color: var(--warning);
  font-size: 12px;
  overflow-wrap: anywhere;
  flex-shrink: 0;
}
.analysis-error button {
  flex-shrink: 0;
  color: var(--text-primary);
  background: var(--surface-2);
  border: 1px solid var(--border);
  padding: 5px 8px;
  border-radius: 3px;
}
.metric-library {
  margin-top: 22px;
}
.library-heading {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
}
.library-heading h2 {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 17px;
  font-weight: 550;
  margin: 0;
}
.library-heading h2 > span {
  font-family: var(--qss-font-mono);
  color: var(--muted);
  font-size: 12px;
}
.library-tabs {
  display: flex;
  gap: 5px;
}
.library-tabs button {
  border: 1px solid transparent;
  border-radius: 5px;
  padding: 7px 10px;
  font-size: 12px;
  background: transparent;
  color: var(--text-secondary);
}
.library-tabs button.active {
  border-color: var(--border);
  background: var(--surface-1);
  color: var(--text-primary);
}
.library-tabs button > span {
  color: var(--muted);
  font-family: var(--qss-font-mono);
  margin-left: 4px;
}
.inline-search {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 10px 0 12px;
  color: var(--muted);
}
.inline-search input {
  flex: 1;
  min-width: 0;
  background: transparent;
  border: 0;
  outline: 0;
  color: var(--text-primary);
  font-size: 12px;
  padding: 6px 0;
}
.inline-search button,
.empty-library button {
  background: transparent;
  border: 0;
  color: var(--text-secondary);
  font-size: 12px;
  text-decoration: underline;
}
.catalogue-note {
  display: flex;
  gap: 8px;
  align-items: center;
  color: var(--muted);
  margin-top: 12px;
  font-size: 12px;
}
.metrics-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  gap: 10px;
}
.metric-card {
  position: relative;
  isolation: isolate;
  min-width: 0;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--surface-1);
  overflow: hidden;
  padding: 12px;
  transition:
    border-color 0.15s,
    background 0.15s;
}
.metric-card:not(.is-unavailable):hover {
  border-color: #70b5e8;
  background: color-mix(in srgb, #70b5e8 4%, var(--surface-1));
}
.metric-card.is-selected {
  border-color: #70b5e8;
}
.card-hit-target {
  position: absolute;
  inset: 0;
  z-index: 1;
  width: 100%;
  height: 100%;
  padding: 0;
  border: 0;
  border-radius: inherit;
  background: transparent;
}
.card-hit-target:focus-visible {
  outline: 2px solid #70b5e8;
  outline-offset: -3px;
}
.metric-card-head {
  display: flex;
  align-items: center;
  gap: 8px;
}
.metric-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--metric-accent, var(--text-secondary));
  background: color-mix(in srgb, var(--metric-accent, var(--text-secondary)) 8%, transparent);
  width: 28px;
  height: 28px;
  border: 1px solid var(--border);
  border-radius: 5px;
  flex-shrink: 0;
}
.metric-card-head > div {
  min-width: 0;
  flex: 1;
}
.metric-category {
  display: block;
  color: var(--muted);
  font-size: 10px;
  line-height: 1.3;
  text-transform: uppercase;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
h3 {
  margin: 3px 0 0;
  font-size: 13px;
  line-height: 1.3;
  font-weight: 600;
}
.metric-source {
  display: block;
  margin-top: 3px;
  font-size: 11px;
  line-height: 1.3;
  color: var(--muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.favorite-button {
  position: relative;
  z-index: 2;
  align-self: start;
  color: var(--muted);
  background: transparent;
  border: 0;
  padding: 4px;
  flex-shrink: 0;
}
.favorite-button:hover,
.favorite-button.favorited {
  color: var(--warning);
}
.card-reading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin: 5px 0 4px;
  min-height: 66px;
  color: var(--text-primary);
}
.card-reading > div {
  flex-shrink: 0;
}
.card-reading strong {
  display: block;
  font-size: 23px;
  line-height: 1.15;
  letter-spacing: -0.5px;
  font-weight: 550;
  font-family: var(--qss-font-mono);
}
.card-unit {
  margin-left: 5px;
  font-size: 10px;
  color: var(--muted);
  font-weight: 400;
  letter-spacing: 0;
}
.card-reading span {
  display: block;
  color: var(--text-secondary);
  font-size: 11px;
  line-height: 1.3;
  font-family: var(--qss-font-mono);
  margin-top: 4px;
}
.card-interpretation {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 3px 8px;
  margin-bottom: 8px;
  font-size: 11px;
  line-height: 1.35;
}
.card-interpretation strong {
  color: var(--metric-accent);
  font-weight: 550;
}
.card-interpretation span {
  color: var(--muted);
  font-size: 10px;
}
.card-status {
  display: flex;
  gap: 6px;
  justify-content: space-between;
  flex-wrap: wrap;
  color: var(--muted);
  font-size: 10px;
  line-height: 1.3;
}
.metric-description {
  color: var(--text-secondary);
  padding: 0;
  font-size: 12px;
  line-height: 1.45;
  margin: 8px 0 0;
  display: -webkit-box;
  -webkit-line-clamp: 1;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.unavailable-reading {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin: 12px 0;
  color: var(--muted);
}
.unavailable-reading > span:first-child {
  font-size: 25px;
}
.unavailable-reading > span:last-child {
  display: flex;
  gap: 6px;
  align-items: center;
  font-size: 12px;
}
.is-unavailable .metric-icon {
  opacity: 0.55;
}
.empty-library {
  padding: 35px;
  display: flex;
  gap: 12px;
  align-items: center;
  justify-content: center;
  font-size: 12px;
  color: var(--muted);
}
.page-footer {
  display: flex;
  gap: 7px;
  align-items: center;
  margin-top: 18px;
  font-size: 11px;
  line-height: 1.5;
  color: var(--muted);
}
.spinning {
  animation: metrics-spin 1.1s linear infinite;
}
@keyframes metrics-spin {
  to {
    transform: rotate(360deg);
  }
}
button:focus-visible,
select:focus-visible,
input:focus-visible,
summary:focus-visible {
  outline: 2px solid #70b5e8;
  outline-offset: 2px;
}
@container analysis (max-width: 1000px) {
  .analysis-body {
    display: block;
  }
  .analysis-body :deep(.history-chart) {
    height: 100%;
    min-height: 320px;
  }
  .analysis-body :deep(.score-context) {
    border-left: 0;
    border-top: 1px solid var(--border);
  }
  .coverage-line > span:last-child {
    margin-left: 0;
  }
  .periods {
    margin-left: 0;
  }
  .workspace-actions {
    margin-left: auto;
  }
}
@container metrics (max-width: 1000px) {
  .brand-meta {
    display: none;
  }
}
@container metrics (max-width: 650px) {
  .metrics-grid {
    grid-template-columns: minmax(0, 1fr);
  }
  .feed-counter {
    display: none;
  }
  .library-heading {
    flex-wrap: wrap;
  }
}
@media (prefers-reduced-motion: reduce) {
  .spinning {
    animation: none;
  }
}
</style>
