<script setup lang="ts">
/**
 * Create or edit an event.
 *
 * Editing one instance of a series ends in the three-way question every
 * calendar has to ask — this event / this and following / all events — because
 * there is no answer that is right by default.
 */
import { useCalendarsStore } from '#plan/stores/calendars'
import { useEventsStore } from '#plan/stores/events'
import { useAppStore } from '#plan/stores/app'
import type { CalendarColor, Occurrence, SeriesScope } from '#plan/types'
import { addDays, atMinutes, formatTime, fromKey, localZone, startOfDay, toKey } from '#plan/utils/datetime'

const props = defineProps<{
  /** Editing an existing instance, or null when creating. */
  occurrence: Occurrence | null
  /** Prefilled start when creating. */
  at: Date | null
}>()

const emit = defineEmits<{ (e: 'close'): void }>()

const calendars = useCalendarsStore()
const events = useEventsStore()
const app = useAppStore()

const title = ref('')
const calendarId = ref('')
const location = ref('')
const description = ref('')
const isAllDay = ref(false)
const dateFrom = ref('')
const timeFrom = ref('09:00')
const dateTo = ref('')
const timeTo = ref('10:00')
const rrule = ref<string | null>(null)
const color = ref<CalendarColor | null>(null)
const error = ref('')

const scopeAsk = ref<null | 'save' | 'delete'>(null)

const isEditing = computed(() => !!props.occurrence)
const isSeries = computed(() => !!props.occurrence?.isRecurring)

function timeString(date: Date): string {
  return `${`${date.getHours()}`.padStart(2, '0')}:${`${date.getMinutes()}`.padStart(2, '0')}`
}

function hydrate() {
  error.value = ''
  scopeAsk.value = null
  const occ = props.occurrence

  if (!occ) {
    const start = props.at ?? atMinutes(startOfDay(new Date()), 9 * 60)
    const end = new Date(start.getTime() + 60 * 60 * 1000)
    title.value = ''
    calendarId.value = calendars.defaultCalendar?.id ?? ''
    location.value = ''
    description.value = ''
    isAllDay.value = false
    dateFrom.value = toKey(start)
    timeFrom.value = timeString(start)
    dateTo.value = toKey(end)
    timeTo.value = timeString(end)
    rrule.value = null
    color.value = null
    return
  }

  title.value = occ.title
  calendarId.value = occ.calendarId
  location.value = occ.location ?? ''
  description.value = occ.description ?? ''
  isAllDay.value = occ.isAllDay
  rrule.value = occ.rrule
  color.value = null

  if (occ.isAllDay) {
    dateFrom.value = (occ.startsOn ?? '').slice(0, 10)
    dateTo.value = (occ.endsOn ?? occ.startsOn ?? '').slice(0, 10)
  } else {
    const start = new Date(occ.startsAt!)
    const end = occ.endsAt ? new Date(occ.endsAt) : new Date(start.getTime() + 3600_000)
    dateFrom.value = toKey(start)
    timeFrom.value = timeString(start)
    dateTo.value = toKey(end)
    timeTo.value = timeString(end)
  }
}

watch(() => [props.occurrence, props.at], hydrate, { immediate: true, deep: false })

/** The start as a local Date, however the form is currently filled in. */
const startDate = computed(() => (dateFrom.value ? fromKey(dateFrom.value) : new Date()))

/**
 * The two ends of a timed event, with the overnight case folded in.
 *
 * An end time *earlier than the start on the same date* is not an error, it is
 * a night shift: 23:00–06:00 means six in the morning, tomorrow. That is what
 * the field means everywhere outside a calendar form, so it is what it means
 * here. A genuinely earlier end DATE stays a mistake — `validate` catches it.
 */
function resolveRange(): { start: Date; end: Date; wraps: boolean } {
  const [fh, fm] = timeFrom.value.split(':').map(Number)
  const [th, tm] = timeTo.value.split(':').map(Number)
  const start = fromKey(dateFrom.value)
  start.setHours(fh ?? 0, fm ?? 0, 0, 0)
  const endKey = dateTo.value || dateFrom.value
  const end = fromKey(endKey)
  end.setHours(th ?? 0, tm ?? 0, 0, 0)
  const wraps = endKey === dateFrom.value && end < start
  // Deliberately derived, never written back into `dateTo`: switching the end
  // time back to 10:00 has to undo the roll-over, and it cannot if the field
  // has meanwhile been rewritten behind the user's back.
  return { start, end: wraps ? addDays(end, 1) : end, wraps }
}

function buildPayload() {
  if (isAllDay.value) {
    return {
      isAllDay: true,
      startsOn: dateFrom.value,
      // endsOn is INCLUSIVE — an empty "to" means a single day.
      endsOn: dateTo.value || dateFrom.value,
      startsAt: null,
      endsAt: null,
    }
  }
  const { start, end } = resolveRange()
  return {
    isAllDay: false,
    startsAt: start.toISOString(),
    endsAt: end.toISOString(),
    startsOn: null,
    endsOn: null,
  }
}

/** Shown next to the end time, so the roll-over is visible before saving. */
const overnight = computed(() => {
  if (isAllDay.value || !dateFrom.value) return null
  const { end, wraps } = resolveRange()
  return wraps ? `+1 day · ${formatTime(end, app.settings.timeFormat)}` : null
})

function validate(): string {
  if (!title.value.trim()) return 'A title is required.'
  if (!calendarId.value) return 'Pick a calendar.'
  if (!dateFrom.value) return 'Pick a start date.'
  const payload = buildPayload()
  if (!payload.isAllDay && payload.endsAt! <= payload.startsAt!) {
    return 'The end has to come after the start.'
  }
  if (payload.isAllDay && payload.endsOn! < payload.startsOn!) {
    return 'The last day has to come after the first.'
  }
  return ''
}

async function save(scope: SeriesScope = 'all') {
  error.value = validate()
  if (error.value) return

  const payload = {
    ...buildPayload(),
    title: title.value.trim(),
    calendarId: calendarId.value,
    location: location.value.trim() || null,
    description: description.value.trim() || null,
    rrule: rrule.value,
    color: color.value,
  }

  try {
    if (!props.occurrence) {
      await events.create({ ...payload, tz: localZone() })
    } else if (isSeries.value && scope !== 'all') {
      await events.applyToSeries(props.occurrence, scope, payload)
    } else {
      await events.update(props.occurrence.eventId, payload)
    }
    emit('close')
  } catch (e) {
    error.value = String(e)
  }
}

function requestSave() {
  error.value = validate()
  if (error.value) return
  // Only a series needs the question; a one-off just saves.
  if (isSeries.value) scopeAsk.value = 'save'
  else void save('all')
}

async function remove(scope: SeriesScope = 'all') {
  if (!props.occurrence) return
  try {
    await events.deleteOccurrence(props.occurrence, scope)
    emit('close')
  } catch (e) {
    error.value = String(e)
  }
}

function requestDelete() {
  if (isSeries.value) scopeAsk.value = 'delete'
  else void remove('all')
}

function applyScope(scope: SeriesScope) {
  const what = scopeAsk.value
  scopeAsk.value = null
  if (what === 'save') void save(scope)
  else void remove(scope)
}

const COLORS: CalendarColor[] = ['slate', 'blue', 'green', 'amber', 'red', 'purple', 'pink', 'teal']
</script>

<template>
  <!-- Absolute, never fixed: `.qss-module-root` carries translateZ(0) and is
       the containing block (PLAN-V3 §3). -->
  <div class="qp-ee__scrim" @click.self="emit('close')">
    <div class="qp-ee" role="dialog" :aria-label="isEditing ? 'Edit event' : 'New event'">
      <header class="qp-ee__head">
        <input
          v-model="title"
          class="qp-ee__title"
          placeholder="Add a title"
          autofocus
          @keydown.enter.prevent="requestSave"
        />
        <button class="qp-ee__x" aria-label="Close" @click="emit('close')">×</button>
      </header>

      <div class="qp-ee__body">
        <label class="qp-ee__allday">
          <input v-model="isAllDay" type="checkbox" />
          <span>All day</span>
        </label>

        <div class="qp-ee__when">
          <input v-model="dateFrom" class="qp-input" type="date" />
          <input v-if="!isAllDay" v-model="timeFrom" class="qp-input qp-ee__time" type="time" />
          <span class="qp-ee__dash">–</span>
          <input v-if="!isAllDay" v-model="timeTo" class="qp-input qp-ee__time" type="time" />
          <span v-if="overnight" class="qp-ee__overnight" :title="`Ends ${overnight}`">+1</span>
          <input v-model="dateTo" class="qp-input" type="date" />
        </div>

        <PlanEventRecurrenceField v-model="rrule" :start="startDate" />

        <div class="qp-ee__row">
          <span class="qp-ee__label">Calendar</span>
          <select v-model="calendarId" class="qp-select">
            <option v-for="cal in calendars.list" :key="cal.id" :value="cal.id">{{ cal.name }}</option>
          </select>
        </div>

        <div class="qp-ee__row">
          <span class="qp-ee__label">Colour</span>
          <div class="qp-ee__colors">
            <button
              class="qp-ee__color"
              :class="{ 'is-on': color === null }"
              title="Use the calendar's colour"
              @click="color = null"
            >
              auto
            </button>
            <button
              v-for="c in COLORS"
              :key="c"
              class="qp-ee__color qp-dot"
              :data-color="c"
              :class="{ 'is-on': color === c }"
              :aria-label="c"
              @click="color = c"
            />
          </div>
        </div>

        <input v-model="location" class="qp-input" placeholder="Location" />
        <textarea v-model="description" class="qp-input qp-ee__notes" rows="3" placeholder="Description" />

        <p v-if="error" class="qp-ee__error">{{ error }}</p>
      </div>

      <!-- The three-way question. Shown instead of the footer so there is no
           way to save past it. -->
      <footer v-if="scopeAsk" class="qp-ee__scope">
        <span>{{ scopeAsk === 'delete' ? 'Delete' : 'Change' }} which events?</span>
        <button class="qp-btn" @click="applyScope('this')">This event</button>
        <button class="qp-btn" @click="applyScope('following')">This and following</button>
        <button class="qp-btn" @click="applyScope('all')">All events</button>
        <button class="qp-btn qp-ee__cancel" @click="scopeAsk = null">Cancel</button>
      </footer>

      <footer v-else class="qp-ee__foot">
        <button v-if="isEditing" class="qp-btn qp-ee__danger" @click="requestDelete">Delete</button>
        <span class="qp-ee__spacer" />
        <button class="qp-btn" @click="emit('close')">Cancel</button>
        <button class="qp-btn qp-ee__primary" @click="requestSave">Save</button>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.qp-ee__scrim {
  position: absolute;
  inset: 0;
  z-index: 60;
  display: grid;
  place-items: center;
  background: rgba(0, 0, 0, 0.45);
}

.qp-ee {
  width: min(460px, calc(100% - 32px));
  max-height: calc(100% - 48px);
  display: flex;
  flex-direction: column;
  border: 1px solid var(--qp-border);
  border-radius: var(--qp-radius-lg);
  background: var(--qp-bg-raised);
  box-shadow: 0 24px 64px rgba(0, 0, 0, 0.5);
}

.qp-ee__head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 14px;
  border-bottom: 1px solid var(--qp-border-subtle);
}

.qp-ee__title {
  flex: 1;
  min-width: 0;
  border: none;
  background: transparent;
  color: var(--qp-text);
  font-size: 17px;
  font-weight: 600;
  outline: none;
}

.qp-ee__x {
  width: 24px;
  height: 24px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--qp-text-muted);
  font-size: 17px;
  line-height: 1;
  cursor: pointer;
}

.qp-ee__x:hover {
  background: var(--qp-bg-hover);
  color: var(--qp-text);
}

.qp-ee__body {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 14px;
  overflow-y: auto;
  /* A computed `visible` on one axis becomes `auto` when the other axis
     scrolls, so this has to be said out loud — otherwise the dialog scrolls
     sideways the moment anything inside is a pixel too wide. */
  overflow-x: hidden;
}

.qp-ee__allday {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--qp-text-secondary);
  font-size: 12px;
  cursor: pointer;
}

/* Native date and time inputs carry a chunky intrinsic width, and a flex item
   defaults to `min-width: auto` — so without these they refuse to shrink and
   push the dialog wider than itself. They wrap onto a second line instead. */
.qp-ee__when {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
}

.qp-ee__when > input[type='date'] {
  flex: 1 1 130px;
  min-width: 0;
}

.qp-ee__time {
  flex: 0 1 96px;
  min-width: 0;
}

.qp-ee__dash {
  color: var(--qp-text-muted);
}

/* The overnight marker. Small, but it is the only thing on screen that says
   the 06:00 belongs to tomorrow. */
.qp-ee__overnight {
  flex: 0 0 auto;
  padding: 1px 5px;
  border-radius: 999px;
  background: var(--qp-bg-hover);
  color: var(--qp-text-secondary);
  font-size: 10px;
  font-weight: 600;
}

.qp-ee__row {
  display: flex;
  align-items: center;
  gap: 10px;
}

.qp-ee__row > .qp-select {
  min-width: 0;
}

.qp-ee__label {
  flex-shrink: 0;
  width: 64px;
  color: var(--qp-text-muted);
  font-size: 11px;
}

.qp-ee__colors {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 5px;
}

.qp-ee__color {
  border: 1px solid transparent;
  border-radius: 999px;
  background: transparent;
  color: var(--qp-text-muted);
  font-size: 10px;
  padding: 2px 6px;
  cursor: pointer;
}

.qp-ee__color.qp-dot {
  width: 16px;
  height: 16px;
  padding: 0;
  border-radius: 5px;
}

.qp-ee__color.is-on {
  outline: 2px solid var(--qp-accent);
  outline-offset: 1px;
}

.qp-ee__notes {
  resize: vertical;
  font-family: inherit;
}

.qp-ee__error {
  margin: 0;
  color: var(--qss-error, #ff4757);
  font-size: 12px;
}

.qp-ee__foot,
.qp-ee__scope {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 14px;
  border-top: 1px solid var(--qp-border-subtle);
}

.qp-ee__scope {
  flex-wrap: wrap;
  color: var(--qp-text-secondary);
  font-size: 12px;
}

.qp-ee__spacer {
  flex: 1;
}

.qp-ee__primary {
  border-color: var(--qp-accent);
  color: var(--qp-text);
}

.qp-ee__danger:hover {
  border-color: var(--qss-error, #ff4757);
  color: var(--qss-error, #ff4757);
}

.qp-ee__cancel {
  margin-left: auto;
}
</style>
