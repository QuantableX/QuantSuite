<script setup lang="ts">
/**
 * A year of trading days as a calendar: one column per week, filling the
 * width it is given (the cells scale with it). Fed by `get_daily_pnl`, so
 * every closed trade of the year counts, not just the table's page.
 */
import type { DailyPnl } from '#algo/types'
import { formatCurrency } from '#algo/utils/format'

const props = withDefaults(defineProps<{
  days: DailyPnl[]
  weeks?: number
}>(), {
  weeks: 53,
})

const DAY_LABELS = ['M', 'T', 'W', 'T', 'F', 'S', 'S']

interface DayCell {
  date: string
  pnl: number
  trades: number
  level: number
  hasData: boolean
  future: boolean
}

// The cells are local Date objects carrying the current time-of-day, so keying
// them via toISOString() would shift the whole grid by a day near midnight.
function toDateString(date: Date): string {
  const y = date.getFullYear()
  const m = String(date.getMonth() + 1).padStart(2, '0')
  const d = String(date.getDate()).padStart(2, '0')
  return `${y}-${m}-${d}`
}

const byDate = computed<Record<string, DailyPnl>>(() =>
  Object.fromEntries(props.days.map((d) => [d.date, d])),
)

const grid = computed<{ cells: DayCell[][]; monthLabels: { label: string; col: number }[] }>(() => {
  const today = new Date()
  const todayStr = toDateString(today)
  const todayDay = today.getDay()
  const mondayOffset = todayDay === 0 ? 6 : todayDay - 1
  const endMonday = new Date(today)
  endMonday.setDate(today.getDate() - mondayOffset)

  const startDate = new Date(endMonday)
  startDate.setDate(startDate.getDate() - (props.weeks - 1) * 7)

  const pnlValues = props.days.map((d) => d.pnl)
  const maxProfit = Math.max(0, ...pnlValues.filter((v) => v > 0))
  const maxLoss = Math.min(0, ...pnlValues.filter((v) => v < 0))

  const columns: DayCell[][] = []
  const monthLabels: { label: string; col: number }[] = []
  let lastMonth = -1

  for (let w = 0; w < props.weeks; w++) {
    const week: DayCell[] = []
    for (let d = 0; d < 7; d++) {
      const cellDate = new Date(startDate)
      cellDate.setDate(startDate.getDate() + w * 7 + d)
      const dateStr = toDateString(cellDate)
      const day = byDate.value[dateStr]
      const pnl = day?.pnl ?? 0
      const hasData = day !== undefined

      let level = 0
      if (hasData) {
        if (pnl > 0 && maxProfit > 0) {
          const ratio = pnl / maxProfit
          level = ratio < 0.25 ? 1 : ratio < 0.5 ? 2 : ratio < 0.75 ? 3 : 4
        } else if (pnl < 0 && maxLoss < 0) {
          const ratio = pnl / maxLoss
          level = ratio < 0.25 ? -1 : ratio < 0.5 ? -2 : ratio < 0.75 ? -3 : -4
        }
      }

      const cellMonth = cellDate.getMonth()
      if (d === 0 && cellMonth !== lastMonth) {
        // The first column's label would collide with the next month's when
        // that month starts a week later; skip it then.
        if (lastMonth !== -1 || w === 0) {
          monthLabels.push({ label: cellDate.toLocaleString('en-US', { month: 'short' }), col: w })
        }
        lastMonth = cellMonth
      }

      week.push({ date: dateStr, pnl, trades: day?.trades ?? 0, level, hasData, future: dateStr > todayStr })
    }
    columns.push(week)
  }

  // Two labels within two columns of each other overlap at narrow widths.
  const spaced = monthLabels.filter((m, i) => i === 0 || m.col - monthLabels[i - 1]!.col >= 3)
  return { cells: columns, monthLabels: spaced }
})

function cellColor(cell: DayCell): string {
  if (cell.future) return 'transparent'
  if (!cell.hasData) return 'var(--qa-bg-hover)'
  switch (cell.level) {
    case 4: return '#1a7f37'
    case 3: return '#26a641'
    case 2: return '#39d353'
    case 1: return '#57e06c'
    case -1: return '#ff8a8a'
    case -2: return '#ff6b6b'
    case -3: return '#ff4757'
    case -4: return '#cc2233'
    default: return 'var(--qa-bg-hover)'
  }
}

const hoveredCell = ref<DayCell | null>(null)
const tooltipPos = ref({ x: 0, y: 0 })

function onCellHover(cell: DayCell, event: MouseEvent) {
  if (cell.future) return
  hoveredCell.value = cell
  tooltipPos.value = { x: event.clientX, y: event.clientY }
}

function onCellLeave() {
  hoveredCell.value = null
}

function formatDateLabel(dateStr: string): string {
  const [y, m, d] = dateStr.split('-').map(Number)
  return new Date(y!, m! - 1, d!).toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' })
}
</script>

<template>
  <div class="heatmap">
    <!-- Month labels, placed by week column -->
    <div class="month-labels">
      <span
        v-for="ml in grid.monthLabels"
        :key="ml.col"
        class="month-label"
        :style="{ left: `${(ml.col / weeks) * 100}%` }"
      >
        {{ ml.label }}
      </span>
    </div>

    <div class="grid-body">
      <div class="day-labels">
        <span v-for="(label, i) in DAY_LABELS" :key="i" class="day-label">{{ label }}</span>
      </div>

      <div class="cells-area" :style="{ gridTemplateColumns: `repeat(${weeks}, minmax(0, 1fr))` }">
        <div v-for="(week, wi) in grid.cells" :key="wi" class="week-column">
          <span
            v-for="(cell, di) in week"
            :key="di"
            class="day-cell"
            :class="{ 'day-cell--future': cell.future }"
            :style="{ backgroundColor: cellColor(cell) }"
            @mouseenter="onCellHover(cell, $event)"
            @mouseleave="onCellLeave"
          />
        </div>
      </div>
    </div>

    <!-- Tooltip -->
    <Teleport to="body">
      <div
        v-if="hoveredCell"
        class="heatmap-tooltip"
        :style="{
          left: `${tooltipPos.x + 12}px`,
          top: `${tooltipPos.y - 8}px`,
        }"
      >
        <span class="tooltip-date">{{ formatDateLabel(hoveredCell.date) }}</span>
        <span
          v-if="hoveredCell.hasData"
          class="tooltip-pnl"
          :class="hoveredCell.pnl >= 0 ? 'text-success' : 'text-error'"
        >
          {{ formatCurrency(hoveredCell.pnl) }} · {{ hoveredCell.trades }} {{ hoveredCell.trades === 1 ? 'trade' : 'trades' }}
        </span>
        <span v-else class="tooltip-pnl text-muted">No trades</span>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.heatmap {
  position: relative;
  min-width: 0;
  /* 53 columns of at most ~24 px: the cells grow with the width up to that. */
  max-width: 1420px;
}

.month-labels {
  position: relative;
  height: 20px;
  margin-left: 30px;
}

.month-label {
  position: absolute;
  top: 0;
  font-size: 12px;
  line-height: 20px;
  color: var(--qa-text-muted);
  font-weight: 500;
  white-space: nowrap;
}

.grid-body {
  display: grid;
  grid-template-columns: 24px minmax(0, 1fr);
  gap: 6px;
  align-items: stretch;
}

.day-labels {
  display: grid;
  grid-template-rows: repeat(7, minmax(0, 1fr));
  gap: 3px;
}

.day-label {
  display: flex;
  align-items: center;
  justify-content: center;
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Fira Code', monospace;
  font-size: 12px;
  font-weight: 500;
  color: var(--qa-text-secondary);
  line-height: 1;
}

.cells-area {
  display: grid;
  gap: 3px;
  min-width: 0;
}

.week-column {
  display: grid;
  grid-template-rows: repeat(7, minmax(0, 1fr));
  gap: 3px;
  min-width: 0;
}

.day-cell {
  display: block;
  width: 100%;
  aspect-ratio: 1 / 1;
  border-radius: 3px;
  cursor: pointer;
  transition: opacity var(--qa-transition);
}

.day-cell--future {
  cursor: default;
}

.day-cell:not(.day-cell--future):hover {
  opacity: 0.8;
  outline: 1px solid var(--qa-text-muted);
}

.heatmap-tooltip {
  position: fixed;
  z-index: 200;
  background: var(--qa-bg-card);
  border: 1px solid var(--qa-border);
  border-radius: var(--qa-radius);
  padding: 6px 10px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  pointer-events: none;
  white-space: nowrap;
}

.tooltip-date {
  font-size: 12px;
  color: var(--qa-text);
  font-weight: 500;
}

.tooltip-pnl {
  font-size: 12px;
  font-weight: 600;
}
</style>
