<script setup lang="ts">
import { computed } from 'vue'
import type { BacktestResult } from '#algo/types'
import { formatCurrency, formatPct, formatDateTime, formatDuration } from '#algo/utils/format'

const props = defineProps<{
  result: BacktestResult
}>()

const stats = computed(() => props.result.stats)

// The chart takes a height in pixels: hand it what its panel has free. A detached
// page (warm cache) measures 0 and keeps the last real size.
const chartBox = ref<HTMLElement | null>(null)
const chartHeight = ref(240)
useResizeObserver(chartBox, ([entry]) => {
  if (entry && entry.contentRect.height > 0) chartHeight.value = Math.floor(entry.contentRect.height)
})

const statCards = computed(() => [
  {
    label: 'Total Return',
    value: formatCurrency(stats.value.total_return),
    sub: formatPct(stats.value.total_return_pct),
    colorClass: stats.value.total_return_pct >= 0 ? 'text-success' : 'text-error',
  },
  {
    label: 'Sharpe Ratio',
    value: stats.value.sharpe_ratio.toFixed(2),
    sub: null,
    colorClass: '',
  },
  {
    label: 'Max Drawdown',
    value: formatCurrency(stats.value.max_drawdown),
    sub: formatPct(-Math.abs(stats.value.max_drawdown_pct)),
    colorClass: 'text-error',
  },
  {
    label: 'Win Rate',
    value: `${stats.value.win_rate.toFixed(1)}%`,
    sub: null,
    colorClass: '',
  },
  {
    label: 'Profit Factor',
    value: stats.value.profit_factor.toFixed(2),
    sub: null,
    colorClass: '',
  },
  {
    label: 'Total Trades',
    value: String(stats.value.total_trades),
    sub: null,
    colorClass: '',
  },
  {
    label: 'Avg Duration',
    value: formatDuration(stats.value.avg_trade_duration_secs),
    sub: null,
    colorClass: '',
  },
])
</script>

<template>
  <div class="backtest-results">
    <!-- Stats, one strip -->
    <div class="stats-grid card">
      <div v-for="stat in statCards" :key="stat.label" class="stat-card">
        <div class="stat-label">{{ stat.label }}</div>
        <div class="stat-figure">
          <span class="stat-value" :class="stat.colorClass">{{ stat.value }}</span>
          <span v-if="stat.sub" class="stat-sub" :class="stat.colorClass">{{ stat.sub }}</span>
        </div>
      </div>
    </div>

    <!-- Equity curve chart -->
    <div class="section section--chart card">
      <div class="section-title">Equity Curve</div>
      <div ref="chartBox" class="chart-box">
        <AlgoBacktestEquityCurveChart :data="result.equity_curve" :height="chartHeight" />
      </div>
    </div>

    <!-- Trade list -->
    <div class="section section--trades card">
      <div class="section-title">Trades</div>
      <div class="table-wrap">
        <table class="table">
          <thead>
            <tr>
              <th>Entry Time</th>
              <th>Exit Time</th>
              <th>Pair</th>
              <th>Side</th>
              <th>Entry Price</th>
              <th>Exit Price</th>
              <th>PnL</th>
              <th>PnL %</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="trade in result.trades" :key="trade.id">
              <td class="mono">{{ formatDateTime(trade.entry_time) }}</td>
              <td class="mono">{{ trade.exit_time ? formatDateTime(trade.exit_time) : '---' }}</td>
              <td>{{ trade.pair }}</td>
              <td :class="trade.side === 'long' ? 'text-accent' : 'text-error'">
                {{ trade.side.toUpperCase() }}
              </td>
              <td class="mono">{{ formatCurrency(trade.entry_price) }}</td>
              <td class="mono">{{ trade.exit_price != null ? formatCurrency(trade.exit_price) : '---' }}</td>
              <td
                class="mono"
                :class="trade.pnl != null && trade.pnl >= 0 ? 'text-success' : 'text-error'"
              >
                {{ trade.pnl != null ? formatCurrency(trade.pnl) : '---' }}
              </td>
              <td
                class="mono"
                :class="trade.pnl_pct != null && trade.pnl_pct >= 0 ? 'text-success' : 'text-error'"
              >
                {{ trade.pnl_pct != null ? formatPct(trade.pnl_pct) : '---' }}
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>

<style scoped>
.backtest-results {
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap);
}

/* Stats: one panel, figures in a row, the return carries the weight */
.stats-grid {
  flex-shrink: 0;
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  gap: var(--qss-gap-tight) 24px;
}

.stat-card {
  flex: 1 1 96px;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.stat-label {
  font-size: var(--qss-text-meta);
  font-weight: 500;
  color: var(--qa-text-muted);
  white-space: nowrap;
}

.stat-figure {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 0 6px;
}

.stat-value {
  font-size: 15px;
  font-weight: 600;
  color: var(--qa-text);
  font-variant-numeric: tabular-nums;
}

.stat-card:first-child {
  flex: 1.6 1 164px;
}

.stat-card:first-child .stat-value {
  font-size: var(--qss-text-metric);
  letter-spacing: -.02em;
  line-height: 1.1;
}

.stat-sub {
  font-size: var(--qss-text-label);
  font-variant-numeric: tabular-nums;
}

/* Sections: the chart and the trades share the height */
.section {
  flex: 1 1 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap-tight);
}

.section--chart {
  flex-grow: 5;
}

.section--trades {
  flex-grow: 4;
  overflow: hidden;
}

.section-title {
  flex-shrink: 0;
  font-size: var(--qss-text-section);
  font-weight: 600;
  color: var(--qa-text);
}

/* The chart takes a height in pixels: it gets what this box has free. */
.chart-box {
  flex: 1 1 0;
  min-height: 0;
  overflow: hidden;
}

/* Trade table */
.table-wrap {
  flex: 1 1 0;
  min-height: 0;
  overflow: auto;
}

.table-wrap .table th {
  position: sticky;
  top: 0;
  background: var(--qss-bg-raised);
  z-index: 1;
}

.table-wrap .table td {
  white-space: nowrap;
}

.mono {
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Fira Code', Menlo, Consolas, monospace;
  font-size: var(--qss-text-label);
}
</style>
