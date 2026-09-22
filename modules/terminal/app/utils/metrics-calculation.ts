import {
  bindings,
  joinedMetricRows,
  metricPoints,
  type FeedSource,
  type History,
  type Point,
} from './metrics.ts'

export interface MetricsCalculation {
  source: FeedSource
  histories: Partial<Record<FeedSource, History>>
  now: number
}
export type MetricsSeries = Record<string, Point[]>

export function affectedMetrics(source: FeedSource) {
  return Object.entries(bindings).filter(
    ([, binding]) =>
      binding.source === source || binding.joins?.some((join) => join.source === source),
  )
}

export function calculateMetric(title: string, { histories, now }: MetricsCalculation) {
  return metricPoints(
    title,
    joinedMetricRows(bindings[title]!, (source) => histories[source]?.rows ?? []),
    now,
  )
}
