/**
 * QuantCode's bookmarks — pinned lines across the workspace's files.
 *
 * The line number is NOT the anchor. Each bookmark on an open file is backed
 * by a Monaco decoration on the shared model, and Monaco moves decorations
 * when text is inserted or deleted above them — so the bookmark follows the
 * code, where a stored line number would stay put while the code walked away
 * (the VS Code Bookmarks extension's exact failure). The store reads the
 * decoration's position back after every edit and that becomes the bookmark's
 * line, both on screen and in what gets persisted.
 *
 * Files that are not open have no model; their bookmarks simply keep the line
 * they were last seen at, and re-anchor the moment a model for that path
 * appears (`onDidCreateModel`). Edits made outside the suite are invisible to
 * any editor's bookmarks — that limitation is universal without content
 * diffing, and not one this store tries to beat.
 */
import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { qs, samePath } from '@quantsuite/core'
import type { BookmarkEntry } from '../shared/types'

let counter = 0
const nextId = () => `bm-${Date.now().toString(36)}-${++counter}`

const SETTINGS_SCOPE = 'code'
const SETTINGS_KEY = 'bookmarks.byWorkspace'
/** Same cap the session uses — the oldest workspace's bookmarks age out. */
const MAX_WORKSPACES = 12

export const useBookmarksStore = defineStore('code/bookmarks', () => {
  const bookmarks = ref<BookmarkEntry[]>([])
  const workspacePath = ref<string | null>(null)

  /** Grouped for the panel: one block per file, lines ascending. */
  const byFile = computed(() => {
    const groups = new Map<string, BookmarkEntry[]>()
    for (const b of bookmarks.value) {
      const list = groups.get(b.path) ?? []
      list.push(b)
      groups.set(b.path, list)
    }
    return [...groups.entries()]
      .map(([path, list]) => ({ path, list: [...list].sort((a, b) => a.line - b.line) }))
      .sort((a, b) => a.path.localeCompare(b.path))
  })

  // ---- Monaco anchoring ----

  /** The monaco namespace, once anything has loaded it. Client only. */
  let monacoNs: any = null
  /** Bookmark id → its decoration id on the file's model. */
  const decorations = new Map<string, string>()
  /** Models this store already listens to, by model id. */
  const attached = new Set<string>()
  let installed = false

  function modelPath(model: any): string | null {
    return model?.uri?.scheme === 'file' ? (model.uri.fsPath as string) : null
  }

  function modelFor(path: string): any | null {
    if (!monacoNs) return null
    return (
      monacoNs.editor.getModels().find((m: any) => samePath(modelPath(m), path)) ?? null
    )
  }

  function lineText(model: any, line: number): string {
    if (line < 1 || line > model.getLineCount()) return ''
    return (model.getLineContent(line) as string).trim().slice(0, 80)
  }

  /** Place (or re-place) one bookmark's decoration on its file's model. */
  function anchor(b: BookmarkEntry, model: any): void {
    const line = Math.min(Math.max(1, b.line), model.getLineCount())
    const [id] = model.deltaDecorations(
      decorations.has(b.id) ? [decorations.get(b.id)!] : [],
      [
        {
          range: new monacoNs.Range(line, 1, line, 1),
          options: {
            description: 'quantcode-bookmark',
            // The whole line is the bookmark; typing at its edges must not
            // stretch the range into the neighbours.
            stickiness: monacoNs.editor.TrackedRangeStickiness.NeverGrowsWhenTypingAtEdges,
            isWholeLine: true,
            linesDecorationsClassName: 'qcode-bookmark-line',
            overviewRuler: {
              color: '#a0a0a880',
              position: monacoNs.editor.OverviewRulerLane.Center,
            },
          },
        },
      ]
    )
    decorations.set(b.id, id)
    b.line = line
    b.text = lineText(model, line)
  }

  /**
   * After an edit: every decoration's position IS its bookmark's line now.
   * A decoration that vanished (a `setValue` reset, or its line deleted) is
   * re-anchored at the last known line — the same recovery VS Code performs.
   */
  function syncFromModel(model: any): void {
    const path = modelPath(model)
    if (!path) return
    let changed = false
    for (const b of bookmarks.value) {
      if (!samePath(b.path, path)) continue
      const decId = decorations.get(b.id)
      const range = decId ? model.getDecorationRange(decId) : null
      if (range) {
        if (b.line !== range.startLineNumber) {
          b.line = range.startLineNumber
          changed = true
        }
        const text = lineText(model, b.line)
        if (b.text !== text) {
          b.text = text
          changed = true
        }
      } else {
        anchor(b, model)
        changed = true
      }
    }
    if (changed) persist()
  }

  function attach(model: any): void {
    const path = modelPath(model)
    if (!path || attached.has(model.id)) return
    if (!bookmarks.value.some((b) => samePath(b.path, path))) return
    attached.add(model.id)
    for (const b of bookmarks.value) {
      if (samePath(b.path, path)) anchor(b, model)
    }
    model.onDidChangeContent(() => syncFromModel(model))
    model.onWillDispose(() => {
      attached.delete(model.id)
      for (const b of bookmarks.value) {
        if (samePath(b.path, path)) decorations.delete(b.id)
      }
    })
  }

  /** Load Monaco lazily and start anchoring. Safe to call repeatedly. */
  async function install(): Promise<void> {
    if (installed || typeof window === 'undefined') return
    installed = true
    try {
      monacoNs = await import('monaco-editor')
    } catch {
      // No Monaco (SSR, tests) — bookmarks fall back to stored lines.
      installed = false
      return
    }
    monacoNs.editor.onDidCreateModel((m: any) => attach(m))
    for (const m of monacoNs.editor.getModels()) attach(m)
  }

  // ---- Actions ----

  /** Toggle a bookmark on a line. Returns true when one was added. */
  function toggle(path: string, line: number): boolean {
    const existing = bookmarks.value.filter((b) => samePath(b.path, path) && b.line === line)
    if (existing.length) {
      for (const b of existing) removeQuiet(b.id)
      persist()
      return false
    }
    bookmarks.value.push({ id: nextId(), path, line, text: '' })
    // Re-read through the array: `push` stores the raw object, and mutations
    // the anchor makes must go through the reactive proxy to reach the panel.
    const b = bookmarks.value[bookmarks.value.length - 1]!
    const model = modelFor(path)
    if (model) {
      attach(model)
      anchor(b, model)
    }
    persist()
    return true
  }

  function removeQuiet(id: string): void {
    const index = bookmarks.value.findIndex((b) => b.id === id)
    if (index === -1) return
    const b = bookmarks.value[index]!
    const decId = decorations.get(id)
    if (decId) {
      modelFor(b.path)?.deltaDecorations([decId], [])
      decorations.delete(id)
    }
    bookmarks.value.splice(index, 1)
  }

  function remove(id: string): void {
    removeQuiet(id)
    persist()
  }

  function clearAll(): void {
    for (const b of [...bookmarks.value]) removeQuiet(b.id)
    persist()
  }

  /** A file was deleted — its bookmarks go with it. */
  function forgetPath(path: string): void {
    const stale = bookmarks.value.filter((b) => samePath(b.path, path))
    if (!stale.length) return
    for (const b of stale) removeQuiet(b.id)
    persist()
  }

  // ---- Persistence, per workspace ----

  let saveTimer: ReturnType<typeof setTimeout> | null = null
  let ready = false

  function persist(): void {
    const key = workspacePath.value
    if (!key || !ready) return
    if (saveTimer) clearTimeout(saveTimer)
    saveTimer = setTimeout(async () => {
      try {
        const all =
          (await qs.core.getSetting<Record<string, BookmarkEntry[]>>(SETTINGS_SCOPE, SETTINGS_KEY)) ?? {}
        delete all[key]
        all[key] = bookmarks.value.map((b) => ({ ...b }))
        const keys = Object.keys(all)
        for (const stale of keys.slice(0, Math.max(0, keys.length - MAX_WORKSPACES))) delete all[stale]
        await qs.core.setSetting(SETTINGS_SCOPE, SETTINGS_KEY, all)
      } catch {
        // No backend — bookmarks live for the session.
      }
    }, 600)
  }

  /** Load this workspace's bookmarks and anchor them to whatever is open. */
  async function load(path: string | null): Promise<void> {
    ready = false
    for (const b of [...bookmarks.value]) removeQuiet(b.id)
    workspacePath.value = path
    if (!path) {
      ready = true
      return
    }
    try {
      const all = await qs.core.getSetting<Record<string, BookmarkEntry[]>>(SETTINGS_SCOPE, SETTINGS_KEY)
      bookmarks.value = (all?.[path] ?? []).map((b) => ({ ...b }))
    } catch {
      bookmarks.value = []
    }
    ready = true
    await install()
    if (monacoNs) for (const m of monacoNs.editor.getModels()) attach(m)
  }

  return {
    bookmarks,
    byFile,
    toggle,
    remove,
    clearAll,
    forgetPath,
    load,
  }
})
