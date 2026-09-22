/** Format a number as currency (USD) */
export function formatCurrency(value: number): string {
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).format(value)
}

/** Format a number as percentage with sign */
export function formatPct(value: number, decimals = 2): string {
  const sign = value >= 0 ? '+' : ''
  return `${sign}${value.toFixed(decimals)}%`
}

/** Format PnL with sign and color class */
export function formatPnl(value: number): { text: string; class: string } {
  const sign = value >= 0 ? '+' : ''
  return {
    text: `${sign}${formatCurrency(value)}`,
    class: value >= 0 ? 'text-success' : 'text-error',
  }
}

/** Format PnL percentage with sign and color class */
export function formatPnlPct(value: number): { text: string; class: string } {
  return {
    text: formatPct(value),
    class: value >= 0 ? 'text-success' : 'text-error',
  }
}

/** Format a number compactly (1.2K, 3.4M) */
export function formatCompact(value: number): string {
  return new Intl.NumberFormat('en-US', {
    notation: 'compact',
    maximumFractionDigits: 1,
  }).format(value)
}

/** Format a quantity (crypto amount) */
export function formatQuantity(value: number, decimals = 6): string {
  return value.toFixed(decimals).replace(/\.?0+$/, '')
}

/** Format a price */
export function formatPrice(value: number): string {
  if (value >= 1000) return formatCurrency(value)
  if (value >= 1) return `$${value.toFixed(4)}`
  return `$${value.toFixed(8)}`
}

/** Format an ISO date string to locale short */
export function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString('en-US', {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
  })
}

/** Format an ISO date to date + time */
export function formatDateTime(iso: string): string {
  return new Date(iso).toLocaleString('en-US', {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  })
}

/** Format an ISO date to just time */
export function formatTime(iso: string): string {
  return new Date(iso).toLocaleTimeString('en-US', {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  })
}

/** Format seconds to human-readable duration (4h 23m) */
export function formatDuration(seconds: number): string {
  if (seconds < 60) return `${Math.floor(seconds)}s`
  const m = Math.floor(seconds / 60) % 60
  const h = Math.floor(seconds / 3600) % 24
  const d = Math.floor(seconds / 86400)
  const parts: string[] = []
  if (d > 0) parts.push(`${d}d`)
  if (h > 0) parts.push(`${h}h`)
  if (m > 0) parts.push(`${m}m`)
  return parts.join(' ') || '0m'
}

/** Format a timeframe code to label */
export function formatTimeframe(tf: string): string {
  const map: Record<string, string> = {
    '1m': '1 Min', '5m': '5 Min', '15m': '15 Min',
    '1h': '1 Hour', '4h': '4 Hour', '1d': '1 Day',
    '1w': '1 Week', '1M': '1 Month',
  }
  return map[tf] ?? tf
}

/** Capitalize first letter */
export function capitalize(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1)
}

/** Truncate string */
export function truncate(s: string, max: number): string {
  return s.length <= max ? s : `${s.slice(0, max)}...`
}
