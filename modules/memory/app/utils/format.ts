/** Small display helpers, shared by the list, the panels and the dashboard. */

export function timeAgo(iso: string | null | undefined): string {
  if (!iso) return ''
  const then = Date.parse(iso)
  if (Number.isNaN(then)) return ''
  const s = Math.max(0, (Date.now() - then) / 1000)
  if (s < 60) return 'now'
  if (s < 3600) return `${Math.floor(s / 60)}m`
  if (s < 86400) return `${Math.floor(s / 3600)}h`
  if (s < 86400 * 30) return `${Math.floor(s / 86400)}d`
  return new Date(then).toLocaleDateString()
}

export function formatCount(n: number): string {
  return n >= 10_000 ? `${Math.round(n / 1000)}k` : String(n)
}
