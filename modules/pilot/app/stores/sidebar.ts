/**
 * The thread sidebar (card 7): the project filter and the open shelves,
 * pin / order / settle / snooze through `pilot_thread_arrange`, the 5 s
 * undo list, deferred deletes, unread finished turns and new-thread drafts.
 * The rules themselves live in `utils/sidebar.ts`.
 *
 * Device storage holds view preferences and the unread marks only; the
 * arrangement itself is in `pilot.db`.
 */
import { defineStore } from 'pinia'
import { pilot } from '#pilot/utils/invoke'
import { usePilotStore } from '#pilot/stores/pilot'
import { useThreadStore } from '#pilot/stores/thread'
import {
  dropExpired,
  inverse,
  latestUndo,
  needsRespacing,
  pushUndo,
  respaced,
  rowStatus,
  sections,
  UNDO_MS,
  visibleOrder,
  type Arrangement,
  type RowStatus,
  type Sections,
  type SidebarThread,
  type UndoEntry,
} from '#pilot/utils/sidebar'
import type { Mode, SessionMeta } from '#pilot/types'

interface Undo extends UndoEntry {
  run: () => Promise<void> | void
}

interface Shelves {
  working: boolean
  settled: boolean
  snoozed: boolean
}

function load<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key)
    return raw ? { ...fallback, ...(JSON.parse(raw) as T) } : fallback
  } catch {
    return fallback
  }
}
function save(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value))
  } catch {
    /* storage unavailable: the preference lives for this session */
  }
}

let undoSeq = 0
const deleteTimers = new Map<string, ReturnType<typeof setTimeout>>()
let undoSweep: ReturnType<typeof setTimeout> | null = null

export const useSidebarStore = defineStore('pilot/sidebar', {
  state: () => ({
    /** A context id, or null for every project. */
    projectFilter: load<{ id: string | null }>('pilot.sidebar.filter.v1', { id: null }).id,
    shelves: load<Shelves>('pilot.sidebar.shelves.v1', { working: false, settled: false, snoozed: false }),
    undo: [] as Undo[],
    /** Deleted rows during their undo window. */
    hidden: {} as Record<string, true>,
    /** Finished while you looked elsewhere; cleared when opened. */
    unread: load<Record<string, number>>('pilot.unread.v1', {}),
    regenerating: {} as Record<string, true>,
    /** Rows whose rename field is open. */
    renaming: null as string | null,
    /** Bumps every minute so wake times and "3m ago" move on. */
    clock: Date.now(),
    error: '',
  }),

  getters: {
    /** A row's status from whatever knows best: the loaded thread, the saved state, or a V2 terminal's signals. */
    statusOf(): (id: string) => RowStatus {
      const store = usePilotStore()
      const threads = useThreadStore()
      return (id) => {
        const meta = store.sessions.find((s) => s.id === id)
        const v = threads.views[id]
        if (meta?.kind === 'terminal' && !v?.loaded) {
          const live = store.live[id]
          if (live?.alive) {
            if (live.state === 'thinking' || live.state === 'working') return 'working'
            if (live.state === 'waiting') return 'waiting'
            if (live.state === 'error') return 'error'
          }
          return 'idle'
        }
        return rowStatus(v?.loaded ? v.state : meta?.state)
      }
    },

    rows(): SidebarThread[] {
      const store = usePilotStore()
      const threads = useThreadStore()
      return store.sessions
        .filter((s) => !this.hidden[s.id])
        .map((s) => {
          const v = threads.views[s.id]
          const started = v?.turnId ? v.turns[v.turnId]?.startedAt : undefined
          return {
            id: s.id,
            contextId: s.contextId,
            pinned: s.pinned,
            sortKey: s.sortKey,
            settledAt: s.settledAt,
            snoozedUntil: s.snoozedUntil,
            status: this.statusOf(s.id),
            sentAt: started ?? s.lastActivityAt ?? s.updatedAt,
          }
        })
    },

    sections(): Sections {
      const store = usePilotStore()
      return sections(this.rows, {
        now: this.clock,
        workingSection: store.settings?.workingSection ?? true,
        contextId: this.projectFilter,
      })
    },

    /** Screen order — Ctrl+1..9 and Ctrl+Shift+[ / ] walk it. */
    visible(): SidebarThread[] {
      return visibleOrder(this.sections, this.shelves)
    },

    latestUndo(): Undo | null {
      return latestUndo(this.undo, this.clock)
    },
  },

  actions: {
    tick() {
      this.clock = Date.now()
    },

    setFilter(id: string | null) {
      this.projectFilter = id
      save('pilot.sidebar.filter.v1', { id })
    },

    toggleShelf(name: keyof Shelves, open?: boolean) {
      this.shelves[name] = open ?? !this.shelves[name]
      save('pilot.sidebar.shelves.v1', this.shelves)
    },

    snapshot(id: string): SidebarThread | null {
      return this.rows.find((r) => r.id === id) ?? null
    },

    /**
     * One change to a thread. With `undoLabel` it can be taken back for
     * five seconds ("Unpinned", "Settled", "Snoozed until …").
     */
    async arrange(id: string, a: Arrangement, undoLabel?: string) {
      const store = usePilotStore()
      const before = this.snapshot(id)
      const meta = store.sessions.find((s) => s.id === id)
      if (!before || !meta) return
      store.patchSession(id, patchOf(a))
      // Only the arrangement comes back: the row's live state may be newer than the saved one.
      try {
        const saved = await pilot<SessionMeta>('plugin:pilot|pilot_thread_arrange', { id, arrangement: a })
        if (saved) store.patchSession(id, arrangementOf(saved))
      } catch (e) {
        store.patchSession(id, arrangementOf(meta))
        this.error = e instanceof Error ? e.message : String(e)
        return
      }
      if (undoLabel) {
        const back = inverse(before, a)
        this.pushUndo(undoLabel, () => this.arrange(id, back))
      }
      if (a.sortKey !== undefined) await this.respaceIfNeeded(a.pinned ? 'pinned' : 'active')
    },

    /** The same change on several threads (the hover action dragged across rows), undone together. */
    async arrangeMany(ids: string[], a: Arrangement, undoLabel?: string) {
      const before = ids.map((id) => this.snapshot(id)).filter((t): t is SidebarThread => t !== null)
      for (const t of before) await this.arrange(t.id, a)
      if (undoLabel && before.length) {
        this.pushUndo(undoLabel, async () => {
          for (const t of before) await this.arrange(t.id, inverse(t, a))
        })
      }
    },

    async respaceIfNeeded(section: 'pinned' | 'active') {
      const list = this.sections[section]
      if (!needsRespacing(list)) return
      for (const r of respaced(list)) await this.arrange(r.id, { sortKey: r.sortKey })
    },

    pin(id: string) {
      const top = this.sections.pinned[0]?.sortKey
      return this.arrange(id, { pinned: true, ...(top !== undefined ? { sortKey: top + 1 } : {}) })
    },
    unpin(id: string) {
      return this.arrange(id, { pinned: false }, 'Unpinned')
    },
    settle(id: string) {
      return this.arrange(id, { settled: true }, 'Settled')
    },
    unsettle(id: string) {
      return this.arrange(id, { settled: false })
    },
    snooze(id: string, until: number, label: string) {
      return this.arrange(id, { snoozedUntil: until }, `Snoozed until ${label}`)
    },
    wake(id: string) {
      return this.arrange(id, { snoozedUntil: null })
    },
    setAutoSettle(id: string, on: boolean) {
      return this.arrange(id, { autoSettle: on })
    },

    pushUndo(label: string, run: () => Promise<void> | void) {
      this.clock = Date.now()
      this.undo = pushUndo(this.undo, { id: ++undoSeq, label, at: this.clock, run })
      this.scheduleSweep()
    },

    /** The toast's Undo, and Ctrl+Z outside editable fields. */
    async undoLatest() {
      this.clock = Date.now()
      const entry = latestUndo(this.undo, this.clock)
      if (!entry) return false
      this.undo = this.undo.filter((e) => e.id !== entry.id)
      await entry.run()
      return true
    },

    dismissUndo(id: number) {
      this.undo = this.undo.filter((e) => e.id !== id)
    },

    scheduleSweep() {
      if (undoSweep) clearTimeout(undoSweep)
      undoSweep = setTimeout(() => {
        undoSweep = null
        this.clock = Date.now()
        this.undo = dropExpired(this.undo, this.clock)
        if (this.undo.length) this.scheduleSweep()
      }, UNDO_MS / 5)
    },

    /** Delete after the undo window; the row disappears at once. */
    deleteDeferred(id: string) {
      const store = usePilotStore()
      const meta = store.sessions.find((s) => s.id === id)
      if (!meta) return
      this.hidden[id] = true
      if (store.activeId === id) {
        const next = this.visible.find((r) => r.id !== id)
        if (next) void store.openSession(next.id)
        else store.closeSession()
      }
      const timer = setTimeout(() => {
        deleteTimers.delete(id)
        void store.remove(id).finally(() => delete this.hidden[id])
      }, UNDO_MS)
      deleteTimers.set(id, timer)
      this.pushUndo(`Deleted "${meta.title || 'New thread'}"`, () => {
        const t = deleteTimers.get(id)
        if (t) clearTimeout(t)
        deleteTimers.delete(id)
        delete this.hidden[id]
      })
    },

    async regenerateTitle(id: string) {
      if (this.regenerating[id]) return
      this.regenerating[id] = true
      this.error = ''
      try {
        const title = await pilot<string>('plugin:pilot|pilot_thread_regenerate_title', { id })
        if (title) usePilotStore().patchSession(id, { title, titleSource: 'auto' })
      } catch (e) {
        this.error = e instanceof Error ? e.message : String(e)
      } finally {
        delete this.regenerating[id]
      }
    },

    async rename(id: string, title: string) {
      const store = usePilotStore()
      const meta = store.sessions.find((s) => s.id === id)
      this.renaming = null
      const next = title.trim()
      if (!meta || !next || next === meta.title) return
      store.patchSession(id, { title: next, titleSource: 'user' })
      await store.update(id, { title: next })
    },

    // ── Unread, "came back" and early wake ──

    markRead(id: string) {
      if (!this.unread[id]) return
      delete this.unread[id]
      save('pilot.unread.v1', this.unread)
    },

    /**
     * A row's status moved. Work that finishes or needs you comes back on
     * top of the active list (with the Working fold), wakes a snoozed thread
     * when it needs you or fails, and leaves an unread mark when you were
     * looking elsewhere.
     */
    statusChanged(id: string, from: RowStatus, to: RowStatus) {
      if (from === to) return
      const store = usePilotStore()
      const meta = store.sessions.find((s) => s.id === id)
      if (!meta) return
      const done = from === 'working' && to !== 'working'
      if (done && to !== 'waiting' && store.activeId !== id) {
        this.unread[id] = Date.now()
        save('pilot.unread.v1', this.unread)
      }
      const wakes = to === 'waiting' || to === 'error' || to === 'limited'
      if (wakes && meta.snoozedUntil && meta.snoozedUntil > Date.now()) void this.wake(id)
      if (done && (store.settings?.workingSection ?? true) && !meta.pinned && !meta.settledAt) {
        const top = this.sections.active.find((r) => r.id !== id)?.sortKey
        if (top !== undefined && top >= meta.sortKey) void this.arrange(id, { sortKey: top + 1 })
      }
    },

    // ── New threads ──

    /** An unsent thread in the context — Ctrl+N reuses it instead of piling up drafts. */
    emptyDraft(contextId: string): SessionMeta | null {
      const store = usePilotStore()
      const threads = useThreadStore()
      return (
        store.sessions.find((s) => {
          if (s.contextId !== contextId || s.kind !== 'chat' || this.hidden[s.id]) return false
          if (s.providerSessionId || s.title.trim() || s.settledAt) return false
          const v = threads.views[s.id]
          return !v?.loaded || v.items.length === 0
        }) ?? null
      )
    },

    /** Ctrl+N: the filtered project, else the open thread's; Ctrl+Alt+N: General (no project). */
    async newThread(opts: { contextId?: string; noProject?: boolean } = {}) {
      const store = usePilotStore()
      const contextId = opts.noProject
        ? 'general'
        : opts.contextId ?? this.projectFilter ?? store.active?.contextId ?? 'general'
      const draft = this.emptyDraft(contextId)
      if (draft) {
        await store.openSession(draft.id)
        return draft
      }
      const mode = (store.settings?.defaultMode ?? 'full') as Mode
      return store.createThread({ contextId, mode })
    },

    /** Open the thread `step` rows away in screen order (Ctrl+Shift+[ / ]). */
    async step(step: 1 | -1) {
      const store = usePilotStore()
      const list = this.visible
      if (!list.length) return
      const at = list.findIndex((r) => r.id === store.activeId)
      const next = at < 0 ? (step > 0 ? 0 : list.length - 1) : (at + step + list.length) % list.length
      await store.openSession(list[next].id)
    },

    async openNth(n: number) {
      const row = this.visible[n]
      if (row) await usePilotStore().openSession(row.id)
    },
  },
})

function arrangementOf(m: SessionMeta): Partial<SessionMeta> {
  return { pinned: m.pinned, sortKey: m.sortKey, settledAt: m.settledAt, snoozedUntil: m.snoozedUntil, autoSettle: m.autoSettle }
}

/** The optimistic local copy of an arrangement. */
function patchOf(a: Arrangement): Partial<SessionMeta> {
  const p: Partial<SessionMeta> = {}
  if (a.pinned !== undefined) p.pinned = a.pinned
  if (a.sortKey !== undefined) p.sortKey = a.sortKey
  if (a.settled === true) {
    p.settledAt = Date.now()
    p.pinned = false
  }
  if (a.settled === false) p.settledAt = null
  if (a.snoozedUntil !== undefined) p.snoozedUntil = a.snoozedUntil
  if (a.autoSettle !== undefined) p.autoSettle = a.autoSettle
  return p
}
