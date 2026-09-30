import { ref } from 'vue'
import type { CodebaseIndexStatus, IndexCodebaseResult, IndexJobStatus } from './useMcpWorkspaces'

type Mode = 'structural' | 'semantic'
type Job = IndexJobStatus & { message?: string }
interface IndexApi {
  getIndexStats: (workspaceId: string) => Promise<CodebaseIndexStatus>
  indexWorkspaceCodebase: (workspaceId: string, mode: string, force: boolean, filter: string, requestId: string) => Promise<IndexCodebaseResult>
}

function resultMessage(result: IndexCodebaseResult): string {
  if (result.error) return `Error: ${result.error}`
  return `Indexed ${result.files_indexed ?? 0} files (${result.files_skipped ?? 0} unchanged, ${result.errors ?? 0} errors). Entries: ${result.total_entries ?? 0}`
}

/** Page-independent backend jobs reconcile delayed/lost invoke responses. */
export function useCodebaseIndexing(api: IndexApi) {
  const statuses = ref<Record<string, CodebaseIndexStatus>>({})
  const jobs = ref<Record<string, Partial<Record<Mode, Job>>>>({})
  const generations = new Map<string, number>()
  const refreshing = new Map<string, Promise<void>>()
  const cancelReads = new Set<() => void>()
  let disposed = false

  function setJob(wid: string, job: Job) {
    jobs.value = { ...jobs.value, [wid]: { ...jobs.value[wid], [job.mode]: job } }
  }
  const busy = (wid: string, mode: Mode) => jobs.value[wid]?.[mode]?.status === 'running'
  const completed = (wid: string, mode: Mode) => !!statuses.value[wid]?.[`${mode}_indexed_at`]
    && !(statuses.value[wid]?.[`${mode}_pending_count`] ?? 0)
    && !busy(wid, mode) && jobs.value[wid]?.[mode]?.status !== 'failed'

  function message(wid: string, mode: Mode): string | null {
    if (busy(wid, mode)) return null
    const job = jobs.value[wid]?.[mode]
    if (job?.message) return job.message
    const stats = statuses.value[wid]
    const pending = stats?.[`${mode}_pending_count`] ?? 0
    if (pending && (stats?.mode === mode || stats?.mode === 'both')) {
      return `Error: Index incomplete: ${pending} files still need indexing. Run Index to finish.`
    }
    return null
  }

  async function refresh(wid: string): Promise<void> {
    if (disposed) return
    if (refreshing.has(wid)) return refreshing.get(wid)
    const generation = generations.get(wid) ?? 0
    const read = (async () => {
      let cancel = () => {}
      const deadline = new Promise<never>((_, reject) => {
        const timer = setTimeout(() => reject(new Error('Index status is unavailable. Retrying automatically.')), 8000)
        cancel = () => { clearTimeout(timer); reject(new Error('Page closed')) }
      })
      cancelReads.add(cancel)
      try {
        const stats = await Promise.race([api.getIndexStats(wid), deadline])
        if (disposed || generation !== (generations.get(wid) ?? 0)) return
        // A failed statistics read must not erase the last known index data.
        statuses.value = { ...statuses.value, [wid]: stats.stats_error
          ? { ...(statuses.value[wid] ?? stats), jobs: stats.jobs, stats_error: stats.stats_error }
          : stats }
        for (const remote of stats.jobs ?? []) {
          const local = jobs.value[wid]?.[remote.mode]
          if (local?.status === 'running' && local.request_id !== remote.request_id) continue
          // The backend may still remember an older successful job when a new
          // invoke fails before starting. It must not erase the newer error.
          if (local && local.request_id !== remote.request_id && remote.started_at <= local.started_at) continue
          if (local?.status !== 'running' && local?.request_id === remote.request_id && remote.status === 'running') continue
          setJob(wid, { ...remote, message: remote.error ? `Error: ${remote.error}` : remote.result ? resultMessage(remote.result) : undefined })
        }
      } catch (error) {
        if (!disposed && generation === (generations.get(wid) ?? 0)) {
          const previous = statuses.value[wid] ?? { workspace_id: wid, status: 'not_indexed' as const, file_count: 0, indexed_at: null }
          statuses.value = { ...statuses.value, [wid]: { ...previous, stats_error: String(error) } }
        }
      } finally {
        cancelReads.delete(cancel)
        cancel()
      }
    })()
    refreshing.set(wid, read)
    try { await read } finally { refreshing.delete(wid) }
  }

  async function start(wid: string, mode: Mode, force: boolean, filter: string) {
    if (disposed || busy(wid, mode)) return
    const requestId = crypto.randomUUID()
    generations.set(wid, (generations.get(wid) ?? 0) + 1)
    setJob(wid, { request_id: requestId, started_at: Date.now(), mode, status: 'running' })
    const settle = (status: 'succeeded' | 'failed', text: string) => {
      const current = jobs.value[wid]?.[mode]
      if (disposed || current?.request_id !== requestId || current.status !== 'running') return
      generations.set(wid, (generations.get(wid) ?? 0) + 1)
      setJob(wid, { ...current, status, message: text })
    }
    try {
      const result = await api.indexWorkspaceCodebase(wid, mode, force, filter, requestId)
      settle(result.error ? 'failed' : 'succeeded', resultMessage(result))
    } catch (error) {
      settle('failed', `Error: ${error instanceof Error ? error.message : error}`)
    } finally {
      // Clearing busy is part of settling the job, not awaiting optional stats.
      if (!disposed) void refresh(wid)
    }
  }

  function poll(selectedId?: string) {
    const ids = new Set(selectedId ? [selectedId] : [])
    for (const [wid, modes] of Object.entries(jobs.value)) {
      if (Object.values(modes).some(job => job?.status === 'running')) ids.add(wid)
    }
    for (const wid of ids) void refresh(wid)
  }

  function dispose() {
    disposed = true
    for (const cancel of cancelReads) cancel()
    cancelReads.clear()
  }

  return { statuses, busy, completed, message, refresh, start, poll, dispose }
}
