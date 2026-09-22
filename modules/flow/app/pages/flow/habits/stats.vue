<script setup lang="ts">
/**
 * Statistics for the selected year — one card per habit.
 *
 * Rate and day counts are year-scoped; streaks are not, because a streak that
 * crosses New Year is one streak. The two live side by side on purpose.
 */
definePageMeta({ layout: 'flow' })

import { useHabitsStore } from '#habit/stores/habits'

const habits = useHabitsStore()

const rows = computed(() => habits.stats?.habits ?? [])
const totalChecks = computed(() => habits.stats?.totalChecks ?? 0)
const bestStreak = computed(() => rows.value.reduce((m, h) => Math.max(m, h.bestStreak), 0))
</script>

<template>
  <div class="qh-stats">
    <div class="qh-stats__scroll">
      <div class="qh-stats__sheet">
        <QPageHeading title="Progress" :meta="String(habits.year)" />
        <section class="qh-stats__totals">
          <div class="qh-stats__total">
            <span class="qh-stats__total-label">Checks in {{ habits.year }}</span>
            <strong class="qh-num">{{ totalChecks }}</strong>
          </div>
          <div class="qh-stats__total">
            <span class="qh-stats__total-label">Habits</span>
            <strong class="qh-num">{{ rows.length }}</strong>
          </div>
          <div class="qh-stats__total">
            <span class="qh-stats__total-label">Best streak</span>
            <strong class="qh-num">{{ bestStreak }}d</strong>
          </div>
        </section>

        <p v-if="!rows.length" class="qh-stats__empty">
          No activity recorded yet.
        </p>

        <section v-for="h in rows" :key="h.id" class="qh-stats__card" :class="{ 'is-paused': h.isPaused }">
          <header class="qh-stats__head">
            <h3 class="qh-stats__name">{{ h.name }}</h3>
            <span v-if="h.isPaused" class="qh-stats__badge">paused</span>
            <span class="qh-stats__since qh-num">since {{ h.startedOn }}</span>
          </header>

          <div class="qh-stats__bar" :title="h.rate === null ? 'No trackable days yet' : `${h.rate}%`">
            <span class="qh-stats__fill" :style="{ width: `${h.rate ?? 0}%` }" />
          </div>

          <dl class="qh-stats__figures">
            <div class="qh-stats__figure">
              <dt>Rate</dt>
              <dd class="qh-num">{{ h.rate === null ? '—' : `${h.rate}%` }}</dd>
            </div>
            <div class="qh-stats__figure">
              <dt>Done</dt>
              <dd class="qh-num">{{ h.checkedDays }} / {{ h.eligibleDays }}</dd>
            </div>
            <div class="qh-stats__figure">
              <dt>Streak</dt>
              <dd class="qh-num">{{ h.currentStreak }}d</dd>
            </div>
            <div class="qh-stats__figure">
              <dt>Best</dt>
              <dd class="qh-num">{{ h.bestStreak }}d</dd>
            </div>
          </dl>
        </section>
      </div>
    </div>
  </div>
</template>

<style scoped>
.qh-stats {
  height: 100%;
  min-height: 0;
}

.qh-stats__scroll {
  height: 100%;
  overflow-y: auto;
}

.qh-stats__sheet {
  display: flex;
  flex-direction: column;
  gap: 14px;
  width: min(1080px, 100%);
  margin: 0 auto;
  padding: var(--qss-surface-padding, 24px);
}

.qh-stats__totals {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 10px;
}

.qh-stats__total {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 12px;
  border: 1px solid var(--qh-border);
  border-radius: var(--qh-radius-lg);
  background: var(--qh-bg-raised);
}

.qh-stats__total-label {
  color: var(--qh-text-muted);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.07em;
  text-transform: uppercase;
}

.qh-stats__total strong {
  color: var(--qh-text);
  font-size: 22px;
  font-weight: 700;
  letter-spacing: -0.02em;
}

.qh-stats__empty {
  margin: 0;
  color: var(--qh-text-muted);
  font-size: 12px;
}

.qh-stats__card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 13px 14px;
  border: 1px solid var(--qh-border-subtle);
  border-radius: var(--qh-radius-lg);
  background: var(--qh-bg-raised);
}

.qh-stats__card.is-paused {
  opacity: 0.75;
}

.qh-stats__head {
  display: flex;
  align-items: baseline;
  gap: 8px;
  min-width: 0;
}

.qh-stats__name {
  margin: 0;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--qh-text);
  font-size: 13px;
  font-weight: 600;
}

.qh-stats__badge {
  flex-shrink: 0;
  padding: 1px 6px;
  border-radius: 999px;
  background: var(--qh-bg-card);
  color: var(--qh-paused);
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.qh-stats__since {
  margin-left: auto;
  flex-shrink: 0;
  color: var(--qh-text-muted);
  font-size: 10px;
}

.qh-stats__bar {
  height: 5px;
  border-radius: 3px;
  background: var(--qh-bg-card);
  overflow: hidden;
}

.qh-stats__fill {
  display: block;
  height: 100%;
  border-radius: 3px;
  background: var(--qh-check);
  transition: width 160ms ease;
}

.qh-stats__figures {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 8px;
  margin: 0;
}

.qh-stats__figure dt {
  color: var(--qh-text-muted);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.qh-stats__figure dd {
  margin: 0;
  color: var(--qh-text);
  font-size: 14px;
  font-weight: 600;
}
</style>
