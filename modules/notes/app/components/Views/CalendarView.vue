<script setup lang="ts">
/**
 * The calendar view — a month grid keyed on one date property.
 *
 * Dates are handled as `YYYY-MM-DD` strings throughout and only turned into a
 * `Date` with explicit local parts. Parsing a bare date string with `new Date`
 * treats it as UTC midnight, which lands a note on the previous day for every
 * user west of Greenwich.
 */
import { useSchemaStore } from '#notes/stores/schema'
import { useNotesStore } from '#notes/stores/notes'
import type { NoteSummary, View } from '#notes/types'

const props = defineProps<{ rows: NoteSummary[]; view: View }>()

const schema = useSchemaStore()
const notes = useNotesStore()
const router = useRouter()

const dateProperty = computed(() =>
  props.view.config.dateBy ? schema.propertyById.get(props.view.config.dateBy) : undefined,
)

const cursor = ref(new Date())

function key(date: Date): string {
  const m = `${date.getMonth() + 1}`.padStart(2, '0')
  const d = `${date.getDate()}`.padStart(2, '0')
  return `${date.getFullYear()}-${m}-${d}`
}

const monthLabel = computed(() =>
  cursor.value.toLocaleDateString(undefined, { month: 'long', year: 'numeric' }),
)

/** Six weeks from the Monday on or before the 1st — a fixed grid, so the
 * layout does not jump by a row between months. */
const days = computed(() => {
  const first = new Date(cursor.value.getFullYear(), cursor.value.getMonth(), 1)
  const offset = (first.getDay() + 6) % 7
  const start = new Date(first)
  start.setDate(first.getDate() - offset)

  return Array.from({ length: 42 }, (_, i) => {
    const date = new Date(start)
    date.setDate(start.getDate() + i)
    return {
      date,
      key: key(date),
      day: date.getDate(),
      inMonth: date.getMonth() === cursor.value.getMonth(),
      isToday: key(date) === key(new Date()),
    }
  })
})

const byDay = computed(() => {
  const map = new Map<string, NoteSummary[]>()
  const property = dateProperty.value
  if (!property) return map
  for (const note of props.rows) {
    const raw = note.properties[property.id]
    if (typeof raw !== 'string' || !raw) continue
    // Values can carry a time; the grid only cares about the day.
    const day = raw.slice(0, 10)
    const bucket = map.get(day)
    if (bucket) bucket.push(note)
    else map.set(day, [note])
  }
  return map
})

const undated = computed(() => {
  const property = dateProperty.value
  if (!property) return props.rows
  return props.rows.filter((n) => {
    const raw = n.properties[property.id]
    return typeof raw !== 'string' || !raw
  })
})

function step(months: number) {
  cursor.value = new Date(cursor.value.getFullYear(), cursor.value.getMonth() + months, 1)
}

async function addOn(day: string) {
  const property = dateProperty.value
  const note = await notes.create({ properties: property ? { [property.id]: day } : undefined })
  router.push(`/notes/n/${note.id}`)
}

const WEEKDAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']
</script>

<template>
  <div class="qn-cal">
    <header class="qn-cal__bar">
      <div class="qn-cal__nav">
        <button @click="step(-1)" aria-label="Previous month">‹</button>
        <span class="qn-cal__month">{{ monthLabel }}</span>
        <button @click="step(1)" aria-label="Next month">›</button>
        <button class="qn-cal__today" @click="cursor = new Date()">Today</button>
      </div>
      <span v-if="!dateProperty" class="qn-cal__hint">
        No date property selected — pick one in the toolbar.
      </span>
    </header>

    <div class="qn-cal__weekdays">
      <span v-for="day in WEEKDAYS" :key="day">{{ day }}</span>
    </div>

    <div class="qn-cal__grid">
      <div
        v-for="cell in days"
        :key="cell.key"
        class="qn-cal__cell"
        :class="{ 'is-outside': !cell.inMonth, 'is-today': cell.isToday }"
      >
        <div class="qn-cal__cell-head">
          <span class="qn-cal__day">{{ cell.day }}</span>
          <button class="qn-cal__add" aria-label="New note on this day" @click="addOn(cell.key)">+</button>
        </div>
        <div class="qn-cal__events">
          <button
            v-for="note in byDay.get(cell.key) ?? []"
            :key="note.id"
            class="qn-cal__event"
            @click="router.push(`/notes/n/${note.id}`)"
          >
            <NotesNoteIcon v-if="note.icon" :icon="note.icon" :size="12" />{{ note.title || 'Untitled' }}
          </button>
        </div>
      </div>
    </div>

    <footer v-if="undated.length" class="qn-cal__undated">
      <span class="qn-cal__undated-label">No date ({{ undated.length }})</span>
      <button
        v-for="note in undated.slice(0, 12)"
        :key="note.id"
        class="qn-cal__event"
        @click="router.push(`/notes/n/${note.id}`)"
      >
        {{ note.title || 'Untitled' }}
      </button>
    </footer>
  </div>
</template>

<style scoped>
.qn-cal {
  height: 100%;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.qn-cal__bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.qn-cal__nav {
  display: flex;
  align-items: center;
  gap: 4px;
}

.qn-cal__nav button {
  min-width: 26px;
  height: 26px;
  padding: 0 8px;
  border: 1px solid var(--qn-border);
  border-radius: 6px;
  background: var(--qn-bg-card);
  color: var(--qn-text-secondary);
  font-size: 13px;
  cursor: pointer;
}

.qn-cal__nav button:hover {
  color: var(--qn-text);
  border-color: var(--qn-accent);
}

.qn-cal__month {
  min-width: 150px;
  padding: 0 6px;
  color: var(--qn-text);
  font-size: 13px;
  font-weight: 600;
}

.qn-cal__today {
  font-size: 12px !important;
}

.qn-cal__hint {
  color: var(--qn-text-muted);
  font-size: 12px;
}

.qn-cal__weekdays {
  display: grid;
  grid-template-columns: repeat(7, 1fr);
  gap: 1px;
  color: var(--qn-text-muted);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.qn-cal__weekdays span {
  padding: 0 6px;
}

.qn-cal__grid {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: repeat(7, 1fr);
  grid-auto-rows: minmax(0, 1fr);
  gap: 1px;
  border: 1px solid var(--qn-border);
  border-radius: 8px;
  background: var(--qn-border);
  overflow: hidden;
}

.qn-cal__cell {
  display: flex;
  flex-direction: column;
  min-height: 0;
  padding: 4px 5px;
  background: var(--qn-bg);
}

.qn-cal__cell.is-outside {
  background: color-mix(in srgb, var(--qn-bg) 80%, var(--qn-bg-sidebar));
}

.qn-cal__cell.is-outside .qn-cal__day {
  color: var(--qn-text-muted);
}

.qn-cal__cell.is-today .qn-cal__day {
  color: var(--qn-bg);
  background: var(--qn-accent);
  border-radius: 999px;
}

.qn-cal__cell-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.qn-cal__day {
  display: grid;
  place-items: center;
  min-width: 18px;
  height: 18px;
  color: var(--qn-text-secondary);
  font-size: 11px;
  font-weight: 600;
}

.qn-cal__add {
  width: 18px;
  height: 18px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--qn-text-muted);
  font-size: 13px;
  line-height: 1;
  cursor: pointer;
  opacity: 0;
}

.qn-cal__cell:hover .qn-cal__add {
  opacity: 1;
}

.qn-cal__add:hover {
  background: var(--qn-bg-hover);
  color: var(--qn-text);
}

.qn-cal__events {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
  margin-top: 2px;
  overflow-y: auto;
}

.qn-cal__event {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 2px 5px;
  border: none;
  border-radius: 4px;
  background: var(--qn-bg-card);
  color: var(--qn-text);
  font-size: 11px;
  line-height: 1.35;
  text-align: left;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  cursor: pointer;
}

.qn-cal__event:hover {
  background: var(--qn-bg-hover);
}

.qn-cal__undated {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
  padding-top: 4px;
  border-top: 1px solid var(--qn-border-subtle);
}

.qn-cal__undated-label {
  color: var(--qn-text-muted);
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}
</style>
