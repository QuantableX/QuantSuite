/**
 * Side chats (round 2, card 10; PandaCode's /btw, T3's side chats): a
 * thread of its own beside a main thread — same project, agent, model and
 * checkout — that asks without queuing into or steering the main run. The
 * open side chat shows in a sheet beside the conversation; its messages
 * carry the main thread's latest context (utils/side.ts). Which main
 * messages a side chat has seen is kept per machine.
 */
import { defineStore } from 'pinia'
import { usePilotStore } from '#pilot/stores/pilot'
import { useThreadStore } from '#pilot/stores/thread'
import { mainContext, sideTitle } from '#pilot/utils/side'
import { chipToken } from '#pilot/utils/mentions'
import type { ContextChip, SessionMeta } from '#pilot/types'

const SEEN_KEY = 'pilot.side.seen.v1'
function loadSeen(): Record<string, number> {
  try {
    return JSON.parse(localStorage.getItem(SEEN_KEY) ?? '{}') as Record<string, number>
  } catch {
    return {}
  }
}

export const MAIN_CONTEXT_LABEL = 'Main thread context'

export const useSideStore = defineStore('pilot/side', {
  state: () => ({
    /** The side chat in the sheet, and its main thread. */
    open: null as { mainId: string; sideId: string } | null,
    /** Per side chat: the newest main item it has seen. */
    seen: loadSeen(),
    error: '',
  }),

  getters: {
    /** The side chats of a main thread, newest first. */
    of(): (mainId: string) => SessionMeta[] {
      return (mainId) => usePilotStore().sessions.filter((s) => s.parentId === mainId).sort((a, b) => (b.lastActivityAt ?? 0) - (a.lastActivityAt ?? 0))
    },
  },

  actions: {
    /** A new side chat of `mainId` with the main thread's setup and checkout. */
    async create(mainId: string, question = '') {
      const store = usePilotStore()
      const main = store.sessions.find((s) => s.id === mainId)
      if (!main) return null
      this.error = ''
      const meta = await store.createThread(
        {
          contextId: main.contextId,
          provider: main.provider,
          model: main.model,
          effort: main.effort,
          mode: main.mode,
          title: sideTitle(question),
          // A worktree thread's side chat works in the same worktree.
          ...(main.worktreePath ? { worktreePath: main.worktreePath } : {}),
          parentId: mainId,
        },
        false,
      )
      if (!meta) {
        this.error = store.lastError || 'The side chat could not be created.'
        return null
      }
      return meta
    },

    /** /btw: ask on the side — the open side chat of this thread, else a new one. */
    async ask(mainId: string, question: string) {
      let sideId = this.open?.mainId === mainId ? this.open.sideId : null
      if (!sideId) {
        const meta = await this.create(mainId, question)
        if (!meta) return false
        sideId = meta.id
      }
      this.open = { mainId, sideId }
      if (question.trim()) return useThreadStore().send(sideId, question.trim())
      return true
    },

    show(mainId: string, sideId: string) {
      this.open = { mainId, sideId }
    },

    async startNew(mainId: string) {
      const meta = await this.create(mainId)
      if (meta) this.open = { mainId, sideId: meta.id }
    },

    close() {
      this.open = null
    },

    /**
     * What a side message carries: a chip with the main thread's context
     * since this side chat last asked (called by the thread store's send).
     */
    contextFor(sideId: string): { token: string; chip: ContextChip } | null {
      const store = usePilotStore()
      const side = store.sessions.find((s) => s.id === sideId)
      const mainId = side?.parentId
      if (!mainId) return null
      const main = store.sessions.find((s) => s.id === mainId)
      if (!main) return null
      const view = useThreadStore().views[mainId]
      const ctx = mainContext({ title: main.title, state: view?.state ?? main.state }, view?.items ?? [], this.seen[sideId] ?? -1)
      this.seen = { ...this.seen, [sideId]: ctx.lastSeq }
      try {
        localStorage.setItem(SEEN_KEY, JSON.stringify(this.seen))
      } catch {
        /* private mode */
      }
      return { token: chipToken(MAIN_CONTEXT_LABEL), chip: { kind: 'thread', label: MAIN_CONTEXT_LABEL, content: ctx.text, path: `quantpilot://thread/${mainId}` } }
    },

    /** Insert into main: the side answer as a quote chip in the main composer (parity 14's quoting). */
    insertIntoMain(sideId: string, itemId: string, text: string) {
      const mainId = usePilotStore().sessions.find((s) => s.id === sideId)?.parentId
      if (!mainId) return
      const words = text.replace(/\s+/g, ' ').trim()
      const chip = { kind: 'quote', label: `“${words.length > 40 ? `${words.slice(0, 40)}…` : words}”`, content: text, path: `pilot-item:${sideId}:${itemId}:0`, comment: null }
      window.dispatchEvent(new CustomEvent('qp-add-context', { detail: { threadId: mainId, chip } }))
    },

    /** Open as full thread: it leaves the main thread and opens in the conversation. */
    async openAsFull(sideId: string) {
      const store = usePilotStore()
      const { pilot } = await import('#pilot/utils/invoke')
      try {
        const meta = await pilot<SessionMeta>('plugin:pilot|pilot_thread_set_parent', { id: sideId, parentId: null })
        if (meta) store.upsertSession(meta)
        this.open = null
        await store.openSession(sideId)
      } catch (e) {
        this.error = e instanceof Error ? e.message : String(e)
      }
    },
  },
})
