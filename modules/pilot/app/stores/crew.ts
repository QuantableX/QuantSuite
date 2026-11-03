/**
 * Crews (round 2, card 07): an orchestrator thread and its worker threads.
 * The Rust side (crate/src/crew.rs) runs them; this store holds every
 * crew's snapshot, kept current by the `pilot-crew` event (no polling),
 * and the user's actions: decide a gate, retry or swap a worker, stop one
 * or the whole crew, resume, read the full log.
 */
import { defineStore } from 'pinia'
import { pilot } from '#pilot/utils/invoke'
import { crewOfThread, pendingGates, workerIds } from '#pilot/utils/crew'
import type { CrewMessage, CrewSnapshot, CrewTask } from '#pilot/types'

export const useCrewStore = defineStore('pilot/crew', {
  state: () => ({
    crews: {} as Record<string, CrewSnapshot>,
    loaded: false,
    error: '',
    /** The crew whose log dialog is open. */
    logOpen: null as string | null,
  }),

  getters: {
    list(state): CrewSnapshot[] {
      return Object.values(state.crews)
    },
    /** The crew a thread leads or works in. */
    ofThread(): (threadId: string | null) => CrewSnapshot | null {
      return (threadId) => crewOfThread(this.list, threadId)
    },
    /** Worker threads: listed in their crew, not in the sidebar. */
    workers(): Set<string> {
      return workerIds(this.list)
    },
    /** Every gate waiting on the user, anywhere. */
    gatesWaiting(): number {
      return this.list.reduce((n, c) => n + pendingGates(c).length, 0)
    },
  },

  actions: {
    async load() {
      try {
        const all = await pilot<CrewSnapshot[]>('plugin:pilot|pilot_crews')
        this.crews = Object.fromEntries((all ?? []).map((c) => [c.crew.id, c]))
        this.loaded = true
      } catch (e) {
        this.error = String(e)
      }
    },
    /** A `pilot-crew` event: one crew's new snapshot, or its deletion. */
    apply(payload: CrewSnapshot | { deleted: string }) {
      if ('deleted' in payload) {
        delete this.crews[payload.deleted]
        if (this.logOpen === payload.deleted) this.logOpen = null
        return
      }
      const old = this.crews[payload.crew.id]
      if (old && old.crew.rev > payload.crew.rev) return
      this.crews[payload.crew.id] = payload
    },
    async decide(gateId: string, approve: boolean, note = ''): Promise<boolean> {
      return this.run('plugin:pilot|pilot_crew_decide', { gateId, approve, note: note.trim() || null })
    },
    async retry(task: CrewTask, provider?: string, model?: string): Promise<boolean> {
      return this.run('plugin:pilot|pilot_crew_retry', { taskId: task.id, provider: provider ?? null, model: model ?? null })
    },
    async stop(crewId: string, taskId?: string): Promise<boolean> {
      return this.run('plugin:pilot|pilot_crew_stop', { crewId, taskId: taskId ?? null })
    },
    async resume(crewId: string): Promise<boolean> {
      return this.run('plugin:pilot|pilot_crew_resume', { crewId })
    },
    async log(crewId: string): Promise<CrewMessage[]> {
      try {
        return (await pilot<CrewMessage[]>('plugin:pilot|pilot_crew_log', { crewId })) ?? []
      } catch (e) {
        this.error = String(e)
        return []
      }
    },
    async run(command: string, args: Record<string, unknown>): Promise<boolean> {
      this.error = ''
      try {
        await pilot(command, args)
        return true
      } catch (e) {
        this.error = String(e)
        return false
      }
    },
  },
})
