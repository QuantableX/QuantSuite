/**
 * The shared data plane (PLAN-V2 §4, E3): notes and to-dos, stored once in
 * `core.db` and rendered by several surfaces at the same time — the shell
 * drawer, the QuantHUD overlay, and (in a later stage) QuantNotes.
 *
 * Storage: section documents in settings (`notes/sections`,
 * `todos/sections`). They are small ordered trees the HUD already edits as
 * a whole; a row per note would force order/parent bookkeeping into every
 * payload for no gain. (The kanban board lives in `./kanban.ts` since
 * PLAN-KANBAN-UNIFY.)
 *
 * Sync: `set_setting` and `upsert_entity`/`delete_entity` emit on the bus,
 * and Tauri broadcasts to every window — the HUD overlay updates the moment
 * the drawer saves, and vice versa. Callers pass what they got from
 * `onChange` through a JSON-equality check before adopting it (their own
 * save echoes back too).
 *
 * In the browser (plain `nuxt dev`) there is no backend: documents fall back
 * to localStorage and change events to a window-local CustomEvent, so every
 * surface stays testable.
 */

import { on as busOn } from './bus'
import { qs } from './commands'

// ── Types (field-compatible with what QuantHUD already stores) ──────────

export interface QuickNote {
  id: string
  title: string
  content: string
}
export interface NoteSection {
  id: string
  name: string
  collapsed: boolean
  notes: QuickNote[]
}

export interface SubTask {
  id: string
  title: string
  done: boolean
}
export interface TodoTask {
  id: string
  title: string
  done: boolean
  expanded: boolean
  subtasks: SubTask[]
}
export interface TodoSection {
  id: string
  name: string
  collapsed: boolean
  tasks: TodoTask[]
}

// ── Plumbing ─────────────────────────────────────────────────────────────

export function dataplaneId(): string {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 8)
}

function inTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI__' in window
}

const DEV_EVENT = 'qss-dataplane'

function devEmit(channel: string) {
  window.dispatchEvent(new CustomEvent(DEV_EVENT, { detail: channel }))
}
function devOn(channel: string, cb: () => void): () => void {
  const h = (e: Event) => {
    if ((e as CustomEvent).detail === channel) cb()
  }
  window.addEventListener(DEV_EVENT, h)
  return () => window.removeEventListener(DEV_EVENT, h)
}

/** A whole-document store in settings, localStorage-backed in the browser. */
function documentStore<T>(scope: string, key: string) {
  const devKey = `qss-${scope}-${key}-dev`

  return {
    async load(): Promise<T | null> {
      if (inTauri()) return await qs.core.getSetting<T>(scope, key)
      const raw = localStorage.getItem(devKey)
      return raw ? (JSON.parse(raw) as T) : null
    },

    async save(value: T): Promise<void> {
      if (inTauri()) {
        await qs.core.setSetting(scope, key, value)
      } else {
        localStorage.setItem(devKey, JSON.stringify(value))
        devEmit(scope)
      }
    },

    /**
     * Fires with the new document whenever any window saves it — including
     * the caller's own save. Compare before adopting.
     */
    onChange(cb: (value: T) => void): () => void {
      if (!inTauri()) {
        return devOn(scope, () => {
          const raw = localStorage.getItem(devKey)
          if (raw) cb(JSON.parse(raw) as T)
        })
      }
      return busOn<{ scope: string; key: string; value: T }>('core.setting.changed', (event) => {
        if (event.payload.scope === scope && event.payload.key === key) cb(event.payload.value)
      })
    },
  }
}

export const notesStore = documentStore<NoteSection[]>('notes', 'sections')
export const todosStore = documentStore<TodoSection[]>('todos', 'sections')

// The kanban board left this file with PLAN-KANBAN-UNIFY: kanban.db (the
// `mcp` module) is the one store for every board — see `./kanban.ts`.
