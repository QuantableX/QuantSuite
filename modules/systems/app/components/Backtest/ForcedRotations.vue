<script setup lang="ts">
import type { BacktestResult } from '#systems/types'

const props = defineProps<{ result: BacktestResult }>()

// With compared indicators the legs of one run are listed at a time; the
// configured indicator's run comes first and is the default.
const runs = computed(() => props.result.strategies ?? [])
const activeKey = ref<string | null>(null)
watch(() => props.result, () => { activeKey.value = null })
const active = computed(() => runs.value.find(r => r.key === activeKey.value) ?? runs.value[0])

const held = computed(() => {
  // Compress the per-day held series into contiguous holding segments.
  const segments: { symbol: string | null; from: string; to: string }[] = []
  for (const point of active.value?.heldAsset ?? props.result.heldAsset) {
    const last = segments[segments.length - 1]
    if (last && last.symbol === point.symbol) {
      last.to = point.time
    } else {
      segments.push({ symbol: point.symbol, from: point.time, to: point.time })
    }
  }
  return segments.slice().reverse()
})
</script>

<template>
  <div class="card qs-forced">
    <div class="qs-forced__inner">
      <div class="qs-forced__head">
        <h3 class="qs-forced__title">Holdings History</h3>
        <span v-if="runs.length === 1 && result.skippedStrategies?.length" class="mono">{{ active?.label }}</span>
        <select
          v-if="runs.length > 1"
          class="select qs-forced__pick"
          :value="active?.key"
          aria-label="Strategy whose holdings are listed"
          @change="activeKey = ($event.target as HTMLSelectElement).value"
        >
          <option v-for="run in runs" :key="run.key" :value="run.key">{{ run.label }}</option>
        </select>
        <span class="pill">{{ held.length }} leg{{ held.length === 1 ? '' : 's' }}</span>
      </div>

      <div class="qs-forced__table">
        <div class="qs-forced__row qs-forced__row--head">
          <span>Asset</span>
          <span>From</span>
          <span>Until</span>
        </div>
        <div class="qs-forced__rows">
          <div
            v-for="(seg, i) in held"
            :key="i"
            class="qs-forced__row"
            :class="{ 'qs-forced__row--cash': !seg.symbol }"
          >
            <span class="qs-forced__sym mono">{{ seg.symbol ?? 'USD' }}</span>
            <span class="qs-forced__cell mono">{{ seg.from }}</span>
            <span class="qs-forced__cell mono">{{ seg.to }}</span>
          </div>
          <div v-if="!held.length" class="qs-forced__empty">No holdings recorded.</div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* The card itself only stretches to match the left tile (grid align-items:
   stretch). The scrollable content is absolutely positioned so its length never
   feeds back into grid row sizing — that was the cause of the runaway height. */
.qs-forced {
  position: relative;
  padding: 0;
  overflow: hidden;
}

.qs-forced__inner {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  padding: 12px 14px;
}

.qs-forced__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  margin-bottom: 10px;
}

.qs-forced__title {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
}

/* One dropdown for the compared runs: a row of tabs wrapped into a block
   once a handful of indicators were compared. */
.qs-forced__pick {
  width: auto;
  margin-left: auto;
  padding: 3px 9px;
  font-size: 12px;
}

.qs-forced__table {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.qs-forced__row {
  display: grid;
  grid-template-columns: 64px 1fr 1fr;
  align-items: center;
  gap: 10px;
  padding: 7px 10px;
}

.qs-forced__row--head {
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--qs-text-muted);
  border-bottom: 1px solid var(--qs-border);
  padding-bottom: 8px;
}

.qs-forced__rows {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding-top: 6px;
}

.qs-forced__rows .qs-forced__row {
  border-radius: 6px;
  background: var(--qs-bg-input);
  font-size: 12px;
}

.qs-forced__rows .qs-forced__row:hover {
  background: var(--qs-bg-hover);
}

/* Cash legs are the forced exits — worth spotting without reading symbols. */
.qs-forced__row--cash {
  box-shadow: inset 2px 0 0 var(--qs-warning);
}

.qs-forced__row--cash .qs-forced__sym {
  color: var(--qs-warning);
}

.qs-forced__sym {
  font-weight: 700;
  color: var(--qs-text);
}

.qs-forced__cell {
  font-size: 11px;
  color: var(--qs-text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.qs-forced__empty {
  font-size: 12px;
  color: var(--qs-text-muted);
  padding: 8px 10px;
}
</style>
