export function formatDateTime(value?: string | null): string {
  if (!value) return '—'
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleString()
}

export function formatRelative(value?: string | null): string {
  if (!value) return 'just now'
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  const diffMs = date.getTime() - Date.now()
  const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' })
  const abs = Math.abs(diffMs)
  const minutes = Math.round(diffMs / 60000)
  if (abs < 3600000) return rtf.format(minutes, 'minute')
  const hours = Math.round(diffMs / 3600000)
  if (abs < 86400000) return rtf.format(hours, 'hour')
  const days = Math.round(diffMs / 86400000)
  return rtf.format(days, 'day')
}

export function formatCount(value: number, noun: string): string {
  return `${value} ${noun}${value === 1 ? '' : 's'}`
}

export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value))
}
