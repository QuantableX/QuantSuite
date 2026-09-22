<script setup lang="ts">
import type { LiveResult } from '#systems/types'

const props = defineProps<{ result: LiveResult }>()

const maxScore = computed(() => Math.max(1, props.result.symbols.length - 1))

const sorted = computed(() =>
  [...props.result.universe].sort((a, b) => (b.score ?? -1) - (a.score ?? -1)),
)

const bestScore = computed(() =>
  props.result.best ? props.result.scores[props.result.best] ?? 0 : 0,
)

function fmtCap(cap: number | null): string {
  if (!cap) return '—'
  if (cap >= 1e12) return `$${(cap / 1e12).toFixed(2)}T`
  if (cap >= 1e9) return `$${(cap / 1e9).toFixed(1)}B`
  if (cap >= 1e6) return `$${(cap / 1e6).toFixed(1)}M`
  return `$${cap.toFixed(0)}`
}
</script>

<template>
  <section class="card qs-rank">
    <header class="qs-rank__head">
      <span class="qs-rank__lbl">Winner</span>
      <span class="qs-rank__symbol">{{ result.best ?? '—' }}</span>
      <span class="qs-rank__wins mono">{{ bestScore }}/{{ maxScore }}</span>
    </header>

    <div class="qs-rank__cols">
      <span>#</span>
      <span>Asset</span>
      <span class="qs-rank__right">Wins</span>
      <span class="qs-rank__right">Mkt Cap</span>
    </div>

    <div class="qs-rank__scroll">
      <!-- Each row doubles as its own score bar: the tinted fill behind the row
           is the win share, so no separate bar column is needed. -->
      <div
        v-for="coin in sorted"
        :key="coin.symbol"
        class="qs-rank__row"
        :class="{ 'qs-rank__row--best': coin.symbol === result.best }"
        :title="coin.name ?? coin.symbol"
      >
        <div class="qs-rank__fill" :style="{ width: `${((coin.score ?? 0) / maxScore) * 100}%` }" />
        <span class="qs-rank__num mono">{{ coin.rank }}</span>
        <span class="qs-rank__sym">
          {{ coin.symbol }}
          <span v-if="!coin.hasData" class="qs-rank__nodata" title="No OHLCV data">no data</span>
        </span>
        <span class="qs-rank__right mono qs-rank__score">{{ coin.score ?? 0 }}</span>
        <span class="qs-rank__right mono qs-rank__cap">{{ fmtCap(coin.marketCap) }}</span>
      </div>
    </div>
  </section>
</template>

<style scoped>
.qs-rank {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
}

.qs-rank__head {
  display: flex;
  align-items: baseline;
  gap: 10px;
  padding: 10px 12px;
  border-bottom: 1px solid var(--qs-border);
}

.qs-rank__lbl {
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--qs-text-muted);
}

.qs-rank__symbol {
  font-size: 20px;
  font-weight: 700;
  line-height: 1;
  color: var(--qs-text);
}

.qs-rank__wins {
  margin-left: auto;
  font-size: 12px;
  color: var(--qs-text-secondary);
}

.qs-rank__cols,
.qs-rank__row {
  display: grid;
  grid-template-columns: 26px 1fr 38px 66px;
  align-items: center;
  gap: 8px;
  padding: 0 12px;
}

.qs-rank__cols {
  height: 22px;
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--qs-text-muted);
  border-bottom: 1px solid var(--qs-border-subtle);
}

.qs-rank__scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
}

/* Rows share out the leftover height (up to a sane cap) for a short universe,
   and collapse to 26px + scroll for a long one. */
.qs-rank__row {
  position: relative;
  flex: 1 0 26px;
  min-height: 26px;
  max-height: 40px;
  font-size: 12px;
}

.qs-rank__row + .qs-rank__row {
  border-top: 1px solid color-mix(in srgb, var(--qs-border-subtle) 45%, transparent);
}

.qs-rank__row:hover {
  background: var(--qs-bg-hover);
}

.qs-rank__fill {
  position: absolute;
  inset: 0 auto 0 0;
  background: color-mix(in srgb, var(--qs-accent) 20%, transparent);
  transition: width 300ms ease;
  pointer-events: none;
}

.qs-rank__row--best .qs-rank__fill {
  background: color-mix(in srgb, var(--qs-success) 26%, transparent);
}

.qs-rank__num,
.qs-rank__sym,
.qs-rank__score,
.qs-rank__cap {
  position: relative;
}

.qs-rank__num {
  font-size: 11px;
  color: var(--qs-text-muted);
}

.qs-rank__sym {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  font-weight: 600;
  color: var(--qs-text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.qs-rank__nodata {
  font-size: 9px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--qs-warning);
}

.qs-rank__right {
  text-align: right;
}

.qs-rank__score {
  color: var(--qs-text-secondary);
}

.qs-rank__cap {
  font-size: 11px;
  color: var(--qs-text-muted);
}
</style>
