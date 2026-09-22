<script setup lang="ts">
/**
 * The agenda — a chronological list of the days that actually have something
 * on them. Empty days are skipped rather than rendered as blank rows; a list
 * of nothing is what a grid is for.
 */
import { useAppStore } from '#plan/stores/app'
import { useEventsStore } from '#plan/stores/events'
import type { Occurrence } from '#plan/types'
import { daySegment, formatTime, isToday, toKey } from '#plan/utils/datetime'

const emit = defineEmits<{
  (e: 'select', occurrence: Occurrence): void
  (e: 'edit', occurrence: Occurrence): void
}>()

const app = useAppStore()
const events = useEventsStore()

const groups = computed(() =>
  app.days
    .map((day) => ({ day, items: events.byDay.get(toKey(day)) ?? [] }))
    .filter((group) => group.items.length > 0),
)

/**
 * The range as seen from `day`. An event that crosses midnight is listed on
 * both days, and each row shows only the part that belongs to it: the ellipsis
 * is the half that lives on the other day.
 */
function timeRange(occ: Occurrence, day: Date): string {
  if (occ.isAllDay) return 'All day'
  const seg = daySegment(occ, day)
  const from = seg.continuesBefore ? '…' : formatTime(seg.start, app.settings.timeFormat)
  const to = seg.continuesAfter ? '…' : formatTime(seg.end, app.settings.timeFormat)
  return `${from} – ${to}`
}
</script>

<template>
  <div class="qp-ag">
    <section v-for="group in groups" :key="toKey(group.day)" class="qp-ag__day">
      <header class="qp-ag__head" :class="{ 'is-today': isToday(group.day) }">
        <span class="qp-ag__dom">{{ group.day.getDate() }}</span>
        <span class="qp-ag__dow">
          {{ group.day.toLocaleDateString(undefined, { weekday: 'long', month: 'short' }) }}
        </span>
      </header>

      <button
        v-for="occ in group.items"
        :key="`${occ.eventId}-${occ.occurrenceStart}`"
        class="qp-ag__row"
        @click="emit('select', occ); emit('edit', occ)"
      >
        <span class="qp-dot" :data-color="occ.color" />
        <span class="qp-ag__time mono">{{ timeRange(occ, group.day) }}</span>
        <span class="qp-ag__title">{{ occ.title || 'Untitled' }}</span>
        <span v-if="occ.location" class="qp-ag__where">{{ occ.location }}</span>
      </button>
    </section>

    <p v-if="!groups.length" class="qp-ag__empty">Nothing scheduled in this range.</p>
  </div>
</template>

<style scoped>
.qp-ag {
  height: 100%;
  min-height: 0;
  padding: 8px 4px 24px;
  overflow-y: auto;
}

.qp-ag__day {
  display: grid;
  grid-template-columns: 96px 1fr;
  gap: 0 12px;
  padding: 8px 0;
  border-bottom: 1px solid var(--qp-border-subtle);
}

.qp-ag__head {
  display: flex;
  align-items: baseline;
  gap: 8px;
  padding-left: 10px;
}

.qp-ag__dom {
  color: var(--qp-text);
  font-size: 17px;
  font-weight: 700;
}

.qp-ag__dow {
  color: var(--qp-text-muted);
  font-size: 11px;
}

.qp-ag__head.is-today .qp-ag__dom,
.qp-ag__head.is-today .qp-ag__dow {
  color: var(--qp-now);
}

.qp-ag__row {
  grid-column: 2;
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 5px 8px;
  border: none;
  border-radius: var(--qp-radius);
  background: transparent;
  color: var(--qp-text);
  font-size: 13px;
  text-align: left;
  cursor: pointer;
}

.qp-ag__row:hover {
  background: var(--qp-bg-hover);
}

.qp-ag__time {
  flex-shrink: 0;
  width: 118px;
  color: var(--qp-text-secondary);
  font-size: 11px;
}

.qp-ag__title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.qp-ag__where {
  flex-shrink: 0;
  color: var(--qp-text-muted);
  font-size: 11px;
}

.qp-ag__empty {
  padding: 40px;
  color: var(--qp-text-muted);
  text-align: center;
}
</style>
