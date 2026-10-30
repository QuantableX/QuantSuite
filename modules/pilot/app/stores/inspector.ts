/**
 * The inspector: a resizable split inside the main area, beside the
 * conversation (docs/PLAN-QUANTPILOT.md §3). Its tabs — Diff (card 11),
 * Files (card 13), Browser (card 14), Agent (card 16: a subagent's
 * transcript, read-only) — open on demand, close with Ctrl+W
 * and come back with Ctrl+Shift+T. Open state, tabs and view options are a
 * per-device preference; the split's width is remembered per thread.
 */
import { defineStore } from 'pinia'

export type DiffScope = 'turn' | 'uncommitted' | 'branch'
export type InspectorTab = 'diff' | 'files' | 'browser' | 'agent'

const TABS: InspectorTab[] = ['diff', 'files', 'browser', 'agent']
const DEFAULT_WIDTH = 560

function load<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key)
    return raw === null ? fallback : (JSON.parse(raw) as T)
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

export const useInspectorStore = defineStore('pilot/inspector', {
  state: () => ({
    open: load('pilot.inspector.open', false),
    /** The open tabs, in order. */
    tabs: load<InspectorTab[]>('pilot.inspector.tabs', ['diff']).filter((t) => TABS.includes(t)),
    tab: load<InspectorTab>('pilot.inspector.tab', 'diff'),
    /** Closed tabs, newest last (Ctrl+Shift+T). */
    closed: [] as InspectorTab[],
    /** The split's width per thread. */
    widths: load<Record<string, number>>('pilot.inspector.widths', {}),
    /** What the Diff tab shows; bumps `nonce` so the same target reopens. */
    target: { scope: 'turn' as DiffScope, turn: null as string | null, path: null as string | null, nonce: 0 },
    sideBySide: load('pilot.diff.split', true),
    wrap: load('pilot.diff.wrap', false),
    /** The file the Files tab shows, per thread; `nonce` re-reveals the same line. */
    file: {} as Record<string, { path: string; line: number | null; nonce: number }>,
    /** Files tab: the tree or the project search beside the viewer. */
    filesMode: 'tree' as 'tree' | 'search',
    showTree: load('pilot.files.tree', true),
    /** Ctrl+P is open. */
    picker: false,
    /** Bumped by Ctrl+Shift+F to focus the search field. */
    searchNonce: 0,
    /** The Browser tab's page per thread (card 14); empty = the start page. */
    browserUrl: load<Record<string, string>>('pilot.browser.urls', {}),
    browserZoom: load('pilot.browser.zoom', 1),
    /** Bumped when something opens a page (a server's Open). */
    browserNonce: 0,
    /** The subagent the Agent tab shows, per thread: its tool call id. */
    agent: {} as Record<string, string>,
  }),
  actions: {
    persist() {
      save('pilot.inspector.open', this.open)
      save('pilot.inspector.tabs', this.tabs)
      save('pilot.inspector.tab', this.tab)
    },
    toggle(open?: boolean) {
      this.open = open ?? !this.open
      if (this.open && !this.tabs.length) this.tabs = ['diff']
      if (this.open && !this.tabs.includes(this.tab)) this.tab = this.tabs[0]
      this.persist()
    },
    /** A tab in front, opening it (and the inspector) when needed. */
    show(tab: InspectorTab) {
      if (!this.tabs.includes(tab)) this.tabs = TABS.filter((t) => t === tab || this.tabs.includes(t))
      this.closed = this.closed.filter((t) => t !== tab)
      this.tab = tab
      this.open = true
      this.persist()
    },
    /** Ctrl+D and friends: the tab in front, or the inspector closed when it already is. */
    toggleTab(tab: InspectorTab) {
      if (this.open && this.tab === tab) this.toggle(false)
      else this.show(tab)
    },
    /** Ctrl+W: close a tab; the last one closes the inspector. */
    closeTab(which?: InspectorTab) {
      const tab = which ?? this.tab
      if (!this.tabs.includes(tab)) return
      const i = this.tabs.indexOf(tab)
      this.tabs = this.tabs.filter((t) => t !== tab)
      this.closed = [...this.closed.filter((t) => t !== tab), tab]
      if (!this.tabs.length) this.open = false
      else if (this.tab === tab) this.tab = this.tabs[Math.min(i, this.tabs.length - 1)]
      this.persist()
    },
    /** Ctrl+Shift+T: the tab closed last comes back. */
    reopenTab() {
      const tab = this.closed.at(-1)
      if (tab) this.show(tab)
    },
    showDiff(scope: DiffScope, turn: string | null = null, path: string | null = null) {
      this.target = { scope, turn, path, nonce: this.target.nonce + 1 }
      this.show('diff')
    },
    /** A file in the Files tab, at a line. */
    openFile(threadId: string, path: string, line: number | null = null) {
      this.file[threadId] = { path, line, nonce: (this.file[threadId]?.nonce ?? 0) + 1 }
      this.filesMode = this.filesMode === 'search' ? 'search' : 'tree'
      this.show('files')
    },
    /** Ctrl+Shift+F: the Files tab with the project search. */
    showSearch() {
      this.filesMode = 'search'
      this.setShowTree(true)
      this.searchNonce++
      this.show('files')
    },
    /** A page in the Browser tab of a thread. */
    openBrowser(threadId: string, url: string) {
      this.setBrowserUrl(threadId, url)
      this.browserNonce++
      this.show('browser')
    },
    /** A subagent's transcript in the Agent tab. */
    openAgent(threadId: string, callId: string) {
      this.agent = { ...this.agent, [threadId]: callId }
      this.show('agent')
    },
    setBrowserUrl(threadId: string, url: string) {
      const next = { ...this.browserUrl }
      if (url) next[threadId] = url
      else delete next[threadId]
      this.browserUrl = next
      save('pilot.browser.urls', next)
    },
    setBrowserZoom(z: number) {
      this.browserZoom = z
      save('pilot.browser.zoom', z)
    },
    widthOf(threadId: string): number {
      return this.widths[threadId] ?? this.widths['*'] ?? DEFAULT_WIDTH
    },
    setWidth(threadId: string, px: number) {
      const w = Math.round(px)
      // The last width a thread got is the default for threads without their own.
      this.widths = { ...this.widths, [threadId]: w, '*': w }
      save('pilot.inspector.widths', this.widths)
    },
    setView(patch: { sideBySide?: boolean; wrap?: boolean }) {
      if (patch.sideBySide !== undefined) {
        this.sideBySide = patch.sideBySide
        save('pilot.diff.split', patch.sideBySide)
      }
      if (patch.wrap !== undefined) {
        this.wrap = patch.wrap
        save('pilot.diff.wrap', patch.wrap)
      }
    },
    setShowTree(v: boolean) {
      this.showTree = v
      save('pilot.files.tree', v)
    },
  },
})
