/**
 * V3 conversation state, one view per opened thread (docs/PLAN-QUANTPILOT.md §4.2).
 *
 * A view loads with `pilot_thread_snapshot` (the newest page of items and
 * the event seq they reflect) and then applies `pilot-thread-event`
 * envelopes in seq order: events at or below the snapshot's seq are already
 * in its items and are dropped; events that arrive while the snapshot is in
 * flight wait in `pending`. Items are upserted whole; deltas append to a
 * streaming assistant or reasoning item.
 *
 * Views stay in memory while the module lives, so switching threads is
 * instant and a running thread keeps streaming in the background.
 */
import { markRaw } from 'vue'
import { defineStore } from 'pinia'
import { pilot } from '#pilot/utils/invoke'
import { usePilotStore } from '#pilot/stores/pilot'
import { useUsageStore } from '#pilot/stores/usage'
import { useSidebarStore } from '#pilot/stores/sidebar'
import { useDrawerStore } from '#pilot/stores/drawer'
import type {
  Answer,
  ProviderCatalog,
  QueueItem,
  QueueState,
  RateWindow,
  SessionMeta,
  ThreadEnvelope,
  ThreadEvent,
  ThreadItem,
  ThreadSnapshot,
  ThreadState,
  FileHit,
  MessageExtras,
  SkillInfo,
  ThreadLife,
  DrawerTerm,
} from '#pilot/types'

export interface TurnInfo {
  status: 'started' | 'completed' | 'failed' | 'interrupted' | 'limited'
  startedAt: number
  durationMs: number | null
  costUsd: number | null
}

export interface ThreadUsage {
  contextTokens: number | null
  contextWindow: number | null
  costUsd: number | null
  inputTokens: number | null
  outputTokens: number | null
}

/** A saved attachment (crate `SavedAttachment`). */
export interface SavedAttachment {
  kind: 'image' | 'file' | 'folder'
  name: string
  path: string
  size: number
  width: number | null
  height: number | null
  thumb: string | null
}

export interface ThreadView {
  id: string
  items: ThreadItem[]
  /** item id → index in `items`. */
  index: Record<string, number>
  /** The newest applied event seq. */
  seq: number
  loaded: boolean
  loading: boolean
  loadingOlder: boolean
  hasMore: boolean
  /** Envelopes that arrived before the first snapshot. */
  pending: ThreadEnvelope[]
  state: ThreadState
  turnId: string | null
  turns: Record<string, TurnInfo>
  usage: ThreadUsage
  rateLimits: RateWindow[]
  model: string | null
  /** The last send or answer that failed, shown above the composer. */
  error: string | null
  /** Bumps when a turn completes — the face goes ^^. */
  cheer: number
  /** Follow-ups waiting for the running turn, in order. */
  queue: QueueItem[]
  /** After a stop, a failure or a restart the queue waits for Resume. */
  queueHeld: boolean
  /** The provider takes messages mid-turn. */
  canSteer: boolean
  /** The queued message open in the composer for editing. */
  editingQueued: string | null
  /** Scheduled continuation, limit reset, owed history, open TUI (card 15). */
  life: ThreadLife
  /** Compact the conversation before the next message (Claude after a long pause, card 15). */
  compactFirst: boolean | null
}

function freshView(id: string): ThreadView {
  return {
    id,
    items: [],
    index: {},
    seq: 0,
    loaded: false,
    loading: false,
    loadingOlder: false,
    hasMore: false,
    pending: [],
    state: 'idle',
    turnId: null,
    turns: {},
    usage: { contextTokens: null, contextWindow: null, costUsd: null, inputTokens: null, outputTokens: null },
    rateLimits: [],
    model: null,
    error: null,
    cheer: 0,
    queue: [],
    queueHeld: false,
    canSteer: false,
    editingQueued: null,
    life: { resumeAt: null, limitResetAt: null, handoffFrom: null, tui: false, goal: null },
    compactFirst: null,
  }
}

function reindex(view: ThreadView) {
  const index: Record<string, number> = {}
  view.items.forEach((it, i) => (index[it.id] = i))
  view.index = index
}

function upsert(view: ThreadView, item: ThreadItem) {
  const at = view.index[item.id]
  if (at !== undefined) {
    view.items[at] = item
    return
  }
  // Almost always the newest item; keep seq order for the rare late one.
  const last = view.items[view.items.length - 1]
  if (!last || last.seq <= item.seq) {
    view.index[item.id] = view.items.length
    view.items.push(item)
    return
  }
  const pos = view.items.findIndex((it) => it.seq > item.seq)
  view.items.splice(pos, 0, item)
  reindex(view)
}

function turnsFromItems(items: ThreadItem[]): Record<string, TurnInfo> {
  const turns: Record<string, TurnInfo> = {}
  for (const it of items) {
    const t = turns[it.turnId]
    if (!t) {
      turns[it.turnId] = { status: 'completed', startedAt: it.createdAt, durationMs: Math.max(0, it.updatedAt - it.createdAt), costUsd: null }
    } else {
      t.durationMs = Math.max(t.durationMs ?? 0, it.updatedAt - t.startedAt)
    }
  }
  return turns
}

function stateOf(meta: SessionMeta | undefined): ThreadState {
  const s = meta?.state
  return s === 'running' || s === 'waiting' || s === 'error' || s === 'interrupted' || s === 'limited' ? s : 'idle'
}

export const useThreadStore = defineStore('pilot/thread', {
  state: () => ({
    views: {} as Record<string, ThreadView>,
    catalogs: {} as Record<string, ProviderCatalog>,
    /** Files dropped on a sidebar row, waiting for that thread's composer. */
    incoming: {} as Record<string, File[]>,
    skillCache: {} as Record<string, SkillInfo[]>,
    /** A new thread's first message to several models (card 10). */
    fanout: {} as Record<string, { provider: string; model: string }[]>,
    /** Files are being dragged over the window: the composer lights up. */
    fileDrag: false,
    /** A message to scroll to and flash (the palette's "In messages", card 17). */
    reveal: null as { threadId: string; itemId: string; nonce: number } | null,
  }),

  getters: {
    view(state): (id: string | null) => ThreadView | null {
      return (id) => (id ? state.views[id] ?? null : null)
    },
    /** A thread whose agent waits on an approval, a question or a plan. */
    waiting(state): string[] {
      return Object.values(state.views)
        .filter((v) => v.state === 'waiting')
        .map((v) => v.id)
    },
    /** A thread's pending approvals, questions and plans, oldest first. */
    pending(state): (id: string | null) => ThreadItem[] {
      return (id) => {
        const v = id ? state.views[id] : null
        if (!v) return []
        return v.items.filter(
          (i) => (i.kind === 'approval' || i.kind === 'question' || i.kind === 'plan') && i.data.status === 'pending'
        )
      }
    },
  },

  actions: {
    ensure(id: string): ThreadView {
      if (!this.views[id]) this.views[id] = freshView(id)
      return this.views[id]
    },

    /** Load (or reload) a thread's newest page. */
    async open(id: string, force = false) {
      const view = this.ensure(id)
      if ((view.loaded && !force) || view.loading) return
      view.loading = true
      try {
        const snap = await pilot<ThreadSnapshot>('plugin:pilot|pilot_thread_snapshot', { id })
        if (!snap) return
        view.items = snap.items
        reindex(view)
        view.seq = snap.seq
        view.hasMore = snap.hasMore
        view.turnId = snap.turnId
        view.turns = turnsFromItems(snap.items)
        if (snap.turnId && view.turns[snap.turnId]) view.turns[snap.turnId].status = 'started'
        view.state = snap.turnId ? (stateOf(snap.thread) === 'waiting' ? 'waiting' : 'running') : stateOf(snap.thread)
        view.usage = {
          contextTokens: snap.thread.contextTokens,
          contextWindow: snap.thread.contextWindow,
          costUsd: snap.thread.costUsd,
          inputTokens: snap.thread.inputTokens,
          outputTokens: snap.thread.outputTokens,
        }
        view.queue = snap.queue?.items ?? []
        view.queueHeld = snap.queue?.held ?? false
        view.canSteer = snap.canSteer ?? false
        if (snap.life) view.life = snap.life
        view.loaded = true
        usePilotStore().upsertSession(snap.thread)
        const pending = view.pending
        view.pending = []
        for (const env of pending) this.apply(env)
      } catch (e) {
        view.error = String(e)
      } finally {
        view.loading = false
      }
    },

    async loadOlder(id: string) {
      const view = this.views[id]
      if (!view || !view.hasMore || view.loadingOlder || !view.items.length) return
      view.loadingOlder = true
      try {
        const snap = await pilot<ThreadSnapshot>('plugin:pilot|pilot_thread_snapshot', { id, beforeSeq: view.items[0].seq })
        if (!snap) return
        const known = new Set(view.items.map((i) => i.id))
        view.items = [...snap.items.filter((i) => !known.has(i.id)), ...view.items]
        reindex(view)
        view.hasMore = snap.hasMore
        view.turns = { ...turnsFromItems(snap.items), ...view.turns }
      } finally {
        view.loadingOlder = false
      }
    },

    /**
     * One live envelope. A thread nobody opened yet only keeps its row
     * current (state, title, activity) — its snapshot will have the rest;
     * envelopes are buffered only while that snapshot loads.
     */
    apply(env: ThreadEnvelope) {
      const view = this.ensure(env.threadId)
      const sidebar = useSidebarStore()
      const before = sidebar.statusOf(env.threadId)
      if (!view.loaded) {
        if (view.loading) view.pending.push(env)
        this.patchRow(env.threadId, env.event)
      } else if (env.seq > view.seq) {
        view.seq = env.seq
        this.applyEvent(view, env.event)
        this.patchRow(env.threadId, env.event)
      }
      sidebar.statusChanged(env.threadId, before, sidebar.statusOf(env.threadId))
    },

    /** The saved row follows the live thread, so the sidebar sorts and labels it right. */
    patchRow(id: string, ev: ThreadEvent) {
      const store = usePilotStore()
      if (ev.type === 'state') store.patchSession(id, { state: ev.state })
      else if (ev.type === 'title') store.patchSession(id, { title: ev.title })
      else if (ev.type === 'turn' && ev.status !== 'started') {
        store.patchSession(id, { lastActivityAt: Date.now() })
        void this.refreshLife(id)
      }
    },

    /** The user wrote: the thread leaves the Settled and Snoozed shelves (the crate does the same). */
    wrote(id: string) {
      usePilotStore().patchSession(id, { settledAt: null, snoozedUntil: null, lastActivityAt: Date.now() })
    },

    applyEvent(view: ThreadView, ev: ThreadEvent) {
      switch (ev.type) {
        case 'item':
          upsert(view, ev.item)
          break
        case 'delta': {
          const at = view.index[ev.itemId]
          if (at !== undefined) {
            const item = view.items[at]
            item.data = { ...item.data, text: (item.data.text ?? '') + ev.text }
          }
          break
        }
        case 'turn': {
          const t = view.turns[ev.turnId] ?? { status: ev.status, startedAt: Date.now(), durationMs: null, costUsd: null }
          t.status = ev.status
          if (ev.status === 'started') {
            t.startedAt = Date.now()
            view.turnId = ev.turnId
          } else {
            t.durationMs = ev.durationMs ?? Date.now() - t.startedAt
            t.costUsd = ev.costUsd
            if (view.turnId === ev.turnId) view.turnId = null
            if (ev.status === 'completed') view.cheer++
          }
          view.turns[ev.turnId] = t
          break
        }
        case 'state':
          view.state = ev.state
          break
        case 'session':
          view.model = ev.model
          usePilotStore().patchSession(view.id, { provider: ev.provider, providerSessionId: ev.providerSessionId, kind: 'chat' })
          break
        case 'usage':
          view.usage = {
            contextTokens: ev.contextTokens ?? view.usage.contextTokens,
            contextWindow: ev.contextWindow ?? view.usage.contextWindow,
            costUsd: ev.costUsd ?? view.usage.costUsd,
            inputTokens: ev.inputTokens ?? view.usage.inputTokens,
            outputTokens: ev.outputTokens ?? view.usage.outputTokens,
          }
          break
        case 'rateLimit':
          view.rateLimits = ev.windows
          break
        case 'title':
          usePilotStore().patchSession(view.id, { title: ev.title })
          break
        case 'queue':
          view.queue = ev.items
          view.queueHeld = ev.held
          break
        case 'goal':
          view.life = { ...view.life, goal: ev.goal }
          break
      }
    },

    applyQueue(id: string, q: QueueState | null) {
      if (!q) return
      const view = this.ensure(id)
      view.queue = q.items
      view.queueHeld = q.held
    },

    /** A follow-up for after the running turn (sent at once when nothing runs). */
    async queueAdd(id: string, text: string, extras: MessageExtras = {}): Promise<boolean> {
      const view = this.ensure(id)
      view.error = null
      try {
        const wasRunning = Boolean(view.turnId)
        this.applyQueue(id, await pilot<QueueState>('plugin:pilot|pilot_queue_add', { id, text, ...extras }))
        if (!wasRunning) this.wrote(id)
        return true
      } catch (e) {
        view.error = String(e)
        return false
      }
    },

    /** A message into the running turn (starts a turn when nothing runs). */
    async steer(id: string, text: string, extras: MessageExtras = {}): Promise<boolean> {
      const view = this.ensure(id)
      view.error = null
      try {
        await pilot('plugin:pilot|pilot_thread_steer', { id, text, ...extras })
        this.wrote(id)
        return true
      } catch (e) {
        view.error = String(e)
        return false
      }
    },

    /** Open a queued message in the composer (null ends the edit). */
    editQueued(id: string, itemId: string | null) {
      this.ensure(id).editingQueued = itemId
    },

    async queueUpdate(id: string, itemId: string, text: string): Promise<boolean> {
      try {
        await pilot('plugin:pilot|pilot_queue_update', { itemId, text })
        return true
      } catch (e) {
        this.ensure(id).error = String(e)
        return false
      }
    },

    async queueRemove(id: string, itemId: string) {
      try {
        await pilot('plugin:pilot|pilot_queue_remove', { itemId })
      } catch (e) {
        this.ensure(id).error = String(e)
      }
    },

    async queueSendNow(id: string, itemId: string) {
      try {
        await pilot('plugin:pilot|pilot_queue_send_now', { itemId })
      } catch (e) {
        this.ensure(id).error = String(e)
      }
    },

    async queueResume(id: string) {
      try {
        await pilot('plugin:pilot|pilot_queue_resume', { id })
      } catch (e) {
        this.ensure(id).error = String(e)
      }
    },

    /** Files from Explorer dropped on the thread's sidebar row. */
    dropFiles(id: string, files: File[]) {
      this.incoming[id] = [...(this.incoming[id] ?? []), ...files.map((f) => markRaw(f))]
    },

    takeIncoming(id: string): File[] {
      const files = this.incoming[id] ?? []
      delete this.incoming[id]
      return files
    },

    /** Save a large paste (or a dropped text file) as a file next to the thread; returns its path. */
    async savePastedText(id: string, text: string, name = 'pasted-text'): Promise<string | null> {
      try {
        return await pilot<string>('plugin:pilot|pilot_attachment_save_text', { id, name, text })
      } catch (e) {
        this.ensure(id).error = String(e)
        return null
      }
    },

    async send(id: string, text: string, extras: MessageExtras = {}): Promise<boolean> {
      const view = this.ensure(id)
      view.error = null
      try {
        await pilot<string>('plugin:pilot|pilot_thread_send', { id, text, ...extras })
        this.wrote(id)
        // `/usage`'s limits above the box close with the next message (card 18).
        const usage = useUsageStore()
        if (usage.inline === id) usage.inline = null
        // The first message of a Worktree thread created its worktree: the row's checkout moved.
        const meta = usePilotStore().sessions.find((s) => s.id === id)
        if (meta?.workspaceMode === 'worktree' && !meta.worktreePath) void usePilotStore().refreshSessions()
        return true
      } catch (e) {
        view.error = String(e)
        return false
      }
    },

    // ── Lifecycle (card 15) ──

    async refreshLife(id: string) {
      try {
        const life = await pilot<ThreadLife>('plugin:pilot|pilot_thread_life', { id })
        if (life) this.ensure(id).life = life
      } catch {
        /* the banner keeps what it showed */
      }
    },

    /** Continue an interrupted or limited turn. */
    async resumeTurn(id: string): Promise<boolean> {
      const view = this.ensure(id)
      view.error = null
      try {
        await pilot('plugin:pilot|pilot_thread_resume', { id })
        this.wrote(id)
        void this.refreshLife(id)
        return true
      } catch (e) {
        view.error = String(e)
        return false
      }
    },

    /** Resume at `at` (the limit's reset), or cancel with null. */
    async schedule(id: string, at: number | null) {
      try {
        await pilot('plugin:pilot|pilot_thread_schedule', { id, at })
      } catch (e) {
        this.ensure(id).error = String(e)
      }
      await this.refreshLife(id)
    },

    /** A new thread from this one up to `itemId`'s turn; it opens. */
    async fork(id: string, itemId: string, worktree: boolean): Promise<SessionMeta | null> {
      try {
        const meta = await pilot<SessionMeta>('plugin:pilot|pilot_thread_fork', { id, itemId, worktree })
        if (!meta) return null
        const store = usePilotStore()
        store.upsertSession(meta)
        await store.openSession(meta.id)
        return meta
      } catch (e) {
        this.ensure(id).error = String(e)
        return null
      }
    },

    async restart(id: string): Promise<boolean> {
      try {
        await pilot('plugin:pilot|pilot_thread_restart', { id })
        return true
      } catch (e) {
        this.ensure(id).error = String(e)
        return false
      }
    },

    /** The conversation in its CLI's terminal UI, in the drawer; the thread waits meanwhile. */
    async openTui(id: string) {
      try {
        const term = await pilot<DrawerTerm>('plugin:pilot|pilot_thread_tui', { id, cols: null, rows: null })
        if (term) await useDrawerStore().adopt(id, term)
      } catch (e) {
        this.ensure(id).error = String(e)
      }
      await this.refreshLife(id)
    },

    /** The TUI closed: the session comes back, and what it added shows up. */
    async tuiDone(id: string) {
      try {
        await pilot<number>('plugin:pilot|pilot_thread_tui_done', { id })
      } catch {
        /* the next message checks again */
      }
      await this.open(id, true)
    },

    async interrupt(id: string) {
      const view = this.ensure(id)
      try {
        await pilot('plugin:pilot|pilot_thread_interrupt', { id })
      } catch (e) {
        view.error = String(e)
      }
    },

    async answer(id: string, requestId: string, answer: Answer) {
      const view = this.ensure(id)
      view.error = null
      try {
        await pilot('plugin:pilot|pilot_thread_answer', { id, requestId, answer })
      } catch (e) {
        view.error = String(e)
      }
    },

    async catalog(provider: string, refresh = false): Promise<ProviderCatalog | null> {
      if (!refresh && this.catalogs[provider]) return this.catalogs[provider]
      try {
        const c = await pilot<ProviderCatalog>('plugin:pilot|pilot_provider_catalog', { provider, refresh })
        if (c) this.catalogs[provider] = c
        return c
      } catch {
        return null
      }
    },

    forget(id: string) {
      delete this.views[id]
    },

    // ── Composer context (card 9) ──

    /** `@`: files and folders of the thread's checkout. */
    async searchFiles(id: string, query: string): Promise<FileHit[]> {
      try {
        return (await pilot<FileHit[]>('plugin:pilot|pilot_files_search', { id, query })) ?? []
      } catch {
        return []
      }
    },

    /** `$`: the thread's provider skills (asked once per thread and session). */
    async skills(id: string, refresh = false): Promise<SkillInfo[]> {
      if (!refresh && this.skillCache[id]) return this.skillCache[id]
      try {
        const list = (await pilot<SkillInfo[]>('plugin:pilot|pilot_skills', { id })) ?? []
        this.skillCache[id] = list
        return list
      } catch {
        return []
      }
    },

    async saveImage(id: string, name: string, data: string) {
      return pilot<SavedAttachment>('plugin:pilot|pilot_attachment_save_image', { id, name, data })
    },

    async saveFile(id: string, name: string, source: { data?: string; path?: string }) {
      return pilot<SavedAttachment>('plugin:pilot|pilot_attachment_save_file', { id, name, data: source.data ?? null, path: source.path ?? null })
    },
  },
})
