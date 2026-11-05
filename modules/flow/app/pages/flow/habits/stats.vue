<script setup lang="ts">
/**
 * Statistics for the selected year — one line per habit.
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

    <!-- One line per habit, with the figures in columns so they compare down the page.
         Below 720px each habit folds back into its own labelled block. -->
    <div v-else class="qh-stats__list">
      <div class="qh-stats__columns" aria-hidden="true">
        <span />
        <span />
        <span>Rate</span>
        <span>Done</span>
        <span>Streak</span>
        <span>Best</span>
        <span />
      </div>

      <section v-for="h in rows" :key="h.id" class="qh-stats__card" :class="{ 'is-paused': h.isPaused }">
        <header class="qh-stats__head">
          <div class="qh-stats__who">
            <h3 class="qh-stats__name">{{ h.name }}</h3>
            <span v-if="h.isPaused" class="qh-stats__badge">paused</span>
          </div>
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
</template>

<style scoped>
.qh-stats {
  /* The one column grid for the header line and every habit, so they align. */
  --qh-stats-cols: minmax(150px, 1.2fr) minmax(120px, 3fr) repeat(4, 84px) 116px;
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap);
  height: 100%;
  min-height: 0;
  padding: var(--qss-page-edge);
  overflow-y: auto;
  container-type: inline-size;
}

.qh-stats__totals {
  flex-shrink: 0;
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: var(--qss-gap);
}

.qh-stats__total {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: var(--qss-panel-pad);
  border: 1px solid var(--qh-border-subtle);
  border-radius: var(--qss-panel-radius);
  background: var(--qh-bg-raised);
}

.qh-stats__total-label {
  color: var(--qh-text-secondary);
  font-size: var(--qss-text-label);
}

.qh-stats__total strong {
  color: var(--qh-text);
  font-size: var(--qss-text-metric);
  font-weight: 600;
  letter-spacing: -0.02em;
  line-height: 1.2;
}

.qh-stats__empty {
  margin: 0;
  color: var(--qh-text-secondary);
  font-size: 13px;
}

/* Hugs its rows and scrolls inside itself once there are more habits than height. */
.qh-stats__list {
  flex: 0 1 auto;
  min-height: 0;
  overflow-y: auto;
  border: 1px solid var(--qh-border-subtle);
  border-radius: var(--qss-panel-radius);
}

.qh-stats__columns,
.qh-stats__card {
  display: grid;
  grid-template-columns: var(--qh-stats-cols);
  align-items: center;
  column-gap: var(--qss-gap);
  min-height: var(--qss-row-h);
  padding: 0 var(--qss-panel-pad);
}

.qh-stats__columns {
  position: sticky;
  top: 0;
  z-index: 1;
  background: var(--qh-bg-raised);
  border-bottom: 1px solid var(--qh-border-subtle);
  color: var(--qh-text-secondary);
  font-size: var(--qss-text-meta);
  font-weight: 500;
  text-align: right;
}

.qh-stats__card + .qh-stats__card {
  border-top: 1px solid var(--qh-border-subtle);
}

.qh-stats__card.is-paused {
  opacity: 0.75;
}

/* Name first, the since date in the last column. */
.qh-stats__head,
.qh-stats__figures,
.qh-stats__figure {
  display: contents;
}

.qh-stats__who {
  grid-column: 1;
  grid-row: 1;
  display: flex;
  align-items: baseline;
  gap: var(--qss-gap-tight);
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
  font-weight: 500;
}

.qh-stats__badge {
  flex-shrink: 0;
  padding: 0 6px;
  border-radius: 999px;
  background: var(--qh-bg-card);
  color: var(--qh-text-secondary);
  font-size: 11px;
}

.qh-stats__since {
  grid-column: 7;
  grid-row: 1;
  color: var(--qh-text-secondary);
  font-size: var(--qss-text-meta);
  text-align: right;
}

.qh-stats__bar {
  grid-column: 2;
  grid-row: 1;
  height: 6px;
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

/* The header line names these columns. */
.qh-stats__figure dt {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip-path: inset(50%);
}

.qh-stats__figure dd {
  grid-row: 1;
  margin: 0;
  color: var(--qh-text);
  font-size: 13px;
  font-weight: 500;
  text-align: right;
}

.qh-stats__figure:nth-child(1) dd { grid-column: 3; }
.qh-stats__figure:nth-child(2) dd { grid-column: 4; }
.qh-stats__figure:nth-child(3) dd { grid-column: 5; }
.qh-stats__figure:nth-child(4) dd { grid-column: 6; }

/* No room for the columns: each habit becomes a labelled block again. */
@container (max-width: 720px) {
  .qh-stats__columns {
    display: none;
  }
  .qh-stats__card {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 6px;
    padding: var(--qss-gap-tight) var(--qss-panel-pad);
  }
  .qh-stats__head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--qss-gap-tight);
  }
  .qh-stats__figures {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: var(--qss-gap-tight);
    margin: 0;
  }
  .qh-stats__figure {
    display: block;
  }
  .qh-stats__figure dt {
    position: static;
    width: auto;
    height: auto;
    clip-path: none;
    color: var(--qh-text-secondary);
    font-size: var(--qss-text-meta);
  }
  .qh-stats__figure dd {
    text-align: left;
  }
}
</style>
