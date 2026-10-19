<script setup lang="ts">
import { useMarketStore } from '#terminal/stores/market'
import { useMetricsFilters } from '#terminal/composables/useMetricsFilters'
import { useOrderbookTick } from '#terminal/composables/useOrderbookTick'
import { ChevronDown, RotateCcw, Search, Star } from 'lucide-vue-next'

const route = useRoute()
// V3: the panel is the shared QRightPanel — 220px default, drag-resizable.
const marketStore = useMarketStore()
const {
  filters,
  favoriteCount,
  activeFilterCount,
  resetFilters,
  sourceOptions,
  favoriteOptions,
  toneOptions,
  symbolOptions,
  categoryOptions,
} = useMetricsFilters()

// Trailing-slash tolerant — a hard load of `/terminal/` must still show the
// order book.
const cleanPath = computed(() => route.path.replace(/\/+$/, '') || '/')
const isTerminal = computed(() => cleanPath.value === '/terminal')
const isMetrics = computed(() => cleanPath.value === '/terminal/metrics')
const isPortfolio = computed(() => cleanPath.value === '/terminal/portfolio')

const headerLabel = computed(() => {
  if (isTerminal.value) return 'ORDER BOOK'
  if (isMetrics.value) return 'FILTERS'
  if (isPortfolio.value) return 'PORTFOLIO'
  return 'WATCHLIST'
})

// Symbol selector: every Binance USDT pair, busiest first. A search puts the
// tokens that start with it first, then the ones that contain it. It picks
// the order book's pair only — the chart keeps its own symbol.
const showSymbolMenu = ref(false)
const symbolQuery = ref('')
const symbolSearchEl = ref<HTMLInputElement | null>(null)
const symbolListEl = ref<HTMLElement | null>(null)

const shownSymbols = computed(() => {
  const q = symbolQuery.value.trim().toUpperCase()
  if (!q) return marketStore.symbols
  const starts = marketStore.symbols.filter((s) => s.base.toUpperCase().startsWith(q))
  const contains = marketStore.symbols.filter((s) => !s.base.toUpperCase().startsWith(q) && s.base.toUpperCase().includes(q))
  return [...starts, ...contains]
})

function shortSymbol(sym: string): string {
  return sym.replace('/USDT', '')
}

function toggleSymbolMenu() {
  showSymbolMenu.value = !showSymbolMenu.value
  showTickMenu.value = false
  if (!showSymbolMenu.value) return
  symbolQuery.value = ''
  void marketStore.loadSymbols()
  void nextTick(() => {
    symbolSearchEl.value?.focus()
    symbolListEl.value?.querySelector('.is-active')?.scrollIntoView({ block: 'nearest' })
  })
}

function onSelectSymbol(sym: string) {
  marketStore.setOrderBookSymbol(sym)
  showSymbolMenu.value = false
}

function pickFirstSymbol() {
  const first = shownSymbols.value[0]
  if (first) onSelectSymbol(first.symbol)
}

// Tick size selector (shared with Orderbook via composable); its steps follow the pair.
const { options: tickOptions, tickSize, selectTick, formatTick } = useOrderbookTick()
const showTickMenu = ref(false)

function onSelectTick(t: number) {
  selectTick(t)
  showTickMenu.value = false
}

function closeMenus() {
  showTickMenu.value = false
  showSymbolMenu.value = false
}
</script>

<template>
  <QRightPanel storage-key="terminal.right" @click="closeMenus">
    <!-- Header -->
    <div class="sidebar-header">
      <span class="header-label">{{ headerLabel }}</span>
      <div class="header-spacer" />
      <!-- Symbol selector (terminal only) -->
      <div v-if="isTerminal" class="tick-selector symbol-selector" @click.stop>
        <button class="tick-btn" @click="toggleSymbolMenu">
          {{ shortSymbol(marketStore.orderBookSymbol) }}
          <ChevronDown class="tick-chevron" />
        </button>
        <div v-if="showSymbolMenu" class="symbol-menu">
          <label class="symbol-search-box">
            <Search class="symbol-search-icon" />
            <input
              ref="symbolSearchEl"
              v-model="symbolQuery"
              class="symbol-search"
              placeholder="Search token"
              aria-label="Search token"
              spellcheck="false"
              autocomplete="off"
              @keydown.enter="pickFirstSymbol"
              @keydown.esc="showSymbolMenu = false"
            />
          </label>
          <div ref="symbolListEl" class="symbol-list">
            <button
              v-for="s in shownSymbols"
              :key="s.symbol"
              class="symbol-row"
              :class="{ 'is-active': s.symbol === marketStore.orderBookSymbol }"
              @click="onSelectSymbol(s.symbol)"
            >
              <span class="symbol-base">{{ s.base }}</span>
              <span class="symbol-quote">{{ s.quote }}</span>
            </button>
            <p v-if="marketStore.symbolsLoading" class="symbol-note">Loading tokens…</p>
            <p v-else-if="marketStore.symbolsError" class="symbol-note" :title="marketStore.symbolsError">Tokens could not be loaded</p>
            <p v-else-if="!shownSymbols.length" class="symbol-note">No token matches</p>
          </div>
        </div>
      </div>
      <!-- Tick selector (terminal only) -->
      <div v-if="isTerminal" class="tick-selector" @click.stop>
        <button class="tick-btn" @click="showTickMenu = !showTickMenu; showSymbolMenu = false">
          {{ formatTick(tickSize) }}
          <ChevronDown class="tick-chevron" />
        </button>
        <div v-if="showTickMenu" class="tick-menu">
          <button
            v-for="t in tickOptions"
            :key="t"
            class="tick-option"
            :class="{ 'tick-active': t === tickSize }"
            @click="onSelectTick(t)"
          >
            {{ formatTick(t) }}
          </button>
        </div>
      </div>
    </div>

    <div class="sidebar-content">
      <!-- Terminal: Orderbook -->
      <template v-if="isTerminal">
        <div class="flex-1 min-h-0 overflow-hidden">
          <TerminalTradingOrderbook />
        </div>
      </template>

      <template v-else-if="isMetrics">
        <div class="filters-panel" @click.stop>
          <div class="filter-summary">
            <div class="summary-card">
              <span class="summary-label">Active</span>
              <strong class="summary-value">{{ activeFilterCount }}</strong>
            </div>
            <div class="summary-card">
              <span class="summary-label">Favorites</span>
              <strong class="summary-value">{{ favoriteCount }}</strong>
            </div>
          </div>

          <label class="filter-search">
            <Search class="filter-search-icon" />
            <input
              v-model="filters.search"
              type="text"
              placeholder="Search title, provider, formula..."
            >
          </label>

          <section class="filter-group">
            <div class="filter-group-header">
              <span>Source</span>
            </div>
            <select v-model="filters.source" class="filter-select">
              <option
                v-for="option in sourceOptions"
                :key="option.value"
                :value="option.value"
              >
                {{ option.label }}
              </option>
            </select>
          </section>

          <section class="filter-group">
            <div class="filter-group-header">
              <span>Favorites</span>
            </div>
            <select v-model="filters.favorite" class="filter-select">
              <option
                v-for="option in favoriteOptions"
                :key="option.value"
                :value="option.value"
              >
                {{ option.label }}
              </option>
            </select>
          </section>

          <section class="filter-group">
            <div class="filter-group-header">
              <span>Reading context</span>
            </div>
            <select v-model="filters.tone" class="filter-select">
              <option
                v-for="option in toneOptions"
                :key="option.value"
                :value="option.value"
              >
                {{ option.label }}
              </option>
            </select>
          </section>

          <section class="filter-group">
            <div class="filter-group-header">
              <span>Asset</span>
            </div>
            <select v-model="filters.symbol" class="filter-select">
              <option
                v-for="option in symbolOptions"
                :key="option.value"
                :value="option.value"
              >
                {{ option.label }}
              </option>
            </select>
          </section>

          <section class="filter-group">
            <div class="filter-group-header">
              <span>Category</span>
            </div>
            <select v-model="filters.category" class="filter-select">
              <option
                v-for="option in categoryOptions"
                :key="option.value"
                :value="option.value"
              >
                {{ option.label }}
              </option>
            </select>
          </section>

          <button
            class="reset-filters"
            type="button"
            :disabled="activeFilterCount === 0"
            @click="resetFilters"
          >
            <RotateCcw class="w-3.5 h-3.5" />
            <span>Reset filters</span>
          </button>
        </div>
      </template>

      <TerminalPortfolioSidebar v-else-if="isPortfolio" />

      <!-- Other pages: Watchlist -->
      <template v-else>
        <div class="empty-state">
          <Star class="w-5 h-5" style="color: var(--muted)" />
          <p class="empty-text">Add symbols to your watchlist</p>
        </div>
      </template>
    </div>
  </QRightPanel>
</template>

<style scoped>
.sidebar-header {
  position: relative;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 8px;
  height: 36px;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}

.header-label {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--text-secondary);
}

.header-spacer {
  flex: 1;
}

/* Tick size selector */
.tick-selector {
  position: relative;
  flex-shrink: 0;
}

.tick-btn {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 2px 6px;
  font-size: 9px;
  font-family: var(--qss-font-mono);
  font-weight: 600;
  color: var(--text-secondary);
  background-color: var(--surface-2);
  border: 1px solid var(--border);
  border-radius: 3px;
  cursor: pointer;
  transition: all 0.15s ease;
}

.tick-btn:hover {
  color: var(--text-primary);
  border-color: var(--text-secondary);
}

.tick-chevron {
  width: 10px;
  height: 10px;
}

.tick-menu {
  position: absolute;
  top: 100%;
  right: 0;
  margin-top: 4px;
  padding: 4px;
  background-color: var(--surface-2);
  border: 1px solid var(--border);
  border-radius: 4px;
  z-index: 50;
  display: flex;
  flex-direction: column;
  gap: 1px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.25);
}

.tick-option {
  padding: 4px 12px;
  font-size: 10px;
  font-family: var(--qss-font-mono);
  font-weight: 500;
  color: var(--text-secondary);
  background: transparent;
  border: none;
  border-radius: 3px;
  cursor: pointer;
  text-align: right;
  white-space: nowrap;
  transition: all 0.1s ease;
}

/* Token menu: hangs from the header (not the small button), so it takes the
   panel's width. The list is a plain block — its rows never shrink, all pairs
   scroll with an empty search. */
.symbol-selector {
  position: static;
}

.symbol-menu {
  position: absolute;
  top: 100%;
  left: 6px;
  right: 6px;
  margin-top: 4px;
  z-index: 50;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background-color: var(--surface-2);
  border: 1px solid var(--border);
  border-radius: 6px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
}

.symbol-search-box {
  display: flex;
  align-items: center;
  gap: 7px;
  height: 32px;
  padding: 0 10px;
  border-bottom: 1px solid var(--border);
  cursor: text;
}

.symbol-search-icon {
  width: 13px;
  height: 13px;
  flex-shrink: 0;
  color: var(--muted);
}

.symbol-search-box:focus-within .symbol-search-icon {
  color: var(--accent);
}

.symbol-search {
  flex: 1;
  min-width: 0;
  height: 100%;
  padding: 0;
  font-size: 12px;
  color: var(--text-primary);
  background: transparent;
  border: none;
}

.symbol-search::placeholder {
  color: var(--muted);
}

/* The modules' global focus ring sits 3px outside the field — inside this
   menu the accent icon marks the focus instead. */
.symbol-search-box .symbol-search:focus-visible {
  outline: none;
}

.symbol-list {
  max-height: 320px;
  overflow-y: auto;
  overscroll-behavior: contain;
  padding: 4px;
}

.symbol-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  width: 100%;
  height: 26px;
  padding: 0 8px;
  background: transparent;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  text-align: left;
}

.symbol-row:hover {
  background-color: var(--surface-3);
}

.symbol-row.is-active {
  background-color: var(--surface-3);
}

.symbol-base {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
  font-weight: 500;
  color: var(--text-primary);
}

.symbol-row.is-active .symbol-base {
  color: var(--accent);
}

.symbol-quote {
  flex-shrink: 0;
  font-size: 10px;
  color: var(--muted);
}

.symbol-note {
  padding: 6px 8px;
  font-size: 11px;
  color: var(--muted);
}

.tick-option:hover {
  background-color: var(--surface-3);
  color: var(--text-primary);
}

.tick-active {
  color: var(--accent);
  background-color: var(--surface-3);
}

.sidebar-content {
  flex: 1;
  min-height: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.filters-panel {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.filter-summary {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px;
}

.summary-card {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 10px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--surface-2);
}

.summary-label {
  font-size: 9px;
  font-weight: 800;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--muted);
}

.summary-value {
  font-family: var(--qss-font-mono);
  font-size: 16px;
  line-height: 1;
  color: var(--text-primary);
}

.filter-search {
  position: relative;
  display: flex;
  align-items: center;
}

.filter-search input {
  width: 100%;
  height: 36px;
  padding: 0 12px 0 34px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--surface-2);
  color: var(--text-primary);
  font-size: 12px;
  outline: none;
}

.filter-search input::placeholder {
  color: var(--muted);
}

.filter-search input:focus {
  border-color: var(--accent);
}

.filter-search-icon {
  position: absolute;
  left: 11px;
  width: 14px;
  height: 14px;
  color: var(--muted);
  pointer-events: none;
}

.filter-group {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.filter-group-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.filter-group-header span {
  font-size: 10px;
  font-weight: 800;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--text-secondary);
}

.filter-select {
  width: 100%;
  height: 36px;
  padding: 0 12px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--surface-2);
  color: var(--text-primary);
  font-size: 12px;
  outline: none;
}

.filter-select:focus {
  border-color: var(--accent);
}

.reset-filters {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  min-height: 36px;
  margin-top: auto;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--surface-2);
  color: var(--text-secondary);
  font-size: 11px;
  font-weight: 700;
  cursor: pointer;
  transition: border-color 0.15s ease, background-color 0.15s ease, color 0.15s ease;
}

.reset-filters:hover:not(:disabled) {
  border-color: var(--text-secondary);
  background: var(--surface-3);
  color: var(--text-primary);
}

.reset-filters:disabled {
  cursor: default;
  opacity: 0.5;
}

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 32px 16px;
}

.empty-text {
  font-size: 11px;
  color: var(--muted);
  text-align: center;
}
</style>
