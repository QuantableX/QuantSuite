<script setup lang="ts">
import { Info, X } from 'lucide-vue-next'
import {
  bandFor,
  bindings,
  formatValue,
  forwardOutcomes,
  percentile,
  type Point,
} from '#terminal/utils/metrics'

const props = defineProps<{
  title: string
  points: Point[]
  prices: Point[]
  asset: string
  quote?: string
}>()
const methodDialog = ref<HTMLDialogElement | null>(null)
const latest = computed(() => props.points.at(-1))
const binding = computed(() => bindings[props.title]!)
const band = computed(() => bandFor(props.title, latest.value?.value ?? NaN))
const summary = computed(
  () =>
    band.value?.explanation.split(/(?<=[.!?])\s/)[0] ||
    'Compare this reading with price and previous outcomes.',
)
const rank = computed(() => (latest.value ? percentile(props.points, latest.value.value) : null))
const lower = ref<number | string>(''),
  upper = ref<number | string>('')
const validRange = computed(
  () =>
    lower.value !== '' &&
    upper.value !== '' &&
    Number.isFinite(Number(lower.value)) &&
    Number.isFinite(Number(upper.value)) &&
    Number(lower.value) <= Number(upper.value),
)
watch(
  () => [props.title, latest.value?.value],
  () => {
    const value = latest.value?.value
    if (value == null) {
      lower.value = ''
      upper.value = ''
      return
    }
    const finiteValues = props.points.map((p) => p.value)
    const lo = Math.min(...finiteValues),
      hi = Math.max(...finiteValues)
    const width = Math.max(Math.abs(value) * 0.1, (hi - lo) * 0.025, 0.001)
    lower.value = Number(
      (band.value && Number.isFinite(band.value.min) ? band.value.min : value - width).toFixed(4),
    )
    upper.value = Number(
      (band.value && Number.isFinite(band.value.max) ? band.value.max : value + width).toFixed(4),
    )
  },
  { immediate: true },
)
const outcomes = computed(() =>
  forwardOutcomes(
    props.points,
    props.prices,
    (v) => validRange.value && v >= Number(lower.value) && v <= Number(upper.value),
  ),
)
const matchedCount = computed(() =>
  validRange.value
    ? props.points.filter((p) => p.value >= Number(lower.value) && p.value <= Number(upper.value))
        .length
    : 0,
)
const pct = (v: number | null) => (v == null ? '—' : `${v > 0 ? '+' : ''}${v.toFixed(1)}%`)
</script>

<template>
  <aside class="score-context">
    <div class="context-heading"><Info :size="13" /><span>READ THE SCORE</span></div>
    <div class="score-reading">
      <strong>{{ formatValue(latest?.value, binding.unit) }}</strong
      ><span>{{ binding.unit }}</span>
    </div>
    <div v-if="band && latest" class="regime-label" :style="{ color: band.color }">
      {{ band.label }}
    </div>
    <p>
      {{
        latest
          ? summary
          : 'The source has no observations for this period. Interpretation becomes available once data is loaded.'
      }}
    </p>
    <div v-if="rank !== null" class="percentile">
      <div>
        <span>Percentile in selected period</span
        ><strong>{{ rank.toFixed(0) }}<small> / 100</small></strong>
      </div>
      <div class="percentile-track"><i :style="{ width: `${rank}%` }" /></div>
    </div>
    <div class="outcomes-heading"><span>AFTER SIMILAR SCORES</span></div>
    <div class="score-range">
      <label
        >From<input
          v-model="lower"
          type="number"
          step="any"
          aria-label="Minimum historical score" /></label
      ><span>–</span
      ><label
        >To<input v-model="upper" type="number" step="any" aria-label="Maximum historical score"
      /></label>
    </div>
    <p v-if="!validRange && latest" class="invalid-range">Enter a valid minimum and maximum.</p>
    <div class="sample-label">
      {{ matchedCount }} matching days · {{ asset }}/{{ quote || 'USDT' }}
    </div>
    <table aria-label="Historical forward returns after similar scores">
      <thead>
        <tr>
          <th>After</th>
          <th>Median</th>
          <th title="Median return after all observed days">All days</th>
          <th>Positive</th>
          <th>n</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="outcome in outcomes" :key="outcome.horizon">
          <td>{{ outcome.horizon }}D</td>
          <td
            :class="{
              positive: (outcome.median ?? 0) > 0,
              negative: (outcome.median ?? 0) < 0,
            }"
          >
            {{ pct(outcome.median) }}
          </td>
          <td class="baseline-cell">{{ pct(outcome.baseline) }}</td>
          <td>
            {{ outcome.positive === null ? '—' : `${outcome.positive.toFixed(0)}%` }}
          </td>
          <td>{{ outcome.count }}</td>
        </tr>
      </tbody>
    </table>
    <p class="method-note">
      Next-day price entry · Overlapping samples<br />n = completed outcomes · Historical, not
      predictive
    </p>
    <div class="source-footer">
      <a :href="binding.url" target="_blank" rel="noopener noreferrer">{{ binding.provider }} ↗</a
      ><button @click="methodDialog?.showModal()">Method & source</button>
    </div>
    <dialog ref="methodDialog" class="method-dialog" aria-label="Analysis method and data source">
      <header>
        <strong>Method & source</strong
        ><button aria-label="Close method details" @click="methodDialog?.close()">
          <X :size="16" />
        </button>
      </header>
      <p>{{ binding.formula }}</p>
      <p v-if="band">{{ band.explanation }}</p>
      <p>{{ binding.note }}</p>
      <p>
        Returns start at the next UTC day's observed price: the Coin Metrics USD reference or the
        selected exchange's close. Entry and exit require observations on the exact dates. Samples
        overlap; n counts completed outcomes. Provider history may be revised. This is descriptive
        history, not a forecast or a publication-time backtest.
      </p>
      <a :href="binding.url" target="_blank" rel="noopener noreferrer">{{ binding.provider }} ↗</a>
    </dialog>
  </aside>
</template>

<style scoped>
.score-context {
  padding: 12px 14px;
  min-height: 0;
  overflow: visible;
  align-self: start;
  border-left: 1px solid var(--border);
  background: color-mix(in srgb, var(--surface-2) 30%, var(--surface-1));
  min-width: 0;
}
.context-heading,
.outcomes-heading {
  display: flex;
  align-items: center;
  gap: 7px;
  color: var(--text-secondary);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.09em;
}
.score-reading {
  display: flex;
  gap: 8px;
  align-items: baseline;
  margin: 8px 0 5px;
}
.score-reading strong {
  font-size: 32px;
  line-height: 1;
  font-weight: 600;
  font-family: var(--qss-font-mono);
  letter-spacing: -1px;
}
.score-reading span {
  font-size: 12px;
  color: var(--muted);
}
.regime-label {
  font-size: 11px;
  font-weight: 600;
  margin-bottom: 6px;
}
p {
  color: var(--text-secondary);
  font-size: 12px;
  line-height: 1.55;
  margin: 6px 0 8px;
}
.percentile {
  margin: 8px 0 10px;
}
.percentile > div:first-child {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  font-size: 12px;
  color: var(--text-secondary);
}
.percentile strong {
  color: var(--text-primary);
  font-family: var(--qss-font-mono);
}
.percentile small {
  font-size: 11px;
  color: var(--muted);
}
.percentile-track {
  margin-top: 8px;
  height: 3px;
  border-radius: 3px;
  background: var(--surface-3);
}
.percentile-track i {
  display: block;
  height: 100%;
  background: #70b5e8;
  border-radius: 3px;
}
.outcomes-heading {
  justify-content: space-between;
  border-top: 1px solid var(--border);
  padding-top: 10px;
}
.score-range {
  display: flex;
  gap: 8px;
  align-items: end;
  margin: 9px 0 6px;
}
.score-range label {
  font-size: 11px;
  color: var(--muted);
  flex: 1;
  min-width: 0;
}
.score-range input {
  width: 100%;
  min-width: 0;
  display: block;
  margin-top: 4px;
  padding: 6px 7px;
  background: var(--surface-0);
  color: var(--text-primary);
  border: 1px solid var(--border);
  border-radius: 4px;
  font-family: var(--qss-font-mono);
  font-size: 11px;
}
.score-range > span {
  margin-bottom: 7px;
  color: var(--muted);
}
.sample-label {
  font-size: 11px;
  color: var(--muted);
  margin: 8px 0;
}
table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12px;
  font-family: var(--qss-font-mono);
}
th {
  color: var(--muted);
  font-size: 11px;
  font-weight: 400;
  padding: 7px 0;
}
td {
  padding: 7px 0;
  border-top: 1px solid var(--border);
}
th,
td {
  text-align: right;
}
th:first-child,
td:first-child {
  text-align: left;
}
.positive {
  color: var(--positive);
}
.negative,
.invalid-range {
  color: var(--negative);
}
.baseline-cell {
  color: var(--text-secondary);
  font-size: 11px;
}
.method-note {
  font-size: 11px;
  margin: 10px 0;
}
.source-footer {
  border-top: 1px solid var(--border);
  padding-top: 9px;
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  gap: 7px;
  font-size: 11px;
  line-height: 1.6;
  color: var(--muted);
}
.source-footer a,
.method-dialog a {
  text-decoration: none;
  color: var(--text-secondary);
}
.source-footer button,
.method-dialog button {
  border: 0;
  background: transparent;
  color: #70b5e8;
  font-size: 11px;
  cursor: pointer;
  padding: 0;
}
.method-dialog {
  color: var(--text-primary);
  background: var(--surface-1);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 20px;
  width: min(480px, 85vw);
  max-height: 85vh;
}
.method-dialog::backdrop {
  background: #0008;
}
.method-dialog header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 15px;
}
.method-dialog a {
  font-size: 12px;
}
@media (max-width: 1050px) {
  .score-context {
    border-left: 0;
    border-top: 1px solid var(--border);
  }
}
</style>
