<script setup lang="ts">
import { Maximize2 } from 'lucide-vue-next'
import type {
  IChartApi,
  ISeriesApi,
  ISeriesPrimitive,
  ISeriesPrimitivePaneRenderer,
  Logical,
  Time,
  UTCTimestamp,
  MouseEventParams,
} from 'lightweight-charts'
import {
  bandFor,
  bindings,
  formatValue,
  type Point,
  type HistoryRow,
} from '#terminal/utils/metrics'
import {
  HIGHLIGHT_ZONES,
  highlightRuns,
  type HighlightMatch,
  type HighlightZone,
} from '#terminal/utils/metric-highlight'

const props = defineProps<{
  title: string
  unit: string
  points: Point[]
  prices: HistoryRow[]
  asset: string
  quote?: string
  priceProvider?: string
  rangeDays: number
  /** Days outside the match are greyed out on both panes; null shows the plain chart. */
  highlight?: HighlightMatch | null
}>()
const priceHost = ref<HTMLDivElement | null>(null),
  metricHost = ref<HTMLDivElement | null>(null)
const overlay = ref(false),
  candles = ref(false)
const priceScaleOverride = ref<boolean | null>(null)
const indicatorScaleOverride = ref<boolean | null>(null)
const displayedPoints = computed(() => {
  const end = Math.max(props.points.at(-1)?.time ?? 0, props.prices.at(-1)?.time ?? 0)
  const start =
    props.rangeDays === 0
      ? Math.max(props.points[0]?.time ?? 0, props.prices[0]?.time ?? 0)
      : end - (props.rangeDays - 1) * 86400
  return {
    metric: props.points.filter((p) => p.time >= start).map((p) => p.value),
    price: props.prices
      .filter((p) => p.time >= start)
      .map((p) => p.values.close!)
      .filter((v) => Number.isFinite(v) && v > 0),
  }
})
const needsLog = (values: number[], ratio: number) =>
  values.length > 1 && Math.min(...values) > 0 && Math.max(...values) / Math.min(...values) > ratio
const logarithmic = computed(
  () => priceScaleOverride.value ?? needsLog(displayedPoints.value.price, 100),
)
const indicatorLogarithmic = computed(
  () =>
    canLogIndicator.value &&
    (indicatorScaleOverride.value ?? needsLog(displayedPoints.value.metric, 20)),
)
const canLogIndicator = computed(
  () =>
    displayedPoints.value.metric.length > 0 &&
    displayedPoints.value.metric.every((value) => value > 0),
)
const canCandle = computed(() =>
  props.prices.some(
    (p) =>
      Number.isFinite(p.values.open) &&
      Number.isFinite(p.values.high) &&
      Number.isFinite(p.values.low),
  ),
)
watch(canCandle, (allowed) => {
  if (!allowed) candles.value = false
})
watch(
  () => [props.title, props.asset, props.quote, props.rangeDays],
  () => {
    priceScaleOverride.value = null
    indicatorScaleOverride.value = null
  },
)
const hoverTime = ref<number | null>(null),
  hoverPrice = ref<number | null>(null),
  hoverMetric = ref<number | null>(null)
const chartError = ref<string | null>(null)
let priceChart: IChartApi | null = null,
  metricChart: IChartApi | null = null
let priceSeries: ISeriesApi<'Line'> | ISeriesApi<'Candlestick'> | null = null,
  metricSeries: ISeriesApi<'Line'> | null = null
let observer: ResizeObserver | null = null,
  disposed = false,
  synchronizing = false
const stamp = (time: number) => time as UTCTimestamp
const PRICE_COLOR = '#e7ad53',
  METRIC_COLOR = '#70b5e8',
  UP_COLOR = '#35b6a0',
  DOWN_COLOR = '#ed7884',
  DIMMED_COLOR = 'rgba(128, 132, 145, 0.42)',
  THRESHOLD_COLOR = '#b9a7e6'

/** Tints matching day runs from edge to edge of their bars, behind the series. */
function zonePrimitive(runs: [number, number, HighlightZone][]): ISeriesPrimitive<Time> {
  let chart: IChartApi | null = null
  const renderer: ISeriesPrimitivePaneRenderer = {
    draw() {},
    drawBackground(target) {
      const scale = chart?.timeScale()
      if (!scale) return
      // Fractional logical indices map to 0 in this version: use bar centres ± half a bar.
      const x = (index: number) => scale.logicalToCoordinate(index as Logical)
      const origin = x(0),
        next = x(1)
      if (origin == null || next == null) return
      const half = (next - origin) / 2
      target.useBitmapCoordinateSpace(({ context, bitmapSize, horizontalPixelRatio }) => {
        for (const [first, last, zone] of runs) {
          context.fillStyle = HIGHLIGHT_ZONES[zone].tint
          // At least 2px, so a single day stays findable on a multi-year view.
          const centre = (x(first)! + x(last)!) / 2
          const span = Math.max(2, x(last)! - x(first)! + 2 * half)
          const left = Math.round((centre - span / 2) * horizontalPixelRatio)
          const width = Math.max(1, Math.round(span * horizontalPixelRatio))
          if (left > bitmapSize.width || left + width < 0) continue
          context.fillRect(left, 0, width, bitmapSize.height)
        }
      })
    },
  }
  return {
    attached: (param) => {
      chart = param.chart as IChartApi
    },
    detached: () => {
      chart = null
    },
    paneViews: () => [{ zOrder: () => 'bottom', renderer: () => renderer }],
  }
}
const priceDate = computed(() =>
  props.prices.at(-1)?.time
    ? new Date(props.prices.at(-1)!.time * 1000).toISOString().slice(0, 10)
    : '—',
)
const lastPrice = computed(() => props.prices.at(-1)?.values.close)
const lastMetric = computed(() => props.points.at(-1)?.value)
const hovered = computed(() => hoverTime.value != null)
const dateLabel = computed(() =>
  hoverTime.value
    ? new Date(hoverTime.value * 1000).toISOString().slice(0, 10)
    : 'Latest observations',
)
const selectedBand = computed(() =>
  bandFor(props.title, hoverMetric.value ?? lastMetric.value ?? NaN),
)

function fit() {
  const latest = Math.max(props.points.at(-1)?.time ?? 0, props.prices.at(-1)?.time ?? 0)
  const earliest =
    props.rangeDays === 0
      ? Math.max(props.points[0]?.time ?? 0, props.prices[0]?.time ?? 0)
      : Math.min(props.points[0]?.time ?? latest, props.prices[0]?.time ?? latest)
  if (!latest || earliest >= latest) return
  const range = {
    from: stamp(
      props.rangeDays === 0 ? earliest : Math.max(earliest, latest - (props.rangeDays - 1) * 86400),
    ),
    to: stamp(latest),
  }
  priceChart?.timeScale().setVisibleRange(range)
  if (!overlay.value) metricChart?.timeScale().setVisibleRange(range)
}

function disposeCharts() {
  priceChart?.remove()
  metricChart?.remove()
  priceChart = null
  metricChart = null
  priceSeries = null
  metricSeries = null
}

function resize() {
  const width = priceHost.value?.clientWidth
  if (!width) return
  // Preserve a manually zoomed date range while resizing both synchronized panes.
  const range = priceChart?.timeScale().getVisibleRange()
  synchronizing = true
  try {
    priceChart?.applyOptions({ width, height: priceHost.value!.clientHeight })
    if (metricHost.value?.clientHeight) {
      metricChart?.applyOptions({
        width,
        height: metricHost.value.clientHeight,
      })
    }
    if (range) {
      priceChart?.timeScale().setVisibleRange(range)
      if (!overlay.value) metricChart?.timeScale().setVisibleRange(range)
    }
  } finally {
    synchronizing = false
  }
}

async function render(keepRange = false) {
  if (!priceHost.value || !metricHost.value || disposed) return
  const lc = await import('lightweight-charts')
  if (disposed || !priceHost.value || !metricHost.value) return
  // Changing the highlight keeps a manually zoomed period instead of refitting.
  const keptRange = keepRange ? priceChart?.timeScale().getVisibleRange() : null
  disposeCharts()
  hoverTime.value = null
  hoverPrice.value = null
  hoverMetric.value = null
  const css = getComputedStyle(priceHost.value)
  const text = css.getPropertyValue('--text-secondary').trim() || '#a0a0aa'
  const border = css.getPropertyValue('--border').trim() || '#393941'
  const options = {
    layout: {
      background: { type: lc.ColorType.Solid, color: 'transparent' },
      textColor: text,
      fontFamily: 'ui-monospace, monospace',
      fontSize: 12,
    },
    grid: {
      vertLines: { visible: false },
      horzLines: { color: border, style: lc.LineStyle.Dotted },
    },
    rightPriceScale: {
      borderVisible: false,
      minimumWidth: 88,
      scaleMargins: { top: 0.07, bottom: 0.07 },
    },
    timeScale: {
      borderColor: border,
      minBarSpacing: 0.05,
      rightOffset: 3,
      lockVisibleTimeRangeOnResize: true,
    },
    crosshair: { mode: lc.CrosshairMode.Normal },
    localization: { locale: 'en-US', dateFormat: 'yyyy-MM-dd' },
  }
  priceChart = lc.createChart(priceHost.value, {
    ...options,
    width: priceHost.value.clientWidth,
    height: priceHost.value.clientHeight,
    timeScale: { ...options.timeScale, visible: overlay.value },
  })
  metricChart = overlay.value
    ? null
    : lc.createChart(metricHost.value, {
        ...options,
        width: metricHost.value.clientWidth || priceHost.value.clientWidth,
        height: metricHost.value.clientHeight || 1,
      })
  priceChart.priceScale('right').applyOptions({
    mode: logarithmic.value ? lc.PriceScaleMode.Logarithmic : lc.PriceScaleMode.Normal,
  })
  // Both panes need the same calendar, including today's sentiment when the
  // latest completed price candle is yesterday. Whitespace preserves that day
  // without fabricating a price or clipping the latest indicator off-screen.
  const availableDates = [...props.prices.map((p) => p.time), ...props.points.map((p) => p.time)]
  const firstDate = Math.min(...availableDates)
  const lastDate = Math.max(...availableDates)
  const calendar = availableDates.length
    ? Array.from(
        { length: Math.round((lastDate - firstDate) / 86400) + 1 },
        (_, i) => firstDate + i * 86400,
      )
    : []
  // Whitespace reserves dates, but the line renderer otherwise connects across it.
  // A transparent outgoing segment leaves an actual gap between observations.
  const gapStarts = (dates: number[]) =>
    new Set(dates.filter((time, i) => dates[i + 1] != null && dates[i + 1]! - time > 86400))
  const priceGaps = gapStarts(props.prices.map((p) => p.time))
  const metricGaps = gapStarts(props.points.map((p) => p.time))
  const candleMap = new Map(props.prices.map((row) => [row.time, row.values]))
  const highlight = props.highlight ?? null
  const dimmed = (time: number) => highlight != null && !highlight.matched.has(time)
  // A point's colour paints its outgoing segment: keep a segment lit when either end
  // matches, so single-day matches and threshold crossings stay visible.
  const dimmedSegment = (time: number) => dimmed(time) && dimmed(time + 86400)
  if (candles.value) {
    const series = priceChart.addCandlestickSeries({
      upColor: UP_COLOR,
      downColor: DOWN_COLOR,
      borderVisible: false,
      wickUpColor: UP_COLOR,
      wickDownColor: DOWN_COLOR,
      priceLineVisible: false,
      priceFormat: {
        type: 'custom',
        formatter: (v: number) => formatValue(v, 'USD'),
      },
    })
    series.setData(
      calendar.map((time) => {
        const { open, high, low, close } = candleMap.get(time) ?? {}
        return [open, high, low, close].every((v) => Number.isFinite(v) && v! > 0)
          ? {
              time: stamp(time),
              open: open!,
              high: high!,
              low: low!,
              close: close!,
              ...(dimmed(time) ? { color: DIMMED_COLOR, wickColor: DIMMED_COLOR } : {}),
            }
          : { time: stamp(time) }
      }),
    )
    priceSeries = series
  } else {
    const series = priceChart.addLineSeries({
      color: PRICE_COLOR,
      crosshairMarkerBackgroundColor: PRICE_COLOR,
      lineWidth: 2,
      priceLineVisible: false,
      priceFormat: {
        type: 'custom',
        formatter: (v: number) => formatValue(v, 'USD'),
      },
    })
    series.setData(
      calendar.map((time) => {
        const value = candleMap.get(time)?.close
        return Number.isFinite(value)
          ? {
              time: stamp(time),
              value: value!,
              color: priceGaps.has(time)
                ? 'transparent'
                : dimmedSegment(time)
                  ? DIMMED_COLOR
                  : PRICE_COLOR,
            }
          : { time: stamp(time) }
      }),
    )
    priceSeries = series
  }
  const host = metricChart ?? priceChart
  metricSeries = host.addLineSeries({
    color: METRIC_COLOR,
    crosshairMarkerBackgroundColor: METRIC_COLOR,
    lineWidth: 2,
    priceScaleId: overlay.value ? 'left' : 'right',
    priceLineVisible: false,
    priceFormat: {
      type: 'custom',
      formatter: (v: number) => formatValue(v, props.unit),
    },
  })
  host.priceScale(overlay.value ? 'left' : 'right').applyOptions({
    mode:
      indicatorLogarithmic.value && canLogIndicator.value
        ? lc.PriceScaleMode.Logarithmic
        : lc.PriceScaleMode.Normal,
  })
  if (overlay.value)
    priceChart.priceScale('left').applyOptions({
      visible: true,
      borderVisible: false,
      minimumWidth: 78,
      scaleMargins: { top: 0.07, bottom: 0.07 },
    })
  // Whitespace at missing dates prevents a visual implication of observed values.
  const points = new Map(props.points.map((p) => [p.time, p.value]))
  metricSeries.setData(
    calendar.map((time) =>
      points.has(time)
        ? {
            time: stamp(time),
            value: points.get(time)!,
            color: metricGaps.has(time)
              ? 'transparent'
              : dimmedSegment(time)
                ? DIMMED_COLOR
                : METRIC_COLOR,
          }
        : { time: stamp(time) },
    ),
  )
  if (highlight) {
    // Both panes share the calendar, so its indices are each pane's logical bar indices.
    const runs = highlightRuns(calendar, highlight.matched)
    priceSeries.attachPrimitive(zonePrimitive(runs))
    if (metricChart) metricSeries.attachPrimitive(zonePrimitive(runs))
    for (const threshold of highlight.thresholds)
      metricSeries.createPriceLine({
        price: threshold.value,
        // Open-ended thresholds share their zone's colour; a closed range stays neutral.
        color: threshold.zone === 'single' ? THRESHOLD_COLOR : HIGHLIGHT_ZONES[threshold.zone].color,
        lineWidth: 1,
        lineStyle: lc.LineStyle.LargeDashed,
        axisLabelVisible: true,
        title: threshold.label,
      })
  }
  const bands = bindings[props.title]?.bands ?? []
  for (const band of bands) {
    if (
      !Number.isFinite(band.min) ||
      (band.min === 0 && props.title !== 'Net Unrealized Profit/Loss')
    )
      continue
    metricSeries.createPriceLine({
      price: band.min,
      color: band.color,
      lineWidth: 1,
      lineStyle: lc.LineStyle.Dashed,
      axisLabelVisible: true,
      title: '',
    })
  }
  const priceMap = new Map(props.prices.map((p) => [p.time, p.values.close]))
  function crosshair(event: MouseEventParams<Time>, fromPrice: boolean) {
    if (synchronizing) return
    synchronizing = true
    try {
      if (typeof event.time !== 'number' || !event.point) {
        hoverTime.value = null
        hoverPrice.value = null
        hoverMetric.value = null
        if (fromPrice) metricChart?.clearCrosshairPosition()
        else priceChart?.clearCrosshairPosition()
        return
      }
      hoverTime.value = event.time
      hoverPrice.value = priceMap.get(event.time) ?? null
      hoverMetric.value = points.get(event.time) ?? null
      if (!overlay.value) {
        if (fromPrice && hoverMetric.value != null && metricSeries)
          metricChart?.setCrosshairPosition(hoverMetric.value, event.time, metricSeries)
        else if (fromPrice) metricChart?.clearCrosshairPosition()
        if (!fromPrice && hoverPrice.value != null && priceSeries)
          priceChart?.setCrosshairPosition(hoverPrice.value, event.time, priceSeries)
        else if (!fromPrice) priceChart?.clearCrosshairPosition()
      }
    } finally {
      synchronizing = false
    }
  }
  priceChart.subscribeCrosshairMove((e) => crosshair(e, true))
  metricChart?.subscribeCrosshairMove((e) => crosshair(e, false))
  // Sync by dates rather than bar indices: providers have different histories.
  if (metricChart) {
    for (const [source, target] of [
      [priceChart, metricChart],
      [metricChart, priceChart],
    ] as const) {
      source.timeScale().subscribeVisibleTimeRangeChange((range) => {
        if (synchronizing || overlay.value || !range) return
        synchronizing = true
        try {
          target.timeScale().setVisibleRange(range)
        } finally {
          synchronizing = false
        }
      })
    }
  }
  if (keptRange) {
    synchronizing = true
    try {
      priceChart.timeScale().setVisibleRange(keptRange)
      metricChart?.timeScale().setVisibleRange(keptRange)
    } finally {
      synchronizing = false
    }
  } else fit()
}

let generation = 0,
  refit = false
async function draw(keepRange = false) {
  // A new metric or data in the same tick as a highlight change still refits.
  if (!keepRange) refit = true
  const current = ++generation
  await nextTick()
  if (current !== generation || disposed) return
  const keep = !refit
  refit = false
  try {
    await render(keep)
    chartError.value = null
  } catch (e) {
    chartError.value = e instanceof Error ? e.message : String(e)
  }
}
watch(
  () => [
    props.title,
    props.points,
    props.prices,
    overlay.value,
    candles.value,
    logarithmic.value,
    indicatorLogarithmic.value,
  ],
  () => draw(),
)
watch(
  () => props.highlight,
  () => draw(true),
)
watch(() => props.rangeDays, fit)
onMounted(() => {
  void draw()
  observer = new ResizeObserver(resize)
  if (priceHost.value) observer.observe(priceHost.value)
  if (metricHost.value) observer.observe(metricHost.value)
  window.addEventListener('resize', resize)
})
onBeforeUnmount(() => {
  disposed = true
  generation++
  observer?.disconnect()
  window.removeEventListener('resize', resize)
  disposeCharts()
})
</script>

<template>
  <div class="history-chart" :class="{ 'with-overlay': overlay }">
    <div class="chart-toolbar">
      <div class="chart-legend">
        <i class="price-dot" /> {{ asset }}/{{ quote || 'USDT' }}
        <strong>{{ formatValue(hovered ? hoverPrice : lastPrice, 'USD') }}</strong
        ><small v-if="!hovered"
          >{{ priceProvider || 'Binance daily close' }} ·
          <template v-if="rangeDays === 0 && prices[0]"
            >{{ new Date(prices[0].time * 1000).toISOString().slice(0, 10) }} → </template
          >{{ priceDate }} UTC</small
        >
      </div>
      <div class="chart-actions">
        <slot name="actions" />
        <button
          :class="{ active: candles }"
          :aria-pressed="candles"
          :disabled="!canCandle"
          :title="
            canCandle
              ? 'Daily candles'
              : 'Choose BTC/USDT for exchange candles; the USD reference is a daily price series'
          "
          @click="candles = !candles"
        >
          Candles
        </button>
        <button
          :class="{ active: logarithmic }"
          :aria-pressed="logarithmic"
          :title="
            priceScaleOverride === null
              ? 'Automatic scale for this period; click to override'
              : 'Toggle logarithmic price scale'
          "
          @click="priceScaleOverride = !logarithmic"
        >
          Log price
        </button>
        <button :class="{ active: overlay }" :aria-pressed="overlay" @click="overlay = !overlay">
          Overlay
        </button>
        <button
          :class="{ active: indicatorLogarithmic }"
          :aria-pressed="indicatorLogarithmic"
          :disabled="!canLogIndicator"
          :title="
            indicatorScaleOverride === null
              ? 'Automatic scale for this period; log requires positive readings'
              : 'Toggle logarithmic indicator scale'
          "
          @click="indicatorScaleOverride = !indicatorLogarithmic"
        >
          Log indicator
        </button>
        <button title="Fit selected period" aria-label="Fit selected period" @click="fit">
          <Maximize2 :size="13" />
        </button>
      </div>
    </div>
    <div v-if="chartError" class="chart-error" role="alert">
      Chart unavailable: {{ chartError }}
    </div>
    <div class="chart-canvas">
      <div ref="priceHost" class="canvas-host" />
      <span v-if="!prices.length" class="empty-label">No price observations available</span>
    </div>
    <div class="indicator-legend">
      <span
        ><i class="metric-dot" /> {{ title }}
        <strong>{{ formatValue(hovered ? hoverMetric : lastMetric, unit) }}</strong
        ><small
          v-if="selectedBand && (!hovered || hoverMetric !== null)"
          :style="{ color: selectedBand.color }"
          >{{ selectedBand.label }}</small
        ></span
      ><time>{{ dateLabel }} · UTC</time>
    </div>
    <div v-show="!overlay" class="chart-canvas">
      <div ref="metricHost" class="canvas-host" />
      <span v-if="!points.length" class="empty-label">No indicator observations available</span>
    </div>
    <div class="chart-footnote">
      <span
        >Scroll to zoom · Drag to pan ·
        {{ overlay ? 'Indicator left / price right' : 'Synchronized dates & crosshair' }}</span
      ><a href="https://www.tradingview.com/" target="_blank" rel="noopener noreferrer"
        >Charts by TradingView</a
      >
    </div>
  </div>
</template>

<style scoped>
.history-chart {
  position: relative;
  display: grid;
  grid-template-rows: auto minmax(0, 3fr) auto minmax(0, 2fr) auto;
  min-width: 0;
  min-height: 0;
  height: 100%;
  overflow: hidden;
}
.history-chart.with-overlay {
  grid-template-rows: auto minmax(0, 1fr) auto auto;
}
.chart-toolbar,
.indicator-legend,
.chart-footnote {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 8px 12px;
  flex-wrap: wrap;
}
.chart-toolbar {
  min-height: 42px;
}
.chart-legend,
.indicator-legend > span {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
  font-size: 12px;
  color: var(--text-secondary);
}
strong {
  font-family: var(--qss-font-mono);
  color: var(--text-primary);
  font-weight: 600;
  font-size: 13px;
}
i {
  display: inline-block;
  height: 7px;
  width: 7px;
  border-radius: 50%;
  flex-shrink: 0;
}
.price-dot {
  background: #e7ad53;
}
.metric-dot {
  background: #70b5e8;
}
.chart-actions {
  display: flex;
  gap: 4px;
}
button {
  display: flex;
  align-items: center;
  border: 1px solid transparent;
  background: transparent;
  color: var(--text-secondary);
  border-radius: 4px;
  padding: 5px 7px;
  font-size: 12px;
  cursor: pointer;
}
button:hover,
button.active {
  background: var(--surface-2);
  border-color: var(--border);
  color: var(--text-primary);
}
button:disabled {
  opacity: 0.4;
  cursor: default;
}
.indicator-legend {
  border-top: 1px solid var(--border);
  min-height: 34px;
}
.indicator-legend time,
small {
  font-size: 11px;
  color: var(--text-secondary);
}
.chart-canvas {
  position: relative;
  min-width: 0;
  min-height: 0;
}
.canvas-host {
  position: absolute;
  inset: 0;
}
.empty-label {
  position: absolute;
  inset: 40% 15% auto;
  text-align: center;
  color: var(--muted);
  font-size: 13px;
  pointer-events: none;
}
.chart-footnote {
  border-top: 1px solid var(--border);
  color: var(--muted);
  font-size: 11px;
  padding: 6px 12px;
}
a {
  color: var(--text-secondary);
  text-decoration: none;
}
.chart-error {
  position: absolute;
  z-index: 2;
  top: 46px;
  left: 12px;
  right: 12px;
  padding: 12px;
  background: var(--surface-1);
  color: var(--negative);
  font-size: 12px;
}
button:focus-visible,
a:focus-visible {
  outline: 2px solid #70b5e8;
  outline-offset: 2px;
}
</style>
