<script setup lang="ts">
/**
 * The SETTINGS side. Deliberately the left panel's mirror: same lead card,
 * same list, same rows in the same order — but where the left row has the
 * selected day's checkbox, this one has a drag grip and the edit button.
 *
 * Everything else — rename, schedule, pause, delete — lives in the habit
 * modal behind the edit button; creating lives behind the header's button.
 *
 * Reordering is pointer-event drag, NOT HTML5 drag-and-drop — HTML5 DnD is
 * broken in WebView2 and untestable synthetically (see memory / QuantPlan's
 * event dragging). Rects are snapshotted once when the drag starts, so the
 * insertion line cannot oscillate under the pointer.
 */
import { useHabitsStore } from '#habit/stores/habits'
import type { Habit } from '#habit/types'
import { WEEKDAYS } from '#habit/utils/dates'
import { ALL_DAYS } from '#habit/types'

const habits = useHabitsStore()

const pausedCount = computed(() => habits.habits.filter((h) => h.isPaused).length)

/** "Mo We Fr" — only shown when the schedule is not every day. */
function scheduleLabel(habit: Habit): string | null {
  if ((habit.daysMask & ALL_DAYS) === ALL_DAYS) return null
  return WEEKDAYS.filter((_, i) => ((habit.daysMask >> i) & 1) === 1).join(' ')
}

// ─── Pointer drag to reorder ──────────────────────────────────────────────

const listEl = ref<HTMLElement | null>(null)
const dragId = ref<string | null>(null)
const dragging = ref(false)
const dropIndex = ref<number | null>(null)
let startY = 0
let rects: { id: string; mid: number }[] = []

function onGripDown(e: PointerEvent, habit: Habit) {
  if (e.button !== 0) return
  ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
  dragId.value = habit.id
  dragging.value = false
  startY = e.clientY
}

function onGripMove(e: PointerEvent) {
  if (!dragId.value) return
  if (!dragging.value) {
    if (Math.abs(e.clientY - startY) < 4) return
    dragging.value = true
    rects = [...(listEl.value?.querySelectorAll<HTMLElement>('[data-drag-id]') ?? [])]
      .filter((el) => el.dataset.dragId !== dragId.value)
      .map((el) => {
        const r = el.getBoundingClientRect()
        return { id: el.dataset.dragId!, mid: r.top + r.height / 2 }
      })
  }
  dropIndex.value = rects.filter((r) => r.mid < e.clientY).length
}

async function onGripUp() {
  const id = dragId.value
  const to = dropIndex.value
  const moved = dragging.value
  dragId.value = null
  dragging.value = false
  dropIndex.value = null
  if (id && moved && to !== null) await habits.reorder(id, to)
}

function onGripCancel() {
  dragId.value = null
  dragging.value = false
  dropIndex.value = null
}

/** This row's index among the non-dragged rows — what dropIndex counts. */
function slotOf(i: number): number {
  let n = 0
  for (let j = 0; j < i; j++) if (habits.habits[j]!.id !== dragId.value) n += 1
  return n
}
</script>

<template>
  <div class="qh-side">
    <section class="qh-side__lead">
      <div class="qh-side__lead-top">
        <span class="qh-side__lead-label">Habits</span>
        <button class="qh-side__lead-act" title="Create a habit" @click="habits.openCreate()">
          + New
        </button>
      </div>
      <strong class="qh-side__headline qh-num">{{ habits.habits.length }}</strong>
      <span class="qh-side__lead-sub qh-num">tracked · {{ pausedCount }} paused</span>
    </section>

    <p v-if="!habits.habits.length" class="qh-side__empty">
      No habits yet.
    </p>

    <ul ref="listEl" class="qh-side__list">
      <li
        v-for="(habit, i) in habits.habits"
        :key="habit.id"
        class="qh-side__row"
        :class="{
          'is-off': habit.isPaused,
          'is-dragging': dragging && habit.id === dragId,
          'drop-before': dragging && habit.id !== dragId && slotOf(i) === dropIndex,
        }"
        :data-drag-id="habit.id"
      >
        <button
          class="qh-side__grip"
          title="Drag to reorder"
          @pointerdown="onGripDown($event, habit)"
          @pointermove="onGripMove"
          @pointerup="onGripUp"
          @pointercancel="onGripCancel"
        >
          <svg width="11" height="11" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
            <circle cx="8" cy="5" r="1.6" /><circle cx="16" cy="5" r="1.6" />
            <circle cx="8" cy="12" r="1.6" /><circle cx="16" cy="12" r="1.6" />
            <circle cx="8" cy="19" r="1.6" /><circle cx="16" cy="19" r="1.6" />
          </svg>
        </button>

        <span class="qh-side__name" :title="`Since ${habit.startedOn}`">{{ habit.name }}</span>

        <span v-if="habit.isPaused" class="qh-side__tag">paused</span>
        <span v-else-if="scheduleLabel(habit)" class="qh-side__tag qh-side__tag--days">
          {{ scheduleLabel(habit) }}
        </span>

        <button class="qh-side__edit" title="Edit — name, days, pause, delete" @click="habits.openEdit(habit.id)">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M17 3a2.85 2.85 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5z" />
          </svg>
        </button>
      </li>
      <li
        v-if="dragging && dropIndex === habits.habits.length - 1"
        class="qh-side__endline"
        aria-hidden="true"
      />
    </ul>
  </div>
</template>

<style scoped>
/* The two panels share this skeleton deliberately — see LeftSidebar.vue. */
.qh-side {
  height: 100%;
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-height: 0;
  padding: 12px 10px;
  overflow-y: auto;
}

.qh-side__lead {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 12px;
  border: 1px solid var(--qh-border);
  border-radius: var(--qh-radius-lg);
  background: var(--qh-bg-raised);
}

.qh-side__lead-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.qh-side__lead-label {
  color: var(--qh-text-muted);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.07em;
  text-transform: uppercase;
}

.qh-side__lead-act {
  padding: 1px 7px;
  border: 1px solid var(--qh-border-subtle);
  border-radius: 999px;
  background: transparent;
  color: var(--qh-text-muted);
  font-size: 10px;
  cursor: pointer;
}

.qh-side__lead-act:hover {
  color: var(--qh-text);
  border-color: var(--qh-accent);
}

.qh-side__headline {
  color: var(--qh-text);
  font-size: 14px;
  font-weight: 700;
  letter-spacing: -0.01em;
}

.qh-side__lead-sub {
  color: var(--qh-text-muted);
  font-size: 11px;
}

.qh-side__empty {
  margin: 0;
  color: var(--qh-text-muted);
  font-size: 12px;
  line-height: 1.5;
}

.qh-side__list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.qh-side__row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 32px;
  padding: 4px 6px;
  border-radius: var(--qh-radius);
  border-top: 2px solid transparent;
}

.qh-side__row:hover {
  background: var(--qh-bg-hover);
}

.qh-side__row.is-off {
  opacity: 0.55;
}

.qh-side__row.is-dragging {
  opacity: 0.35;
}

.qh-side__row.drop-before {
  border-top-color: var(--qh-accent);
}

.qh-side__endline {
  height: 2px;
  border-radius: 1px;
  background: var(--qh-accent);
}

.qh-side__grip {
  display: grid;
  place-items: center;
  width: 18px;
  height: 18px;
  flex-shrink: 0;
  padding: 0;
  border: none;
  background: transparent;
  color: var(--qh-text-muted);
  cursor: grab;
  /* Pointer capture owns the gesture — the browser must not scroll it. */
  touch-action: none;
}

.qh-side__grip:active {
  cursor: grabbing;
  color: var(--qh-text);
}

.qh-side__name {
  flex: 1;
  min-width: 0;
  color: var(--qh-text);
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.qh-side__tag {
  flex-shrink: 0;
  padding: 1px 6px;
  border-radius: 999px;
  background: var(--qh-bg-card);
  color: var(--qh-paused);
  font-size: 10px;
}

.qh-side__tag--days {
  color: var(--qh-text-muted);
}

.qh-side__edit {
  display: grid;
  place-items: center;
  width: 22px;
  height: 22px;
  flex-shrink: 0;
  padding: 0;
  border: none;
  border-radius: var(--qh-radius);
  background: transparent;
  color: var(--qh-text-muted);
  cursor: pointer;
}

.qh-side__edit:hover {
  background: var(--qh-bg-card);
  color: var(--qh-text);
}
</style>
