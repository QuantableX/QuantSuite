<script setup lang="ts">
/**
 * One row per bot (PLAN-QUANTALGO §3.4) — every bot, whatever the page's
 * Paper | Live switch says; a bot has one mode anyway.
 */
import type { TradeStatsBreakdown, TradeStats } from '#algo/types'
import { formatCurrency, formatPct } from '#algo/utils/format'

const props = defineProps<{
  breakdown: TradeStatsBreakdown
}>()

// Bots with trades first, best PnL on top; the idle ones keep their order.
const bots = computed(() =>
  [...props.breakdown.bots].sort((a, b) => {
    const ta = a.stats.total_trades > 0 ? 1 : 0
    const tb = b.stats.total_trades > 0 ? 1 : 0
    if (ta !== tb) return tb - ta
    return b.stats.total_pnl - a.stats.total_pnl
  }),
)

function profitFactor(s: TradeStats): string {
  if (s.total_trades === 0) return '--'
  return Number.isFinite(s.profit_factor) ? s.profit_factor.toFixed(2) : '∞'
}

function pnlClass(value: number): string {
  return value > 0 ? 'text-success' : value < 0 ? 'text-error' : ''
}
</script>

<template>
  <div class="breakdown">
    <div class="breakdown__bots card">
      <table class="table breakdown__table">
        <thead>
          <tr>
            <th>Bot</th>
            <th>Mode</th>
            <th class="num">Trades</th>
            <th class="num">Win rate</th>
            <th class="num">PnL</th>
            <th class="num">Avg win</th>
            <th class="num">Avg loss</th>
            <th class="num">Profit factor</th>
            <th class="num">Best</th>
            <th class="num">Worst</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in bots" :key="row.bot_id ?? row.name" :class="{ 'row--idle': row.stats.total_trades === 0 }">
            <td class="cell-bot">{{ row.name }}</td>
            <td>
              <span class="mode-tag" :class="`mode-tag--${row.trading_mode}`">{{ row.trading_mode }}</span>
            </td>
            <td class="num mono">{{ row.stats.total_trades }}</td>
            <td class="num mono">{{ row.stats.total_trades ? formatPct(row.stats.win_rate, 1).replace('+', '') : '--' }}</td>
            <td class="num mono" :class="pnlClass(row.stats.total_pnl)">{{ formatCurrency(row.stats.total_pnl) }}</td>
            <td class="num mono">{{ row.stats.total_trades ? formatCurrency(row.stats.avg_win) : '--' }}</td>
            <td class="num mono">{{ row.stats.total_trades ? formatCurrency(row.stats.avg_loss) : '--' }}</td>
            <td class="num mono">{{ profitFactor(row.stats) }}</td>
            <td class="num mono">{{ row.stats.total_trades ? formatCurrency(row.stats.best_trade) : '--' }}</td>
            <td class="num mono">{{ row.stats.total_trades ? formatCurrency(row.stats.worst_trade) : '--' }}</td>
          </tr>
          <tr v-if="!bots.length">
            <td colspan="10" class="text-muted">No bots yet</td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<style scoped>
.breakdown {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.breakdown__bots {
  padding: 0;
  overflow-x: auto;
}

.breakdown__table thead th {
  position: sticky;
  top: 0;
  z-index: 1;
  background: var(--qa-bg-card);
}

.breakdown__table {
  width: 100%;
}

.breakdown__table th {
  white-space: nowrap;
}

.breakdown__table th.num,
.breakdown__table td.num {
  text-align: right;
  white-space: nowrap;
}

.cell-bot {
  font-weight: 500;
  color: var(--qa-text);
  white-space: nowrap;
}

.row--idle td {
  color: var(--qa-text-muted);
}

.mode-tag {
  display: inline-block;
  padding: 1px 6px;
  border-radius: 4px;
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--qa-text-secondary);
  border: 1px solid var(--qa-border-subtle);
}

.mode-tag--live {
  color: var(--qa-error);
  border-color: color-mix(in srgb, var(--qa-error) 45%, transparent);
}
</style>
