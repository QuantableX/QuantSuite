<script setup lang="ts">
/**
 * QuantFlow's habit toolbar: create, year and CW/month grouping controls.
 */
import { useAppStore } from '#habit/stores/app'
import { useHabitsStore } from '#habit/stores/habits'
import type { Grouping } from '#habit/types'

const app = useAppStore()
const habits = useHabitsStore()

const GROUPINGS: { value: Grouping; label: string }[] = [
  { value: 'week', label: 'CW' },
  { value: 'month', label: 'Month' },
]

</script>

<template>
  <div class="qh-toolbar">
    <div class="qh-hd">
      <div class="qh-hd__mid">
        <h1 class="qh-hd__title">Habits</h1>
        <button class="qh-hd__new" title="Create a habit — it starts today" @click="habits.openCreate()">
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">
            <path d="M12 5v14 M5 12h14" />
          </svg>
          <span>New habit</span>
        </button>
      </div>

      <div class="qh-hd__controls">
        <div class="qh-hd__year">
          <button class="qh-hd__step" title="Previous year" @click="habits.setYear(habits.year - 1)">‹</button>
          <button
            class="qh-hd__now qh-num"
            :class="{ 'is-current': habits.year === habits.currentYear }"
            title="Back to this year"
            @click="habits.setYear(habits.currentYear)"
          >
            {{ habits.year }}
          </button>
          <button class="qh-hd__step" title="Next year" @click="habits.setYear(habits.year + 1)">›</button>
        </div>

        <div class="qh-hd__group" role="group" aria-label="Row grouping">
          <button
            v-for="g in GROUPINGS"
            :key="g.value"
            class="qh-hd__group-btn"
            :class="{ 'is-active': app.grouping === g.value }"
            :aria-pressed="app.grouping === g.value"
            @click="app.setGrouping(g.value)"
          >
            {{ g.label }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.qh-toolbar { display: flex; flex-shrink: 0; min-height: 78px; border-bottom: 1px solid var(--qh-border-subtle); }
.qh-hd__title { margin: 0; font-size: 24px; font-weight: 500; letter-spacing: -.035em; }
.qh-hd {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 16px 24px;
  flex-wrap: wrap;
}

.qh-hd__mid {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 20px;
}

.qh-hd__new {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 7px 12px;
  border: 1px solid var(--qh-border);
  border-radius: var(--qh-radius);
  background: var(--qh-bg-card);
  color: var(--qh-text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: border-color 120ms ease, color 120ms ease;
}

.qh-hd__new:hover {
  border-color: var(--qh-accent);
  color: var(--qh-text);
}

.qh-hd__controls {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 10px;
}

.qh-hd__year {
  display: flex;
  align-items: center;
  gap: 2px;
}

.qh-hd__step {
  display: grid;
  place-items: center;
  width: 22px;
  height: 22px;
  border: none;
  border-radius: var(--qh-radius);
  background: transparent;
  color: var(--qh-text-muted);
  font-size: 14px;
  cursor: pointer;
}

.qh-hd__step:hover {
  background: var(--qh-bg-hover);
  color: var(--qh-text);
}

.qh-hd__now {
  padding: 3px 8px;
  border: none;
  border-radius: var(--qh-radius);
  background: transparent;
  color: var(--qh-text-secondary);
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
}

.qh-hd__now:hover {
  background: var(--qh-bg-hover);
}

.qh-hd__now.is-current {
  color: var(--qh-text);
}

.qh-hd__group {
  display: flex;
  padding: 2px;
  border: 1px solid var(--qh-border-subtle);
  border-radius: var(--qh-radius);
  background: var(--qh-bg-raised);
}

.qh-hd__group-btn {
  padding: 2px 8px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--qh-text-muted);
  font-size: 11px;
  cursor: pointer;
}

.qh-hd__group-btn.is-active {
  background: var(--qh-bg-card);
  color: var(--qh-text);
}
</style>
