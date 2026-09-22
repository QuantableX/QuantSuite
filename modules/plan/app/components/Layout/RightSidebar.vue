<script setup lang="ts">
/**
 * The right panel: the selected event's details, or — with nothing selected —
 * the next few things coming up.
 */
import { useAppStore } from '#plan/stores/app'
import { useCalendarsStore } from '#plan/stores/calendars'
import { useEventsStore } from '#plan/stores/events'
import type { Occurrence } from '#plan/types'
import { formatTime, occurrenceEnd, occurrenceStart } from '#plan/utils/datetime'

const emit = defineEmits<{ (e: 'edit', occurrence: Occurrence): void }>()

const app = useAppStore()
const calendars = useCalendarsStore()
const events = useEventsStore()

const selected = computed(() => events.selected)

const upcoming = computed(() => {
  const now = Date.now()
  return events.occurrences
    .filter((o) => !o.isAllDay && occurrenceEnd(o).getTime() >= now)
    .sort((a, b) => occurrenceStart(a).getTime() - occurrenceStart(b).getTime())
    .slice(0, 8)
})

function when(occ: Occurrence): string {
  const start = occurrenceStart(occ)
  const day = start.toLocaleDateString(undefined, { weekday: 'short', day: 'numeric', month: 'short' })
  if (occ.isAllDay) return `${day} · all day`
  const from = formatTime(start, app.settings.timeFormat)
  const to = formatTime(occurrenceEnd(occ), app.settings.timeFormat)
  return `${day} · ${from} – ${to}`
}
</script>

<template>
  <div class="qp-rs">
    <header class="qp-rs__head">{{ selected ? 'Event' : 'Up next' }}</header>

    <div class="qp-rs__body">
      <template v-if="selected">
        <div class="qp-rs__title">
          <span class="qp-dot" :data-color="selected.color" />
          <h2>{{ selected.title || 'Untitled' }}</h2>
        </div>
        <p class="qp-rs__when">{{ when(selected) }}</p>

        <dl class="qp-rs__meta">
          <template v-if="selected.location">
            <dt>Location</dt>
            <dd>{{ selected.location }}</dd>
          </template>
          <dt>Calendar</dt>
          <dd>{{ calendars.byId.get(selected.calendarId)?.name ?? '—' }}</dd>
          <template v-if="selected.isRecurring">
            <dt>Repeats</dt>
            <dd class="mono">{{ selected.rrule }}</dd>
          </template>
          <template v-if="selected.isOverride">
            <dt>Note</dt>
            <dd>This occurrence differs from the series.</dd>
          </template>
        </dl>

        <p v-if="selected.description" class="qp-rs__desc">{{ selected.description }}</p>

        <div class="qp-rs__actions">
          <button class="qp-btn" @click="emit('edit', selected)">Edit</button>
          <button class="qp-btn" @click="events.selected = null">Close</button>
        </div>
      </template>

      <template v-else>
        <button
          v-for="occ in upcoming"
          :key="`${occ.eventId}-${occ.occurrenceStart}`"
          class="qp-rs__row"
          @click="events.selected = occ"
        >
          <span class="qp-dot" :data-color="occ.color" />
          <span class="qp-rs__row-text">
            <span class="qp-rs__row-title">{{ occ.title || 'Untitled' }}</span>
            <span class="qp-rs__row-when">{{ when(occ) }}</span>
          </span>
        </button>
        <p v-if="!upcoming.length" class="qp-rs__empty">Nothing coming up in this range.</p>
      </template>
    </div>
  </div>
</template>

<style scoped>
.qp-rs {
  height: 100%;
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.qp-rs__head {
  flex-shrink: 0;
  padding: 12px 14px;
  border-bottom: 1px solid var(--qp-border);
  color: var(--qp-text-secondary);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.qp-rs__body {
  flex: 1;
  min-height: 0;
  padding: 12px;
  overflow-y: auto;
}

.qp-rs__title {
  display: flex;
  align-items: center;
  gap: 8px;
}

.qp-rs__title h2 {
  margin: 0;
  color: var(--qp-text);
  font-size: 15px;
  font-weight: 600;
  line-height: 1.35;
}

.qp-rs__when {
  margin: 6px 0 12px;
  color: var(--qp-text-secondary);
  font-size: 12px;
}

.qp-rs__meta {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 4px 10px;
  margin: 0 0 12px;
  font-size: 12px;
}

.qp-rs__meta dt {
  color: var(--qp-text-muted);
}

.qp-rs__meta dd {
  margin: 0;
  color: var(--qp-text-secondary);
  overflow-wrap: anywhere;
}

.qp-rs__desc {
  margin: 0 0 12px;
  color: var(--qp-text-secondary);
  font-size: 12px;
  line-height: 1.55;
  white-space: pre-wrap;
}

.qp-rs__actions {
  display: flex;
  gap: 8px;
}

.qp-rs__row {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  width: 100%;
  padding: 7px 8px;
  border: none;
  border-radius: var(--qp-radius);
  background: transparent;
  text-align: left;
  cursor: pointer;
}

.qp-rs__row:hover {
  background: var(--qp-bg-hover);
}

.qp-rs__row .qp-dot {
  margin-top: 4px;
}

.qp-rs__row-text {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.qp-rs__row-title {
  color: var(--qp-text);
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.qp-rs__row-when {
  color: var(--qp-text-muted);
  font-size: 11px;
}

.qp-rs__empty {
  padding: 12px 4px;
  color: var(--qp-text-muted);
  font-size: 12px;
  line-height: 1.5;
}
</style>
