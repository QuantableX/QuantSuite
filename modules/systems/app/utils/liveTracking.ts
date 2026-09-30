import type { LiveResult, RunConfig } from '#systems/types'

export function liveToday(): string {
  return new Date().toISOString().slice(0, 10)
}

export function liveDateError(value: string, today = liveToday()): string | null {
  const date = new Date(`${value}T00:00:00Z`)
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value) || !Number.isFinite(date.getTime()) || date.toISOString().slice(0, 10) !== value) {
    return 'Choose a valid live start day (YYYY-MM-DD).'
  }
  return value > today ? 'Live start day must be on or before today (UTC).' : null
}

export function matchesLiveWindow(result: LiveResult, config: RunConfig): boolean {
  return result.tracking?.startDate === config.liveStartDate && result.tracking.endDate === result.asOf
}
