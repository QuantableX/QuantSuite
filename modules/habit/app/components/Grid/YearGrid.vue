<script setup lang="ts">
/**
 * The day sheet, in two shapes. CW grouping: the whole year as one scroll,
 * week sections stacked. Month grouping: ONE month at a time, with a centered
 * month title and ‹ › paging — crossing January/December pages the year too.
 *
 * Each row carries one progress bar — how much of that day's habits got done
 * — and clicking a row selects the day: the LEFT panel then holds that day's
 * checkboxes, the right panel the habits' settings.
 */
import { useAppStore } from '#habit/stores/app'
import { useHabitsStore } from '#habit/stores/habits'
import { daysOfYear, MONTHS, rowLabel, todayKey, type DayRow } from '#habit/utils/dates'

const app = useAppStore()
const habits = useHabitsStore()

const today = computed(() => todayKey())

// ─── Month paging (month grouping only) ───────────────────────────────────

const month = ref(new Date().getMonth())

/** Entering month view lands on the month you live in, not on January. */
watch(
  () => app.grouping,
  (g) => {
    if (g === 'month' && habits.year === habits.currentYear) month.value = new Date().getMonth()
  }
)

function prevMonth() {
  if (month.value > 0) {
    month.value -= 1
  } else {
    month.value = 11
    void habits.setYear(habits.year - 1)
  }
}

function nextMonth() {
  if (month.value < 11) {
    month.value += 1
  } else {
    month.value = 0
    void habits.setYear(habits.year + 1)
  }
}

// ─── Sections ─────────────────────────────────────────────────────────────

interface Section {
  key: string
  label: string
  sub: string
  rows: DayRow[]
}

/** CW mode: contiguous ISO-week runs over the whole year (runs, not buckets,
 * so the weeks that straddle New Year stay in calendar order). Month mode:
 * exactly one section — the paged month. */
const sections = computed<Section[]>(() => {
  const days = daysOfYear(habits.year)

  if (app.grouping === 'month') {
    return [
      {
        key: `m${month.value}-${habits.year}`,
        label: MONTHS[month.value]!,
        sub: '',
        rows: days.filter((r) => r.month === month.value),
      },
    ]
  }

  const out: Section[] = []
  for (const row of days) {
    const id = `w${row.week}`
    const last = out[out.length - 1]
    if (last && last.key === id) {
      last.rows.push(row)
      continue
    }
    out.push({ key: id, label: `CW ${row.week}`, sub: '', rows: [row] })
  }
  for (const s of out) {
    const first = s.rows[0]!
    const lastRow = s.rows[s.rows.length - 1]!
    s.key = `${s.key}-${first.key}`
    s.sub = `${first.date.getDate()}.${first.month + 1}. – ${lastRow.date.getDate()}.${lastRow.month + 1}.`
  }
  return out
})

const dayLabel = (row: DayRow) => rowLabel(row)

/** A section's own average, over the days that had anything to track. */
function sectionPct(section: Section): number | null {
  let sum = 0
  let counted = 0
  for (const row of section.rows) {
    const p = habits.dayProgress(row.key)
    if (p.eligible > 0) {
      sum += p.pct
      counted += 1
    }
  }
  return counted > 0 ? Math.round(sum / counted) : null
}

const monthPct = computed(() => (sections.value[0] ? sectionPct(sections.value[0]) : null))

// ─── Scroll ───────────────────────────────────────────────────────────────

/** Land on today, not on January 1st — the row you came to tick. */
const scrollEl = ref<HTMLElement | null>(null)

function scrollToToday(smooth = false) {
  if (habits.year !== habits.currentYear) return
  const el = scrollEl.value?.querySelector<HTMLElement>('[data-today="true"]')
  el?.scrollIntoView({ block: 'center', behavior: smooth ? 'smooth' : 'auto' })
}

onMounted(() => void nextTick(() => scrollToToday()))
onActivated(() => scrollToToday())
watch(
  () => [habits.year, app.grouping, habits.habits.length > 0],
  () => void nextTick(() => scrollToToday())
)
</script>

<template>
  <div ref="scrollEl" class="qh-grid">
    <div v-if="habits.habits.length" class="qh-grid__inner">
      <div v-if="app.grouping === 'month'" class="qh-grid__monthnav">
        <button class="qh-grid__mbtn" title="Previous month" @click="prevMonth">‹</button>
        <div class="qh-grid__mtitle">
          <strong>{{ MONTHS[month] }} <span class="qh-num">{{ habits.year }}</span></strong>
          <span v-if="monthPct !== null" class="qh-grid__msub qh-num">{{ monthPct }}% done</span>
        </div>
        <button class="qh-grid__mbtn" title="Next month" @click="nextMonth">›</button>
      </div>

      <template v-for="section in sections" :key="section.key">
        <div v-if="app.grouping === 'week'" class="qh-grid__section">
          <span class="qh-grid__section-label">{{ section.label }}</span>
          <span class="qh-grid__section-sub qh-num">{{ section.sub }}</span>
          <span v-if="sectionPct(section) !== null" class="qh-grid__section-pct qh-num">
            {{ sectionPct(section) }}%
          </span>
        </div>

        <button
          v-for="row in section.rows"
          :key="row.key"
          type="button"
          class="qh-grid__row"
          :class="{
            'is-today': row.key === today,
            'is-selected': row.key === habits.selectedDay,
            'is-weekend': row.weekday >= 5,
            'is-future': row.key > today,
          }"
          :data-today="row.key === today || undefined"
          @click="habits.selectDay(row.key)"
        >
          <span class="qh-grid__day qh-num">{{ dayLabel(row) }}</span>

          <span class="qh-grid__bar" :class="{ 'is-full': habits.dayProgress(row.key).pct === 100 }">
            <span
              v-if="habits.dayProgress(row.key).eligible > 0"
              class="qh-grid__fill"
              :style="{ width: `${habits.dayProgress(row.key).pct}%` }"
            />
          </span>

          <span class="qh-grid__count qh-num">
            <template v-if="habits.dayProgress(row.key).eligible > 0">
              {{ habits.dayProgress(row.key).done }}/{{ habits.dayProgress(row.key).eligible }}
            </template>
            <template v-else>–</template>
          </span>

          <span class="qh-grid__pct qh-num">
            <template v-if="habits.dayProgress(row.key).eligible > 0">
              {{ habits.dayProgress(row.key).pct }}%
            </template>
          </span>
        </button>
      </template>
    </div>

    <QEmptyState v-else title="No habits yet">
      <button class="qh-btn" @click="habits.openCreate()">New habit</button>
    </QEmptyState>
  </div>
</template>

<style scoped>
.qh-grid {
  position: relative;
  height: 100%;
  overflow-y: auto;
}

.qh-grid__inner {
  width: min(880px, 100%);
  margin: 0 auto;
  padding: 16px 24px 32px;
}

/* One month at a time: ‹ [ September 2026 ] › */
.qh-grid__monthnav {
  position: sticky;
  top: 0;
  z-index: 2;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 4px 8px;
  background: var(--qh-bg);
}

.qh-grid__mbtn {
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

.qh-grid__mbtn:hover {
  border-color: var(--qh-accent);
  color: var(--qh-text);
}

.qh-grid__mtitle {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 1px;
}

.qh-grid__mtitle strong {
  color: var(--qh-text);
  font-size: 13px;
  font-weight: 700;
  letter-spacing: 0.02em;
}

.qh-grid__mtitle strong .qh-num {
  color: var(--qh-text-muted);
  font-weight: 600;
}

.qh-grid__msub {
  color: var(--qh-text-muted);
  font-size: 10px;
}

.qh-grid__section {
  position: sticky;
  top: 0;
  z-index: 2;
  display: flex;
  align-items: baseline;
  gap: 8px;
  padding: 12px 10px 5px;
  background: var(--qh-bg);
}

.qh-grid__section-label {
  color: var(--qh-text);
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.05em;
  text-transform: uppercase;
}

.qh-grid__section-sub {
  color: var(--qh-text-muted);
  font-family: var(--qss-font-mono, ui-monospace, monospace);
  font-size: 10px;
}

.qh-grid__section-pct {
  margin-left: auto;
  color: var(--qh-text-muted);
  font-family: var(--qss-font-mono, ui-monospace, monospace);
  font-size: 10px;
}

.qh-grid__row {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 8px 12px;
  border: 1px solid transparent;
  border-radius: var(--qh-radius);
  background: transparent;
  cursor: pointer;
  text-align: left;
}

.qh-grid__row:hover {
  background: var(--qh-bg-hover);
}

.qh-grid__row.is-weekend {
  background: color-mix(in srgb, var(--qh-bg-raised) 55%, transparent);
}

.qh-grid__row.is-weekend:hover {
  background: var(--qh-bg-hover);
}

.qh-grid__row.is-today .qh-grid__day {
  color: var(--qh-text);
  font-weight: 700;
}

.qh-grid__row.is-selected {
  border-color: var(--qh-accent);
  background: var(--qh-bg-card);
}

.qh-grid__row.is-future {
  opacity: 0.75;
}

.qh-grid__day {
  flex-shrink: 0;
  width: 58px;
  color: var(--qh-text-secondary);
  font-family: var(--qss-font-mono, ui-monospace, monospace);
  font-size: 11px;
}

.qh-grid__bar {
  flex: 1;
  min-width: 0;
  height: 8px;
  border-radius: 4px;
  background: var(--qh-bg-card);
  overflow: hidden;
}

.qh-grid__fill {
  display: block;
  height: 100%;
  border-radius: 4px;
  background: color-mix(in srgb, var(--qh-check) 78%, var(--qh-bg-card));
  transition: width 140ms ease;
}

.qh-grid__bar.is-full .qh-grid__fill {
  background: var(--qh-check);
}

.qh-grid__count {
  flex-shrink: 0;
  width: 34px;
  color: var(--qh-text-muted);
  font-family: var(--qss-font-mono, ui-monospace, monospace);
  font-size: 10px;
  text-align: right;
}

.qh-grid__pct {
  flex-shrink: 0;
  width: 38px;
  color: var(--qh-text-secondary);
  font-family: var(--qss-font-mono, ui-monospace, monospace);
  font-size: 11px;
  text-align: right;
}

</style>
