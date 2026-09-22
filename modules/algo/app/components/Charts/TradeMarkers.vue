<script setup lang="ts">
import type { Trade } from '#algo/types'
import { formatPrice, formatCurrency, formatDate, capitalize } from '#algo/utils/format'

const props = defineProps<{
  trades: Trade[]
}>()

const INITIAL_LIMIT = 20
const showAll = ref(false)

const sortedTrades = computed(() => {
  return [...props.trades].sort(
    (a, b) => new Date(b.entry_time).getTime() - new Date(a.entry_time).getTime()
  )
})

const visibleTrades = computed(() => {
  if (showAll.value) return sortedTrades.value
  return sortedTrades.value.slice(0, INITIAL_LIMIT)
})

const hasMore = computed(() => sortedTrades.value.length > INITIAL_LIMIT)

function pnlDisplay(trade: Trade): { text: string; cls: string } {
  if (trade.pnl === null) return { text: '--', cls: 'text-muted' }
  const sign = trade.pnl >= 0 ? '+' : ''
  return {
    text: `${sign}${formatCurrency(trade.pnl)}`,
    cls: trade.pnl >= 0 ? 'text-success' : 'text-error',
  }
}
</script>

<template>
  <div class="trade-markers-container">
    <div v-if="!trades.length" class="empty-state">
      No trades to display
    </div>
    <template v-else>
      <div class="markers-list">
        <div
          v-for="trade in visibleTrades"
          :key="trade.id"
          class="marker-item"
        >
          <span
            class="side-dot"
            :class="trade.side === 'long' ? 'dot-long' : 'dot-short'"
          />
          <span class="marker-pair mono">{{ trade.pair }}</span>
          <span
            class="marker-side"
            :class="trade.side === 'long' ? 'text-accent' : 'text-error'"
          >
            {{ capitalize(trade.side) }}
          </span>
          <span class="marker-prices mono">
            {{ formatPrice(trade.entry_price) }}
            <span class="text-muted">&rarr;</span>
            {{ trade.exit_price !== null ? formatPrice(trade.exit_price) : '--' }}
          </span>
          <span class="marker-pnl" :class="pnlDisplay(trade).cls">
            {{ pnlDisplay(trade).text }}
          </span>
          <span class="marker-date text-muted">
            {{ formatDate(trade.entry_time) }}
          </span>
        </div>
      </div>
      <button
        v-if="hasMore && !showAll"
        class="btn btn-sm show-all-btn"
        @click="showAll = true"
      >
        Show all ({{ sortedTrades.length }})
      </button>
    </template>
  </div>
</template>

<style scoped>
.trade-markers-container {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.markers-list {
  max-height: 200px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.marker-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 5px 8px;
  border-radius: 4px;
  font-size: 12px;
  transition: background var(--qa-transition);
}

.marker-item:hover {
  background: var(--qa-bg-hover);
}

.side-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
}

.dot-long {
  background: var(--qa-accent);
}

.dot-short {
  background: var(--qa-error);
}

.marker-pair {
  font-weight: 600;
  color: var(--qa-text);
  min-width: 80px;
}

.marker-side {
  font-weight: 500;
  min-width: 40px;
}

.marker-prices {
  color: var(--qa-text-secondary);
  min-width: 160px;
}

.marker-pnl {
  font-weight: 600;
  min-width: 80px;
  text-align: right;
}

.marker-date {
  font-size: 11px;
  margin-left: auto;
  white-space: nowrap;
}

.show-all-btn {
  align-self: flex-start;
}

.empty-state {
  padding: 20px;
  text-align: center;
  color: var(--qa-text-muted);
  font-size: 13px;
}
</style>
