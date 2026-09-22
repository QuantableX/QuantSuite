<script setup lang="ts">
import { bindings, DAY, type Point } from '#terminal/utils/metrics'
import type { CardReading } from '#terminal/utils/metric-card'

const props = defineProps<{
  title: string
  points: Point[]
  reading: CardReading
  spot: Point[]
}>()
const recent = computed(() => {
  const last = props.points.at(-1)
  return last ? props.points.filter((p) => p.time >= last.time - 59 * DAY) : []
})
const value = computed(() => props.points.at(-1)?.value ?? 0)
const historyDays = computed(() =>
  recent.value.length
    ? Math.round((recent.value.at(-1)!.time - recent.value[0]!.time) / DAY) + 1
    : 0,
)
const polar = (percent: number, radius = 44) => {
  const angle = Math.PI * (1 - percent / 100)
  return { x: 80 + radius * Math.cos(angle), y: 53 - radius * Math.sin(angle) }
}
function arc(from: number, to: number) {
  const a = polar(from),
    b = polar(to)
  return `M ${a.x} ${a.y} A 44 44 0 0 1 ${b.x} ${b.y}`
}
const needle = computed(() => polar(Math.min(100, Math.max(0, value.value)), 35))
const bandMax = computed(() =>
  Math.max(props.title === 'MVRV Ratio' ? 4 : 5, Math.ceil(value.value)),
)
const bandX = (v: number) => 6 + Math.min(1, Math.max(0, v / bandMax.value)) * 148
const segments = computed(() =>
  (bindings[props.title]?.bands ?? []).map((b) => ({
    x: bandX(Math.max(0, b.min)),
    width: Math.max(0, bandX(Math.min(bandMax.value, b.max)) - bandX(Math.max(0, b.min))),
    color: b.color,
  })),
)
const ticks = computed(() =>
  props.title === 'MVRV Ratio' ? [0, 1, 2, 3.5] : [0, 1, Math.min(4, bandMax.value)],
)
const geometry = computed(() => {
  const points = ['flow', 'activity'].includes(props.reading.kind)
    ? recent.value.filter((p) => p.time >= (recent.value.at(-1)?.time ?? 0) - 29 * DAY)
    : recent.value
  if (!points.length)
    return {
      line: '',
      fill: '',
      reference: '',
      baselineY: 30,
      last: null as { x: number; y: number } | null,
      bars: [] as { x: number; y: number; height: number; color: string }[],
    }
  const start = points[0]!.time,
    end = points.at(-1)!.time
  const sameDates = new Set(points.map((p) => p.time))
  const reference =
    props.reading.kind === 'comparison' ? props.spot.filter((p) => sameDates.has(p.time)) : []
  const values = [...points, ...reference].map((p) => p.value)
  if (props.reading.baseline != null && !reference.length) values.push(props.reading.baseline)
  let lo = Math.min(...values),
    hi = Math.max(...values)
  if (props.reading.kind === 'flow') {
    hi = Math.max(Math.abs(lo), Math.abs(hi), 0.0001)
    lo = -hi
  }
  const padding = (hi - lo) * 0.12 || Math.abs(hi) * 0.05 || 1
  lo -= padding
  hi += padding
  const x = (time: number) => 5 + ((time - start) / (end - start || DAY)) * 150
  const y = (v: number) => 52 - ((v - lo) / (hi - lo)) * 44
  const baselineY = y(props.reading.kind === 'comparison' ? lo : (props.reading.baseline ?? lo))
  const line = (data: Point[]) =>
    data
      .map(
        (p, i) =>
          `${i && p.time - data[i - 1]!.time === DAY ? 'L' : 'M'}${x(p.time).toFixed(2)},${y(p.value).toFixed(2)}`,
      )
      .join(' ')
  // Close each contiguous segment separately instead of filling across missing days.
  const groups: Point[][] = []
  for (const point of points) {
    if (!groups.length || point.time - groups.at(-1)!.at(-1)!.time !== DAY) groups.push([])
    groups.at(-1)!.push(point)
  }
  const fill = groups
    .map(
      (group) =>
        `${line(group)} L${x(group.at(-1)!.time)},${baselineY} L${x(group[0]!.time)},${baselineY} Z`,
    )
    .join(' ')
  const daily = points.filter((p) => p.time >= end - 29 * DAY)
  const barX = (time: number) => 5 + ((time - (end - 29 * DAY)) / (29 * DAY)) * 150
  const bars = daily.map((p) => ({
    x: barX(p.time) - 1.5,
    y: Math.min(y(p.value), baselineY),
    height: Math.max(1, Math.abs(y(p.value) - baselineY)),
    color:
      props.reading.kind === 'flow'
        ? p.value < 0
          ? props.title === 'Exchange Netflow'
            ? '#35b6a0'
            : '#b49bea'
          : '#e7ad53'
        : p.value >= (props.reading.baseline ?? 0)
          ? '#70b5e8'
          : '#b49bea',
  }))
  return {
    line: line(points),
    fill,
    reference: line(reference),
    baselineY,
    bars,
    last: { x: x(end), y: y(points.at(-1)!.value) },
  }
})
</script>

<template>
  <svg
    v-if="points.length"
    class="card-visual"
    :data-kind="reading.kind"
    viewBox="0 0 160 66"
    role="img"
    :aria-label="`${title}: ${reading.label}. ${reading.reference}`"
  >
    <title>{{ reading.label }}. {{ reading.reference }}</title>
    <g v-if="reading.kind === 'gauge'">
      <path
        v-for="band in bindings[title]?.bands"
        :key="band.min"
        :d="arc(band.min + 0.8, Math.min(100, band.max) - 0.8)"
        :stroke="band.color"
        fill="none"
        stroke-width="7"
      />
      <line x1="80" y1="53" :x2="needle.x" :y2="needle.y" class="needle" />
      <circle cx="80" cy="53" r="3" :fill="reading.color" />
      <text x="29" y="65">0</text>
      <line x1="80" x2="80" y1="3" y2="13" class="reference-line" />
      <text x="123" y="65">100</text>
    </g>
    <g v-else-if="reading.kind === 'share'">
      <rect x="5" y="25" width="150" height="12" rx="4" fill="currentColor" opacity="0.15" />
      <rect
        x="5"
        y="25"
        :width="150 * (reading.share ?? 0)"
        height="12"
        rx="4"
        :fill="reading.color"
      />
      <text x="5" y="52">0%</text>
      <text x="155" y="52" text-anchor="end">100%</text>
      <text x="80" y="65" text-anchor="middle">
        Share of {{ title.includes('Holder') ? 'cohort' : 'supply' }}
      </text>
    </g>
    <g v-else-if="reading.kind === 'bands'">
      <rect
        v-for="segment in segments"
        :key="segment.x"
        :x="segment.x"
        y="25"
        :width="Math.max(0, segment.width - 1.5)"
        height="9"
        rx="2"
        :fill="segment.color"
        opacity="0.7"
      />
      <line :x1="bandX(1)" :x2="bandX(1)" y1="21" y2="39" class="reference-line" />
      <path
        :d="`M${bandX(value) - 4},15 L${bandX(value) + 4},15 L${bandX(value)},21 Z`"
        :fill="reading.color"
      />
      <circle :cx="bandX(value)" cy="29.5" r="3" class="marker" />
      <text v-for="tick in ticks" :key="tick" :x="bandX(tick)" y="51" text-anchor="middle">
        {{ tick }}
      </text>
      <text x="155" y="64" text-anchor="end">
        {{
          /SOPR/.test(title)
            ? 'Spent / creation value'
            : /MVRV/.test(title)
              ? 'Market / realized value'
              : title === 'Mayer Multiple'
                ? 'Price / 200D average'
                : 'Issuance / annual mean'
        }}
      </text>
    </g>
    <g v-else-if="reading.kind === 'split' && reading.share != null">
      <text x="5" y="17" fill="#35b6a0">
        {{ reading.splitUnit === 'volume' ? 'B' : 'L' }}
        {{ (reading.share * 100).toFixed(0) }}%
      </text>
      <text x="155" y="17" text-anchor="end" fill="#b49bea">
        S {{ ((1 - reading.share) * 100).toFixed(0) }}%
      </text>
      <rect x="5" y="26" :width="150 * reading.share" height="10" rx="2" fill="#35b6a0" />
      <rect
        :x="5 + 150 * reading.share"
        y="26"
        :width="150 * (1 - reading.share)"
        height="10"
        rx="2"
        fill="#b49bea"
      />
      <line x1="80" x2="80" y1="21" y2="42" class="reference-line" />
      <text x="80" y="58" text-anchor="middle">50 / 50 {{ reading.splitUnit ?? 'accounts' }}</text>
    </g>
    <g v-else>
      <template
        v-if="reading.kind === 'flow' || (reading.kind === 'activity' && reading.baseline != null)"
      >
        <rect
          v-for="(bar, index) in geometry.bars"
          :key="index"
          :x="bar.x"
          :y="bar.y"
          width="3"
          :height="bar.height"
          rx="0.6"
          :fill="bar.color"
          opacity="0.8"
        />
      </template>
      <template v-else>
        <path :d="geometry.fill" :fill="reading.color" opacity="0.12" />
        <path :d="geometry.line" :stroke="reading.color" fill="none" stroke-width="1.6" />
        <circle
          v-if="geometry.last"
          :cx="geometry.last.x"
          :cy="geometry.last.y"
          r="2"
          :fill="reading.color"
        />
      </template>
      <path
        v-if="geometry.reference"
        :d="geometry.reference"
        fill="none"
        stroke="#e7ad53"
        stroke-width="1.6"
      />
      <g v-else-if="reading.baseline != null">
        <line x1="3" x2="157" :y1="geometry.baselineY" :y2="geometry.baselineY" class="baseline" />
        <text
          x="155"
          :y="Math.max(8, geometry.baselineY - 3)"
          text-anchor="end"
          class="baseline-label"
        >
          {{ reading.baseline === 0 ? '0' : 'AVG' }}
        </text>
      </g>
      <text x="5" y="65">
        {{ reading.kind === 'flow' || reading.kind === 'activity' ? '30D' : `${historyDays}D` }}
      </text>
      <text
        v-if="reading.kind === 'comparison' && geometry.reference"
        x="155"
        y="65"
        text-anchor="end"
      >
        <tspan :fill="reading.color">Cost</tspan>
        /
        <tspan fill="#e7ad53">BTC spot</tspan>
      </text>
      <text v-else x="155" y="65" text-anchor="end">
        {{
          reading.kind === 'flow'
            ? title.includes('Funding')
              ? title === 'Annualized Funding'
                ? 'Simple APR'
                : 'Daily net payment'
              : 'Inflow / outflow'
            : reading.baseline === 0
              ? 'Profit / loss'
              : ''
        }}
      </text>
    </g>
  </svg>
  <div v-else class="visual-empty">No history</div>
</template>

<style scoped>
.card-visual {
  display: block;
  width: 160px;
  max-width: 48%;
  height: 66px;
  overflow: visible;
  flex-shrink: 1;
}
text {
  font-size: 12px;
  font-family: var(--qss-font-mono);
}
text:not([fill]) {
  fill: var(--text-secondary);
}
.needle,
.reference-line {
  stroke: var(--text-primary);
  stroke-width: 1.5;
}
.marker {
  fill: var(--surface-1);
  stroke: var(--text-primary);
  stroke-width: 1.5;
}
.baseline {
  stroke: var(--text-secondary);
  stroke-dasharray: 3 3;
  stroke-opacity: 0.65;
}
.baseline-label {
  paint-order: stroke;
  stroke: var(--surface-1);
  stroke-width: 3;
  stroke-linejoin: round;
}
.visual-empty {
  font-size: 11px;
  color: var(--muted);
}
</style>
