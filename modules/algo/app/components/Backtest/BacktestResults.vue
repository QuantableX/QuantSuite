<script setup lang="ts">
import { computed } from 'vue'
import type { BacktestResult } from '#algo/types'
import { formatCurrency, formatPct, formatDateTime, formatDuration } from '#algo/utils/format'

const props = defineProps<{
  result: BacktestResult
}>()

const stats = computed(() => props.result.stats)

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
    <!-- Stats grid -->
    <div class="stats-grid">
      <div v-for="stat in statCards" :key="stat.label" class="stat-card">
        <div class="stat-label">{{ stat.label }}</div>
        <div class="stat-value" :class="stat.colorClass">
          {{ stat.value }}
        </div>
        <div v-if="stat.sub" class="stat-sub" :class="stat.colorClass">
          {{ stat.sub }}
        </div>
      </div>
    </div>

    <!-- Equity curve chart -->
    <div class="section">
      <div class="section-title">Equity Curve</div>
      <AlgoBacktestEquityCurveChart :data="result.equity_curve" />
    </div>

    <!-- Trade list -->
    <div class="section">
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
  gap: 20px;
}

/* Stats grid */
.stats-grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 12px;
}

.stat-card {
  background: var(--qa-bg-card);
  border: 1px solid var(--qa-border);
  border-radius: var(--qa-radius);
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 4px;
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
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Fira Code', Menlo, Consolas, monospace;
}

.stat-sub {
  font-size: 12px;
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Fira Code', Menlo, Consolas, monospace;
}

/* Sections */
.section {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.section-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--qa-text-secondary);
}

/* Trade table */
.table-wrap {
  overflow: auto;
  max-height: 400px;
  border: 1px solid var(--qa-border);
  border-radius: var(--qa-radius);
}

.table-wrap .table th {
  position: sticky;
  top: 0;
  background: var(--qa-bg-card);
  z-index: 1;
}

.mono {
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Fira Code', Menlo, Consolas, monospace;
  font-size: 12px;
}
</style>
