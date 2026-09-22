<script setup lang="ts">
/**
 * The month view — a fixed six-week grid of day cells with event chips.
 *
 * Six weeks always, so the layout does not jump by a row between a month that
 * needs five and one that needs six. Cells cap their chips and show "+N more"
 * rather than growing: a day with twelve events must not stretch its week.
 */
import { useAppStore } from '#plan/stores/app'
import { useEventsStore } from '#plan/stores/events'
import type { Occurrence } from '#plan/types'
import { daySegment, formatTime, isToday, toKey } from '#plan/utils/datetime'

const emit = defineEmits<{
  (e: 'select', occurrence: Occurrence): void
  (e: 'edit', occurrence: Occurrence): void
  (e: 'create-at', at: Date): void
}>()

const app = useAppStore()
const events = useEventsStore()

const MAX_CHIPS = 3

const weekdayLabels = computed(() =>
  app.days.slice(0, 7).map((d) => d.toLocaleDateString(undefined, { weekday: 'short' })),
)

function chipsFor(day: Date) {
  return events.byDay.get(toKey(day)) ?? []
}

function inCursorMonth(day: Date): boolean {
  return day.getMonth() === app.cursor.getMonth()
}

/** On the morning an overnight event spills into, the useful hour is its end. */
function timeLabel(occ: Occurrence, day: Date): string {
  if (occ.isAllDay || !occ.startsAt) return ''
  const seg = daySegment(occ, day)
  if (seg.continuesBefore) return `→ ${formatTime(seg.end, app.settings.timeFormat)}`
  return formatTime(seg.start, app.settings.timeFormat)
}

/** Clicking "+N more" narrows to that day rather than opening a popover —
 * the day view is the thing a popover would be a worse copy of. */
function openDay(day: Date) {
  app.setCursor(day)
  app.setView('day')
}
</script>

<template>
  <div class="qp-mg">
    <div class="qp-mg__weekdays">
      <span v-for="label in weekdayLabels" :key="label">{{ label }}</span>
    </div>

    <div class="qp-mg__grid">
      <div
        v-for="day in app.days"
        :key="toKey(day)"
        class="qp-mg__cell"
        :class="{ 'is-outside': !inCursorMonth(day), 'is-today': isToday(day) }"
        @dblclick="emit('create-at', day)"
      >
        <header class="qp-mg__head">
          <button class="qp-mg__num" @click="openDay(day)">{{ day.getDate() }}</button>
        </header>

        <div class="qp-mg__chips">
          <button
            v-for="occ in chipsFor(day).slice(0, MAX_CHIPS)"
            :key="`${occ.eventId}-${occ.occurrenceStart}`"
            class="qp-mg__chip qp-ev"
            :data-color="occ.color"
            @click.stop="emit('select', occ); emit('edit', occ)"
          >
            <span v-if="timeLabel(occ, day)" class="qp-mg__chip-time">{{ timeLabel(occ, day) }}</span>
            <span class="qp-mg__chip-title">{{ occ.title || 'Untitled' }}</span>
          </button>

          <button
            v-if="chipsFor(day).length > MAX_CHIPS"
            class="qp-mg__more"
            @click.stop="openDay(day)"
          >
            +{{ chipsFor(day).length - MAX_CHIPS }} more
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.qp-mg {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

.qp-mg__weekdays {
  display: grid;
  grid-template-columns: repeat(7, 1fr);
  border-bottom: 1px solid var(--qp-border);
}

.qp-mg__weekdays span {
  padding: 6px 8px;
  color: var(--qp-text-muted);
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.05em;
  text-transform: uppercase;
}

.qp-mg__grid {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: repeat(7, 1fr);
  grid-auto-rows: minmax(0, 1fr);
}

.qp-mg__cell {
  display: flex;
  flex-direction: column;
  min-height: 0;
  padding: 2px 3px 4px;
  border-right: 1px solid var(--qp-border-subtle);
  border-bottom: 1px solid var(--qp-border-subtle);
}

.qp-mg__cell.is-outside {
  background: color-mix(in srgb, var(--qp-bg-raised) 45%, transparent);
}

.qp-mg__cell.is-outside .qp-mg__num {
  color: var(--qp-text-muted);
}

.qp-mg__head {
  display: flex;
  justify-content: flex-start;
}

.qp-mg__num {
  min-width: 20px;
  height: 20px;
  padding: 0 5px;
  border: none;
  border-radius: 999px;
  background: transparent;
  color: var(--qp-text-secondary);
  font-size: 11px;
  font-weight: 600;
  cursor: pointer;
}

.qp-mg__num:hover {
  background: var(--qp-bg-hover);
  color: var(--qp-text);
}

.qp-mg__cell.is-today .qp-mg__num {
  background: var(--qp-now);
  color: #fff;
}

.qp-mg__chips {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
  overflow: hidden;
}

.qp-mg__chip {
  display: flex;
  align-items: baseline;
  gap: 4px;
  overflow: hidden;
  padding: 1px 5px;
  border-radius: 3px;
  font-size: 11px;
  text-align: left;
  white-space: nowrap;
  cursor: pointer;
}

.qp-mg__chip-time {
  flex-shrink: 0;
  color: var(--qp-text-secondary);
  font-size: 10px;
}

.qp-mg__chip-title {
  overflow: hidden;
  text-overflow: ellipsis;
}

.qp-mg__more {
  padding: 0 5px;
  border: none;
  background: transparent;
  color: var(--qp-text-muted);
  font-size: 10px;
  text-align: left;
  cursor: pointer;
}

.qp-mg__more:hover {
  color: var(--qp-text);
}
</style>
