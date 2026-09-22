<script setup lang="ts">
import type { Trade } from '#algo/types'
import { useBotsStore } from '#algo/stores/bots'
import { useExchangeStore } from '#algo/stores/exchange'
import {
  formatDateTime,
  formatPrice,
  formatDuration,
  formatDate,
  formatQuantity,
  formatCurrency,
  capitalize,
} from '#algo/utils/format'

const botsStore = useBotsStore()
const exchangeStore = useExchangeStore()

const props = defineProps<{
  trades: Trade[]
}>()

function botLabel(trade: Trade): string {
  if (trade.bot_id) return botsStore.name(trade.bot_id)
  return trade.is_backtest ? 'Backtest' : '--'
}

const emit = defineEmits<{
  select: [trade: Trade]
  updateNotes: [id: string, notes: string]
}>()

type SortKey =
  | 'entry_time'
  | 'pair'
  | 'side'
  | 'entry_price'
  | 'exit_price'
  | 'pnl'
  | 'pnl_pct'
  | 'duration'
  | 'strategy_id'
  | 'bot'

const sortBy = ref<SortKey>('entry_time')
const sortDir = ref<'asc' | 'desc'>('desc')
const expandedId = ref<string | null>(null)
const notesMap = ref<Record<string, string>>({})

const columns: { key: SortKey; label: string }[] = [
  { key: 'entry_time', label: 'Date' },
  { key: 'pair', label: 'Pair' },
  { key: 'side', label: 'Side' },
  { key: 'entry_price', label: 'Entry' },
  { key: 'exit_price', label: 'Exit' },
  { key: 'pnl', label: 'PnL ($)' },
  { key: 'pnl_pct', label: 'PnL (%)' },
  { key: 'duration', label: 'Duration' },
  { key: 'strategy_id', label: 'Strategy' },
  { key: 'bot', label: 'Bot' },
]

function getDurationSecs(trade: Trade): number {
  if (!trade.exit_time) return 0
  const entry = new Date(trade.entry_time).getTime()
  const exit = new Date(trade.exit_time).getTime()
  return Math.max(0, (exit - entry) / 1000)
}

function getSortValue(trade: Trade, key: SortKey): string | number {
  switch (key) {
    case 'entry_time': return new Date(trade.entry_time).getTime()
    case 'pair': return trade.pair.toLowerCase()
    case 'side': return trade.side
    case 'entry_price': return trade.entry_price
    case 'exit_price': return trade.exit_price ?? 0
    case 'pnl': return trade.pnl ?? 0
    case 'pnl_pct': return trade.pnl_pct ?? 0
    case 'duration': return getDurationSecs(trade)
    case 'strategy_id': return trade.strategy_id.toLowerCase()
    case 'bot': return botLabel(trade).toLowerCase()
  }
}

const sortedTrades = computed(() => {
  const arr = [...props.trades]
  arr.sort((a, b) => {
    const aVal = getSortValue(a, sortBy.value)
    const bVal = getSortValue(b, sortBy.value)
    let cmp = 0
    if (typeof aVal === 'number' && typeof bVal === 'number') {
      cmp = aVal - bVal
    } else {
      cmp = String(aVal).localeCompare(String(bVal))
    }
    return sortDir.value === 'asc' ? cmp : -cmp
  })
  return arr
})

function toggleSort(key: SortKey) {
  if (sortBy.value === key) {
    sortDir.value = sortDir.value === 'asc' ? 'desc' : 'asc'
  } else {
    sortBy.value = key
    sortDir.value = key === 'entry_time' ? 'desc' : 'asc'
  }
}

function sortIndicator(key: SortKey): string {
  if (sortBy.value !== key) return ''
  return sortDir.value === 'asc' ? ' \u25B2' : ' \u25BC'
}

function toggleExpand(trade: Trade) {
  if (expandedId.value === trade.id) {
    expandedId.value = null
  } else {
    expandedId.value = trade.id
    if (!(trade.id in notesMap.value)) {
      notesMap.value[trade.id] = trade.notes ?? ''
    }
  }
  emit('select', trade)
}

function formatPnlValue(val: number | null): { text: string; cls: string } {
  if (val === null) return { text: '--', cls: '' }
  const sign = val >= 0 ? '+' : ''
  return {
    text: `${sign}${formatCurrency(val)}`,
    cls: val >= 0 ? 'text-success' : 'text-error',
  }
}

function formatPnlPctValue(val: number | null): { text: string; cls: string } {
  if (val === null) return { text: '--', cls: '' }
  const sign = val >= 0 ? '+' : ''
  return {
    text: `${sign}${val.toFixed(2)}%`,
    cls: val >= 0 ? 'text-success' : 'text-error',
  }
}

function saveNotes(tradeId: string) {
  emit('updateNotes', tradeId, notesMap.value[tradeId] ?? '')
}
</script>

<template>
  <div class="trade-table-wrapper">
    <table v-if="trades.length" class="table">
      <thead>
        <tr>
          <th
            v-for="col in columns"
            :key="col.key"
            class="sortable-header"
            @click="toggleSort(col.key)"
          >
            {{ col.label }}{{ sortIndicator(col.key) }}
          </th>
        </tr>
      </thead>
      <tbody>
        <template v-for="trade in sortedTrades" :key="trade.id">
          <tr class="trade-row" @click="toggleExpand(trade)">
            <td>{{ formatDateTime(trade.entry_time) }}</td>
            <td class="mono">{{ trade.pair }}</td>
            <td>
              <span :class="trade.side === 'long' ? 'text-accent' : 'text-error'">
                {{ capitalize(trade.side) }}
              </span>
            </td>
            <td class="mono">{{ formatPrice(trade.entry_price) }}</td>
            <td class="mono">{{ trade.exit_price !== null ? formatPrice(trade.exit_price) : '--' }}</td>
            <td>
              <span :class="formatPnlValue(trade.pnl).cls">
                {{ formatPnlValue(trade.pnl).text }}
              </span>
            </td>
            <td>
              <span :class="formatPnlPctValue(trade.pnl_pct).cls">
                {{ formatPnlPctValue(trade.pnl_pct).text }}
              </span>
            </td>
            <td>{{ trade.exit_time ? formatDuration(getDurationSecs(trade)) : '--' }}</td>
            <td>{{ trade.strategy_id }}</td>
            <td class="cell-bot">
              {{ botLabel(trade) }}
              <span v-if="trade.trading_mode" class="mode-tag" :class="`mode-tag--${trade.trading_mode}`">{{ trade.trading_mode }}</span>
            </td>
          </tr>
          <tr v-if="expandedId === trade.id" class="expanded-row">
            <td :colspan="columns.length">
              <div class="expanded-panel">
                <div class="detail-grid">
                  <div class="detail-item">
                    <span class="detail-label">Trade ID</span>
                    <span class="detail-value mono">{{ trade.id }}</span>
                  </div>
                  <div class="detail-item">
                    <span class="detail-label">Exchange</span>
                    <span class="detail-value">{{ exchangeStore.label(trade.exchange) }}</span>
                  </div>
                  <div class="detail-item">
                    <span class="detail-label">Quantity</span>
                    <span class="detail-value mono">{{ formatQuantity(trade.quantity) }}</span>
                  </div>
                  <div class="detail-item">
                    <span class="detail-label">Fee</span>
                    <span class="detail-value mono">{{ formatCurrency(trade.fee) }}</span>
                  </div>
                  <div class="detail-item">
                    <span class="detail-label">Backtest</span>
                    <span class="detail-value">{{ trade.is_backtest ? 'Yes' : 'No' }}</span>
                  </div>
                  <div v-if="trade.backtest_id" class="detail-item">
                    <span class="detail-label">Backtest ID</span>
                    <span class="detail-value mono">{{ trade.backtest_id }}</span>
                  </div>
                  <div class="detail-item">
                    <span class="detail-label">Entry Time</span>
                    <span class="detail-value">{{ formatDateTime(trade.entry_time) }}</span>
                  </div>
                  <div class="detail-item">
                    <span class="detail-label">Exit Time</span>
                    <span class="detail-value">{{ trade.exit_time ? formatDateTime(trade.exit_time) : '--' }}</span>
                  </div>
                </div>
                <div class="notes-section">
                  <label class="label">Notes</label>
                  <textarea
                    v-model="notesMap[trade.id]"
                    class="input notes-textarea"
                    placeholder="Add trade notes..."
                    rows="3"
                  />
                  <button class="btn btn-sm btn-primary notes-save" @click.stop="saveNotes(trade.id)">
                    Save Notes
                  </button>
                </div>
              </div>
            </td>
          </tr>
        </template>
      </tbody>
    </table>
    <div v-else class="empty-state">
      No trades recorded yet. Start a bot or run a backtest to see them here.
    </div>
  </div>
</template>

<style scoped>
.trade-table-wrapper {
  min-width: 100%;
  min-height: 100%;
  display: flex;
  flex-direction: column;
}

/* The table scrolls inside its block; the header stays put. */
.trade-table-wrapper .table thead th {
  position: sticky;
  top: 0;
  z-index: 1;
  background: var(--qa-bg-card);
}

.sortable-header {
  cursor: pointer;
  user-select: none;
  white-space: nowrap;
  transition: color var(--qa-transition);
}

.sortable-header:hover {
  color: var(--qa-text-secondary);
}

.trade-row {
  cursor: pointer;
}

.expanded-row td {
  padding: 0 !important;
  border-bottom: 1px solid var(--qa-border);
}

.expanded-panel {
  padding: 16px;
  background: var(--qa-bg-sidebar);
  border-top: 1px solid var(--qa-border-subtle);
}

.detail-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  gap: 12px;
  margin-bottom: 16px;
}

.detail-item {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.detail-label {
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--qa-text-muted);
}

.detail-value {
  font-size: 13px;
  color: var(--qa-text);
}

.notes-section {
  display: flex;
  flex-direction: column;
  gap: 6px;
  max-width: 500px;
}

.notes-textarea {
  resize: vertical;
  min-height: 60px;
  font-size: 13px;
  font-family: inherit;
}

.notes-save {
  align-self: flex-end;
}

/* The one box the page shows without trades: the message sits in its middle. */
.empty-state {
  flex: 1 1 auto;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 40px;
  text-align: center;
  color: var(--qa-text-muted);
  font-size: 13px;
}
.cell-bot {
  white-space: nowrap;
}

.mode-tag {
  display: inline-block;
  margin-left: 6px;
  padding: 0 5px;
  border-radius: 4px;
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--qa-text-muted);
  border: 1px solid var(--qa-border-subtle);
  vertical-align: middle;
}

.mode-tag--live {
  color: var(--qa-error);
  border-color: color-mix(in srgb, var(--qa-error) 45%, transparent);
}
</style>
