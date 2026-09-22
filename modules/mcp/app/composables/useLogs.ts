export type LogDirection = 'In' | 'Out'

export interface LogEntry {
  id: number
  timestamp: number
  direction: LogDirection
  source: string
  source_type: string
  content: string
}

// Mirrors the backend ring buffer (LogStore::max_entries) — without it the
// incremental path below would keep entries the store has already dropped.
const MAX_LOG_ENTRIES = 1000

// Every caller shares the `logs` state, so the guard against overlapping loads
// has to be shared too: each request takes a token and drops its result if a
// newer request (or a clear) has since taken over. Without it two refreshes in
// flight read the same sinceId and append the same tail twice.
let loadToken = 0

export function useLogs() {
  const logs = useState<LogEntry[]>('logs', () => [])
  const loading = useState('logs-loading', () => false)

  async function refresh() {
    const token = ++loadToken
    loading.value = true
    try {
      if (window.__TAURI_INTERNALS__) {
        const { invoke } = await import('@tauri-apps/api/core')
        // Ids are monotonic, so after the first load the 3 s poll asks for the
        // tail only instead of transferring and re-rendering the whole buffer.
        const sinceId = logs.value.length ? logs.value[logs.value.length - 1]!.id : null
        const fetched = await invoke<LogEntry[]>('plugin:mcp|list_logs', { sinceId })
        if (token !== loadToken) return
        if (sinceId === null) {
          logs.value = fetched
        } else if (fetched.length) {
          logs.value = [...logs.value, ...fetched].slice(-MAX_LOG_ENTRIES)
        }
      }
    } catch (e) {
      console.error('Failed to load logs:', e)
    } finally {
      if (token === loadToken) loading.value = false
    }
  }

  async function clear() {
    try {
      if (window.__TAURI_INTERNALS__) {
        const { invoke } = await import('@tauri-apps/api/core')
        await invoke('plugin:mcp|clear_logs')
        // Retire any refresh still in flight — its tail belongs to the buffer
        // that was just cleared and would refill the emptied list.
        loadToken++
        logs.value = []
      }
    } catch (e) {
      console.error('Failed to clear logs:', e)
    }
  }

  return { logs, loading, refresh, clear }
}
