<script setup lang="ts">
import { ref, watch, onMounted, onBeforeUnmount } from 'vue'
import type { EquityPoint } from '#algo/types'

interface OverlaySeries {
  name: string
  color: string
  data: EquityPoint[]
}

const props = withDefaults(
  defineProps<{
    data: EquityPoint[]
    height?: number
    overlays?: OverlaySeries[]
  }>(),
  {
    height: 350,
    overlays: () => [],
  },
)

const containerRef = ref<HTMLDivElement | null>(null)

let chart: import('lightweight-charts').IChartApi | null = null
let primarySeries: import('lightweight-charts').ISeriesApi<'Area'> | null = null
let overlaySeries: import('lightweight-charts').ISeriesApi<'Area'>[] = []
let resizeObserver: ResizeObserver | null = null

function toChartData(points: EquityPoint[]) {
  return points.map((p) => ({
    time: (Math.floor(new Date(p.time).getTime() / 1000)) as import('lightweight-charts').UTCTimestamp,
    value: p.equity,
  }))
}

async function initChart() {
  if (!containerRef.value) return

  const lc = await import('lightweight-charts')

  chart = lc.createChart(containerRef.value, {
    width: containerRef.value.clientWidth,
    height: props.height,
    layout: {
      background: { type: lc.ColorType.Solid, color: 'transparent' },
      textColor: '#9a9aa5',
    },
    grid: {
      vertLines: { color: '#292930' },
      horzLines: { color: '#292930' },
    },
    timeScale: {
      borderColor: '#47474f',
      timeVisible: true,
    },
    rightPriceScale: {
      borderColor: '#47474f',
    },
    crosshair: {
      vertLine: { color: '#47474f', width: 1, style: lc.LineStyle.Dashed },
      horzLine: { color: '#47474f', width: 1, style: lc.LineStyle.Dashed },
    },
    handleScroll: true,
    handleScale: true,
  })

  // Primary area series (used when no overlays)
  if (props.overlays.length === 0) {
    primarySeries = chart.addAreaSeries({
      lineColor: '#a0a0a8',
      topColor: 'rgba(160, 160, 168, 0.3)',
      bottomColor: 'rgba(160, 160, 168, 0.02)',
      lineWidth: 2,
    })
    primarySeries.setData(toChartData(props.data))
  } else {
    // Overlay mode: render each series
    setOverlays()
  }

  chart.timeScale().fitContent()

  // Resize observer
  resizeObserver = new ResizeObserver((entries) => {
    if (!chart || !containerRef.value) return
    const entry = entries[0]
    if (!entry) return
    const { width } = entry.contentRect
    // width 0 = detached (V3 warm cache) — keep the last real size; the
    // observer fires again with the true width on reactivation.
    if (width > 0) {
      chart.applyOptions({ width })
    }
  })
  resizeObserver.observe(containerRef.value)
}

function setOverlays() {
  if (!chart) return

  // Remove old overlay series
  for (const s of overlaySeries) {
    chart.removeSeries(s)
  }
  overlaySeries = []

  for (const overlay of props.overlays) {
    const series = chart.addAreaSeries({
      lineColor: overlay.color,
      topColor: `${overlay.color}26`,
      bottomColor: `${overlay.color}05`,
      lineWidth: 2,
    })
    series.setData(toChartData(overlay.data))
    overlaySeries.push(series)
  }

  chart.timeScale().fitContent()
}

onMounted(() => {
  initChart()
})

// Watch data changes for single-series mode — callers always replace the array,
// so a reference watch is enough.
watch(
  () => props.data,
  (newData: EquityPoint[]) => {
    if (primarySeries) {
      primarySeries.setData(toChartData(newData))
      chart?.timeScale().fitContent()
    }
  },
)

// Watch overlay changes
watch(
  () => props.overlays,
  (newOverlays: OverlaySeries[]) => {
    if (newOverlays.length > 0) {
      // If we had a primary series, remove it
      if (primarySeries && chart) {
        chart.removeSeries(primarySeries)
        primarySeries = null
      }
      setOverlays()
    }
  },
)

// Watch height
watch(
  () => props.height,
  (h: number) => {
    chart?.applyOptions({ height: h })
  },
)

onBeforeUnmount(() => {
  if (resizeObserver) {
    resizeObserver.disconnect()
    resizeObserver = null
  }
  if (chart) {
    chart.remove()
    chart = null
  }
  primarySeries = null
  overlaySeries = []
})
</script>

<template>
  <div ref="containerRef" class="equity-chart" :style="{ height: `${height}px` }" />
</template>

<style scoped>
.equity-chart {
  width: 100%;
  border-radius: var(--qa-radius);
  overflow: hidden;
}
</style>
