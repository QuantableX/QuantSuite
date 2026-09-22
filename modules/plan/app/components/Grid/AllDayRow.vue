<script setup lang="ts">
/**
 * The all-day strip above the time grid.
 *
 * All-day events are spans of *dates*, so a bar is laid out by column index,
 * not by minutes — an event from Tuesday to Thursday is one bar three columns
 * wide, not three separate chips.
 */
import { useAppStore } from '#plan/stores/app'
import { useEventsStore } from '#plan/stores/events'
import type { Occurrence } from '#plan/types'
import { fromKey, toKey } from '#plan/utils/datetime'

const emit = defineEmits<{
  (e: 'select', occurrence: Occurrence): void
  (e: 'edit', occurrence: Occurrence): void
}>()

const app = useAppStore()
const events = useEventsStore()

interface Bar {
  occurrence: Occurrence
  /** Column index of the first visible day, and how many columns it spans. */
  column: number
  span: number
  /** Which stacked row it sits in. */
  row: number
}

const bars = computed<Bar[]>(() => {
  const columns = app.days.map(toKey)
  const index = new Map(columns.map((key, i) => [key, i]))

  const spans = events.occurrences
    .filter((o) => o.isAllDay && o.startsOn)
    .map((occurrence) => {
      const start = occurrence.startsOn!.slice(0, 10)
      // endsOn is INCLUSIVE, so a one-day event spans exactly one column.
      const end = (occurrence.endsOn ?? start).slice(0, 10)
      const firstKey = columns[0] ?? start
      const lastKey = columns[columns.length - 1] ?? end
      // Outside the visible range entirely — both edges have to be checked.
      // Testing only the start clamps a span that already ended to the left
      // edge, which draws last month's holiday on this week.
      if (fromKey(end) < fromKey(firstKey) || fromKey(start) > fromKey(lastKey)) return null

      const first = index.get(start)
      // A span that began before the visible week still draws from the left
      // edge, so clamp rather than drop it.
      const from = first ?? 0
      // Likewise on the right: a span running past the week fills to the edge.
      const to = index.get(end) ?? columns.length - 1
      return { occurrence, column: from, span: Math.max(1, to - from + 1) }
    })
    .filter((b): b is { occurrence: Occurrence; column: number; span: number } => b !== null)
    .sort((a, b) => a.column - b.column || b.span - a.span)

  // Stack: first row whose occupied columns do not overlap this bar.
  const rows: boolean[][] = []
  return spans.map((bar) => {
    let row = rows.findIndex((used) => !used.slice(bar.column, bar.column + bar.span).some(Boolean))
    if (row === -1) {
      row = rows.length
      rows.push(new Array(app.days.length).fill(false))
    }
    for (let i = bar.column; i < bar.column + bar.span; i++) rows[row]![i] = true
    return { ...bar, row }
  })
})

const rowCount = computed(() => bars.value.reduce((max, b) => Math.max(max, b.row + 1), 0))
</script>

<template>
  <div
    v-if="rowCount > 0"
    class="qp-ad"
    :style="{
      gridTemplateColumns: `var(--qp-gutter-w) repeat(${app.days.length}, 1fr)`,
      gridTemplateRows: `repeat(${rowCount}, 20px)`,
    }"
  >
    <span class="qp-ad__label" :style="{ gridRow: `1 / span ${rowCount}` }">All-day</span>
    <button
      v-for="bar in bars"
      :key="`${bar.occurrence.eventId}-${bar.occurrence.occurrenceStart}`"
      class="qp-ad__bar qp-ev"
      :data-color="bar.occurrence.color"
      :style="{ gridColumn: `${bar.column + 2} / span ${bar.span}`, gridRow: bar.row + 1 }"
      @click="emit('select', bar.occurrence); emit('edit', bar.occurrence)"
    >
      {{ bar.occurrence.title || 'Untitled' }}
    </button>
  </div>
</template>

<style scoped>
.qp-ad {
  display: grid;
  gap: 2px;
  padding: 3px 0;
  border-bottom: 1px solid var(--qp-border);
}

.qp-ad__label {
  grid-column: 1;
  padding-right: 8px;
  color: var(--qp-text-muted);
  font-size: 10px;
  text-align: right;
}

.qp-ad__bar {
  overflow: hidden;
  padding: 1px 6px;
  border-radius: 4px;
  font-size: 11px;
  text-align: left;
  text-overflow: ellipsis;
  white-space: nowrap;
  cursor: pointer;
}
</style>
