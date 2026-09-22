import {
  affectedMetrics,
  calculateMetric,
  type MetricsCalculation,
  type MetricsSeries,
} from './metrics-calculation.ts'

/** A single worker per store; immutable inputs also allow recovery after a worker failure. */
export function createMetricsProcessor() {
  let worker: Worker | null = null
  let sequence = 0
  let disposed = false
  let calculations: Promise<unknown> = Promise.resolve()
  const pending = new Map<
    number,
    {
      resolve: (series: MetricsSeries) => void
      reject: (error: Error) => void
      timer: ReturnType<typeof setTimeout>
    }
  >()

  function stop(error: Error) {
    worker?.terminate()
    worker = null
    for (const job of pending.values()) {
      clearTimeout(job.timer)
      job.reject(error)
    }
    pending.clear()
  }

  async function calculate(input: MetricsCalculation): Promise<MetricsSeries> {
    if (disposed) throw new Error('Metrics processing stopped')
    // Older webviews and non-browser consumers still yield between metrics.
    if (typeof Worker === 'undefined') {
      const series: MetricsSeries = {}
      for (const [title] of affectedMetrics(input.source)) {
        await new Promise<void>((resolve) => setTimeout(resolve, 0))
        if (disposed) throw new Error('Metrics processing stopped')
        series[title] = calculateMetric(title, input)
      }
      return series
    }
    if (!worker) {
      worker = new Worker(new URL('../workers/metrics.worker.ts', import.meta.url), {
        type: 'module',
      })
      worker.onmessage = ({
        data,
      }: MessageEvent<{
        id: number
        series: MetricsSeries
        error?: string
      }>) => {
        const job = pending.get(data.id)
        if (!job) return
        clearTimeout(job.timer)
        pending.delete(data.id)
        if (data.error) job.reject(new Error(data.error))
        else job.resolve(data.series)
      }
      worker.onerror = () => stop(new Error('Background metrics processing failed. Please retry.'))
      worker.onmessageerror = () => stop(new Error('Could not read background metrics results.'))
    }
    const id = ++sequence
    return new Promise<MetricsSeries>((resolve, reject) => {
      const timer = setTimeout(
        () => stop(new Error('Background metrics processing timed out. Please retry.')),
        30_000,
      )
      pending.set(id, { resolve, reject, timer })
      try {
        worker!.postMessage({ id, ...input })
      } catch (error) {
        clearTimeout(timer)
        pending.delete(id)
        reject(error instanceof Error ? error : new Error(String(error)))
      }
    })
  }

  return {
    calculate(input: MetricsCalculation): Promise<MetricsSeries> {
      // Preserve arrival order for joins, including the yielding fallback.
      const job = calculations.then(() => calculate(input))
      calculations = job.catch(() => {})
      return job
    },
    dispose() {
      disposed = true
      stop(new Error('Metrics processing stopped'))
    },
  }
}
