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
    </template>
  </div>
</template>

<style scoped>
.balance-display {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.summary-row {
  display: flex;
  gap: 16px;
  flex-wrap: wrap;
}

.summary-item {
  background: var(--qa-bg-card);
  border: 1px solid var(--qa-border);
  border-radius: var(--qa-radius);
  padding: 14px 18px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  flex: 1;
  min-width: 140px;
}

.summary-label {
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--qa-text-muted);
}

.summary-value {
  font-size: 20px;
  font-weight: 600;
  color: var(--qa-text);
}

.asset-cell {
  font-weight: 600;
}

.asset-name {
  color: var(--qa-text);
}

.empty-state {
  padding: 32px;
  text-align: center;
  color: var(--qa-text-muted);
  font-size: 13px;
}
</style>
