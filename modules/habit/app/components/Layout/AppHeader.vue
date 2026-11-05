<script setup lang="ts">
/**
 * QuantFlow's habit toolbar: create, year and CW/month grouping controls.
 */
import { useAppStore } from '#habit/stores/app'
import { useHabitsStore } from '#habit/stores/habits'
import type { Grouping } from '#habit/types'
import { MONTHS } from '#habit/utils/dates'

const app = useAppStore()
const habits = useHabitsStore()
const route = useRoute()

/** Paging belongs to the day sheet; Progress covers the whole year. */
const showMonth = computed(() => app.grouping === 'month' && habits.habits.length > 0 && !route.path.endsWith('/stats'))

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

      <!-- One month at a time: ‹ [ September 2026 ] › -->
      <div v-if="showMonth" class="qh-hd__month">
        <button class="qh-hd__mbtn" title="Previous month" @click="habits.prevMonth()">‹</button>
        <div class="qh-hd__mtitle">
          <strong>{{ MONTHS[habits.month] }} <span class="qh-num">{{ habits.year }}</span></strong>
          <span v-if="habits.monthPct !== null" class="qh-hd__msub qh-num">{{ habits.monthPct }}% done</span>
        </div>
        <button class="qh-hd__mbtn" title="Next month" @click="habits.nextMonth()">›</button>
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
.qh-toolbar { display: flex; flex-shrink: 0; min-height: 78px; border-bottom: 1px solid var(--qh-border-subtle); container-type: inline-size; }
.qh-hd__title { margin: 0; font-size: 24px; font-weight: 500; letter-spacing: -.035em; }
/* Equal outer columns keep the month in the middle of the page, above the day sheet. */
.qh-hd {
  flex: 1;
  min-width: 0;
  display: grid;
  grid-template-columns: 1fr auto 1fr;
  align-items: center;
  gap: 16px;
  padding: 16px 24px;
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

/* A fixed width: the arrows stay put while paging from May to September. */
.qh-hd__month {
  grid-column: 2;
  display: flex;
  align-items: center;
  gap: 8px;
  width: 240px;
}

.qh-hd__mbtn {
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  flex-shrink: 0;
  border: 1px solid var(--qh-border-subtle);
  border-radius: var(--qh-radius);
  background: var(--qh-bg-card);
  color: var(--qh-text-muted);
  font-size: 15px;
  cursor: pointer;
}

.qh-hd__mbtn:hover {
  border-color: var(--qh-accent);
  color: var(--qh-text);
}

.qh-hd__mtitle {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 1px;
}

.qh-hd__mtitle strong {
  color: var(--qh-text);
  font-size: 13px;
  font-weight: 700;
  letter-spacing: 0.02em;
}

.qh-hd__mtitle strong .qh-num {
  color: var(--qh-text-muted);
  font-weight: 600;
}

.qh-hd__msub {
  color: var(--qh-text-muted);
  font-size: 10px;
}

.qh-hd__controls {
  grid-column: 3;
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

/* No room for three in a row: the month takes a row of its own. */
@container (max-width: 700px) {
  .qh-hd { grid-template-columns: auto 1fr; }
  .qh-hd__controls { grid-column: 2; grid-row: 1; }
  .qh-hd__month { grid-column: 1 / -1; grid-row: 2; justify-self: center; }
}
</style>
