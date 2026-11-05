<script setup lang="ts">
import type { TradeStats } from '#algo/types'
import { formatCurrency, formatPct, formatDuration } from '#algo/utils/format'

const props = defineProps<{
  stats: TradeStats | null
}>()

const statCards = computed(() => {
  if (!props.stats) return []
  const s = props.stats
  const funding = s.funding ?? 0
  return [
    {
      label: 'PnL',
      value: formatCurrency(s.total_pnl),
      colorClass: s.total_pnl > 0 ? 'value-positive' : s.total_pnl < 0 ? 'value-negative' : '',
    },
    // Perp funding is in the PnL above; shown apart when there is any.
    ...(funding !== 0
      ? [{ label: 'Funding', value: formatCurrency(funding), colorClass: funding > 0 ? 'value-positive' : 'value-negative' }]
      : []),
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
/* One strip of figures, no tiles: the PnL carries the weight. */
.stats-header {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  gap: var(--qss-gap-tight) 20px;
}

.stat-card {
  display: flex;
  flex-direction: column;
  gap: 2px;
  flex: 1 1 84px;
  min-width: 0;
}

.stat-label {
  font-size: var(--qss-text-meta);
  font-weight: 500;
  color: var(--qa-text-muted);
  white-space: nowrap;
}

.stat-value {
  font-size: 14px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  color: var(--qa-text);
  white-space: nowrap;
}

.stat-card:first-child .stat-value {
  font-size: var(--qss-text-metric);
  line-height: 1.1;
  letter-spacing: -.02em;
}

.value-positive {
  color: var(--qa-success);
}

.value-negative {
  color: var(--qa-error);
}

.empty-state {
  width: 100%;
  color: var(--qa-text-muted);
  font-size: var(--qss-text-section);
}
</style>
