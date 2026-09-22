<script setup lang="ts">
/**
 * The day and week grid: an hour gutter, one column per day, event blocks
 * absolutely positioned inside their column.
 *
 * Three interactions live here, all on pointer events rather than HTML5 drag —
 * HTML5 drag gives no usable coordinates while the pointer moves, which is
 * exactly what a calendar needs:
 *
 *   * drag on empty space → create
 *   * drag a block → move
 *   * drag a block's bottom edge → resize
 *
 * All three are optimistic: the ghost follows the pointer and the write lands
 * on release. A block that snaps back to its old place for a moment after the
 * drop reads as broken.
 */
import { inActiveKeepAliveTree } from '@quantsuite/core'
import { useAppStore } from '#plan/stores/app'
import { useEventsStore } from '#plan/stores/events'
import { useCalendarsStore } from '#plan/stores/calendars'
import { layoutDay } from '#plan/composables/useOverlapLayout'
import type { Occurrence, PlacedOccurrence } from '#plan/types'
import {
  atMinutes,
  formatHour,
  formatTime,
  isToday,
  localZone,
  minutesSinceMidnight,
  occurrenceEnd,
  occurrenceStart,
  snapMinutes,
  toKey,
} from '#plan/utils/datetime'

const emit = defineEmits<{
  (e: 'create-at', at: Date): void
  (e: 'select', occurrence: Occurrence): void
  (e: 'edit', occurrence: Occurrence): void
  (e: 'series-change', payload: { occurrence: Occurrence; patch: Record<string, unknown> }): void
}>()

const app = useAppStore()
const events = useEventsStore()
const calendars = useCalendarsStore()

const scroller = ref<HTMLElement | null>(null)
const grid = ref<HTMLElement | null>(null)
const now = ref(new Date())

/** A day is 00:00 to 24:00. All of it, always — the grid scrolls to now. */
const DAY_MINUTES = 24 * 60
const HOURS = Array.from({ length: 24 }, (_, i) => i)
const hours = computed(() => HOURS)

/**
 * The whole day fits the pane — the grid never scrolls.
 *
 * All 24 hours are laid out inside whatever height the pane has, so the
 * calendar adapts to the monitor instead of asking the user to scroll for the
 * evening. The cost is real and deliberate: on a short window an hour is only
 * a couple of dozen pixels and a 15-minute event is a sliver. That is the
 * trade the "no scrolling" rule buys.
 */
/**
 * CSS owns the layout; this only reads it back.
 *
 * The grid is `height: 100%` with 24 `1fr` rows, so the browser fits the day
 * to the pane on its own — no measurement is needed for anything to *look*
 * right. The measured height exists solely to place the absolutely positioned
 * event blocks and the now-line, which cannot be expressed as grid tracks.
 * Driving the height from JS instead made the whole grid depend on a
 * measurement landing, and when it did not the day collapsed.
 */
const paneHeight = ref(0)

/**
 * One hour in pixels, derived from that height.
 *
 * The hour ROWS are `1fr` tracks of a grid, not elements carrying this as
 * their own height — a flex parent overrides a child's height, a grid track
 * cannot be argued with. This value only positions the absolute event blocks.
 */
const HOUR_PX = computed(() => paneHeight.value / 24)

function toPx(minutes: number): number {
  return (minutes / 60) * HOUR_PX.value
}

function measure() {
  const el = grid.value ?? scroller.value
  if (el && el.clientHeight > 0) paneHeight.value = el.clientHeight
}

let resize: ResizeObserver | null = null

/** Placed blocks per day column, keyed by local day. */
const placedByDay = computed(() => {
  const map = new Map<string, ReturnType<typeof layoutDay>>()
  for (const day of app.days) {
    const key = toKey(day)
    map.set(key, layoutDay(events.byDay.get(key) ?? [], day))
  }
  return map
})

const nowOffset = computed(() => toPx(minutesSinceMidnight(now.value)))
// Every minute of the day is on the grid now, so the line is always placed.
const nowVisible = computed(() => true)

// ── Dragging ───────────────────────────────────────────────────────────────

type DragMode = 'create' | 'move' | 'resize'

interface Drag {
  mode: DragMode
  dayKey: string
  startMinutes: number
  endMinutes: number
  occurrence: Occurrence | null
  /**
   * The event's REAL span, plus the instant the grabbed block started at.
   *
   * A block on screen may be only the visible half of an event that crosses
   * midnight, so a move is applied as a shift of the real start and end rather
   * than rebuilt from the block's clamped minutes — otherwise dragging the
   * 00:00-06:00 tail of a night shift would throw the evening away.
   */
  origin: { start: Date; end: Date; segmentStart: Date } | null
  /** Where in the block the pointer grabbed it, for `move`. */
  grabOffset: number
  /**
   * Whether the snapped position ever actually changed.
   *
   * A press that never moves is a *click*, and a click on an event opens it —
   * otherwise there is no way to edit or delete one without a right panel.
   * Comparing snapped minutes rather than raw pixels means a few pixels of
   * hand jitter still counts as a click.
   */
  moved: boolean
}

const drag = ref<Drag | null>(null)

/**
 * Where the pointer is, in minutes past midnight.
 *
 * Measured against the column the pointer is actually over, NOT the cached
 * `HOUR_PX`. Where the user clicked is the one thing on this grid that must
 * not depend on a measurement having landed: a `paneHeight` left over from a
 * different pane size scales every click, and the new event lands at an hour
 * nobody pointed at.
 */
function minutesFromEvent(event: PointerEvent, column: HTMLElement): number {
  const rect = column.getBoundingClientRect()
  if (rect.height <= 0) return 0
  const y = Math.min(Math.max(event.clientY - rect.top, 0), rect.height)
  return snapMinutes((y / rect.height) * DAY_MINUTES, app.settings.slotMinutes)
}

function onColumnPointerDown(event: PointerEvent, day: Date) {
  if (event.button !== 0) return
  const column = event.currentTarget as HTMLElement
  // The bottom edge of the column is 24:00, which as a *start* is tomorrow.
  // Hold it back one slot so a click on the last pixel of the evening still
  // makes an event in the evening.
  const at = Math.min(minutesFromEvent(event, column), DAY_MINUTES - app.settings.slotMinutes)
  column.setPointerCapture(event.pointerId)
  drag.value = {
    mode: 'create',
    dayKey: toKey(day),
    startMinutes: at,
    endMinutes: at + app.settings.slotMinutes,
    occurrence: null,
    origin: null,
    grabOffset: 0,
    moved: false,
  }
}

/** The real span of a block, and where the block as drawn begins. */
function originOf(placed: PlacedOccurrence, day: Date) {
  return {
    start: occurrenceStart(placed.occurrence),
    end: occurrenceEnd(placed.occurrence),
    segmentStart: atMinutes(day, placed.startMinutes),
  }
}

function onBlockPointerDown(event: PointerEvent, placed: PlacedOccurrence, day: Date) {
  if (event.button !== 0) return
  event.stopPropagation()
  const column = (event.currentTarget as HTMLElement).closest('.qp-tg__col') as HTMLElement | null
  if (!column) return
  const at = minutesFromEvent(event, column)
  column.setPointerCapture(event.pointerId)
  drag.value = {
    mode: 'move',
    dayKey: toKey(day),
    startMinutes: placed.startMinutes,
    endMinutes: placed.endMinutes,
    occurrence: placed.occurrence,
    origin: originOf(placed, day),
    grabOffset: at - placed.startMinutes,
    moved: false,
  }
}

function onResizePointerDown(event: PointerEvent, placed: PlacedOccurrence, day: Date) {
  if (event.button !== 0) return
  event.stopPropagation()
  const column = (event.currentTarget as HTMLElement).closest('.qp-tg__col') as HTMLElement | null
  if (!column) return
  column.setPointerCapture(event.pointerId)
  drag.value = {
    mode: 'resize',
    dayKey: toKey(day),
    startMinutes: placed.startMinutes,
    endMinutes: placed.endMinutes,
    occurrence: placed.occurrence,
    origin: originOf(placed, day),
    grabOffset: 0,
    moved: false,
  }
}

function onColumnPointerMove(event: PointerEvent, day: Date) {
  const current = drag.value
  if (!current) return
  const column = event.currentTarget as HTMLElement
  const at = minutesFromEvent(event, column)
  const slot = app.settings.slotMinutes
  const before = `${current.dayKey}:${current.startMinutes}:${current.endMinutes}`

  if (current.mode === 'create') {
    // Dragging upward is a selection too — normalise so start < end.
    const anchor = current.grabOffset || current.startMinutes
    current.grabOffset = anchor
    current.startMinutes = Math.min(anchor, at)
    current.endMinutes = Math.max(anchor + slot, at)
  } else if (current.mode === 'move') {
    const length = current.endMinutes - current.startMinutes
    const nextStart = Math.max(0, at - current.grabOffset)
    current.startMinutes = nextStart
    current.endMinutes = nextStart + length
    // Dragging across columns moves the day, which is half the point of a
    // week view.
    current.dayKey = toKey(day)
  } else {
    current.endMinutes = Math.max(current.startMinutes + slot, at)
  }

  if (`${current.dayKey}:${current.startMinutes}:${current.endMinutes}` !== before) {
    current.moved = true
  }
}

async function onPointerUp() {
  const current = drag.value
  drag.value = null
  if (!current) return

  const day = app.days.find((d) => toKey(d) === current.dayKey) ?? app.days[0]!

  if (current.mode === 'create') {
    const starts = atMinutes(day, current.startMinutes)
    // A click without movement is not a drag — open the editor prefilled
    // rather than creating a fifteen-minute event nobody asked for.
    if (current.endMinutes - current.startMinutes <= app.settings.slotMinutes) {
      emit('create-at', starts)
      return
    }
    const calendar = calendars.defaultCalendar
    if (!calendar) return
    await events.create({
      calendarId: calendar.id,
      title: 'New event',
      isAllDay: false,
      startsAt: starts.toISOString(),
      endsAt: atMinutes(day, current.endMinutes).toISOString(),
      tz: localZone(),
    })
    return
  }

  const occ = current.occurrence
  const origin = current.origin
  if (!occ || !origin) return

  // Pressed but never dragged: that is a click, and a click opens the event.
  if (!current.moved) {
    emit('select', occ)
    emit('edit', occ)
    return
  }

  let patch: { startsAt: string; endsAt: string }
  if (current.mode === 'move') {
    // Shift the whole event by however far the block travelled, which keeps
    // the length of one that runs past midnight instead of cutting it down to
    // the column it happened to be grabbed in.
    const delta = atMinutes(day, current.startMinutes).getTime() - origin.segmentStart.getTime()
    patch = {
      startsAt: new Date(origin.start.getTime() + delta).toISOString(),
      endsAt: new Date(origin.end.getTime() + delta).toISOString(),
    }
  } else {
    // The handle only exists on the block that carries the real end, so this
    // day's minutes ARE the end — but the start is the event's own, which the
    // block may have had clamped to midnight.
    patch = {
      startsAt: origin.start.toISOString(),
      endsAt: atMinutes(day, current.endMinutes).toISOString(),
    }
  }
  if (occ.isRecurring) {
    emit('series-change', { occurrence: occ, patch })
    return
  }
  await events.update(occ.eventId, patch)
}

/** The block being dragged, rendered as a ghost over its column. */
function ghostStyle(dayKey: string) {
  const current = drag.value
  if (!current || current.dayKey !== dayKey) return null
  return {
    top: `${toPx(current.startMinutes)}px`,
    height: `${Math.max(toPx(current.endMinutes - current.startMinutes), 12)}px`,
  }
}

function ghostLabel(): string {
  const current = drag.value
  if (!current) return ''
  const day = app.days.find((d) => toKey(d) === current.dayKey) ?? new Date()
  return `${formatTime(atMinutes(day, current.startMinutes), app.settings.timeFormat)} – ${formatTime(
    atMinutes(day, current.endMinutes),
    app.settings.timeFormat,
  )}`
}

function blockStyle(placed: PlacedOccurrence) {
  return {
    top: `${toPx(placed.startMinutes)}px`,
    height: `${Math.max(toPx(placed.endMinutes - placed.startMinutes) - 2, 14)}px`,
    left: `${placed.left * 100}%`,
    width: `calc(${placed.width * 100}% - 3px)`,
    zIndex: placed.zIndex,
  }
}

function timeLabel(placed: PlacedOccurrence): string {
  const occ = placed.occurrence
  if (!occ.startsAt) return ''
  // The tail of an overnight event says where it ENDS. Repeating "23:00" at
  // the top of the next morning reads as a bug.
  if (placed.continuesBefore) return `→ ${formatTime(occurrenceEnd(occ), app.settings.timeFormat)}`
  return formatTime(occurrenceStart(occ), app.settings.timeFormat)
}

// ── The now line ───────────────────────────────────────────────────────────
//
// V3 warm cache: the module stays mounted in a deactivated <KeepAlive> stage,
// so the tick has to stand down when the user leaves. Armed in BOTH hooks
// because an async-resolved page mounts after the stage's activation flush and
// would otherwise never get its first onActivated.
let tick: ReturnType<typeof setInterval> | null = null
function ensureTicking() {
  tick ??= setInterval(() => {
    now.value = new Date()
  }, 30_000)
}

onMounted(() => {
  measure()
  if (typeof ResizeObserver !== 'undefined') {
    resize = new ResizeObserver(measure)
    // Both: the pane is what changes with the window, the grid is what the
    // blocks are positioned inside.
    if (scroller.value) resize.observe(scroller.value)
    if (grid.value) resize.observe(grid.value)
  }
  // A measurement taken before the browser has laid the pane out reads 0.
  nextTick(measure)
  if (inActiveKeepAliveTree()) ensureTicking()
})
onActivated(() => {
  ensureTicking()
  // A pane resized while this module sat in the warm cache measured 0 the
  // whole time; re-fit on the way back in.
  nextTick(measure)
})
onDeactivated(() => {
  if (tick) {
    clearInterval(tick)
    tick = null
  }
})
onUnmounted(() => {
  resize?.disconnect()
  resize = null
  if (tick) clearInterval(tick)
})
</script>

<template>
  <div class="qp-tg">
    <!-- Day headers stay above the scroll, the way every calendar does it. -->
    <header class="qp-tg__head" :style="{ gridTemplateColumns: `var(--qp-gutter-w) repeat(${app.days.length}, 1fr)` }">
      <span class="qp-tg__corner" />
      <div v-for="day in app.days" :key="toKey(day)" class="qp-tg__day" :class="{ 'is-today': isToday(day) }">
        <span class="qp-tg__dow">{{ day.toLocaleDateString(undefined, { weekday: 'short' }) }}</span>
        <span class="qp-tg__dom">{{ day.getDate() }}</span>
      </div>
    </header>

    <PlanGridAllDayRow @select="emit('select', $event)" @edit="emit('edit', $event)" />

    <div ref="scroller" class="qp-tg__scroll">
      <div ref="grid" class="qp-tg__body" :style="{ gridTemplateColumns: `var(--qp-gutter-w) repeat(${app.days.length}, 1fr)` }">
        <div class="qp-tg__gutter" :style="{ gridTemplateRows: 'repeat(24, 1fr)' }">
          <span v-for="hour in hours" :key="hour" class="qp-tg__hour">
            {{ formatHour(hour, app.settings.timeFormat) }}
          </span>
        </div>

        <div
          v-for="day in app.days"
          :key="toKey(day)"
          class="qp-tg__col"
          :class="{ 'is-today': isToday(day) }"
          @pointerdown="onColumnPointerDown($event, day)"
          @pointermove="onColumnPointerMove($event, day)"
          @pointerup="onPointerUp"
          @pointercancel="drag = null"
        >
          <div class="qp-tg__lines" :style="{ gridTemplateRows: 'repeat(24, 1fr)' }">
            <span v-for="hour in hours" :key="hour" class="qp-tg__line" />
          </div>

          <article
            v-for="placed in placedByDay.get(toKey(day)) ?? []"
            :key="`${placed.occurrence.eventId}-${placed.occurrence.occurrenceStart}`"
            class="qp-tg__block qp-ev"
            :data-color="placed.occurrence.color"
            :class="{ 'is-from-before': placed.continuesBefore, 'is-into-next': placed.continuesAfter }"
            :style="blockStyle(placed)"
            @pointerdown="onBlockPointerDown($event, placed, day)"
          >
            <span class="qp-tg__block-time">{{ timeLabel(placed) }}</span>
            <span class="qp-tg__block-title">{{ placed.occurrence.title || 'Untitled' }}</span>
            <!-- Only the half that owns the real end may be resized. -->
            <span
              v-if="!placed.continuesAfter"
              class="qp-tg__handle"
              @pointerdown="onResizePointerDown($event, placed, day)"
            />
          </article>

          <div v-if="ghostStyle(toKey(day))" class="qp-tg__ghost" :style="ghostStyle(toKey(day))!">
            {{ ghostLabel() }}
          </div>

          <div v-if="isToday(day) && nowVisible" class="qp-tg__now" :style="{ top: `${nowOffset}px` }" />
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.qp-tg {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

.qp-tg__head {
  display: grid;
  border-bottom: 1px solid var(--qp-border);
}

.qp-tg__day {
  display: flex;
  align-items: baseline;
  justify-content: center;
  gap: 6px;
  padding: 8px 4px;
  border-left: 1px solid var(--qp-border-subtle);
}

.qp-tg__dow {
  color: var(--qp-text-muted);
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.qp-tg__dom {
  color: var(--qp-text);
  font-size: 15px;
  font-weight: 600;
}

.qp-tg__day.is-today .qp-tg__dom {
  color: var(--qp-now);
}

/* The day is scaled to fit, so this pane clips rather than scrolls. */
.qp-tg__scroll {
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

.qp-tg__body {
  display: grid;
  position: relative;
  /* The day fits the pane; the 24 rows are 1fr tracks inside it. */
  height: 100%;
}

.qp-tg__gutter {
  display: grid;
}

.qp-tg__hour {
  display: block;
  padding-right: 8px;
  color: var(--qp-text-muted);
  font-size: 10px;
  padding-top: 2px;
  text-align: right;
}

.qp-tg__col {
  position: relative;
  border-left: 1px solid var(--qp-border-subtle);
  touch-action: none;
}

.qp-tg__col.is-today {
  background: color-mix(in srgb, var(--qp-accent) 4%, transparent);
}

/* An absolute overlay so the hour rules do not participate in the column's
   own box — the event blocks are absolute in the same space. */
.qp-tg__lines {
  position: absolute;
  inset: 0;
  display: grid;
  pointer-events: none;
}

.qp-tg__line {
  border-bottom: 1px solid var(--qp-border-subtle);
}

.qp-tg__block {
  position: absolute;
  overflow: hidden;
  padding: 2px 5px;
  border-radius: 4px;
  font-size: 11px;
  line-height: 1.3;
  cursor: pointer;
  user-select: none;
}

/* Half of an event that crosses midnight: the cut edge loses its corners, so
   the two blocks read as one thing continuing rather than two events. */
.qp-tg__block.is-from-before {
  border-top-left-radius: 0;
  border-top-right-radius: 0;
}

.qp-tg__block.is-into-next {
  border-bottom-left-radius: 0;
  border-bottom-right-radius: 0;
}

.qp-tg__block-time {
  margin-right: 5px;
  color: var(--qp-text-secondary);
}

.qp-tg__block-title {
  font-weight: 500;
}

.qp-tg__handle {
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  height: 6px;
  cursor: ns-resize;
}

.qp-tg__ghost {
  position: absolute;
  left: 0;
  right: 3px;
  z-index: 20;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px dashed var(--qp-accent);
  border-radius: 4px;
  background: color-mix(in srgb, var(--qp-accent) 18%, transparent);
  color: var(--qp-text);
  font-size: 10px;
  pointer-events: none;
}

.qp-tg__now {
  position: absolute;
  left: 0;
  right: 0;
  z-index: 15;
  height: 0;
  border-top: 2px solid var(--qp-now);
  pointer-events: none;
}

.qp-tg__now::before {
  content: '';
  position: absolute;
  left: -4px;
  top: -4px;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--qp-now);
}
</style>
