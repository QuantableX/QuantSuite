<script setup lang="ts">
/**
 * The price shelf: every cached series (`<EXCHANGE>_<SYMBOL>_<TF>.csv`)
 * with its bars and date span. Each track uses its own exchange candles.
 */
import { useForgeStore } from '#script/stores/forge'
import { formatBytes } from '#script/utils/format'

const forge = useForgeStore()

const rows = computed(() =>
  [...forge.shelf].sort((a, b) => {
    if (a.timeframe !== b.timeframe) return a.timeframe === '1d' ? -1 : b.timeframe === '1d' ? 1 : a.timeframe.localeCompare(b.timeframe)
    return a.key.localeCompare(b.key)
  }),
)

const daily = computed(() => rows.value.filter((r) => r.timeframe === '1d'))
const newest = computed(() => daily.value.reduce<string | null>((acc, r) => (r.last && (!acc || r.last > acc) ? r.last : acc), null))

function refresh() {
  forge.openForge({ kind: 'refresh', indicators: [], fast: false, timeframe: 'all' })
}
</script>

<template>
  <div class="qsf-shelf">
    <div class="qsf-head">
      <p class="muted qsf-note">
        <template v-if="forge.info">
          {{ daily.length }} daily series<template v-if="newest">, newest bar {{ newest }}</template> ·
          <span class="mono">{{ forge.info.price_dir }}</span>
        </template>
        <template v-else>{{ forge.infoLoading ? 'Reading the vault…' : 'The shelf has not been read.' }}</template>
      </p>
      <div class="qsf-head-actions">
        <button class="qsc-btn is-sm is-primary" :disabled="forge.isRunning || forge.starting" @click="refresh()">Refresh shelf</button>
      </div>
    </div>

    <div class="qsc-card is-flush qsf-table-wrap">
      <table class="qsc-table">
        <thead>
          <tr>
            <th>Series</th>
            <th>Exchange</th>
            <th>Symbol</th>
            <th>Timeframe</th>
            <th class="num">Bars</th>
            <th>First</th>
            <th>Last</th>
            <th class="num">Size</th>
            <th>Modified</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="r in rows" :key="r.key">
            <td class="mono">{{ r.key }}</td>
            <td>{{ r.exchange }}</td>
            <td class="mono">{{ r.symbol }}</td>
            <td class="mono">{{ r.timeframe }}</td>
            <td class="num mono">{{ r.bars.toLocaleString('en-US') }}</td>
            <td class="mono">{{ r.first ?? '—' }}</td>
            <td class="mono">{{ r.last ?? '—' }}</td>
            <td class="num mono">{{ formatBytes(r.size_bytes) }}</td>
            <td class="mono muted">{{ new Date(r.modified).toLocaleString('en-US', { dateStyle: 'medium', timeStyle: 'short', hourCycle: 'h23' }) }}</td>
          </tr>
          <tr v-if="!rows.length">
            <td colspan="9" class="muted qsf-empty">No series on the shelf — refresh it to fetch the default shelf (BTC across venues, ETH, XRP, BNB, SOL).</td>
          </tr>
        </tbody>
      </table>
    </div>

    <p class="muted qsf-foot">
      Each of the 1d, 4h and 1h tracks uses its own candles. Refreshes continue from the last cached candle.
      Scores stay untested until that track has evidence.
    </p>
  </div>
</template>

<style scoped>
.qsf-shelf {
  flex: 1 1 0;
  min-height: 240px;
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap-tight);
}
.qsf-shelf > * {
  flex-shrink: 0;
}
.qsf-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--qss-gap);
}
.qsf-head-actions {
  display: flex;
  gap: 6px;
  flex-shrink: 0;
}
.qsf-note {
  color: var(--qss-text-secondary);
  font-size: var(--qss-text-label);
  font-variant-numeric: tabular-nums;
}
/* The table takes the height; its rows scroll under a header that stays. */
.qsf-shelf > .qsf-table-wrap.qsc-card {
  flex: 1 1 0;
  min-height: 0;
  overflow: auto;
}
.qsf-table-wrap thead th {
  position: sticky;
  top: 0;
  z-index: 1;
  background: var(--qss-bg-raised);
  box-shadow: inset 0 -1px 0 var(--qss-border);
}
.qsf-empty {
  padding: var(--qss-gap-tight) 10px;
}
.qsf-foot {
  color: var(--qss-text-secondary);
  font-size: var(--qss-text-meta);
}
</style>
