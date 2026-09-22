<script setup lang="ts">
import type { BacktestResult, EquityPoint } from '#systems/types'
import type { MouseEventParams, Time } from 'lightweight-charts'

const props = defineProps<{ result: BacktestResult }>()

const container = ref<HTMLDivElement | null>(null)
let chart: any = null
let resizeObserver: ResizeObserver | null = null
const seriesMap = new Map<string, any>()
let dataLen = 0
let disposed = false

const logScale = ref(true)
const autoLock = ref(true)
const priceScaleMode = { normal: 0, log: 1 }

interface SeriesDef {
  key: string
  label: string
  color: string
  data: EquityPoint[]
  width: number
  visible: boolean
}

const legend = ref<{ key: string; label: string; color: string; visible: boolean }[]>([])

const tooltip = ref<HTMLElement | null>(null)
const hover = shallowRef<{
  date: string
  side: 'left' | 'right'
  width: number
  height: number
  rows: { key: string; label: string; color: string; value: number | null }[]
} | null>(null)
const equityFormat = new Intl.NumberFormat(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 4 })

function hoverDate(time: Time): string {
  if (typeof time === 'number') return `${new Date(time * 1000).toISOString().slice(0, 16).replace('T', ' ')} UTC`
  if (typeof time === 'string') return time
  return `${time.year}-${String(time.month).padStart(2, '0')}-${String(time.day).padStart(2, '0')}`
}

function clearHover() {
  hover.value = null
}

function onCrosshairMove(event: MouseEventParams<Time>) {
  // Keep the selected date readable while scrolling a long list of values.
  if (tooltip.value?.matches(':hover')) return
  const pane = chart?.paneSize()
  const point = event.point
  if (!pane || event.time === undefined || !point || point.x < 0 || point.y < 0 || point.x >= pane.width || point.y >= pane.height) {
    clearHover()
    return
  }
  const rows = legend.value.filter(item => item.visible).map(item => {
    const data = event.seriesData.get(seriesMap.get(item.key))
    const value = data && 'value' in data && typeof data.value === 'number' && Number.isFinite(data.value) ? data.value : null
    return { key: item.key, label: item.label, color: item.color, value }
  })
  // Read the series' actual value at this x coordinate, never a y-pixel
  // conversion (which would be wrong for the other curves and log scales).
  hover.value = rows.some(row => row.value !== null) ? {
    date: hoverDate(event.time),
    side: point.x < pane.width / 2 ? 'right' : 'left',
    width: pane.width,
    height: pane.height,
    rows,
  } : null
}

const seriesMenuOpen = ref(false)
const seriesMenuRef = ref<HTMLElement | null>(null)
const activeCount = computed(() => legend.value.filter(l => l.visible).length)
onClickOutside(seriesMenuRef, () => { seriesMenuOpen.value = false })

// The configured indicator draws white as before; compared indicators get
// colours that stay apart from the orange/blue benchmarks and the grey B&H.
const strategyColors = ['#e8e8ee', '#c084fc', '#4ade80', '#f472b6', '#facc15', '#2dd4bf', '#fb7185']

function buildDefs(): SeriesDef[] {
  const defs: SeriesDef[] = []
  const runs = props.result.strategies ?? []
  runs.forEach((run, i) => {
    defs.push({
      key: `strategy:${run.key}`,
      label: runs.length > 1 || props.result.skippedStrategies?.length ? run.label : 'Rotation Strategy',
      color: strategyColors[i % strategyColors.length]!,
      data: run.equityStrategy,
      width: 2,
      visible: true,
    })
  })

  const benchColors = ['#ffa502', '#7fd1ff']
  Object.entries(props.result.benchmarks).forEach(([name, data], i) => {
    defs.push({
      key: `bench:${name}`,
      label: name,
      // Modulo over a non-empty literal is always in range; Nuxt 4 enables
      // noUncheckedIndexedAccess, which the standalone app's tsconfig did not.
      color: benchColors[i % benchColors.length]!,
      data,
      width: 1,
      visible: true,
    })
  })

  const bhShades = ['#8a8a93', '#6e6e7a', '#5a5a64', '#9a9aa5', '#76767f']
  Object.entries(props.result.buyAndHold).forEach(([sym, data], i) => {
    defs.push({
      key: `bh:${sym}`,
      label: `B&H ${sym}`,
      color: bhShades[i % bhShades.length]!,
      data,
      width: 1,
      visible: false, // default to only the strategy + benchmarks selected
    })
  })
  return defs
}

async function render() {
  if (!container.value) return
  const lc = await import('lightweight-charts')
  if (disposed || !container.value) return
  if (lc.PriceScaleMode) {
    priceScaleMode.normal = lc.PriceScaleMode.Normal
    priceScaleMode.log = lc.PriceScaleMode.Logarithmic
  }
  const css = getComputedStyle(document.documentElement)
  const textColor = css.getPropertyValue('--qs-text-secondary').trim() || '#9a9aa5'
  const gridColor = css.getPropertyValue('--qs-border').trim() || '#47474f'

  if (!chart) {
    chart = lc.createChart(container.value, {
      layout: {
        background: { type: lc.ColorType.Solid, color: 'transparent' },
        textColor,
        fontFamily: 'ui-monospace, monospace',
      },
      grid: {
        vertLines: { color: 'transparent' },
        horzLines: { color: gridColor, style: lc.LineStyle?.Dotted ?? 1 },
      },
      rightPriceScale: {
        borderColor: gridColor,
        mode: logScale.value ? priceScaleMode.log : priceScaleMode.normal,
        autoScale: autoLock.value,
      },
      // The default minimum of half a pixel per bar cannot show a run of
      // several years of daily candles at once (2020-2026 is ~2400 bars);
      // the fit would silently clip the early years off the left edge.
      timeScale: { borderColor: gridColor, timeVisible: false, minBarSpacing: 0.05 },
      crosshair: { mode: lc.CrosshairMode.Magnet },
      autoSize: false,
    })
    chart.subscribeCrosshairMove(onCrosshairMove)
    resizeObserver = new ResizeObserver(() => {
      // clientWidth 0 = detached (V3 warm cache) — keep the last real size;
      // the observer fires again with the true size on reactivation.
      if (chart && container.value && container.value.clientWidth > 0) {
        clearHover()
        chart.applyOptions({ width: container.value.clientWidth, height: container.value.clientHeight })
      }
    })
    resizeObserver.observe(container.value)
  }

  // Reset existing series.
  clearHover()
  for (const s of seriesMap.values()) chart.removeSeries(s)
  seriesMap.clear()

  const defs = buildDefs()
  legend.value = defs.map(d => ({ key: d.key, label: d.label, color: d.color, visible: d.visible }))
  dataLen = Math.max(0, ...defs.map(d => d.data.length))

  // Intraday backtests encode time as a UNIX timestamp (number); daily ones
  // use `YYYY-MM-DD` strings. Show hours on the axis only for intraday.
  const intraday = typeof props.result.equityStrategy?.[0]?.time === 'number'
  chart.applyOptions({ timeScale: { timeVisible: intraday, secondsVisible: false } })

  for (const def of defs) {
    const series = chart.addLineSeries({
      color: def.color,
      lineWidth: def.width,
      priceLineVisible: false,
      lastValueVisible: false,
      visible: def.visible,
    })
    series.setData(def.data)
    seriesMap.set(def.key, series)
  }

  chart.applyOptions({ width: container.value.clientWidth, height: container.value.clientHeight })
  applyScaleOptions()
  fitPadded()
}

// Fit the curve to view but leave a small gap between the line and the left
// and right edges (price axis), instead of letting it touch both edges.
function fitPadded() {
  if (!chart) return
  if (dataLen <= 1) {
    chart.timeScale().fitContent()
    return
  }
  const pad = Math.max(0.5, dataLen * 0.0025)
  chart.timeScale().setVisibleLogicalRange({ from: -pad, to: dataLen - 1 + pad })
}

function applyScaleOptions() {
  if (!chart) return
  chart.priceScale('right').applyOptions({
    mode: logScale.value ? priceScaleMode.log : priceScaleMode.normal,
    autoScale: autoLock.value,
  })
}

function toggleLog() {
  logScale.value = !logScale.value
  applyScaleOptions()
}

function toggleAutoLock() {
  autoLock.value = !autoLock.value
  applyScaleOptions()
  if (autoLock.value) fit()
}

function fit() {
  if (!chart) return
  fitPadded()
  chart.priceScale('right').applyOptions({ autoScale: true })
  if (!autoLock.value) {
    // a one-shot fit shouldn't permanently re-enable autoscale unless locked
    chart.priceScale('right').applyOptions({ autoScale: false })
  }
}

function toggle(key: string) {
  const item = legend.value.find(l => l.key === key)
  const series = seriesMap.get(key)
  if (!item || !series) return
  clearHover()
  item.visible = !item.visible
  series.applyOptions({ visible: item.visible })
}

onMounted(render)
watch(() => props.result, render)
onDeactivated(clearHover)

onUnmounted(() => {
  disposed = true
  clearHover()
  resizeObserver?.disconnect()
  resizeObserver = null
  if (chart) {
    chart.unsubscribeCrosshairMove(onCrosshairMove)
    chart.remove()
    chart = null
  }
  seriesMap.clear()
})
</script>

<template>
  <div class="card qs-chart">
    <div class="qs-chart__head">
      <div class="qs-chart__titlewrap">
        <h3 class="qs-chart__title">Equity Curves</h3>
        <div class="qs-chart__controls">
          <button
            class="qs-chart__ctrl"
            :class="{ 'qs-chart__ctrl--on': logScale }"
            title="Toggle logarithmic price scale"
            @click="toggleLog"
          >
            Log
          </button>
          <button
            class="qs-chart__ctrl"
            :class="{ 'qs-chart__ctrl--on': autoLock }"
            title="Auto-lock the price scale so the full curve always stays in view"
            @click="toggleAutoLock"
          >
            Auto-fit
          </button>
          <button class="qs-chart__ctrl" title="Fit all data to view" @click="fit">Fit</button>
        </div>
      </div>
      <div ref="seriesMenuRef" class="qs-series">
        <button
          class="qs-chart__ctrl qs-series__btn"
          :class="{ 'qs-chart__ctrl--on': seriesMenuOpen }"
          title="Show or hide individual graphs"
          @click="seriesMenuOpen = !seriesMenuOpen"
        >
          Series
          <span class="qs-series__count">{{ activeCount }}/{{ legend.length }}</span>
          <span class="qs-series__caret">▾</span>
        </button>
        <div v-if="seriesMenuOpen" class="qs-series__menu">
          <button
            v-for="l in legend"
            :key="l.key"
            class="qs-series__item"
            :class="{ 'qs-series__item--off': !l.visible }"
            @click="toggle(l.key)"
          >
            <span class="qs-series__check">{{ l.visible ? '✓' : '' }}</span>
            <span class="qs-chart__swatch" :style="{ background: l.color }" />
            <span class="qs-series__label">{{ l.label }}</span>
          </button>
        </div>
      </div>
    </div>
    <div class="qs-chart__plot" @mouseleave="clearHover">
      <div ref="container" class="qs-chart__canvas" />
      <div v-if="hover" class="qs-chart__hover-layer" :style="{ width: `${hover.width}px`, height: `${hover.height}px` }">
        <div ref="tooltip" class="qs-chart__tooltip" :class="`qs-chart__tooltip--${hover.side}`" role="tooltip" @wheel.stop>
          <div class="qs-chart__tooltip-date">{{ hover.date }}</div>
          <div v-for="row in hover.rows" :key="row.key" class="qs-chart__tooltip-row">
            <span class="qs-chart__swatch" :style="{ background: row.color }" />
            <span class="qs-chart__tooltip-name">{{ row.label }}</span>
            <span class="qs-chart__tooltip-value">{{ row.value === null ? '—' : `${equityFormat.format(row.value)}×` }}</span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.qs-chart {
  padding: 12px 14px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-height: 0;
}

.qs-chart__head {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  flex-wrap: wrap;
}

.qs-chart__titlewrap {
  display: flex;
  align-items: center;
  gap: 12px;
}

.qs-chart__title {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
}

.qs-chart__controls {
  display: flex;
  gap: 4px;
}

.qs-chart__ctrl {
  padding: 3px 9px;
  border: 1px solid var(--qs-border);
  border-radius: 6px;
  background: var(--qs-bg-input);
  color: var(--qs-text-secondary);
  font-size: 11px;
  cursor: pointer;
  transition: color var(--qs-transition), border-color var(--qs-transition), background var(--qs-transition);
}

.qs-chart__ctrl:hover {
  color: var(--qs-text);
  border-color: var(--qs-accent);
}

.qs-chart__ctrl--on {
  color: var(--qs-bg);
  background: var(--qs-accent);
  border-color: var(--qs-accent);
}

.qs-series {
  position: relative;
}

.qs-series__btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.qs-series__count {
  font-variant-numeric: tabular-nums;
  opacity: 0.8;
}

.qs-series__caret {
  font-size: 9px;
}

.qs-series__menu {
  position: absolute;
  top: calc(100% + 6px);
  right: 0;
  z-index: 20;
  min-width: 200px;
  max-height: 320px;
  overflow-y: auto;
  padding: 6px;
  background: var(--qs-bg-card);
  border: 1px solid var(--qs-border);
  border-radius: var(--qs-radius);
  box-shadow: 0 16px 40px rgba(0, 0, 0, 0.4);
}

.qs-series__item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 7px 8px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qs-text);
  font-size: 12px;
  cursor: pointer;
  text-align: left;
  transition: background var(--qs-transition);
}

.qs-series__item:hover {
  background: var(--qs-bg-hover);
}

.qs-series__check {
  flex-shrink: 0;
  width: 12px;
  font-size: 11px;
  color: var(--qs-accent);
}

.qs-series__label {
  flex: 1;
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.qs-series__item--off .qs-series__label,
.qs-series__item--off .qs-chart__swatch {
  opacity: 0.45;
}

.qs-chart__swatch {
  flex-shrink: 0;
  width: 10px;
  height: 10px;
  border-radius: 2px;
}

/* Grows into whatever height the page hands the card; the ResizeObserver in
   render() keeps the chart canvas in sync. Falls back to a usable height when
   the card is laid out by content instead of by the page grid. */
.qs-chart__plot {
  position: relative;
  display: flex;
  flex: 1 1 auto;
  min-width: 0;
  min-height: 240px;
}

.qs-chart__canvas {
  width: 100%;
  flex: 1 1 auto;
  min-width: 0;
  min-height: 240px;
  /* The chart canvas is sized imperatively; clip it so a frame rendered at the
     old width can never push the page into a horizontal scroll. */
  overflow: hidden;
}

.qs-chart__hover-layer {
  position: absolute;
  z-index: 3;
  top: 0;
  left: 0;
  pointer-events: none;
}

.qs-chart__tooltip {
  position: absolute;
  top: 8px;
  width: min(360px, calc(100% - 16px));
  max-height: calc(100% - 16px);
  overflow-y: auto;
  padding: 10px 12px;
  border: 1px solid var(--qs-border);
  border-radius: var(--qs-radius);
  background: var(--qs-bg-card);
  box-shadow: 0 6px 24px rgba(0, 0, 0, 0.25);
  color: var(--qs-text);
  font-size: 12px;
  pointer-events: auto;
}
.qs-chart__tooltip--left { left: 8px; }
.qs-chart__tooltip--right { right: 8px; }
.qs-chart__tooltip-date {
  margin-bottom: 8px;
  font-weight: 600;
}
.qs-chart__tooltip-row {
  display: grid;
  grid-template-columns: 10px minmax(0, 1fr) auto;
  align-items: baseline;
  gap: 8px;
  margin-top: 5px;
}
.qs-chart__tooltip-name { overflow-wrap: anywhere; }
.qs-chart__tooltip-value {
  font-family: ui-monospace, monospace;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}
</style>
