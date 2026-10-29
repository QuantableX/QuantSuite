/**
 * The inspector: a resizable split inside the main area, beside the
 * conversation (docs/PLAN-QUANTPILOT.md §3). Card 11 brings its Diff tab;
 * Files (13) and Browser (14) join it. Open state, tab and width are a
 * per-device preference.
 */
import { defineStore } from 'pinia'

export type DiffScope = 'turn' | 'uncommitted' | 'branch'
export type InspectorTab = 'diff'

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
    tab: 'diff' as InspectorTab,
    width: load('pilot.inspector.width', 560),
    /** What the Diff tab shows; bumps `nonce` so the same target reopens. */
    target: { scope: 'turn' as DiffScope, turn: null as string | null, path: null as string | null, nonce: 0 },
    sideBySide: load('pilot.diff.split', true),
    wrap: load('pilot.diff.wrap', false),
  }),
  actions: {
    toggle(open?: boolean) {
      this.open = open ?? !this.open
      save('pilot.inspector.open', this.open)
    },
    /** Ctrl+D: the Diff tab in or out. */
    toggleTab(tab: InspectorTab) {
      if (this.open && this.tab === tab) this.toggle(false)
      else {
        this.tab = tab
        this.toggle(true)
      }
    },
    showDiff(scope: DiffScope, turn: string | null = null, path: string | null = null) {
      this.target = { scope, turn, path, nonce: this.target.nonce + 1 }
      this.tab = 'diff'
      this.toggle(true)
    },
    setWidth(px: number) {
      this.width = Math.round(px)
      save('pilot.inspector.width', this.width)
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
  },
})
