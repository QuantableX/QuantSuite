<script setup lang="ts">
import type { LiveResult } from '#systems/types'

const props = defineProps<{ result: LiveResult }>()

const symbols = computed(() => props.result.symbols)

function cell(i: number, j: number): number | null {
  return props.result.scoreMatrix?.[i]?.[j] ?? null
}

function cellClass(i: number, j: number): string {
  if (i === j) return 'qs-mx__cell--diag'
  const v = cell(i, j)
  if (v === null) return 'qs-mx__cell--na'
  return v === 1 ? 'qs-mx__cell--win' : 'qs-mx__cell--lose'
}

function cellTitle(i: number, j: number): string {
  const a = symbols.value[i]
  const b = symbols.value[j]
  if (i === j) return a ?? ''
  const v = cell(i, j)
  if (v === null) return `${a} vs ${b}: no data`
  return v === 1 ? `${a} beats ${b}` : `${a} loses to ${b}`
}
</script>

<template>
  <section class="card qs-mx">
    <header class="qs-mx__head">
      <h3 class="qs-mx__title">Pairwise Score Matrix</h3>
      <span class="qs-mx__hint">row beats column → green</span>
    </header>
    <div class="qs-mx__scroll">
      <table class="qs-mx__table" :style="{ '--n': symbols.length }">
        <thead>
          <tr>
            <th class="qs-mx__corner" />
            <th v-for="s in symbols" :key="`h-${s}`" class="qs-mx__colhead mono" :title="s">
              <span>{{ s }}</span>
            </th>
            <th class="qs-mx__winhead">W</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(rowSym, i) in symbols" :key="`r-${rowSym}`">
            <th
              class="qs-mx__rowhead mono"
              :class="{ 'qs-mx__rowhead--best': rowSym === result.best }"
              :title="rowSym"
            >
              {{ rowSym }}
            </th>
            <td
              v-for="(colSym, j) in symbols"
              :key="`c-${rowSym}-${colSym}`"
              class="qs-mx__cell"
              :class="cellClass(i, j)"
              :title="cellTitle(i, j)"
            >
              <div class="qs-mx__sq" />
            </td>
            <td class="qs-mx__wins mono">{{ result.scores[rowSym] ?? 0 }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </section>
</template>

<style scoped>
.qs-mx {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
}

.qs-mx__head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 10px;
  padding: 10px 12px;
  border-bottom: 1px solid var(--qs-border);
}

.qs-mx__title {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
}

.qs-mx__hint {
  font-size: 11px;
  color: var(--qs-text-muted);
}

.qs-mx__scroll {
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: 0 12px 12px;
}

/* The grid stretches to fill the panel: cells stay square (the inner square
   drives row height), clamped so a small universe does not blow up and a large
   one falls back to scrolling. */
.qs-mx__table {
  table-layout: fixed;
  width: 100%;
  min-width: calc(74px + var(--n) * 22px);
  max-width: calc(74px + var(--n) * 42px);
  border-collapse: separate;
  border-spacing: 0;
  font-size: 11px;
}

.qs-mx__corner,
.qs-mx__colhead,
.qs-mx__winhead,
.qs-mx__rowhead {
  position: sticky;
  background: var(--qs-bg-card);
  color: var(--qs-text-secondary);
  font-weight: 600;
  z-index: 1;
}

.qs-mx__corner,
.qs-mx__colhead,
.qs-mx__winhead {
  top: 0;
  height: 52px;
  padding: 0 0 4px;
  vertical-align: bottom;
}

.qs-mx__corner,
.qs-mx__rowhead {
  left: 0;
}

/* table-layout: fixed takes its column widths from the header row, so the
   row-label column has to be sized on the corner cell. */
.qs-mx__corner {
  width: 44px;
  z-index: 2;
}

/* Vertical labels keep every column at cell width, so a 15-asset matrix
   still fits without horizontal scrolling. */
.qs-mx__colhead span {
  writing-mode: vertical-rl;
  transform: rotate(180deg);
  display: inline-block;
  max-height: 46px;
  overflow: hidden;
}

.qs-mx__winhead {
  width: 30px;
  padding-left: 8px;
  text-align: center;
}

.qs-mx__rowhead {
  width: 44px;
  padding: 0 8px 0 0;
  text-align: right;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.qs-mx__rowhead--best {
  color: var(--qs-text);
}

.qs-mx__cell {
  padding: 0;
  border: 1px solid var(--qs-bg-card);
  border-radius: 2px;
}

.qs-mx__sq {
  width: 100%;
  aspect-ratio: 1;
  max-height: 40px;
}

.qs-mx__cell--win {
  background: var(--qs-success);
}

.qs-mx__cell--lose {
  background: color-mix(in srgb, var(--qs-error) 55%, transparent);
}

.qs-mx__cell--diag {
  background: var(--qs-bg-input);
}

.qs-mx__cell--na {
  background: repeating-linear-gradient(
    45deg,
    var(--qs-bg-input),
    var(--qs-bg-input) 3px,
    var(--qs-bg-hover) 3px,
    var(--qs-bg-hover) 6px
  );
}

.qs-mx__wins {
  padding-left: 8px;
  text-align: center;
  color: var(--qs-text-secondary);
}
</style>
