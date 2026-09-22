/** Relative time for version rows and file dates. */
export function timeAgo(iso: string | null | undefined): string {
  if (!iso) return '—'
  const t = Date.parse(iso)
  if (!Number.isFinite(t)) return '—'
  const s = Math.max(0, Math.round((Date.now() - t) / 1000))
  if (s < 45) return 'just now'
  const m = Math.round(s / 60)
  if (m < 60) return `${m} min ago`
  const h = Math.round(m / 60)
  if (h < 24) return `${h} h ago`
  const d = Math.round(h / 24)
  if (d < 14) return `${d} d ago`
  return new Date(t).toLocaleDateString()
}

export function formatBytes(n: number | null | undefined): string {
  if (n == null) return '—'
  if (n < 1024) return `${n} B`
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`
  return `${(n / 1024 / 1024).toFixed(1)} MB`
}

export function shortSha(sha: string | null | undefined): string {
  return sha ? sha.slice(0, 8) : '—'
}

/** The grade's tone: certified/top, weak, failed, or none. */
export function gradeClass(grade: string | null | undefined, certified = false): string {
  if (!grade) return 'is-none'
  if (certified) return 'is-certified'
  const g = grade.trim().charAt(0)
  if (g === 'S' || g === 'A') return 'is-top'
  if (g === 'F') return 'is-fail'
  if (g === 'C' || g === 'D') return 'is-weak'
  return ''
}

export function formatParams(params: Record<string, unknown> | null | undefined): string {
  if (!params) return ''
  return Object.entries(params)
    .map(([k, v]) => `${k}=${Array.isArray(v) ? `(${v.join(',')})` : typeof v === 'number' ? +v.toFixed(4) : String(v)}`)
    .join('  ')
}

export const AUTHOR_LABELS: Record<string, string> = {
  baseline: 'baseline',
  user: 'saved',
  external: 'outside',
  restore: 'restore',
  template: 'template',
}
