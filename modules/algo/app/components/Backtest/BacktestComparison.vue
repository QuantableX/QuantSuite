<script setup lang="ts">
import { computed } from 'vue'
import type { BacktestResult, EquityPoint } from '#algo/types'
import { formatCurrency, formatPct } from '#algo/utils/format'

const props = defineProps<{
  results: BacktestResult[]
}>()

const COLORS = [
  '#a0a0a8',
  '#b8b8c0',
  '#ffa502',
  '#ff4757',
  '#5f9ee8',
  '#c471ed',
  '#f7d794',
  '#8a8a94',
]

function colorForIndex(i: number): string {
  return COLORS[i % COLORS.length] ?? '#a0a0a8'
}

interface OverlaySeries {
  name: string
  color: string
  data: EquityPoint[]
}

const overlayData = computed<OverlaySeries[]>(() =>
  props.results.map((r, i) => ({
    name: r.name,
    color: colorForIndex(i),
    data: r.equity_curve,
  })),
)

interface StatRow {
  label: string
  values: string[]
}

const statRows = computed<StatRow[]>(() => {
  const rows: StatRow[] = [
    {
      label: 'Total Return',
      values: props.results.map((r) =>
        `${formatCurrency(r.stats.total_return)} (${formatPct(r.stats.total_return_pct)})`,
      ),
    },
    {
      label: 'Sharpe Ratio',
      values: props.results.map((r) => r.stats.sharpe_ratio.toFixed(2)),
    },
    {
      label: 'Max Drawdown',
      values: props.results.map((r) =>
        `${formatCurrency(r.stats.max_drawdown)} (${formatPct(-Math.abs(r.stats.max_drawdown_pct))})`,
      ),
    },
    {
      label: 'Win Rate',
      values: props.results.map((r) => `${r.stats.win_rate.toFixed(1)}%`),
    },
    {
      label: 'Profit Factor',
      values: props.results.map((r) => r.stats.profit_factor.toFixed(2)),
    },
    {
      label: 'Total Trades',
      values: props.results.map((r) => String(r.stats.total_trades)),
    },
  ]
  return rows
})
</script>

<template>
  <div class="comparison">
    <!-- Overlay equity curves -->
    <div class="section">
      <div class="section-title">Equity Curves</div>
      <AlgoBacktestEquityCurveChart
        v-if="overlayData.length > 0 && overlayData[0]"
        :data="overlayData[0].data"
        :overlays="overlayData"
        :height="400"
      />
      <div class="legend">
        <div
          v-for="(series, i) in overlayData"
          :key="series.name"
          class="legend-item"
        >
          <span
            class="legend-swatch"
            :style="{ background: colorForIndex(i) }"
          />
          <span class="legend-label">{{ series.name }}</span>
        </div>
      </div>
    </div>

    <!-- Comparison table -->
    <div class="section">
      <div class="section-title">Statistics Comparison</div>
      <div class="table-wrap">
        <table class="table">
          <thead>
            <tr>
              <th>Metric</th>
              <th v-for="r in results" :key="r.id">
                <div class="col-header">
                  <span class="col-name">{{ r.name }}</span>
                </div>
              </th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in statRows" :key="row.label">
              <td class="metric-label">{{ row.label }}</td>
              <td
                v-for="(val, i) in row.values"
                :key="i"
                class="mono"
              >
                {{ val }}
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>

<style scoped>
.comparison {
  display: flex;
  flex-direction: column;
  gap: 24px;
}

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

/* Legend */
.legend {
  display: flex;
  flex-wrap: wrap;
  gap: 16px;
  padding: 8px 0;
}

.legend-item {
  display: flex;
  align-items: center;
  gap: 6px;
}

.legend-swatch {
  width: 12px;
  height: 3px;
  border-radius: 2px;
  flex-shrink: 0;
}

.legend-label {
  font-size: 12px;
  color: var(--qa-text-secondary);
}

/* Table */
.table-wrap {
  overflow: auto;
  border: 1px solid var(--qa-border);
  border-radius: var(--qa-radius);
}

.col-header {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.col-name {
  font-size: 12px;
  font-weight: 600;
}

.metric-label {
  font-size: 12px;
  font-weight: 500;
  color: var(--qa-text-secondary);
  white-space: nowrap;
}

.mono {
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Fira Code', Menlo, Consolas, monospace;
  font-size: 12px;
}
</style>
