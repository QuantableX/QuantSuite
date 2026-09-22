<script setup lang="ts">
/**
 * The small month in the sidebar — navigation, not display.
 *
 * It keeps its own cursor so paging ahead to look at December does not drag
 * the main view along; clicking a day is what moves the calendar.
 */
import { useAppStore } from '#plan/stores/app'
import { addDays, addMonths, isSameDay, isToday, startOfWeek, toKey } from '#plan/utils/datetime'

const app = useAppStore()

const localCursor = ref(new Date(app.cursor))

// Following the main view keeps the two in step when the user navigates from
// the header; the local cursor only diverges while browsing ahead here.
watch(
  () => app.cursor,
  (next) => {
    if (next.getMonth() !== localCursor.value.getMonth() || next.getFullYear() !== localCursor.value.getFullYear()) {
      localCursor.value = new Date(next)
    }
  },
)

const label = computed(() =>
  localCursor.value.toLocaleDateString(undefined, { month: 'long', year: 'numeric' }),
)

const weekdays = computed(() => {
  const first = startOfWeek(new Date(), app.settings.weekStartsOn)
  return Array.from({ length: 7 }, (_, i) =>
    addDays(first, i).toLocaleDateString(undefined, { weekday: 'narrow' }),
  )
})

const cells = computed(() => {
  const firstOfMonth = new Date(localCursor.value.getFullYear(), localCursor.value.getMonth(), 1)
  const gridStart = startOfWeek(firstOfMonth, app.settings.weekStartsOn)
  return Array.from({ length: 42 }, (_, i) => addDays(gridStart, i))
})

function inMonth(day: Date): boolean {
  return day.getMonth() === localCursor.value.getMonth()
}
</script>

<template>
  <div class="qp-mm">
    <header class="qp-mm__head">
      <button aria-label="Previous month" @click="localCursor = addMonths(localCursor, -1)">
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="m15 6-6 6 6 6" />
        </svg>
      </button>
      <span>{{ label }}</span>
      <button aria-label="Next month" @click="localCursor = addMonths(localCursor, 1)">
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="m9 6 6 6-6 6" />
        </svg>
      </button>
    </header>

    <div class="qp-mm__weekdays">
      <span v-for="(day, i) in weekdays" :key="i">{{ day }}</span>
    </div>

    <div class="qp-mm__grid">
      <button
        v-for="day in cells"
        :key="toKey(day)"
        class="qp-mm__day"
        :class="{
          'is-outside': !inMonth(day),
          'is-today': isToday(day),
          'is-selected': isSameDay(day, app.cursor),
        }"
        @click="app.setCursor(day)"
      >
        {{ day.getDate() }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.qp-mm {
  padding: 4px 2px 10px;
}

.qp-mm__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 4px;
  padding: 0 4px 6px;
  color: var(--qp-text-secondary);
  font-size: 12px;
  font-weight: 600;
}

.qp-mm__head button {
  display: grid;
  place-items: center;
  width: 20px;
  height: 20px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--qp-text-muted);
  cursor: pointer;
}

.qp-mm__head button:hover {
  background: var(--qp-bg-hover);
  color: var(--qp-text);
}

.qp-mm__weekdays,
.qp-mm__grid {
  display: grid;
  grid-template-columns: repeat(7, 1fr);
}

.qp-mm__weekdays span {
  padding: 2px 0;
  color: var(--qp-text-muted);
  font-size: 10px;
  text-align: center;
}

.qp-mm__day {
  aspect-ratio: 1;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--qp-text-secondary);
  font-size: 11px;
  cursor: pointer;
}

.qp-mm__day:hover {
  background: var(--qp-bg-hover);
  color: var(--qp-text);
}

.qp-mm__day.is-outside {
  color: var(--qp-text-muted);
  opacity: 0.55;
}

.qp-mm__day.is-today {
  color: var(--qp-now);
  font-weight: 700;
}

.qp-mm__day.is-selected {
  background: var(--qp-bg-card);
  color: var(--qp-text);
  box-shadow: inset 0 0 0 1px var(--qp-accent);
}
</style>
