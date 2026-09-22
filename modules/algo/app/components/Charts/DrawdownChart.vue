<script setup lang="ts">
import type { EquityPoint } from '#algo/types'
import {
  createChart,
  type IChartApi,
  type ISeriesApi,
  type UTCTimestamp,
  ColorType,
  CrosshairMode,
} from 'lightweight-charts'

const props = withDefaults(defineProps<{
  data: EquityPoint[]
  height?: number
}>(), {
  height: 200,
})

const chartContainer = ref<HTMLDivElement | null>(null)
let chart: IChartApi | null = null
let areaSeries: ISeriesApi<'Area'> | null = null
let resizeObserver: ResizeObserver | null = null

function toTimestamp(iso: string): UTCTimestamp {
  return Math.floor(new Date(iso).getTime() / 1000) as UTCTimestamp
}

const drawdownData = computed(() => {
  if (!props.data.length) return []

  const firstPoint = props.data[0]
  if (!firstPoint) return []

  let peak = firstPoint.equity
  return props.data.map((p) => {
    if (p.equity > peak) peak = p.equity
    const dd = peak > 0 ? ((p.equity - peak) / peak) * 100 : 0
    return {
      time: toTimestamp(p.time),
      value: dd,
    }
  })
})

function buildChartOptions(width: number) {
  return {
    width,
    height: props.height,
    layout: {
      background: { type: ColorType.Solid, color: 'transparent' },
      textColor: '#6e6e7a',
      fontFamily: '-apple-system, BlinkMacSystemFont, Segoe UI, Roboto, sans-serif',
      fontSize: 11,
    },
    grid: {
      vertLines: { color: 'rgba(71,71,79,0.3)' },
      horzLines: { color: 'rgba(71,71,79,0.3)' },
    },
    crosshair: {
      mode: CrosshairMode.Magnet,
    },
    rightPriceScale: {
      borderColor: 'rgba(71,71,79,0.5)',
    },
    timeScale: {
      borderColor: 'rgba(71,71,79,0.5)',
      timeVisible: true,
    },
    handleScroll: true,
    handleScale: true,
  }
}

function updateSeriesData() {
  if (!areaSeries) return

  const dd = drawdownData.value
  if (!dd.length) {
    areaSeries.setData([])
    return
  }

  areaSeries.setData(dd)
}

function initChart() {
  if (!chartContainer.value) return

  const width = chartContainer.value.clientWidth
  chart = createChart(chartContainer.value, buildChartOptions(width))

  areaSeries = chart.addAreaSeries({
    lineColor: '#ff4757',
    topColor: 'rgba(255,71,87,0.02)',
    bottomColor: 'rgba(255,71,87,0.25)',
    lineWidth: 2,
    priceFormat: {
      type: 'custom',
      formatter: (price: number) => `${price.toFixed(2)}%`,
    },
  })

  updateSeriesData()
  chart.timeScale().fitContent()
}

function setupResizeObserver() {
  if (!chartContainer.value || !chart) return

  resizeObserver = new ResizeObserver((entries) => {
    const entry = entries[0]
    if (!chart || !entry) return
    const { width } = entry.contentRect
    if (width > 0) {
      chart.applyOptions({ width })
    }
  })

  resizeObserver.observe(chartContainer.value)
}

function cleanup() {
  if (resizeObserver) {
    resizeObserver.disconnect()
    resizeObserver = null
  }
  if (chart) {
    chart.remove()
    chart = null
    areaSeries = null
  }
}

// Reference watch is enough — callers always replace the array, never mutate it.
watch(() => props.data, () => {
  if (chart && areaSeries) {
    updateSeriesData()
    chart.timeScale().fitContent()
  }
})

watch(() => props.height, (h) => {
  if (chart) {
    chart.applyOptions({ height: h })
  }
})

onMounted(() => {
  initChart()
  setupResizeObserver()
})

onUnmounted(() => {
  cleanup()
})
</script>

<template>
  <div class="drawdown-container">
    <div
      ref="chartContainer"
      class="chart-el"
      :style="{ height: `${height}px` }"
    />
    <div v-if="!data.length" class="chart-empty">
      No drawdown data available
    </div>
  </div>
</template>

<style scoped>
.drawdown-container {
  position: relative;
  width: 100%;
}

.chart-el {
  width: 100%;
}

.chart-empty {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--qa-text-muted);
  font-size: 13px;
  pointer-events: none;
}
</style>
