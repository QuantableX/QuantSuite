<script setup lang="ts">
import { TrendingUp, TrendingDown, Flame, Target } from 'lucide-vue-next'
import { useJournalStore } from '#terminal/stores/journal'

const store = useJournalStore()

// Equity curve data (cumulative PnL by trade date)
const equityCurve = computed(() => {
  const sorted = [...store.closedTrades].sort((a, b) => a.date.localeCompare(b.date))
  let cumPnl = 0
  return sorted.map(t => {
    cumPnl += t.pnl ?? 0
    return { date: t.date, pnl: t.pnl ?? 0, cumPnl: Math.round(cumPnl * 100) / 100 }
  })
})

const equityMax = computed(() => {
  if (equityCurve.value.length === 0) return 1
  const vals = equityCurve.value.map(e => Math.abs(e.cumPnl))
  return Math.max(...vals, 1)
})

// PnL by pair
const pnlByPair = computed(() => {
  const map = new Map<string, number>()
  for (const t of store.closedTrades) {
    map.set(t.pair, (map.get(t.pair) ?? 0) + (t.pnl ?? 0))
  }
  return [...map.entries()]
    .map(([pair, pnl]) => ({ pair, pnl: Math.round(pnl * 100) / 100 }))
    .sort((a, b) => b.pnl - a.pnl)
})

const pnlByPairMax = computed(() => {
  if (pnlByPair.value.length === 0) return 1
  return Math.max(...pnlByPair.value.map(p => Math.abs(p.pnl)), 1)
})

// PnL by day of week
const pnlByDay = computed(() => {
  const days = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat']
  const map = new Map<number, { pnl: number; count: number }>()
  for (const t of store.closedTrades) {
    const dow = new Date(t.date).getDay()
    const cur = map.get(dow) ?? { pnl: 0, count: 0 }
    cur.pnl += t.pnl ?? 0
    cur.count++
    map.set(dow, cur)
  }
  return days.map((name, i) => ({
    name,
    pnl: Math.round((map.get(i)?.pnl ?? 0) * 100) / 100,
    count: map.get(i)?.count ?? 0,
  }))
})

const pnlByDayMax = computed(() => {
  return Math.max(...pnlByDay.value.map(d => Math.abs(d.pnl)), 1)
})

// PnL by strategy
const pnlByStrategy = computed(() => {
  const map = new Map<string, { pnl: number; count: number; wins: number }>()
  for (const t of store.closedTrades) {
    const key = t.strategy || 'No Strategy'
    const cur = map.get(key) ?? { pnl: 0, count: 0, wins: 0 }
    cur.pnl += t.pnl ?? 0
    cur.count++
    if ((t.pnl ?? 0) > 0) cur.wins++
    map.set(key, cur)
  }
  return [...map.entries()]
    .map(([strategy, data]) => ({ strategy, ...data, pnl: Math.round(data.pnl * 100) / 100, winRate: data.count > 0 ? Math.round(data.wins / data.count * 100) : 0 }))
    .sort((a, b) => b.pnl - a.pnl)
})

// Monthly performance
const monthlyPerformance = computed(() => {
  const map = new Map<string, { trades: number; wins: number; pnl: number; best: number; worst: number }>()
  for (const t of store.closedTrades) {
    const month = t.date.slice(0, 7)
    const cur = map.get(month) ?? { trades: 0, wins: 0, pnl: 0, best: -Infinity, worst: Infinity }
    cur.trades++
    cur.pnl += t.pnl ?? 0
    if ((t.pnl ?? 0) > 0) cur.wins++
    if ((t.pnl ?? 0) > cur.best) cur.best = t.pnl ?? 0
    if ((t.pnl ?? 0) < cur.worst) cur.worst = t.pnl ?? 0
    map.set(month, cur)
  }
  return [...map.entries()]
    .map(([month, data]) => ({
      month,
      ...data,
      pnl: Math.round(data.pnl * 100) / 100,
      best: data.best === -Infinity ? 0 : Math.round(data.best * 100) / 100,
      worst: data.worst === Infinity ? 0 : Math.round(data.worst * 100) / 100,
      winRate: data.trades > 0 ? Math.round(data.wins / data.trades * 100) : 0,
    }))
    .sort((a, b) => b.month.localeCompare(a.month))
})

// Emotion analysis
const emotionAnalysis = computed(() => {
  const map = new Map<string, { count: number; pnl: number; wins: number }>()
  const labels: Record<string, string> = { confident: '😎 Confident', neutral: '😐 Neutral', fearful: '😰 Fearful', greedy: '🤑 Greedy', frustrated: '😤 Frustrated' }
  for (const t of store.closedTrades) {
    const cur = map.get(t.emotion) ?? { count: 0, pnl: 0, wins: 0 }
    cur.count++
    cur.pnl += t.pnl ?? 0
    if ((t.pnl ?? 0) > 0) cur.wins++
    map.set(t.emotion, cur)
  }
  return [...map.entries()].map(([emotion, data]) => ({
    emotion,
    label: labels[emotion] || emotion,
    ...data,
    pnl: Math.round(data.pnl * 100) / 100,
    avgPnl: data.count > 0 ? Math.round(data.pnl / data.count * 100) / 100 : 0,
    winRate: data.count > 0 ? Math.round(data.wins / data.count * 100) : 0,
  })).sort((a, b) => b.avgPnl - a.avgPnl)
})

function formatPnl(v: number): string {
  return (v >= 0 ? '+$' : '-$') + Math.abs(v).toFixed(2)
}
</script>

<template>
  <div class="space-y-4">
    <!-- Equity Curve -->
    <div class="card p-3">
      <h3 class="text-xs font-semibold mb-3" style="color: var(--text-primary)">Equity Curve</h3>
      <div v-if="equityCurve.length" class="flex items-end gap-[2px] h-32">
        <div
          v-for="(point, i) in equityCurve"
          :key="i"
          class="flex-1 rounded-t-sm transition-all relative group"
          :style="{
            height: Math.max((Math.abs(point.cumPnl) / equityMax) * 100, 2) + '%',
            backgroundColor: point.cumPnl >= 0 ? 'var(--positive)' : 'var(--negative)',
            opacity: 0.6 + (i / equityCurve.length) * 0.4,
            alignSelf: point.cumPnl >= 0 ? 'flex-end' : 'flex-end',
          }"
        >
          <div class="absolute bottom-full left-1/2 -translate-x-1/2 mb-1 px-2 py-1 rounded text-[9px] font-mono whitespace-nowrap opacity-0 group-hover:opacity-100 transition-opacity z-10" style="background-color: var(--surface-3); color: var(--text-primary)">
            {{ point.date }} · {{ formatPnl(point.cumPnl) }}
          </div>
        </div>
      </div>
      <div v-else class="h-32 flex items-center justify-center">
        <span class="text-xs" style="color: var(--muted)">No closed trades yet</span>
      </div>
    </div>

    <!-- Win/Loss Distribution + Streaks -->
    <div class="grid grid-cols-2 gap-3">
      <!-- Win/Loss -->
      <div class="card p-3">
        <h3 class="text-xs font-semibold mb-3" style="color: var(--text-primary)">Win / Loss Distribution</h3>
        <div class="flex items-center gap-2 mb-2">
          <span class="text-[11px] font-semibold" style="color: var(--positive)">{{ store.winningTrades.length }}W</span>
          <div class="flex-1 h-4 rounded overflow-hidden flex" style="background-color: var(--surface-2)">
            <div
              class="h-full transition-all"
              :style="{ width: store.winRate + '%', backgroundColor: 'var(--positive)' }"
            />
            <div
              class="h-full transition-all"
              :style="{ width: (100 - store.winRate) + '%', backgroundColor: 'var(--negative)' }"
            />
          </div>
          <span class="text-[11px] font-semibold" style="color: var(--negative)">{{ store.losingTrades.length }}L</span>
        </div>
        <div class="grid grid-cols-2 gap-2 text-center">
          <div>
            <div class="text-sm font-bold font-mono" style="color: var(--positive)">{{ formatPnl(store.avgWin) }}</div>
            <div class="text-[10px]" style="color: var(--muted)">Avg Win</div>
          </div>
          <div>
            <div class="text-sm font-bold font-mono" style="color: var(--negative)">{{ formatPnl(store.avgLoss) }}</div>
            <div class="text-[10px]" style="color: var(--muted)">Avg Loss</div>
          </div>
        </div>
      </div>

      <!-- Streaks -->
      <div class="card p-3">
        <h3 class="text-xs font-semibold mb-3" style="color: var(--text-primary)">Streaks</h3>
        <div class="space-y-2">
          <div class="flex items-center justify-between">
            <span class="text-[11px]" style="color: var(--text-secondary)">Current</span>
            <span class="text-xs font-bold font-mono" :style="{ color: store.streaks.currentType === 'win' ? 'var(--positive)' : 'var(--negative)' }">
              {{ store.streaks.current }} {{ store.streaks.currentType === 'win' ? 'Win' : 'Loss' }}
            </span>
          </div>
          <div class="flex items-center justify-between">
            <span class="text-[11px]" style="color: var(--text-secondary)">Best Win Streak</span>
            <span class="text-xs font-bold font-mono" style="color: var(--positive)">{{ store.streaks.longestWin }}</span>
          </div>
          <div class="flex items-center justify-between">
            <span class="text-[11px]" style="color: var(--text-secondary)">Worst Loss Streak</span>
            <span class="text-xs font-bold font-mono" style="color: var(--negative)">{{ store.streaks.longestLoss }}</span>
          </div>
          <div class="flex items-center justify-between">
            <span class="text-[11px]" style="color: var(--text-secondary)">Max Drawdown</span>
            <span class="text-xs font-bold font-mono" style="color: var(--negative)">-${{ store.maxDrawdown.toFixed(2) }}</span>
          </div>
        </div>
      </div>
    </div>

    <!-- PnL by Pair -->
    <div class="card p-3">
      <h3 class="text-xs font-semibold mb-3" style="color: var(--text-primary)">PnL by Pair</h3>
      <div class="space-y-2">
        <div v-for="item in pnlByPair" :key="item.pair" class="flex items-center gap-3">
          <span class="text-[11px] font-semibold w-20 shrink-0" style="color: var(--text-primary)">{{ item.pair }}</span>
          <div class="flex-1 flex items-center">
            <div class="w-1/2 flex justify-end">
              <div
                v-if="item.pnl < 0"
                class="h-4 rounded-l transition-all"
                :style="{ width: (Math.abs(item.pnl) / pnlByPairMax * 100) + '%', backgroundColor: 'var(--negative)', minWidth: '2px' }"
              />
            </div>
            <div class="w-px h-4 shrink-0" style="background-color: var(--border)" />
            <div class="w-1/2">
              <div
                v-if="item.pnl > 0"
                class="h-4 rounded-r transition-all"
                :style="{ width: (Math.abs(item.pnl) / pnlByPairMax * 100) + '%', backgroundColor: 'var(--positive)', minWidth: '2px' }"
              />
            </div>
          </div>
          <span class="text-[11px] font-mono font-semibold w-20 text-right shrink-0" :style="{ color: item.pnl >= 0 ? 'var(--positive)' : 'var(--negative)' }">
            {{ formatPnl(item.pnl) }}
          </span>
        </div>
      </div>
    </div>

    <!-- PnL by Day of Week -->
    <div class="card p-3">
      <h3 class="text-xs font-semibold mb-3" style="color: var(--text-primary)">PnL by Day of Week</h3>
      <div class="flex items-end gap-2 h-24">
        <div v-for="day in pnlByDay" :key="day.name" class="flex-1 flex flex-col items-center gap-1">
          <div class="text-[9px] font-mono" :style="{ color: day.pnl >= 0 ? 'var(--positive)' : 'var(--negative)' }">
            {{ day.count > 0 ? formatPnl(day.pnl) : '' }}
          </div>
          <div
            class="w-full rounded-t transition-all"
            :style="{
              height: day.count > 0 ? Math.max((Math.abs(day.pnl) / pnlByDayMax * 60), 3) + 'px' : '3px',
              backgroundColor: day.count > 0 ? (day.pnl >= 0 ? 'var(--positive)' : 'var(--negative)') : 'var(--surface-2)',
            }"
          />
          <span class="text-[10px]" style="color: var(--muted)">{{ day.name }}</span>
        </div>
      </div>
    </div>

    <!-- PnL by Strategy -->
    <div class="card p-3">
      <h3 class="text-xs font-semibold mb-3" style="color: var(--text-primary)">PnL by Strategy</h3>
      <div class="space-y-2">
        <div v-for="item in pnlByStrategy" :key="item.strategy" class="flex items-center gap-3">
          <span class="text-[11px] font-medium w-28 shrink-0" style="color: var(--text-primary)">{{ item.strategy }}</span>
          <div class="flex-1 h-3 rounded overflow-hidden" style="background-color: var(--surface-2)">
            <div
              class="h-full rounded transition-all"
              :style="{ width: Math.min(item.winRate, 100) + '%', backgroundColor: item.winRate >= 50 ? 'var(--positive)' : 'var(--negative)' }"
            />
          </div>
          <span class="text-[10px] font-mono w-10 text-right shrink-0" style="color: var(--text-secondary)">{{ item.winRate }}%</span>
          <span class="text-[11px] font-mono font-semibold w-20 text-right shrink-0" :style="{ color: item.pnl >= 0 ? 'var(--positive)' : 'var(--negative)' }">
            {{ formatPnl(item.pnl) }}
          </span>
        </div>
      </div>
    </div>

    <!-- Monthly Performance -->
    <div class="card p-3">
      <h3 class="text-xs font-semibold mb-3" style="color: var(--text-primary)">Monthly Performance</h3>
      <div class="overflow-x-auto">
        <table class="w-full">
          <thead>
            <tr class="text-[10px] uppercase tracking-wider" style="color: var(--muted)">
              <th class="text-left py-1 pr-3">Month</th>
              <th class="text-right py-1 px-2">Trades</th>
              <th class="text-right py-1 px-2">Win Rate</th>
              <th class="text-right py-1 px-2">PnL</th>
              <th class="text-right py-1 px-2">Best</th>
              <th class="text-right py-1 pl-2">Worst</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="m in monthlyPerformance" :key="m.month" style="border-top: 1px solid var(--border)">
              <td class="text-[11px] font-medium py-1.5 pr-3" style="color: var(--text-primary)">{{ m.month }}</td>
              <td class="text-[11px] font-mono py-1.5 px-2 text-right" style="color: var(--text-secondary)">{{ m.trades }}</td>
              <td class="text-[11px] font-mono py-1.5 px-2 text-right" :style="{ color: m.winRate >= 50 ? 'var(--positive)' : 'var(--negative)' }">{{ m.winRate }}%</td>
              <td class="text-[11px] font-mono font-semibold py-1.5 px-2 text-right" :style="{ color: m.pnl >= 0 ? 'var(--positive)' : 'var(--negative)' }">{{ formatPnl(m.pnl) }}</td>
              <td class="text-[11px] font-mono py-1.5 px-2 text-right" style="color: var(--positive)">{{ formatPnl(m.best) }}</td>
              <td class="text-[11px] font-mono py-1.5 pl-2 text-right" style="color: var(--negative)">{{ formatPnl(m.worst) }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <!-- Emotion Analysis -->
    <div class="card p-3">
      <h3 class="text-xs font-semibold mb-3" style="color: var(--text-primary)">Emotion Analysis</h3>
      <div class="space-y-2">
        <div v-for="item in emotionAnalysis" :key="item.emotion" class="flex items-center gap-3 py-1" style="border-bottom: 1px solid var(--border)">
          <span class="text-[11px] w-28 shrink-0" style="color: var(--text-primary)">{{ item.label }}</span>
          <span class="text-[10px] font-mono w-8 text-center" style="color: var(--text-secondary)">{{ item.count }}</span>
          <span class="text-[10px] font-mono w-10 text-right" :style="{ color: item.winRate >= 50 ? 'var(--positive)' : 'var(--negative)' }">{{ item.winRate }}%</span>
          <span class="text-[11px] font-mono font-semibold w-16 text-right" :style="{ color: item.avgPnl >= 0 ? 'var(--positive)' : 'var(--negative)' }">
            {{ formatPnl(item.avgPnl) }}
          </span>
          <span class="text-[10px]" style="color: var(--muted)">avg/trade</span>
        </div>
      </div>
    </div>
  </div>
</template>
