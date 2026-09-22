<script setup lang="ts">
import type { BacktestResult, PerformanceMetrics } from '#systems/types'

const props = defineProps<{ result: BacktestResult }>()

type Fmt = 'mult' | 'pct' | 'ratio'
/** `mult` compares against 1.0 (break-even), `sign` against 0. */
type Tone = 'mult' | 'sign'

const rows: {
  key: keyof PerformanceMetrics
  label: string
  fmt: Fmt
  tone?: Tone
  highlight?: boolean
}[] = [
  { key: 'netReturnMultiplier', label: 'Net Return', fmt: 'mult', tone: 'mult', highlight: true },
  { key: 'maxDrawdownPct', label: 'Max Drawdown', fmt: 'pct' },
  { key: 'sharpe', label: 'Sharpe', fmt: 'ratio' },
  { key: 'sortino', label: 'Sortino', fmt: 'ratio' },
  { key: 'omega', label: 'Omega', fmt: 'ratio' },
  { key: 'meanAllPct', label: 'Mean Return', fmt: 'pct', tone: 'sign' },
  { key: 'stddevAllPct', label: 'Volatility', fmt: 'pct' },
]

interface Column {
  key: string
  label: string
  metrics: PerformanceMetrics
  strong?: boolean
}

const columns = computed<Column[]>(() => {
  // One column per trend signal, the configured indicator first and tinted;
  // a lone run keeps the plain "Strategy" heading.
  const runs = props.result.strategies ?? []
  const cols: Column[] = runs.map((run, i) => ({
    key: `strategy:${run.key}`,
    label: runs.length > 1 || props.result.skippedStrategies?.length ? run.label : 'Strategy',
    metrics: run.metricsStrategy,
    strong: i === 0,
  }))
  for (const [name, m] of Object.entries(props.result.metricsBenchmarks)) {
    cols.push({ key: `bench:${name}`, label: name, metrics: m })
  }
  for (const [sym, m] of Object.entries(props.result.metricsBuyAndHold)) {
    cols.push({ key: `bh:${sym}`, label: `B&H ${sym}`, metrics: m })
  }
  return cols
})

function onTableWheel(event: WheelEvent) {
  // Preserve zoom and native horizontal trackpad / Shift-wheel gestures.
  if (event.ctrlKey || event.shiftKey || Math.abs(event.deltaX) >= Math.abs(event.deltaY)) return

  const container = event.currentTarget as HTMLElement
  const maxScroll = container.scrollWidth - container.clientWidth
  if (maxScroll <= 0) return

  const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? container.clientWidth : 1
  const nextScroll = Math.max(0, Math.min(maxScroll, container.scrollLeft + event.deltaY * unit))
  // Let the page scroll normally when the table has reached either edge.
  if (nextScroll === container.scrollLeft) return

  event.preventDefault()
  container.scrollLeft = nextScroll
}

function fmt(value: number | null | undefined, kind: Fmt): string {
  if (value === null || value === undefined || Number.isNaN(value)) return '—'
  if (kind === 'mult') return `${value.toFixed(2)}×`
  if (kind === 'pct') return `${value.toFixed(2)}%`
  return value.toFixed(2)
}

function tone(value: number | null | undefined, kind?: Tone): string {
  if (!kind || value === null || value === undefined || Number.isNaN(value)) return ''
  const pivot = kind === 'mult' ? 1 : 0
  if (value > pivot) return 'qs-metrics__val--up'
  if (value < pivot) return 'qs-metrics__val--down'
  return ''
}
</script>

<template>
  <div class="card qs-metrics">
    <h3 class="qs-metrics__title">Performance Metrics</h3>
    <div class="qs-metrics__scroll" @wheel="onTableWheel">
      <table class="qs-metrics__table">
        <thead>
          <tr>
            <th class="qs-metrics__rowlabel" />
            <th v-for="c in columns" :key="c.key" class="qs-metrics__col" :class="{ 'qs-metrics__col--strong': c.strong }">
              {{ c.label }}
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in rows" :key="row.key" :class="{ 'qs-metrics__row--highlight': row.highlight }">
            <th class="qs-metrics__rowlabel">{{ row.label }}</th>
            <td
              v-for="c in columns"
              :key="`${row.key}-${c.key}`"
              class="qs-metrics__val mono"
              :class="[{ 'qs-metrics__val--strong': c.strong }, tone(c.metrics[row.key], row.tone)]"
            >
              {{ fmt(c.metrics[row.key], row.fmt) }}
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<style scoped>
.qs-metrics {
  padding: 12px 14px;
}

.qs-metrics__title {
  margin: 0 0 10px;
  font-size: 13px;
  font-weight: 600;
}

.qs-metrics__scroll {
  overflow-x: auto;
}

.qs-metrics__table {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
}

.qs-metrics__col,
.qs-metrics__rowlabel {
  padding: 6px 10px;
  text-align: right;
  color: var(--qs-text-secondary);
  font-weight: 600;
  white-space: nowrap;
}

.qs-metrics__col {
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  border-bottom: 1px solid var(--qs-border);
}

.qs-metrics__rowlabel {
  text-align: left;
  position: sticky;
  left: 0;
  z-index: 1;
  background: var(--qs-bg-card);
}

/* The strategy column is what every other column is read against, so it gets a
   standing tint instead of relying on weight alone. */
.qs-metrics__col--strong,
.qs-metrics__val--strong {
  color: var(--qs-text);
  background: color-mix(in srgb, var(--qs-accent) 10%, transparent);
}

.qs-metrics__val {
  padding: 6px 10px;
  text-align: right;
  color: var(--qs-text-secondary);
  border-top: 1px solid var(--qs-border-subtle);
}

.qs-metrics__table tbody tr:hover .qs-metrics__val {
  background: var(--qs-bg-hover);
}

.qs-metrics__table tbody tr:hover .qs-metrics__val--strong {
  background: color-mix(in srgb, var(--qs-accent) 10%, var(--qs-bg-hover));
}

.qs-metrics__val--up {
  color: var(--qs-success);
}

.qs-metrics__val--down {
  color: var(--qs-error);
}

.qs-metrics__row--highlight .qs-metrics__val,
.qs-metrics__row--highlight .qs-metrics__rowlabel {
  font-weight: 700;
}

.qs-metrics__row--highlight .qs-metrics__rowlabel {
  color: var(--qs-text);
}
</style>
