import {
  affectedMetrics,
  calculateMetric,
  type MetricsCalculation,
} from '../utils/metrics-calculation.ts'

self.onmessage = ({ data }: MessageEvent<MetricsCalculation & { id: number }>) => {
  try {
    const series = Object.fromEntries(
      affectedMetrics(data.source).map(([title]) => [title, calculateMetric(title, data)]),
    )
    self.postMessage({ id: data.id, series })
  } catch (error) {
    self.postMessage({
      id: data.id,
      error: error instanceof Error ? error.message : String(error),
    })
  }
}
