<script setup lang="ts">
import type { EquityPoint, Trade } from '#algo/types'
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
  trades?: Trade[]
  height?: number
}>(), {
  height: 400,
})

const chartContainer = ref<HTMLDivElement | null>(null)
let chart: IChartApi | null = null
let areaSeries: ISeriesApi<'Area'> | null = null
let resizeObserver: ResizeObserver | null = null

function toTimestamp(iso: string): UTCTimestamp {
  return Math.floor(new Date(iso).getTime() / 1000) as UTCTimestamp
}

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
  if (!areaSeries || !props.data.length) return

  const seriesData = props.data.map((p) => ({
    time: toTimestamp(p.time),
    value: p.equity,
  }))

  areaSeries.setData(seriesData)

  if (props.trades?.length) {
    const markers = props.trades
      .filter((t) => t.entry_time)
      .map((trade) => {
        const isEntry = true
        const isLong = trade.side === 'long'
        return [
          {
            time: toTimestamp(trade.entry_time),
            position: isLong ? ('belowBar' as const) : ('aboveBar' as const),
            color: isLong ? '#a0a0a8' : '#ff4757',
            shape: isLong ? ('arrowUp' as const) : ('arrowDown' as const),
            text: `${isLong ? 'L' : 'S'} ${trade.pair}`,
          },
          ...(trade.exit_time
            ? [{
                time: toTimestamp(trade.exit_time),
                position: 'inBar' as const,
                color: '#a0a0a8',
                shape: 'circle' as const,
                text: `Exit ${trade.pair}`,
              }]
            : []),
        ]
      })
      .flat()
      .sort((a, b) => (a.time as number) - (b.time as number))

    areaSeries.setMarkers(markers)
  } else {
    areaSeries.setMarkers([])
  }
}

function initChart() {
  if (!chartContainer.value) return

  const width = chartContainer.value.clientWidth
  chart = createChart(chartContainer.value, buildChartOptions(width))

  areaSeries = chart.addAreaSeries({
    lineColor: '#a0a0a8',
    topColor: 'rgba(160,160,168,0.25)',
    bottomColor: 'rgba(160,160,168,0.02)',
    lineWidth: 2,
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

// Reference watch is enough — callers always replace the arrays, never mutate them.
watch(() => [props.data, props.trades], () => {
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
  <div class="equity-curve-container">
    <div
      ref="chartContainer"
      class="chart-el"
      :style="{ height: `${height}px` }"
    />
    <div v-if="!data.length" class="chart-empty">
      No equity data available
    </div>
  </div>
</template>

<style scoped>
.equity-curve-container {
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
