/**
 * The terminal drawer (card 13): per thread, tabs of shells in its
 * checkout — a tab can be split — and the project actions' Run terminals.
 * The PTYs live in the console's registry (owner `pilot-drawer`, the crate
 * tracks which belong to which thread); the layout of tabs and splits is
 * kept per device. Output is watched for the dev servers a terminal
 * starts, and for whether a shell sits idle at its prompt (those close
 * when the thread settles; running ones stay).
 */
import { defineStore } from 'pinia'
import { pilot } from '#pilot/utils/invoke'
import { atPrompt, serverUrls } from '#pilot/utils/terminal'
import type { DrawerTerm, ProjectAction } from '#pilot/types'

export { serverUrls }

export interface DrawerTab {
  id: string
  panes: string[]
  dir: 'row' | 'column'
  label: string
  action: string | null
  /** The pane with focus. */
  active: string
}

export interface DevServer {
  port: number
  url: string
  /** The drawer terminal that printed it; `null` when an agent's command did. */
  ptyId: string | null
  label: string
}

const LAYOUT_KEY = 'pilot.drawer.layout'

function loadLayout(): Record<string, DrawerTab[]> {
  try {
    return JSON.parse(localStorage.getItem(LAYOUT_KEY) ?? '{}') as Record<string, DrawerTab[]>
  } catch {
    return {}
  }
}

/** Recent output per terminal, outside the reactive state. */
const tails = new Map<string, { text: string; at: number }>()

export const useDrawerStore = defineStore('pilot/drawer', {
  state: () => ({
    open: {} as Record<string, boolean>,
    height: (() => {
      try {
        return Number(localStorage.getItem('pilot.drawer.height')) || 260
      } catch {
        return 260
      }
    })(),
    tabs: loadLayout(),
    activeTab: {} as Record<string, string>,
    servers: {} as Record<string, DevServer[]>,
    exited: {} as Record<string, boolean>,
    error: {} as Record<string, string>,
  }),

  getters: {
    threadOfPty(state): Record<string, string> {
      const out: Record<string, string> = {}
      for (const [tid, tabs] of Object.entries(state.tabs)) for (const t of tabs) for (const p of t.panes) out[p] = tid
      return out
    },
  },

  actions: {
    persist() {
      try {
        localStorage.setItem(LAYOUT_KEY, JSON.stringify(this.tabs))
      } catch {
        /* the layout lives for this session */
      }
    },
    setHeight(px: number) {
      this.height = Math.round(px)
      try {
        localStorage.setItem('pilot.drawer.height', String(this.height))
      } catch {
        /* session only */
      }
    },

    /** The thread's live terminals, in the remembered layout; new ones (a Run hook) get a tab each. */
    async load(threadId: string) {
      let terms: DrawerTerm[] = []
      try {
        terms = (await pilot<DrawerTerm[]>('plugin:pilot|pilot_terminals', { id: threadId })) ?? []
      } catch {
        return
      }
      const alive = new Map(terms.map((t) => [t.ptyId, t]))
      const tabs = (this.tabs[threadId] ?? [])
        .map((t) => ({ ...t, panes: t.panes.filter((p) => alive.has(p)) }))
        .filter((t) => t.panes.length)
        .map((t) => ({ ...t, active: t.panes.includes(t.active) ? t.active : t.panes[0] }))
      const placed = new Set(tabs.flatMap((t) => t.panes))
      for (const t of terms) if (!placed.has(t.ptyId)) tabs.push(this.tabFor(t))
      this.tabs = { ...this.tabs, [threadId]: tabs }
      if (!tabs.some((t) => t.id === this.activeTab[threadId])) this.activeTab[threadId] = tabs.at(-1)?.id ?? ''
      this.persist()
    },

    /** A terminal the crate opened (the TUI): its own tab, in front. */
    async adopt(threadId: string, t: DrawerTerm) {
      this.open = { ...this.open, [threadId]: true }
      if (this.placed(threadId, t.ptyId)) return
      this.tabs = { ...this.tabs, [threadId]: [...(this.tabs[threadId] ?? []), this.tabFor(t)] }
      this.activeTab[threadId] = `tab-${t.ptyId}`
      this.persist()
    },

    tabFor(t: DrawerTerm): DrawerTab {
      return { id: `tab-${t.ptyId}`, panes: [t.ptyId], dir: 'row', label: t.label, action: t.action, active: t.ptyId }
    },

    toggle(threadId: string, open?: boolean) {
      const next = open ?? !this.open[threadId]
      this.open = { ...this.open, [threadId]: next }
      if (next && !(this.tabs[threadId] ?? []).length) void this.newTab(threadId)
    },

    async spawn(threadId: string, action: string | null = null): Promise<DrawerTerm | null> {
      try {
        const t = await pilot<DrawerTerm>('plugin:pilot|pilot_terminal_open', { id: threadId, action, cols: null, rows: null })
        delete this.error[threadId]
        return t
      } catch (e) {
        this.error = { ...this.error, [threadId]: String(e) }
        return null
      }
    },

    /** A terminal already in a tab (a `load` that ran meanwhile placed it): that tab in front. */
    placed(threadId: string, ptyId: string): boolean {
      const tab = (this.tabs[threadId] ?? []).find((x) => x.panes.includes(ptyId))
      if (tab) this.activeTab[threadId] = tab.id
      return Boolean(tab)
    },

    async newTab(threadId: string) {
      const t = await this.spawn(threadId)
      if (!t) return
      this.open = { ...this.open, [threadId]: true }
      if (this.placed(threadId, t.ptyId)) return
      const tabs = [...(this.tabs[threadId] ?? []), this.tabFor(t)]
      this.tabs = { ...this.tabs, [threadId]: tabs }
      this.activeTab[threadId] = tabs.at(-1)!.id
      this.open = { ...this.open, [threadId]: true }
      this.persist()
    },

    /** Ctrl+D (beside) / Ctrl+Shift+D (below): one more shell in the tab in front. */
    async split(threadId: string, dir: 'row' | 'column') {
      const tab = this.current(threadId)
      if (!tab) return this.newTab(threadId)
      const t = await this.spawn(threadId)
      if (!t) return
      // A `load` meanwhile may have given it a tab of its own: it joins this one instead.
      const others = (this.tabs[threadId] ?? []).filter((x) => x.id !== tab.id && !(x.panes.length === 1 && x.panes[0] === t.ptyId))
      const fresh = (this.tabs[threadId] ?? []).find((x) => x.id === tab.id) ?? tab
      if (fresh.panes.includes(t.ptyId)) return
      this.tabs = { ...this.tabs, [threadId]: (this.tabs[threadId] ?? []).filter((x) => others.includes(x) || x.id === tab.id) }
      this.updateTab(threadId, tab.id, { panes: [...fresh.panes, t.ptyId], dir: fresh.panes.length > 1 ? fresh.dir : dir, active: t.ptyId })
    },

    current(threadId: string): DrawerTab | null {
      const tabs = this.tabs[threadId] ?? []
      return tabs.find((t) => t.id === this.activeTab[threadId]) ?? tabs[0] ?? null
    },

    updateTab(threadId: string, tabId: string, patch: Partial<DrawerTab>) {
      this.tabs = { ...this.tabs, [threadId]: (this.tabs[threadId] ?? []).map((t) => (t.id === tabId ? { ...t, ...patch } : t)) }
      this.persist()
    },

    /** Ctrl+W: the pane with focus; the tab goes with its last pane, the drawer with its last tab. */
    async closePane(threadId: string, ptyId: string) {
      await pilot('plugin:pilot|pilot_terminal_close', { ptyId }).catch(() => undefined)
      tails.delete(ptyId)
      const tabs = (this.tabs[threadId] ?? [])
        .map((t) => {
          const panes = t.panes.filter((p) => p !== ptyId)
          return { ...t, panes, active: t.active === ptyId ? (panes.at(-1) ?? '') : t.active }
        })
        .filter((t) => t.panes.length)
      this.tabs = { ...this.tabs, [threadId]: tabs }
      if (!tabs.some((t) => t.id === this.activeTab[threadId])) this.activeTab[threadId] = tabs.at(-1)?.id ?? ''
      if (!tabs.length) this.open = { ...this.open, [threadId]: false }
      this.servers = { ...this.servers, [threadId]: (this.servers[threadId] ?? []).filter((s) => s.ptyId !== ptyId) }
      this.persist()
    },

    async closeTab(threadId: string, tabId: string) {
      const tab = (this.tabs[threadId] ?? []).find((t) => t.id === tabId)
      for (const p of tab?.panes ?? []) await this.closePane(threadId, p)
    },

    /** A project action: its Run terminal (a live one is reused) in front. */
    async runAction(threadId: string, action: ProjectAction) {
      const t = await this.spawn(threadId, action.id)
      if (!t) return
      const tabs = this.tabs[threadId] ?? []
      const existing = tabs.find((x) => x.panes.includes(t.ptyId))
      if (existing) this.activeTab[threadId] = existing.id
      else {
        this.tabs = { ...this.tabs, [threadId]: [...tabs, this.tabFor(t)] }
        this.activeTab[threadId] = `tab-${t.ptyId}`
        this.persist()
      }
      this.open = { ...this.open, [threadId]: true }
    },

    /** Output from any PTY: the drawer's own feed the tails and the server list. */
    onOutput(ptyId: string, data: string) {
      const threadId = this.threadOfPty[ptyId]
      if (!threadId) return
      const prev = tails.get(ptyId)?.text ?? ''
      tails.set(ptyId, { text: (prev + data).slice(-800), at: Date.now() })
      const found = serverUrls(data)
      if (!found.length) return
      const tab = (this.tabs[threadId] ?? []).find((t) => t.panes.includes(ptyId))
      const list = [...(this.servers[threadId] ?? [])]
      for (const s of found) {
        if (!list.some((x) => x.port === s.port)) list.push({ ...s, ptyId, label: tab?.label ?? 'Terminal' })
      }
      this.servers = { ...this.servers, [threadId]: list }
    },

    /** The thread settled: shells idle at their prompt close; running ones (and Run terminals) stay. */
    async closeIdle(threadId: string) {
      for (const tab of [...(this.tabs[threadId] ?? [])]) {
        if (tab.action) continue
        for (const p of tab.panes) {
          const t = tails.get(p)
          if (t && Date.now() - t.at > 2000 && atPrompt(t.text)) await this.closePane(threadId, p)
        }
      }
    },
  },
})
