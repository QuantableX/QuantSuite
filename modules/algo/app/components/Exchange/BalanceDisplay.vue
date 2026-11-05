<script setup lang="ts">
import type { Balance } from '#algo/types'
import { formatCurrency, formatQuantity } from '#algo/utils/format'

const props = defineProps<{
  balances: Balance[]
}>()

const sortedBalances = computed(() =>
  [...props.balances].sort((a, b) => b.total - a.total)
)

const totals = computed(() => {
  let total = 0
  let available = 0
  let inPositions = 0
  for (const b of props.balances) {
    total += b.total
    available += b.available
    inPositions += b.in_positions
  }
  return { total, available, inPositions }
})

function isStablecoinOrFiat(asset: string): boolean {
  const fiatLike = ['USD', 'USDT', 'USDC', 'BUSD', 'DAI', 'TUSD', 'UST', 'EUR', 'GBP']
  return fiatLike.includes(asset.toUpperCase())
}

function formatBalance(value: number, asset: string): string {
  if (isStablecoinOrFiat(asset)) return formatCurrency(value)
  return formatQuantity(value)
}
</script>

<template>
  <div class="balance-display">
    <div v-if="!balances.length" class="empty-state">
      No balance data
    </div>
    <template v-else>
      <!-- Summary row -->
      <div class="summary-row">
        <div class="summary-item">
          <span class="summary-label">Total Balance</span>
          <span class="summary-value">{{ formatCurrency(totals.total) }}</span>
        </div>
        <div class="summary-item">
          <span class="summary-label">Available</span>
          <span class="summary-value">{{ formatCurrency(totals.available) }}</span>
        </div>
        <div class="summary-item">
          <span class="summary-label">In Positions</span>
          <span class="summary-value">{{ formatCurrency(totals.inPositions) }}</span>
        </div>
      </div>

      <!-- Asset table -->
      <div class="asset-table">
        <table class="table">
          <thead>
            <tr>
              <th>Asset</th>
              <th>Total</th>
              <th>Available</th>
              <th>In Positions</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="b in sortedBalances" :key="b.asset">
              <td class="asset-cell">
                <span class="asset-name">{{ b.asset }}</span>
              </td>
              <td class="mono">{{ formatBalance(b.total, b.asset) }}</td>
              <td class="mono">{{ formatBalance(b.available, b.asset) }}</td>
              <td class="mono">{{ formatBalance(b.in_positions, b.asset) }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </template>
  </div>
</template>

<style scoped>
.balance-display {
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap);
}

/* Figures on the panel's own ground: the total carries the weight. */
.summary-row {
  flex-shrink: 0;
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  gap: var(--qss-gap-tight) 32px;
}

.summary-item {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.summary-label {
  font-size: var(--qss-text-meta);
  font-weight: 500;
  color: var(--qa-text-muted);
}

.summary-value {
  font-size: 15px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  color: var(--qa-text);
}

.summary-item:first-child .summary-value {
  font-size: var(--qss-text-metric);
  line-height: 1.1;
  letter-spacing: -.02em;
}

.asset-table {
  flex: 1 1 auto;
  min-height: 0;
  overflow: auto;
}

.asset-table .table th {
  position: sticky;
  top: 0;
  z-index: 1;
  background: var(--qss-bg-raised);
}

.asset-cell {
  font-weight: 600;
}

.asset-name {
  color: var(--qa-text);
}

.empty-state {
  padding: var(--qss-gap-tight) 0;
  color: var(--qa-text-muted);
  font-size: var(--qss-text-section);
}
</style>
