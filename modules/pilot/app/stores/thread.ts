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
import { defineStore } from 'pinia'
import { pilot } from '#pilot/utils/invoke'
import { usePilotStore } from '#pilot/stores/pilot'
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

    /** One live envelope. */
    apply(env: ThreadEnvelope) {
      const view = this.ensure(env.threadId)
      if (!view.loaded) {
        view.pending.push(env)
        return
      }
      if (env.seq <= view.seq) return
      view.seq = env.seq
      this.applyEvent(view, env.event)
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
      }
    },

    applyQueue(id: string, q: QueueState | null) {
      if (!q) return
      const view = this.ensure(id)
      view.queue = q.items
      view.queueHeld = q.held
    },

    /** A follow-up for after the running turn (sent at once when nothing runs). */
    async queueAdd(id: string, text: string): Promise<boolean> {
      const view = this.ensure(id)
      view.error = null
      try {
        this.applyQueue(id, await pilot<QueueState>('plugin:pilot|pilot_queue_add', { id, text }))
        return true
      } catch (e) {
        view.error = String(e)
        return false
      }
    },

    /** A message into the running turn (starts a turn when nothing runs). */
    async steer(id: string, text: string): Promise<boolean> {
      const view = this.ensure(id)
      view.error = null
      try {
        await pilot('plugin:pilot|pilot_thread_steer', { id, text })
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

    /** Save a large paste as a file next to the thread; returns its path. */
    async savePastedText(id: string, text: string): Promise<string | null> {
      try {
        return await pilot<string>('plugin:pilot|pilot_attachment_save_text', { id, name: 'pasted-text', text })
      } catch (e) {
        this.ensure(id).error = String(e)
        return null
      }
    },

    async send(id: string, text: string): Promise<boolean> {
      const view = this.ensure(id)
      view.error = null
      try {
        await pilot<string>('plugin:pilot|pilot_thread_send', { id, text })
        return true
      } catch (e) {
        view.error = String(e)
        return false
      }
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
  },
})
