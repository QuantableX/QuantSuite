/**
 * Suite workspaces (docs/PLAN-WORKSPACES.md).
 *
 * **A workspace IS a folder.** Not a container you add folders to — one
 * workspace, one path, always. Multi-root was removed on 2026-08-26; nothing
 * here carries a `folders` list and nothing should grow one back.
 *
 * There is exactly ONE registry: the `core.db` entity table
 * (`module: 'core'`, `kind: 'workspace'`), with the open one in the setting
 * `core / workspace.active`. This module is that registry's only implementation
 * — the shell's `useWorkspaces()` and QuantCode's `useWorkspacesStore()` are
 * thin reactive shells over these functions and hold no persistence of their
 * own. That is what lets an agent later read every workspace the user has,
 * across modules, from one place.
 *
 * Framework-free on purpose: a Pinia store in a module and a Nuxt `useState`
 * in the shell both call in here, and neither may import the other
 * (ARCHITECTURE.md §4).
 *
 * In a plain browser (`nuxt dev`, no Tauri) everything falls back to
 * localStorage so the picker and the explorer stay testable.
 */

import type { Entity } from './types'
import { qs } from './commands'
import * as bus from './bus'

/** One workspace: a project folder the user opened. */
export interface Workspace {
  /** `core:workspace:<hash of path>` — derived from the path, so reopening
   *  the same folder upserts rather than creating a second entry. */
  id: string
  /** The folder's own name. */
  name: string
  /** THE folder. Absolute path, as the OS reported it. */
  path: string
  pinned: boolean
  lastOpenedAt: number
}

/** What `core / workspace.active` holds — the open workspace, minimally. */
export interface ActiveWorkspace {
  name: string
  path: string
}

/** Topic announcing a workspace was opened. Modules adopt the folder from here. */
export const WORKSPACE_OPENED = 'core.workspace.opened'

const SETTINGS_SCOPE = 'core'
const ACTIVE_KEY = 'workspace.active'
const REOPEN_KEY = 'workspace.reopenLast'
const DEV_STORE = 'qss-workspaces-dev'

// ---------------------------------------------------------------------------
// Environment
// ---------------------------------------------------------------------------

/**
 * Whether a Tauri backend is present. `withGlobalTauri` puts the handle on
 * `window`, so this is synchronous and safe to branch on before any invoke.
 */
function hasTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI__' in window
}

interface DevStore {
  list: Workspace[]
  active: ActiveWorkspace | null
  reopenLast: boolean
}

function readDev(): DevStore {
  if (typeof localStorage === 'undefined') return { list: [], active: null, reopenLast: false }
  try {
    const raw = JSON.parse(localStorage.getItem(DEV_STORE) ?? '{}')
    return {
      list: Array.isArray(raw.list) ? raw.list : [],
      active: raw.active ?? null,
      reopenLast: !!raw.reopenLast,
    }
  } catch {
    return { list: [], active: null, reopenLast: false }
  }
}

function writeDev(store: DevStore): void {
  if (typeof localStorage === 'undefined') return
  try {
    localStorage.setItem(DEV_STORE, JSON.stringify(store))
  } catch {
    // Private mode, quota — dev convenience only, never fatal.
  }
}

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

/** Path in one shape: forward slashes, no trailing separator, lower-cased. */
function normalize(path: string): string {
  return path.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase()
}

/**
 * The id for a folder — djb2 over the normalised path.
 *
 * Deriving it from the path (rather than a random uuid) is what makes the
 * registry idempotent: opening the same folder twice, from the dashboard and
 * from QuantCode, lands on one entry.
 */
export function workspaceIdFor(path: string): string {
  const norm = normalize(path)
  let h = 5381
  for (let i = 0; i < norm.length; i++) h = ((h << 5) + h + norm.charCodeAt(i)) >>> 0
  return `core:workspace:${h.toString(36)}`
}

/** The folder's own name — the default workspace name. */
export function workspaceNameFor(path: string): string {
  return path.replace(/\\/g, '/').replace(/\/+$/, '').split('/').pop() ?? 'Workspace'
}

/** Whether two paths mean the same folder, whatever separators they carry. */
export function samePath(a: string | null | undefined, b: string | null | undefined): boolean {
  if (!a || !b) return false
  return normalize(a) === normalize(b)
}

/** Build the entry for a folder without opening it. */
export function workspaceFor(path: string, name?: string): Workspace {
  return {
    id: workspaceIdFor(path),
    name: name ?? workspaceNameFor(path),
    path,
    pinned: false,
    lastOpenedAt: Date.now(),
  }
}

/** Pinned first, then most recently opened. The one display order. */
export function sortWorkspaces(list: Workspace[]): Workspace[] {
  return [...list].sort(
    (a, b) => Number(b.pinned) - Number(a.pinned) || b.lastOpenedAt - a.lastOpenedAt
  )
}

// ---------------------------------------------------------------------------
// Change notification
// ---------------------------------------------------------------------------

type ChangeHandler = () => void

const changeHandlers = new Set<ChangeHandler>()
let busOff: (() => void) | null = null

function notify(): void {
  for (const handler of changeHandlers) {
    try {
      handler()
    } catch (e) {
      // One bad subscriber must not stop the others (same contract as the bus).
      console.error('[workspaces] change handler threw', e)
    }
  }
}

/**
 * Subscribe to "the registry changed" — a workspace was opened, removed or
 * pinned, here or in another window.
 *
 * Opens go out on the bus and come back to every webview including this one,
 * so the handler fires once for them; removals and pins have no topic and are
 * announced locally. Either way the contract is the same: re-read, don't
 * assume what changed.
 */
export function onWorkspacesChanged(handler: ChangeHandler): () => void {
  if (!busOff && hasTauri()) {
    busOff = bus.on(WORKSPACE_OPENED, () => notify())
  }
  changeHandlers.add(handler)
  return () => {
    changeHandlers.delete(handler)
  }
}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

function fromEntity(e: Entity): Workspace {
  const payload = (e.payload ?? {}) as Partial<Workspace>
  return {
    id: e.id,
    name: e.title,
    path: payload.path ?? e.subtitle ?? '',
    pinned: !!payload.pinned,
    lastOpenedAt: payload.lastOpenedAt ?? e.updated_at,
  }
}

function toEntity(ws: Workspace): Entity {
  return {
    id: ws.id,
    module: 'core',
    kind: 'workspace',
    title: ws.name,
    subtitle: ws.path,
    route: '/canvas',
    updated_at: Date.now(),
    payload: { path: ws.path, pinned: ws.pinned, lastOpenedAt: ws.lastOpenedAt },
  }
}

/** The registry rows, or a throw — `listWorkspaces` is the forgiving face of this. */
async function listRegistered(): Promise<Workspace[]> {
  const entities = await qs.core.listEntities({ module: 'core', kind: 'workspace', limit: 200 })
  return entities.map(fromEntity).filter((w) => !!w.path)
}

/** Every known workspace, unsorted. Empty on failure — never throws. */
export async function listWorkspaces(): Promise<Workspace[]> {
  if (!hasTauri()) return readDev().list
  try {
    return await listRegistered()
  } catch (e) {
    console.error('[workspaces] listing failed', e)
    return []
  }
}

/** The setting as stored, unvalidated. */
async function readActiveSetting(): Promise<ActiveWorkspace | null> {
  try {
    const active = (await qs.core.getSetting<ActiveWorkspace>(SETTINGS_SCOPE, ACTIVE_KEY)) ?? null
    return active?.path ? active : null
  } catch {
    return null
  }
}

/**
 * The open workspace, or null.
 *
 * `workspace.active` is a pointer INTO the registry, and a pointer whose
 * folder is not registered is nothing — not a workspace. The Rust side
 * (`qs_core::workspaces::active`) has read it that way from the start; the
 * webview used to follow the raw path, so when a module that is gone
 * (QuantControl's crews, 2026-09-02) left `modules/memory/vault/Crew` in the
 * setting, every canvas terminal opened there. A dangling pointer is cleared
 * on sight, so every reader agrees from then on. Only a listing that
 * actually answered may judge: a failed read says nothing about the
 * registry, and the pointer is kept then.
 */
export async function getActiveWorkspace(): Promise<ActiveWorkspace | null> {
  if (!hasTauri()) {
    const store = readDev()
    if (store.active && !store.list.some((w) => samePath(w.path, store.active?.path))) {
      writeDev({ ...store, active: null })
      return null
    }
    return store.active
  }
  const active = await readActiveSetting()
  if (!active) return null
  let registered: Workspace[]
  try {
    registered = await listRegistered()
  } catch {
    return active
  }
  if (registered.some((w) => samePath(w.path, active.path))) return active
  console.warn(`[workspaces] the open workspace "${active.name}" (${active.path}) is not registered — closing it`)
  await clearActiveWorkspace().catch(() => {})
  return null
}

/**
 * No open workspace. The one way the pointer is unset: when its target
 * leaves the registry (`removeWorkspace`) or turned out not to be in it
 * (`getActiveWorkspace`). Announces nothing — the callers re-read.
 */
export async function clearActiveWorkspace(): Promise<void> {
  if (!hasTauri()) {
    writeDev({ ...readDev(), active: null })
    return
  }
  await qs.core.setSetting(SETTINGS_SCOPE, ACTIVE_KEY, null)
}

/** Whether a cold start reopens the last workspace (setting, off by default). */
export async function getReopenLast(): Promise<boolean> {
  if (!hasTauri()) return readDev().reopenLast
  try {
    return (await qs.core.getSetting<boolean>(SETTINGS_SCOPE, REOPEN_KEY)) ?? false
  } catch {
    return false
  }
}

export async function setReopenLast(value: boolean): Promise<void> {
  if (!hasTauri()) {
    writeDev({ ...readDev(), reopenLast: value })
    return
  }
  await qs.core.setSetting(SETTINGS_SCOPE, REOPEN_KEY, value)
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

/**
 * Open a workspace — THE write path.
 *
 * Bumps recency, upserts the entity, marks it active, announces it. Nothing
 * else in the suite may write `core / workspace.active`: every surface that
 * cares learns about the change from `core.workspace.opened`.
 *
 * Throws on failure so callers can surface it; a silently failed open looks
 * exactly like a click that did nothing.
 */
export async function openWorkspace(input: Workspace): Promise<Workspace> {
  const ws: Workspace = { ...input, lastOpenedAt: Date.now() }
  const active: ActiveWorkspace = { name: ws.name, path: ws.path }

  if (!hasTauri()) {
    const store = readDev()
    const i = store.list.findIndex((w) => w.id === ws.id)
    if (i >= 0) store.list[i] = ws
    else store.list.push(ws)
    writeDev({ ...store, active })
    notify()
    return ws
  }

  await qs.core.upsertEntity(toEntity(ws))
  await qs.core.setSetting(SETTINGS_SCOPE, ACTIVE_KEY, active)
  // Comes back to this webview too, so `onWorkspacesChanged` fires once here
  // and once in every other window — no local notify, or it would fire twice.
  await bus.emit(WORKSPACE_OPENED, active)
  return ws
}

/** Open a folder by path, creating its entry if this is the first time. */
export async function openWorkspacePath(path: string, name?: string): Promise<Workspace> {
  const known = (await listWorkspaces()).find((w) => samePath(w.path, path))
  return openWorkspace(known ?? workspaceFor(path, name))
}

/**
 * Folder dialog → open. Returns null when the user cancelled.
 *
 * The dialog plugin is imported lazily: in a plain browser it does not exist,
 * and asking plainly beats an unexplained no-op.
 */
export async function openFolderAsWorkspace(): Promise<Workspace | null> {
  let path: string | null = null
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const selected = await open({ directory: true, multiple: false, title: 'Open workspace folder' })
    if (typeof selected === 'string') path = selected
  } catch {
    path = typeof prompt === 'function' ? prompt('Folder path:') : null
  }
  if (!path) return null
  return openWorkspacePath(path)
}

/**
 * Forget a saved workspace. The folder on disk is not touched. Forgetting
 * the open one closes it too — the active pointer must not outlive its
 * target, or every folder-bound surface keeps working in a folder the list
 * no longer shows.
 */
export async function removeWorkspace(id: string): Promise<void> {
  if (!hasTauri()) {
    const store = readDev()
    const removed = store.list.find((w) => w.id === id)
    store.list = store.list.filter((w) => w.id !== id)
    if (removed && samePath(store.active?.path, removed.path)) store.active = null
    writeDev(store)
    notify()
    return
  }
  const removed = (await listWorkspaces()).find((w) => w.id === id)
  await qs.core.deleteEntity(id)
  const active = await readActiveSetting()
  if (removed && samePath(active?.path, removed.path)) await clearActiveWorkspace()
  notify()
}

/** Pin or unpin — pinned entries sort to the top of every list. */
export async function setWorkspacePinned(id: string, pinned: boolean): Promise<void> {
  if (!hasTauri()) {
    const store = readDev()
    const ws = store.list.find((w) => w.id === id)
    if (!ws) return
    ws.pinned = pinned
    writeDev(store)
    notify()
    return
  }

  const ws = (await listWorkspaces()).find((w) => w.id === id)
  if (!ws) return
  await qs.core.upsertEntity(toEntity({ ...ws, pinned }))
  notify()
}
