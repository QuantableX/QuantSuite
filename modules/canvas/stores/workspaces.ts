/**
 * QuantCode's reactive view of the suite workspaces.
 *
 * A workspace **is** a folder — one path, no multi-root
 * (docs/PLAN-WORKSPACES.md, 2026-08-26). `foldersOf()`, `addFolder()` and
 * `removeFolder()` are gone; so is this module's own `workspaces.json`.
 *
 * The registry is `@quantsuite/core`'s `workspaces` module (`core.db`
 * entities + the setting `core / workspace.active`) — the same one the shell's
 * dashboard writes. That is the whole point: one list, one id space, no
 * adoption step between them. This store adds reactivity and nothing else.
 *
 * The module reads `@quantsuite/core`, never the shell (ARCHITECTURE.md §4).
 */
import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import {
  getActiveWorkspace,
  listWorkspaces,
  onWorkspacesChanged,
  openFolderAsWorkspace,
  openWorkspace,
  openWorkspacePath,
  removeWorkspace as removeFromRegistry,
  samePath,
  setWorkspacePinned,
  sortWorkspaces,
  workspaceIdFor,
  workspaceNameFor,
  type Workspace,
} from '@quantsuite/core'

export const useWorkspacesStore = defineStore('canvas/workspaces', () => {
  // ---- State ----
  const workspaces = ref<Workspace[]>([])
  const activePath = ref<string | null>(null)
  const activeName = ref<string | null>(null)
  const loaded = ref(false)
  /** The registry has been read at least once — before that, "nothing
   *  active" means "not known yet", not "nothing open". */
  const settled = ref(false)

  // ---- Getters ----

  /** Pinned first, then most recently opened — the switcher's order. */
  const sorted = computed(() => sortWorkspaces(workspaces.value))

  /**
   * The open workspace. Falls back to a synthetic entry built from the active
   * path: the setting is what QuantCode actually works against, and an entity
   * row that failed to write must not leave the explorer looking empty.
   */
  const activeWorkspace = computed<Workspace | undefined>(() => {
    const path = activePath.value
    if (!path) return undefined
    return (
      workspaces.value.find((w) => samePath(w.path, path)) ?? {
        id: workspaceIdFor(path),
        name: activeName.value ?? workspaceNameFor(path),
        path,
        pinned: false,
        lastOpenedAt: 0,
      }
    )
  })

  const activeWorkspaceId = computed<string | null>(() => activeWorkspace.value?.id ?? null)

  // ---- The CANVAS host's content view (2026-08-31) ----
  //
  // The canvas board follows what the canvas module's own explorer shows —
  // and the "General" view is a content context OF ITS OWN (user decision:
  // the middle area must never keep the previous workspace's content).
  // `contentWorkspaceId` is what every canvas surface keys its state by:
  // the active workspace, or the General sentinel. The sentinel is not a
  // folder, so General's board lives in memory and is never written to any
  // workspace's private storage.

  /** The canvas board's key for the General view — not a folder on disk. */
  const GENERAL_CONTENT = '__general__'

  function readCanvasGeneral(): boolean {
    try {
      const raw = JSON.parse(localStorage.getItem('qc-explorer-sel:canvas') ?? 'null')
      return !!(raw && typeof raw === 'object' && raw.general)
    } catch {
      return false
    }
  }

  const canvasGeneral = ref(readCanvasGeneral())

  function setCanvasGeneral(on: boolean): void {
    canvasGeneral.value = on
  }

  /**
   * The workspace the canvas host's explorer shows, when it is a workspace
   * (its per-host pin, or the suite-active it fell back to). The explorer
   * emits on every effective change; `null` until it has spoken, and after
   * the title bar switched workspaces itself.
   *
   * The explorer's pin and the suite-global `workspace.active` can differ:
   * every host keeps its own pin (2026-08-31), and the pointer can be unset
   * from outside (2026-09-02). The board follows THIS host's explorer —
   * what is in front of the user — so a terminal opens in the folder the
   * tree shows, not in whatever the pointer happens to say.
   */
  const explorerPick = ref<string | null>(null)

  function setExplorerSelection(s: { general: boolean; pinned: boolean; workspaceId: string | null }): void {
    canvasGeneral.value = s.pinned
    explorerPick.value = s.general ? null : s.workspaceId
  }

  /** The title bar picked a workspace: the board follows the suite-active
   *  until the explorer speaks again. */
  function followActive(): void {
    explorerPick.value = null
  }

  /**
   * What the canvas board shows: a workspace id, or the General sentinel.
   *
   * The explorer's workspace when the registry knows it, else the
   * suite-active one. A board must have a context: with nothing open — the
   * registry read and no workspace anywhere — that context is General, the
   * one that is not a folder (2026-09-02; before, a dangling
   * `workspace.active` always supplied an id, and once it read as none,
   * `createWindow` had nothing to add to and the canvas could not open a
   * terminal). `null` only until the registry has been read.
   */
  const contentWorkspaceId = computed<string | null>(() => {
    if (canvasGeneral.value) return GENERAL_CONTENT
    const picked = explorerPick.value
    if (picked && workspaces.value.some((w) => w.id === picked)) return picked
    return activeWorkspaceId.value ?? (settled.value ? GENERAL_CONTENT : null)
  })

  /** The board's workspace as a FOLDER — undefined in the General view, so
   *  folder-bound surfaces (terminal cwd, specs, notes) show their
   *  no-workspace state instead of writing into the wrong project. */
  const contentWorkspace = computed<Workspace | undefined>(() => {
    const id = contentWorkspaceId.value
    if (!id || id === GENERAL_CONTENT) return undefined
    return workspaces.value.find((w) => w.id === id) ?? (activeWorkspace.value?.id === id ? activeWorkspace.value : undefined)
  })

  // ---- Actions ----

  async function refresh(): Promise<void> {
    workspaces.value = await listWorkspaces()
    const active = await getActiveWorkspace()
    activePath.value = active?.path ?? null
    activeName.value = active?.name ?? null
    settled.value = true
  }

  let subscribed = false

  /**
   * Load once, then keep in sync.
   *
   * Self-initialising on purpose: both QuantCanvas and QuantConsole host the
   * file explorer, and whichever the user opens first must find a filled
   * registry. Until 2026-08-26 the console reached across the module boundary
   * to load this store itself; it no longer has to.
   */
  async function ensureLoaded(): Promise<void> {
    if (!subscribed) {
      subscribed = true
      // A workspace opened anywhere — the dashboard, the explorer, another
      // window — arrives here. Never torn down: the store outlives every
      // component that uses it, and the handler only re-reads.
      onWorkspacesChanged(() => {
        void refresh()
      })
    }
    if (loaded.value) return
    loaded.value = true
    await refresh()
  }

  /** Open a known workspace. Announces `core.workspace.opened` suite-wide. */
  async function setActiveWorkspace(id: string): Promise<void> {
    const target = workspaces.value.find((w) => w.id === id)
    if (!target) return
    await openWorkspace(target)
    await refresh()
  }

  /** Open a folder by path, registering it if it is new. */
  async function openPath(path: string, name?: string): Promise<void> {
    await openWorkspacePath(path, name)
    await refresh()
  }

  /** Folder dialog → open. Returns false when cancelled. */
  async function openFolder(): Promise<boolean> {
    const opened = await openFolderAsWorkspace()
    if (!opened) return false
    await refresh()
    return true
  }

  /** Forget a workspace. The folder on disk is untouched. */
  async function removeWorkspace(id: string): Promise<void> {
    await removeFromRegistry(id)
    await refresh()
  }

  async function togglePin(id: string): Promise<void> {
    const ws = workspaces.value.find((w) => w.id === id)
    if (!ws) return
    await setWorkspacePinned(id, !ws.pinned)
    await refresh()
  }

  return {
    // State
    workspaces,
    loaded,
    settled,
    // Getters
    sorted,
    activeWorkspace,
    activeWorkspaceId,
    contentWorkspaceId,
    contentWorkspace,
    GENERAL_CONTENT,
    // Actions
    setCanvasGeneral,
    setExplorerSelection,
    followActive,
    ensureLoaded,
    refresh,
    setActiveWorkspace,
    openPath,
    openFolder,
    removeWorkspace,
    togglePin,
  }
})
