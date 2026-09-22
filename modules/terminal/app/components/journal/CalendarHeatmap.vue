<script setup lang="ts">
import { useJournalStore } from '#terminal/stores/journal'

const store = useJournalStore()

const emit = defineEmits<{
  selectDate: [date: string]
}>()

const hoveredDay = ref<{ date: string; trades: number; pnl: number; x: number; y: number } | null>(null)

// Build 3 months of days
const calendarDays = computed(() => {
  const today = new Date()
  const start = new Date(today)
  start.setMonth(start.getMonth() - 3)
  start.setDate(1)

  // Build PnL map from trades
  const pnlMap = new Map<string, { pnl: number; count: number }>()
  for (const t of store.closedTrades) {
    const cur = pnlMap.get(t.date) ?? { pnl: 0, count: 0 }
    cur.pnl += t.pnl ?? 0
    cur.count++
    pnlMap.set(t.date, cur)
  }
  // Include open trades for count
  for (const t of store.openTrades) {
    const cur = pnlMap.get(t.date) ?? { pnl: 0, count: 0 }
    cur.count++
    pnlMap.set(t.date, cur)
  }

  const days: Array<{ date: string; pnl: number; count: number; blank?: boolean }> = []

  // Add blank days for first week alignment
  const firstDay = start.getDay()
  for (let i = 0; i < firstDay; i++) {
    days.push({ date: '', pnl: 0, count: 0, blank: true })
  }

  const current = new Date(start)
  while (current <= today) {
    const dateStr = current.toISOString().slice(0, 10)
    const data = pnlMap.get(dateStr)
    days.push({
      date: dateStr,
      pnl: data ? Math.round(data.pnl * 100) / 100 : 0,
      count: data?.count ?? 0,
    })
    current.setDate(current.getDate() + 1)
  }

  return days
})

// Organize into weeks (columns) for GitHub-style layout
const weeks = computed(() => {
  const result: Array<typeof calendarDays.value> = []
  let week: typeof calendarDays.value = []
  for (const day of calendarDays.value) {
    week.push(day)
    if (week.length === 7) {
      result.push(week)
      week = []
    }
  }
  if (week.length > 0) result.push(week)
  return result
})

const maxAbsPnl = computed(() => {
  const vals = calendarDays.value.filter(d => d.count > 0).map(d => Math.abs(d.pnl))
  return Math.max(...vals, 1)
})

function getDayColor(day: typeof calendarDays.value[0]): string {
  if (day.blank) return 'transparent'
  if (day.count === 0) return 'var(--surface-2)'
  const intensity = Math.min(Math.abs(day.pnl) / maxAbsPnl.value, 1)
  const alpha = 0.25 + intensity * 0.75
  if (day.pnl >= 0) return `rgba(34, 197, 94, ${alpha})`
  return `rgba(239, 68, 68, ${alpha})`
}

function onHover(day: typeof calendarDays.value[0], event: MouseEvent) {
  if (day.blank || !day.date) return
  hoveredDay.value = {
    date: day.date,
    trades: day.count,
    pnl: day.pnl,
    x: event.clientX,
    y: event.clientY,
  }
}

function onLeave() {
  hoveredDay.value = null
}

function onClickDay(day: typeof calendarDays.value[0]) {
  if (day.blank || !day.date || day.count === 0) return
  emit('selectDate', day.date)
}

// Month labels
const monthLabels = computed(() => {
  const labels: Array<{ name: string; offset: number }> = []
  let lastMonth = -1
  let weekIdx = 0
  for (const week of weeks.value) {
    for (const day of week) {
      if (!day.blank && day.date) {
        const month = new Date(day.date).getMonth()
        if (month !== lastMonth) {
          labels.push({ name: new Date(day.date).toLocaleString('en', { month: 'short' }), offset: weekIdx })
          lastMonth = month
        }
        break
      }
    }
    weekIdx++
  }
  return labels
})
</script>

<template>
  <div class="card p-3">
    <h3 class="text-xs font-semibold mb-3" style="color: var(--text-primary)">Trading Calendar</h3>

    <!-- Month labels -->
    <div class="flex gap-[3px] mb-1 ml-7">
      <template v-for="(label, i) in monthLabels" :key="i">
        <span
          class="text-[9px] absolute"
          :style="{ color: 'var(--muted)', left: (label.offset * 15 + 28) + 'px' }"
        />
      </template>
      <div class="flex gap-[3px]">
        <div v-for="(week, wi) in weeks" :key="wi" class="relative" style="width: 12px">
          <span
            v-if="monthLabels.find(l => l.offset === wi)"
            class="text-[9px] absolute -top-3.5 left-0 whitespace-nowrap"
            style="color: var(--muted)"
          >
            {{ monthLabels.find(l => l.offset === wi)?.name }}
          </span>
        </div>
      </div>
    </div>

    <!-- Day labels + Grid -->
    <div class="flex gap-1">
      <!-- Day of week labels -->
      <div class="flex flex-col gap-[3px] pt-0">
        <span v-for="(d, i) in ['S', 'M', 'T', 'W', 'T', 'F', 'S']" :key="i" class="text-[9px] h-3 flex items-center justify-end pr-1" style="color: var(--muted)">
          {{ i % 2 === 1 ? d : '' }}
        </span>
      </div>

      <!-- Weeks grid -->
      <div class="flex gap-[3px]">
        <div v-for="(week, wi) in weeks" :key="wi" class="flex flex-col gap-[3px]">
          <div
            v-for="(day, di) in week"
            :key="di"
            class="w-3 h-3 rounded-sm transition-all"
            :class="{ 'cursor-pointer hover:brightness-125': !day.blank && day.count > 0 }"
            :style="{ backgroundColor: getDayColor(day) }"
            @mouseenter="onHover(day, $event)"
            @mouseleave="onLeave"
            @click="onClickDay(day)"
          />
        </div>
      </div>
    </div>

    <!-- Legend -->
    <div class="flex items-center gap-2 mt-3">
      <span class="text-[10px]" style="color: var(--muted)">Less</span>
      <div class="w-3 h-3 rounded-sm" style="background-color: var(--surface-2)" />
      <div class="w-3 h-3 rounded-sm" style="background-color: rgba(239, 68, 68, 0.5)" />
      <div class="w-3 h-3 rounded-sm" style="background-color: rgba(239, 68, 68, 0.9)" />
      <div class="w-3 h-3 rounded-sm" style="background-color: rgba(34, 197, 94, 0.5)" />
      <div class="w-3 h-3 rounded-sm" style="background-color: rgba(34, 197, 94, 0.9)" />
      <span class="text-[10px]" style="color: var(--muted)">More</span>
      <span class="text-[10px] ml-2" style="color: var(--muted)">Loss</span>
      <span class="text-[10px]" style="color: var(--muted)">/</span>
      <span class="text-[10px]" style="color: var(--muted)">Profit</span>
    </div>

    <!-- Tooltip -->
    <Teleport to="body">
      <div
        v-if="hoveredDay"
        class="fixed z-50 px-2.5 py-1.5 rounded text-[10px] pointer-events-none"
        :style="{
          left: hoveredDay.x + 12 + 'px',
          top: hoveredDay.y - 40 + 'px',
          backgroundColor: 'var(--surface-3)',
          border: '1px solid var(--border)',
          color: 'var(--text-primary)',
        }"
      >
        <div class="font-semibold">{{ hoveredDay.date }}</div>
        <div style="color: var(--text-secondary)">{{ hoveredDay.trades }} trade{{ hoveredDay.trades !== 1 ? 's' : '' }}</div>
        <div v-if="hoveredDay.trades > 0" class="font-mono font-semibold" :style="{ color: hoveredDay.pnl >= 0 ? 'var(--positive)' : 'var(--negative)' }">
          {{ hoveredDay.pnl >= 0 ? '+' : '' }}${{ hoveredDay.pnl.toFixed(2) }}
        </div>
      </div>
    </Teleport>
  </div>
</template>
