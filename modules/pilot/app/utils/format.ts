export function timeAgo(ms: number | null | undefined): string {
  if (!ms) return ''
  const diff = Date.now() - ms
  if (diff < 45_000) return 'just now'
  const m = Math.round(diff / 60_000)
  if (m < 60) return `${m}m ago`
  const h = Math.round(m / 60)
  if (h < 24) return `${h}h ago`
  const d = Math.round(h / 24)
  if (d < 14) return `${d}d ago`
  return new Date(ms).toLocaleDateString()
}

export function fmtTokens(n: number | null | undefined): string {
  if (!n) return '0'
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`
  return String(n)
}

export function fmtCost(usd: number | null | undefined): string {
  if (usd === null || usd === undefined) return '—'
  if (usd === 0) return '$0.00'
  return usd < 0.01 ? `$${usd.toFixed(4)}` : `$${usd.toFixed(2)}`
}

/** The last path segment, for chips and file rows. */
export function basename(path: string): string {
  const parts = path.replace(/[\\/]+$/, '').split(/[\\/]/)
  return parts[parts.length - 1] || path
}

/** Everything but the last segment, for the muted half of a file row. */
export function dirname(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, '')
  const i = Math.max(trimmed.lastIndexOf('\\'), trimmed.lastIndexOf('/'))
  return i > 0 ? trimmed.slice(0, i) : ''
}

/** The built-in adapters' names, for when status has not loaded yet. */
const BUILT_IN_LABELS: Record<string, string> = {
  claude: 'Claude Code',
  codex: 'Codex',
  pi: 'pi',
  omp: 'omp',
  opencode: 'OpenCode',
  gemini: 'Gemini CLI',
}

/** A row's agent: the adapter started in its terminal, or just the terminal. */
export function providerLabel(id: string, known?: { id: string; label: string }[]): string {
  if (!id) return 'terminal'
  return known?.find((a) => a.id === id)?.label ?? BUILT_IN_LABELS[id] ?? id
}

export function modeLabel(m: string): string {
  return m === 'auto' ? 'Auto' : m === 'full' ? 'Full access' : 'Ask'
}
