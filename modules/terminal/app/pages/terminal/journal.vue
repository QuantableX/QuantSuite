<script setup lang="ts">
definePageMeta({ layout: 'terminal' })

import {
  Plus,
  TrendingUp,
  TrendingDown,
  Calendar,
  Filter,
  ArrowUpRight,
  ArrowDownRight,
  Clock,
  DollarSign,
  Target,
  BarChart3,
  ChevronDown,
  ChevronUp,
  Search,
  Download,
  Upload,
  Activity,
  ArrowUpDown,
  X,
} from 'lucide-vue-next'
import { useJournalStore } from '#terminal/stores/journal'
import type { JournalTrade } from '#terminal/stores/journal'

const store = useJournalStore()

// Initialize store
onMounted(() => {
  store.loadFromStorage()
})

// UI state
const activeTab = ref<'trades' | 'analytics' | 'calendar'>('trades')
const showTradeForm = ref(false)
const showImportExport = ref(false)
const editingTrade = ref<JournalTrade | null>(null)
const closingTrade = ref<JournalTrade | null>(null)
const expandedTradeId = ref<string | null>(null)
const showFilters = ref(false)

function openNewTrade() {
  editingTrade.value = null
  closingTrade.value = null
  showTradeForm.value = true
}

function openEditTrade(trade: JournalTrade) {
  editingTrade.value = trade
  closingTrade.value = null
  showTradeForm.value = true
}

function openCloseTrade(trade: JournalTrade) {
  closingTrade.value = trade
  editingTrade.value = null
  showTradeForm.value = true
}

function handleSave(data: any) {
  if (editingTrade.value) {
    store.updateTrade(editingTrade.value.id, data)
  } else {
    store.addTrade(data)
  }
  showTradeForm.value = false
  editingTrade.value = null
}

function handleCloseTrade(exitPrice: number, fees: number) {
  if (closingTrade.value) {
    store.closeTrade(closingTrade.value.id, exitPrice, fees)
  }
  showTradeForm.value = false
  closingTrade.value = null
}

function handleDeleteTrade(id: string) {
  store.deleteTrade(id)
  expandedTradeId.value = null
}

function toggleExpand(id: string) {
  expandedTradeId.value = expandedTradeId.value === id ? null : id
}

function onCalendarSelectDate(date: string) {
  store.filters.dateFrom = date
  store.filters.dateTo = date
  activeTab.value = 'trades'
  showFilters.value = true
}

function clearFilters() {
  store.filters.dateFrom = ''
  store.filters.dateTo = ''
  store.filters.pair = ''
  store.filters.side = 'all'
  store.filters.status = 'all'
  store.filters.tags = []
  store.filters.strategy = ''
}

const hasActiveFilters = computed(() => {
  const f = store.filters
  return f.dateFrom || f.dateTo || f.pair || f.side !== 'all' || f.status !== 'all' || f.tags.length > 0 || f.strategy
})

function formatPnl(v: number): string {
  return (v >= 0 ? '+' : '-') + Math.abs(v).toFixed(2) + '$'
}
</script>

<template>
  <div class="terminal-workspace qj">
    <!-- Header -->
    <QPageHeading title="Trade journal">
      <button class="qj-btn" @click="showImportExport = true">
        <Download class="w-3 h-3" />
        Import/Export
      </button>
      <button class="btn-primary flex items-center gap-2" @click="openNewTrade">
        <Plus class="w-3.5 h-3.5" />
        Log Trade
      </button>
    </QPageHeading>

    <!-- Stats: one panel, the PnL leads -->
    <div class="card qj-stats">
      <div class="qj-stat">
        <span class="qj-stat__label">Total PnL <DollarSign class="w-3 h-3" /></span>
        <span class="qj-stat__row">
          <strong class="is-lead" :style="{ color: store.totalPnl >= 0 ? 'var(--positive)' : 'var(--negative)' }">{{ formatPnl(store.totalPnl) }}</strong>
        </span>
      </div>
      <div class="qj-stat">
        <span class="qj-stat__label">Win Rate <Target class="w-3 h-3" /></span>
        <span class="qj-stat__row">
          <strong :style="{ color: store.winRate >= 50 ? 'var(--positive)' : 'var(--negative)' }">{{ store.winRate.toFixed(1) }}%</strong>
          <small>{{ store.winningTrades.length }}W / {{ store.losingTrades.length }}L</small>
        </span>
      </div>
      <div class="qj-stat">
        <span class="qj-stat__label">Profit Factor <Activity class="w-3 h-3" /></span>
        <span class="qj-stat__row">
          <strong :style="{ color: store.profitFactor >= 1 ? 'var(--positive)' : 'var(--negative)' }">{{ store.profitFactor === Infinity ? '∞' : store.profitFactor.toFixed(2) }}</strong>
        </span>
      </div>
      <div class="qj-stat">
        <span class="qj-stat__label">Total Trades <BarChart3 class="w-3 h-3" /></span>
        <span class="qj-stat__row"><strong>{{ store.trades.length }}</strong></span>
      </div>
      <div class="qj-stat">
        <span class="qj-stat__label">Open <Clock class="w-3 h-3" /></span>
        <span class="qj-stat__row"><strong>{{ store.openTrades.length }}</strong></span>
      </div>
      <div class="qj-stat">
        <span class="qj-stat__label">Best Trade <TrendingUp class="w-3 h-3" /></span>
        <span class="qj-stat__row">
          <strong style="color: var(--positive)">{{ store.bestTrade ? formatPnl(store.bestTrade.pnl ?? 0) : '0.00$' }}</strong>
        </span>
      </div>
    </div>

    <!-- Tabs, and the trades' filters and sort on the same row -->
    <div class="qj-bar">
      <div class="qj-group">
        <button
          v-for="tab in ([{ key: 'trades', label: 'Trades', icon: BarChart3 }, { key: 'analytics', label: 'Analytics', icon: Activity }, { key: 'calendar', label: 'Calendar', icon: Calendar }] as const)"
          :key="tab.key"
          class="qj-chip"
          :class="{ 'is-on': activeTab === tab.key }"
          @click="activeTab = tab.key"
        >
          <component :is="tab.icon" class="w-3.5 h-3.5" />
          {{ tab.label }}
        </button>
      </div>

      <template v-if="activeTab === 'trades'">
        <span class="qj-sep" />
        <button class="qj-chip" :class="{ 'is-on': showFilters }" @click="showFilters = !showFilters">
          <Filter class="w-3 h-3" />
          Filters
          <span v-if="hasActiveFilters" class="qj-dot" />
        </button>

        <!-- Quick side filter -->
        <div class="qj-group">
          <button
            v-for="opt in (['all', 'long', 'short'] as const)"
            :key="opt"
            class="qj-chip"
            :class="{ 'is-on': store.filters.side === opt }"
            @click="store.filters.side = opt"
          >
            {{ opt === 'all' ? 'All Sides' : opt.charAt(0).toUpperCase() + opt.slice(1) }}
          </button>
        </div>
        <span class="qj-sep" />
        <div class="qj-group">
          <button
            v-for="opt in (['all', 'open', 'closed'] as const)"
            :key="opt"
            class="qj-chip"
            :class="{ 'is-on': store.filters.status === opt }"
            @click="store.filters.status = opt"
          >
            {{ opt === 'all' ? 'All Status' : opt.charAt(0).toUpperCase() + opt.slice(1) }}
          </button>
        </div>

        <span class="qj-spacer" />

        <!-- Sort -->
        <div class="qj-group">
          <ArrowUpDown class="w-3 h-3" style="color: var(--muted)" />
          <button
            v-for="sf in (['date', 'pnl', 'pair'] as const)"
            :key="sf"
            class="qj-chip"
            :class="{ 'is-on': store.sortField === sf }"
            @click="store.setSort(sf)"
          >
            {{ sf.charAt(0).toUpperCase() + sf.slice(1) }}
            <template v-if="store.sortField === sf">{{ store.sortDir === 'desc' ? '↓' : '↑' }}</template>
          </button>
        </div>
      </template>
    </div>

    <!-- Trades Tab -->
    <template v-if="activeTab === 'trades'">
      <!-- Expanded Filters -->
      <div v-if="showFilters" class="card qj-filters">
        <div>
          <label>Date From</label>
          <input v-model="store.filters.dateFrom" type="date" class="qj-input" />
        </div>
        <div>
          <label>Date To</label>
          <input v-model="store.filters.dateTo" type="date" class="qj-input" />
        </div>
        <div>
          <label>Pair</label>
          <div class="relative">
            <Search class="w-3 h-3 absolute left-2 top-1/2 -translate-y-1/2" style="color: var(--muted)" />
            <input v-model="store.filters.pair" type="text" placeholder="Search..." class="qj-input pl-7" />
          </div>
        </div>
        <div>
          <label>Strategy</label>
          <select v-model="store.filters.strategy" class="qj-input">
            <option value="">All</option>
            <option v-for="s in store.allStrategies" :key="s" :value="s">{{ s }}</option>
          </select>
        </div>
        <div class="flex items-end">
          <button v-if="hasActiveFilters" class="qj-btn" @click="clearFilters">
            <X class="w-3 h-3" />
            Clear
          </button>
        </div>
      </div>

      <!-- Trade Table: the rows scroll inside the panel -->
      <div class="card qj-table">
        <div class="qj-table__scroll">
          <div class="qj-table__inner">
            <!-- Table Header -->
            <div class="qj-row qj-row--head">
              <span>Date</span>
              <span>Pair</span>
              <span>Side</span>
              <span class="qj-num">Entry</span>
              <span class="qj-num">Exit</span>
              <span class="qj-num">Size</span>
              <span class="qj-num">PnL</span>
              <span>Notes</span>
            </div>

            <!-- Trade Rows -->
            <template v-for="trade in store.filteredTrades" :key="trade.id">
              <div class="qj-row qj-row--trade" @click="toggleExpand(trade.id)">
                <span class="text-[11px] font-mono" style="color: var(--text-secondary)">
                  {{ trade.date.slice(5) }}
                </span>
                <span class="text-xs font-semibold truncate" style="color: var(--text-primary)">
                  {{ trade.pair }}
                </span>
                <span class="flex items-center gap-1">
                  <ArrowUpRight v-if="trade.side === 'long'" class="w-3 h-3" style="color: var(--positive)" />
                  <ArrowDownRight v-else class="w-3 h-3" style="color: var(--negative)" />
                  <span class="text-[11px] font-medium" :style="{ color: trade.side === 'long' ? 'var(--positive)' : 'var(--negative)' }">
                    {{ trade.side }}
                  </span>
                </span>
                <span class="qj-num text-[11px] font-mono" style="color: var(--text-primary)">
                  {{ trade.entry.toLocaleString('en-US') }}$
                </span>
                <span class="qj-num text-[11px] font-mono" style="color: var(--text-primary)">
                  {{ trade.exit ? trade.exit.toLocaleString('en-US') + '$' : '—' }}
                </span>
                <span class="qj-num text-[11px] font-mono" style="color: var(--text-secondary)">
                  {{ trade.size }}
                </span>
                <span v-if="trade.pnl !== null" class="qj-num text-[11px] font-bold font-mono" :style="{ color: trade.pnl >= 0 ? 'var(--positive)' : 'var(--negative)' }">
                  {{ formatPnl(trade.pnl) }}
                </span>
                <span v-else class="qj-num flex items-center gap-1 text-[11px] font-medium" style="color: var(--accent)">
                  <Clock class="w-3 h-3" /> Open
                </span>
                <div class="flex items-center gap-2 min-w-0">
                  <span class="text-[11px] truncate" style="color: var(--text-secondary)">{{ trade.notes }}</span>
                  <div class="flex gap-1 shrink-0">
                    <span
                      v-for="tag in trade.tags.slice(0, 2)"
                      :key="tag"
                      class="text-[11px] font-medium px-1.5 rounded"
                      style="background-color: var(--surface-3); color: var(--text-secondary)"
                    >
                      {{ tag }}
                    </span>
                    <span v-if="trade.tags.length > 2" class="text-[11px] font-medium px-1" style="color: var(--muted)">
                      +{{ trade.tags.length - 2 }}
                    </span>
                  </div>
                  <component
                    :is="expandedTradeId === trade.id ? ChevronUp : ChevronDown"
                    class="w-3 h-3 shrink-0 ml-auto"
                    style="color: var(--muted)"
                  />
                </div>
              </div>

              <!-- Expanded Detail -->
              <TerminalJournalTradeDetail
                v-if="expandedTradeId === trade.id"
                :trade="trade"
                @edit="openEditTrade"
                @delete="handleDeleteTrade"
                @close="openCloseTrade"
              />
            </template>
          </div>

          <!-- Empty state -->
          <QEmptyState v-if="store.filteredTrades.length === 0" title="No trades match your filters">
            <button v-if="hasActiveFilters" class="qj-btn" @click="clearFilters">Clear filters</button>
          </QEmptyState>
        </div>
      </div>
    </template>

    <!-- Analytics Tab -->
    <div v-if="activeTab === 'analytics'" class="qj-pane">
      <TerminalJournalTradeAnalytics />
    </div>

    <!-- Calendar Tab -->
    <div v-if="activeTab === 'calendar'" class="qj-pane">
      <TerminalJournalCalendarHeatmap @select-date="onCalendarSelectDate" />
    </div>

    <!-- Modals -->
    <TerminalJournalTradeFormModal
      v-if="showTradeForm"
      :trade="editingTrade || closingTrade"
      :close-only="!!closingTrade"
      @close="showTradeForm = false; editingTrade = null; closingTrade = null"
      @save="handleSave"
      @close-trade="handleCloseTrade"
    />

    <TerminalJournalImportExport
      v-if="showImportExport"
      @close="showImportExport = false"
    />
  </div>
</template>

<style scoped>
.qj-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: var(--qss-ctl-h);
  padding: 0 10px;
  border-radius: var(--qss-ctl-radius);
  background: var(--surface-2);
  color: var(--text-secondary);
  font-size: var(--qss-ctl-font);
  font-weight: 500;
  white-space: nowrap;
}
.qj-btn:hover { filter: brightness(1.1); }

/* One panel, split by hairlines; the shadows are clipped at the panel's own edge. */
.qj-stats {
  display: grid;
  grid-template-columns: repeat(6, minmax(0, 1fr));
  overflow: hidden;
}
.qj-stat {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
  padding: var(--qss-gap-tight) var(--qss-panel-pad);
  box-shadow: -1px 0 0 var(--border), 0 -1px 0 var(--border);
}
.qj-stat__label {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
  color: var(--text-secondary);
  font-size: var(--qss-text-label);
}
.qj-stat__label svg { flex-shrink: 0; color: var(--muted); }
.qj-stat__row {
  display: flex;
  align-items: baseline;
  flex-wrap: wrap;
  gap: 2px 8px;
  min-height: 28px;
  font-variant-numeric: tabular-nums;
}
.qj-stat__row strong {
  color: var(--text-primary);
  font-size: var(--qss-text-title);
  font-weight: 600;
  line-height: 28px;
  overflow-wrap: anywhere;
}
.qj-stat__row strong.is-lead { font-size: var(--qss-text-metric); letter-spacing: -.02em; }
.qj-stat__row small { color: var(--text-secondary); font-size: var(--qss-text-meta); }

.qj-bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--qss-gap-tight);
}
.qj-group { display: flex; align-items: center; gap: 2px; }
.qj-sep { width: 1px; height: 16px; background: var(--border); }
.qj-spacer { flex: 1 1 0; }
.qj-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: var(--qss-ctl-h);
  padding: 0 8px;
  border: 1px solid transparent;
  border-radius: var(--qss-ctl-radius);
  color: var(--text-secondary);
  font-size: var(--qss-ctl-font);
  font-weight: 500;
  white-space: nowrap;
}
.qj-chip:hover { color: var(--text-primary); }
.qj-chip.is-on {
  border-color: var(--border);
  background: var(--surface-2);
  color: var(--text-primary);
}
.qj-dot { width: 6px; height: 6px; border-radius: 50%; background: currentColor; }

.qj-filters {
  display: grid;
  grid-template-columns: repeat(5, minmax(0, 1fr));
  gap: var(--qss-gap);
  padding: var(--qss-panel-pad);
}
.qj-filters label {
  display: block;
  margin-bottom: 4px;
  color: var(--text-secondary);
  font-size: var(--qss-text-label);
}
.qj-input {
  width: 100%;
  height: var(--qss-ctl-h);
  padding-inline: 8px;
  border: 1px solid var(--border);
  border-radius: var(--qss-ctl-radius);
  outline: none;
  background: var(--surface-2);
  color: var(--text-primary);
  font-size: var(--qss-ctl-font);
}
.qj-input.pl-7 { padding-left: 28px; }

/* The table takes the height; its rows scroll, the page does not. */
.qj > .qj-table {
  flex: 1 1 0;
  min-height: 180px;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.qj-table__scroll { flex: 1 1 0; min-height: 0; overflow: auto; }
.qj-table__inner { min-width: 720px; }
.qj-row {
  display: grid;
  grid-template-columns: 56px minmax(90px, 1fr) 64px minmax(90px, 1fr) minmax(90px, 1fr) minmax(56px, .7fr) minmax(90px, 1fr) minmax(0, 3fr);
  align-items: center;
  column-gap: var(--qss-gap);
  min-height: var(--qss-row-h);
  padding-inline: var(--qss-panel-pad);
  border-bottom: 1px solid var(--border);
  font-variant-numeric: tabular-nums;
}
.qj-row--head {
  position: sticky;
  top: 0;
  z-index: 1;
  background: var(--surface-2);
  color: var(--text-secondary);
  font-size: var(--qss-text-meta);
  font-weight: 500;
}
.qj-row--trade { cursor: pointer; transition: background-color .12s; }
.qj-row--trade:hover { background: var(--surface-2); }
.qj-num { justify-self: end; text-align: right; }

.qj > .qj-pane { flex: 1 1 0; min-height: 0; overflow: hidden auto; }

@container terminal-workspace (max-width: 850px) {
  .qj-stats { grid-template-columns: repeat(3, minmax(0, 1fr)); }
  .qj-filters { grid-template-columns: repeat(3, minmax(0, 1fr)); }
}
@container terminal-workspace (max-width: 520px) {
  .qj-stats { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .qj-filters { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .qj-spacer { display: none; }
}
</style>
