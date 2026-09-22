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

function refresh(timeframe: 'all' | '1m' = 'all') {
  forge.openForge({ kind: 'refresh', indicators: [], fast: false, timeframe })
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
        <button class="qsc-btn is-sm" :disabled="forge.isRunning || forge.starting" @click="refresh('1m')">Refresh 1m</button>
        <button class="qsc-btn is-sm is-primary" :disabled="forge.isRunning || forge.starting" @click="refresh('all')">Refresh shelf</button>
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
            <td class="num mono">{{ r.bars.toLocaleString() }}</td>
            <td class="mono">{{ r.first ?? '—' }}</td>
            <td class="mono">{{ r.last ?? '—' }}</td>
            <td class="num mono">{{ formatBytes(r.size_bytes) }}</td>
            <td class="mono muted">{{ new Date(r.modified).toLocaleString() }}</td>
          </tr>
          <tr v-if="!rows.length">
            <td colspan="9" class="muted qsf-empty">No series on the shelf — refresh it to fetch the default shelf (BTC across venues, ETH, XRP, BNB, SOL).</td>
          </tr>
        </tbody>
      </table>
    </div>

    <p class="muted qsf-foot">
      Each of the 1d, 4h, 1h and 1m tracks uses its own candles. Refresh 1m loads the minute shelf;
      further refreshes continue from the last cached candle. Scores stay untested until that track has evidence.
    </p>
  </div>
</template>

<style scoped>
.qsf-shelf {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.qsf-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}
.qsf-head-actions {
  display: flex;
  gap: 6px;
  flex-shrink: 0;
}
.qsf-note {
  font-size: 12px;
}
.qsf-table-wrap {
  overflow-x: auto;
}
.qsf-empty {
  text-align: center;
  padding: 24px 12px;
}
.qsf-foot {
  font-size: 11px;
}
</style>
