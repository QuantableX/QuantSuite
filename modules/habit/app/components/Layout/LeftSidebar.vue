<script setup lang="ts">
/**
 * The CHECK side. Mirrors the right panel row for row — same habits, same
 * order, same geometry — but here each row is the selected day's checkbox.
 * The right panel is where the same rows carry their settings.
 */
import { useHabitsStore } from '#habit/stores/habits'
import { longDayLabel, todayKey } from '#habit/utils/dates'
import type { DayState, Habit } from '#habit/types'

const habits = useHabitsStore()

const day = computed(() => habits.selectedDay)
const isToday = computed(() => day.value === todayKey())

const rows = computed(() =>
  habits.habits.map((habit) => ({ habit, state: habits.dayState(habit, day.value) }))
)

const progress = computed(() => habits.dayProgress(day.value))

const HINTS: Partial<Record<DayState, string>> = {
  before: 'not yet started',
  paused: 'paused',
  untracked: 'off day',
  future: 'future',
}

function streakFor(id: string): number {
  return habits.stats?.habits.find((h) => h.id === id)?.currentStreak ?? 0
}

function onToggle(habit: Habit, state: DayState) {
  if (state === 'open' || state === 'checked') void habits.toggle(habit, day.value)
}
</script>

<template>
  <div class="qh-side">
    <section class="qh-side__lead">
      <div class="qh-side__lead-top">
        <span class="qh-side__lead-label">{{ isToday ? 'Today' : 'Selected day' }}</span>
        <button v-if="!isToday" class="qh-side__lead-act" @click="habits.selectDay(todayKey())">
          Back to today
        </button>
      </div>
      <strong class="qh-side__headline">{{ longDayLabel(day) }}</strong>
      <div class="qh-side__meter" :class="{ 'is-full': progress.eligible > 0 && progress.done === progress.eligible }">
        <span class="qh-side__meter-fill" :style="{ width: `${progress.pct}%` }" />
      </div>
      <span class="qh-side__lead-sub qh-num">
        <template v-if="progress.eligible > 0">{{ progress.done }} of {{ progress.eligible }} done · {{ progress.pct }}%</template>
        <template v-else>nothing to track on this day</template>
      </span>
    </section>

    <p v-if="!rows.length" class="qh-side__empty">
      No habits yet.
    </p>

    <ul class="qh-side__list">
      <li
        v-for="{ habit, state } in rows"
        :key="habit.id"
        class="qh-side__row"
        :class="{ 'is-off': state !== 'open' && state !== 'checked' }"
      >
        <button
          class="qh-side__check"
          :class="{ 'is-checked': state === 'checked' }"
          :disabled="state !== 'open' && state !== 'checked'"
          :title="state === 'checked' ? 'Untick' : 'Done'"
          @click="onToggle(habit, state)"
        >
          <svg v-if="state === 'checked'" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M4 13l5 5L20 6" />
          </svg>
          <span v-else-if="state === 'paused'" class="qh-side__pausemark">–</span>
        </button>
        <span class="qh-side__name">{{ habit.name }}</span>
        <span v-if="HINTS[state]" class="qh-side__tag">{{ HINTS[state] }}</span>
        <span v-else-if="streakFor(habit.id) > 0" class="qh-side__aside qh-num" title="Current streak">
          {{ streakFor(habit.id) }}d
        </span>
      </li>
    </ul>
  </div>
</template>

<style scoped>
/* The two panels share this skeleton deliberately — see RightSidebar.vue. */
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

.qh-side__meter {
  height: 6px;
  border-radius: 3px;
  background: var(--qh-bg-card);
  overflow: hidden;
}

.qh-side__meter-fill {
  display: block;
  height: 100%;
  border-radius: 3px;
  background: color-mix(in srgb, var(--qh-check) 78%, var(--qh-bg-card));
  transition: width 140ms ease;
}

.qh-side__meter.is-full .qh-side__meter-fill {
  background: var(--qh-check);
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
}

.qh-side__row:hover {
  background: var(--qh-bg-hover);
}

.qh-side__row.is-off {
  opacity: 0.55;
}

.qh-side__check {
  display: grid;
  place-items: center;
  width: 18px;
  height: 18px;
  flex-shrink: 0;
  padding: 0;
  border: 1px solid var(--qh-border);
  border-radius: 5px;
  background: var(--qh-bg-input);
  color: #fff;
  cursor: pointer;
  transition: background 100ms ease, border-color 100ms ease;
}

.qh-side__check:disabled {
  cursor: default;
}

.qh-side__check:hover:not(:disabled) {
  border-color: var(--qh-check);
}

.qh-side__check.is-checked {
  border-color: var(--qh-check);
  background: var(--qh-check);
}

.qh-side__pausemark {
  color: var(--qh-paused);
  font-size: 11px;
  line-height: 1;
}

.qh-side__name {
  flex: 1;
  min-width: 0;
  color: var(--qh-text-secondary);
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.qh-side__row:not(.is-off) .qh-side__name {
  color: var(--qh-text);
}

.qh-side__tag {
  flex-shrink: 0;
  padding: 1px 6px;
  border-radius: 999px;
  background: var(--qh-bg-card);
  color: var(--qh-paused);
  font-size: 10px;
}

.qh-side__aside {
  flex-shrink: 0;
  color: var(--qh-text-muted);
  font-size: 11px;
}
</style>
