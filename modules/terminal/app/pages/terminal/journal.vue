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
  return (v >= 0 ? '+$' : '-$') + Math.abs(v).toFixed(2)
}
</script>

<template>
  <div class="terminal-workspace space-y-4 h-full overflow-y-auto">
    <!-- Header -->
    <QPageHeading title="Trade journal">
      <div class="flex items-center gap-2">
        <button
          class="flex items-center gap-1.5 px-2.5 py-1.5 rounded text-[11px] font-medium transition-all hover:brightness-110"
          style="background-color: var(--surface-2); color: var(--text-secondary)"
          @click="showImportExport = true"
        >
          <Download class="w-3 h-3" />
          Import/Export
        </button>
        <button class="btn-primary flex items-center gap-2 text-xs" @click="openNewTrade">
          <Plus class="w-3.5 h-3.5" />
          Log Trade
        </button>
      </div>
    </QPageHeading>

    <!-- Stats Row -->
    <div class="grid grid-cols-6 gap-3">
      <div class="card p-3">
        <div class="flex items-center justify-between mb-2">
          <span class="text-[10px] font-medium uppercase tracking-wider" style="color: var(--muted)">Total PnL</span>
          <DollarSign class="w-3.5 h-3.5" style="color: var(--muted)" />
        </div>
        <div class="text-lg font-bold font-mono" :style="{ color: store.totalPnl >= 0 ? 'var(--positive)' : 'var(--negative)' }">
          {{ formatPnl(store.totalPnl) }}
        </div>
      </div>
      <div class="card p-3">
        <div class="flex items-center justify-between mb-2">
          <span class="text-[10px] font-medium uppercase tracking-wider" style="color: var(--muted)">Win Rate</span>
          <Target class="w-3.5 h-3.5" style="color: var(--muted)" />
        </div>
        <div class="text-lg font-bold font-mono" :style="{ color: store.winRate >= 50 ? 'var(--positive)' : 'var(--negative)' }">
          {{ store.winRate.toFixed(1) }}%
        </div>
        <span class="text-[10px]" style="color: var(--text-secondary)">{{ store.winningTrades.length }}W / {{ store.losingTrades.length }}L</span>
      </div>
      <div class="card p-3">
        <div class="flex items-center justify-between mb-2">
          <span class="text-[10px] font-medium uppercase tracking-wider" style="color: var(--muted)">Profit Factor</span>
          <Activity class="w-3.5 h-3.5" style="color: var(--muted)" />
        </div>
        <div class="text-lg font-bold font-mono" :style="{ color: store.profitFactor >= 1 ? 'var(--positive)' : 'var(--negative)' }">
          {{ store.profitFactor === Infinity ? '∞' : store.profitFactor.toFixed(2) }}
        </div>
      </div>
      <div class="card p-3">
        <div class="flex items-center justify-between mb-2">
          <span class="text-[10px] font-medium uppercase tracking-wider" style="color: var(--muted)">Total Trades</span>
          <BarChart3 class="w-3.5 h-3.5" style="color: var(--muted)" />
        </div>
        <div class="text-lg font-bold font-mono" style="color: var(--text-primary)">
          {{ store.trades.length }}
        </div>
      </div>
      <div class="card p-3">
        <div class="flex items-center justify-between mb-2">
          <span class="text-[10px] font-medium uppercase tracking-wider" style="color: var(--muted)">Open</span>
          <Clock class="w-3.5 h-3.5" style="color: var(--muted)" />
        </div>
        <div class="text-lg font-bold font-mono" style="color: var(--accent)">
          {{ store.openTrades.length }}
        </div>
      </div>
      <div class="card p-3">
        <div class="flex items-center justify-between mb-2">
          <span class="text-[10px] font-medium uppercase tracking-wider" style="color: var(--muted)">Best Trade</span>
          <TrendingUp class="w-3.5 h-3.5" style="color: var(--muted)" />
        </div>
        <div class="text-lg font-bold font-mono" style="color: var(--positive)">
          {{ store.bestTrade ? formatPnl(store.bestTrade.pnl ?? 0) : '$0.00' }}
        </div>
      </div>
    </div>

    <!-- Tab Navigation -->
    <div class="flex items-center gap-1" style="border-bottom: 1px solid var(--border)">
      <button
        v-for="tab in ([{ key: 'trades', label: 'Trades', icon: BarChart3 }, { key: 'analytics', label: 'Analytics', icon: Activity }, { key: 'calendar', label: 'Calendar', icon: Calendar }] as const)"
        :key="tab.key"
        class="flex items-center gap-1.5 px-3 py-2 text-xs font-medium transition-all -mb-px"
        :style="{
          color: activeTab === tab.key ? 'var(--accent)' : 'var(--text-secondary)',
          borderBottom: activeTab === tab.key ? '2px solid var(--accent)' : '2px solid transparent',
        }"
        @click="activeTab = tab.key"
      >
        <component :is="tab.icon" class="w-3.5 h-3.5" />
        {{ tab.label }}
      </button>
    </div>

    <!-- Trades Tab -->
    <template v-if="activeTab === 'trades'">
      <!-- Filter Bar -->
      <div class="space-y-2">
        <div class="flex items-center gap-2">
          <button
            class="flex items-center gap-1.5 px-2.5 py-1.5 rounded text-[11px] font-medium transition-all"
            :style="{
              backgroundColor: showFilters ? 'var(--accent)' : 'var(--surface-2)',
              color: showFilters ? '#fff' : 'var(--text-secondary)',
            }"
            @click="showFilters = !showFilters"
          >
            <Filter class="w-3 h-3" />
            Filters
            <span v-if="hasActiveFilters" class="w-1.5 h-1.5 rounded-full" style="background-color: var(--accent)" />
          </button>

          <!-- Quick side filter -->
          <div class="flex gap-1">
            <button
              v-for="opt in (['all', 'long', 'short'] as const)"
              :key="opt"
              class="px-2.5 py-1 rounded text-[11px] font-medium transition-all"
              :style="{
                backgroundColor: store.filters.side === opt ? 'var(--accent)' : 'var(--surface-2)',
                color: store.filters.side === opt ? '#fff' : 'var(--text-secondary)',
              }"
              @click="store.filters.side = opt"
            >
              {{ opt === 'all' ? 'All Sides' : opt.charAt(0).toUpperCase() + opt.slice(1) }}
            </button>
          </div>
          <div class="w-px h-4" style="background-color: var(--border)" />
          <div class="flex gap-1">
            <button
              v-for="opt in (['all', 'open', 'closed'] as const)"
              :key="opt"
              class="px-2.5 py-1 rounded text-[11px] font-medium transition-all"
              :style="{
                backgroundColor: store.filters.status === opt ? 'var(--accent)' : 'var(--surface-2)',
                color: store.filters.status === opt ? '#fff' : 'var(--text-secondary)',
              }"
              @click="store.filters.status = opt"
            >
              {{ opt === 'all' ? 'All Status' : opt.charAt(0).toUpperCase() + opt.slice(1) }}
            </button>
          </div>

          <div class="flex-1" />

          <!-- Sort -->
          <div class="flex items-center gap-1">
            <ArrowUpDown class="w-3 h-3" style="color: var(--muted)" />
            <button
              v-for="sf in (['date', 'pnl', 'pair'] as const)"
              :key="sf"
              class="px-2 py-1 rounded text-[11px] font-medium transition-all"
              :style="{
                backgroundColor: store.sortField === sf ? 'var(--accent)' : 'var(--surface-2)',
                color: store.sortField === sf ? '#fff' : 'var(--text-secondary)',
              }"
              @click="store.setSort(sf)"
            >
              {{ sf.charAt(0).toUpperCase() + sf.slice(1) }}
              <template v-if="store.sortField === sf">{{ store.sortDir === 'desc' ? '↓' : '↑' }}</template>
            </button>
          </div>
        </div>

        <!-- Expanded Filters -->
        <div v-if="showFilters" class="card p-3 grid grid-cols-5 gap-3">
          <div>
            <label class="text-[10px] font-medium mb-1 block" style="color: var(--muted)">Date From</label>
            <input v-model="store.filters.dateFrom" type="date" class="w-full px-2 py-1.5 rounded text-[11px]" style="background-color: var(--surface-2); border: 1px solid var(--border); color: var(--text-primary); outline: none" />
          </div>
          <div>
            <label class="text-[10px] font-medium mb-1 block" style="color: var(--muted)">Date To</label>
            <input v-model="store.filters.dateTo" type="date" class="w-full px-2 py-1.5 rounded text-[11px]" style="background-color: var(--surface-2); border: 1px solid var(--border); color: var(--text-primary); outline: none" />
          </div>
          <div>
            <label class="text-[10px] font-medium mb-1 block" style="color: var(--muted)">Pair</label>
            <div class="relative">
              <Search class="w-3 h-3 absolute left-2 top-1/2 -translate-y-1/2" style="color: var(--muted)" />
              <input v-model="store.filters.pair" type="text" placeholder="Search..." class="w-full pl-7 pr-2 py-1.5 rounded text-[11px]" style="background-color: var(--surface-2); border: 1px solid var(--border); color: var(--text-primary); outline: none" />
            </div>
          </div>
          <div>
            <label class="text-[10px] font-medium mb-1 block" style="color: var(--muted)">Strategy</label>
            <select v-model="store.filters.strategy" class="w-full px-2 py-1.5 rounded text-[11px]" style="background-color: var(--surface-2); border: 1px solid var(--border); color: var(--text-primary); outline: none">
              <option value="">All</option>
              <option v-for="s in store.allStrategies" :key="s" :value="s">{{ s }}</option>
            </select>
          </div>
          <div class="flex items-end">
            <button
              v-if="hasActiveFilters"
              class="flex items-center gap-1 px-2.5 py-1.5 rounded text-[11px] font-medium transition-all hover:brightness-110"
              style="background-color: var(--surface-2); color: var(--text-secondary)"
              @click="clearFilters"
            >
              <X class="w-3 h-3" />
              Clear
            </button>
          </div>
        </div>
      </div>

      <!-- Trade Table -->
      <div class="card overflow-hidden">
        <!-- Table Header -->
        <div class="grid grid-cols-[80px_100px_60px_90px_90px_70px_90px_1fr] gap-2 px-4 py-2 text-[10px] font-semibold uppercase tracking-wider" style="color: var(--muted); border-bottom: 1px solid var(--border); background-color: var(--surface-2)">
          <span>Date</span>
          <span>Pair</span>
          <span>Side</span>
          <span>Entry</span>
          <span>Exit</span>
          <span>Size</span>
          <span>PnL</span>
          <span>Notes</span>
        </div>

        <!-- Trade Rows -->
        <template v-for="trade in store.filteredTrades" :key="trade.id">
          <div
            class="grid grid-cols-[80px_100px_60px_90px_90px_70px_90px_1fr] gap-2 px-4 py-3 items-center transition-colors cursor-pointer hover:bg-[var(--surface-2)]"
            style="border-bottom: 1px solid var(--border)"
            @click="toggleExpand(trade.id)"
          >
            <span class="text-[11px] font-mono" style="color: var(--text-secondary)">
              {{ trade.date.slice(5) }}
            </span>
            <span class="text-xs font-semibold" style="color: var(--text-primary)">
              {{ trade.pair }}
            </span>
            <span class="flex items-center gap-1">
              <ArrowUpRight v-if="trade.side === 'long'" class="w-3 h-3" style="color: var(--positive)" />
              <ArrowDownRight v-else class="w-3 h-3" style="color: var(--negative)" />
              <span class="text-[11px] font-medium" :style="{ color: trade.side === 'long' ? 'var(--positive)' : 'var(--negative)' }">
                {{ trade.side }}
              </span>
            </span>
            <span class="text-[11px] font-mono" style="color: var(--text-primary)">
              ${{ trade.entry.toLocaleString() }}
            </span>
            <span class="text-[11px] font-mono" style="color: var(--text-primary)">
              {{ trade.exit ? '$' + trade.exit.toLocaleString() : '—' }}
            </span>
            <span class="text-[11px] font-mono" style="color: var(--text-secondary)">
              {{ trade.size }}
            </span>
            <span v-if="trade.pnl !== null" class="text-[11px] font-bold font-mono" :style="{ color: trade.pnl >= 0 ? 'var(--positive)' : 'var(--negative)' }">
              {{ formatPnl(trade.pnl) }}
            </span>
            <span v-else class="flex items-center gap-1 text-[11px] font-medium" style="color: var(--accent)">
              <Clock class="w-3 h-3" /> Open
            </span>
            <div class="flex items-center gap-2 min-w-0">
              <span class="text-[11px] truncate" style="color: var(--text-secondary)">{{ trade.notes }}</span>
              <div class="flex gap-1 shrink-0">
                <span
                  v-for="tag in trade.tags.slice(0, 2)"
                  :key="tag"
                  class="text-[9px] font-medium px-1.5 py-0.5 rounded"
                  style="background-color: var(--surface-3); color: var(--muted)"
                >
                  {{ tag }}
                </span>
                <span v-if="trade.tags.length > 2" class="text-[9px] font-medium px-1 py-0.5" style="color: var(--muted)">
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

        <!-- Empty state -->
        <div v-if="store.filteredTrades.length === 0" class="p-8 text-center">
          <BarChart3 class="w-8 h-8 mx-auto mb-2" style="color: var(--muted)" />
          <p class="text-xs" style="color: var(--muted)">No trades match your filters</p>
          <button
            v-if="hasActiveFilters"
            class="mt-2 text-[11px] font-medium"
            style="color: var(--accent)"
            @click="clearFilters"
          >Clear filters</button>
        </div>
      </div>
    </template>

    <!-- Analytics Tab -->
    <TerminalJournalTradeAnalytics v-if="activeTab === 'analytics'" />

    <!-- Calendar Tab -->
    <TerminalJournalCalendarHeatmap
      v-if="activeTab === 'calendar'"
      @select-date="onCalendarSelectDate"
    />

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
