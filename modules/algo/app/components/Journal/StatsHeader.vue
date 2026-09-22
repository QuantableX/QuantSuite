<script setup lang="ts">
import type { TradeStats } from '#algo/types'
import { formatCurrency, formatPct, formatDuration } from '#algo/utils/format'

const props = defineProps<{
  stats: TradeStats | null
}>()

const statCards = computed(() => {
  if (!props.stats) return []
  const s = props.stats
  return [
    {
      label: 'PnL',
      value: formatCurrency(s.total_pnl),
      colorClass: s.total_pnl > 0 ? 'value-positive' : s.total_pnl < 0 ? 'value-negative' : '',
    },
    {
      label: 'Total Trades',
      value: String(s.total_trades),
      colorClass: '',
    },
    {
      label: 'Win Rate',
      value: formatPct(s.win_rate, 1),
      colorClass: s.win_rate > 50 ? 'value-positive' : s.win_rate < 50 ? 'value-negative' : '',
    },
    {
      label: 'Avg Win',
      value: formatCurrency(s.avg_win),
      colorClass: 'value-positive',
    },
    {
      label: 'Avg Loss',
      value: formatCurrency(s.avg_loss),
      colorClass: 'value-negative',
    },
    {
      label: 'Profit Factor',
      value: s.profit_factor.toFixed(2),
      colorClass: s.profit_factor > 1 ? 'value-positive' : s.profit_factor < 1 ? 'value-negative' : '',
    },
    {
      label: 'Expectancy',
      value: formatCurrency(s.expectancy),
      colorClass: s.expectancy >= 0 ? 'value-positive' : 'value-negative',
    },
    {
      label: 'Best Trade',
      value: formatCurrency(s.best_trade),
      colorClass: 'value-positive',
    },
    {
      label: 'Worst Trade',
      value: formatCurrency(s.worst_trade),
      colorClass: 'value-negative',
    },
  ]
})
</script>

<template>
  <div class="stats-header">
    <template v-if="stats">
      <div v-for="card in statCards" :key="card.label" class="stat-card">
        <span class="stat-label">{{ card.label }}</span>
        <span class="stat-value" :class="card.colorClass">{{ card.value }}</span>
      </div>
    </template>
    <template v-else>
      <div class="empty-state">No data</div>
    </template>
  </div>
</template>

<style scoped>
.stats-header {
  display: flex;
  gap: 12px;
  overflow-x: auto;
  padding-bottom: 4px;
}

.stat-card {
  background: var(--qa-bg-card);
  border: 1px solid var(--qa-border);
  border-radius: var(--qa-radius);
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  flex: 1 1 0;
  min-width: 96px;
}

.stat-label {
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--qa-text-muted);
}

.stat-value {
  font-size: 18px;
  font-weight: 600;
  color: var(--qa-text);
}

.value-positive {
  color: var(--qa-success);
}

.value-negative {
  color: var(--qa-error);
}

.empty-state {
  width: 100%;
  padding: 24px;
  text-align: center;
  color: var(--qa-text-muted);
  font-size: 13px;
}
</style>
