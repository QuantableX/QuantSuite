/**
 * The shell's reactive view of the suite workspaces.
 *
 * A workspace **is** a project folder — one folder, no multi-root
 * (docs/PLAN-WORKSPACES.md). Kanban, notes, processes and agents are global
 * and do not scope to it; the one app a workspace binds is QuantCode.
 *
 * This composable holds NO persistence. The registry lives in
 * `@quantsuite/core`'s `workspaces` module (`core.db` entities + the setting
 * `core / workspace.active`), and QuantCode's own store is a second thin shell
 * over that same implementation. Opening a workspace here announces
 * `core.workspace.opened`; the module picks the folder up from the bus and
 * never reads the shell (ARCHITECTURE.md §4).
 */

import {
  listWorkspaces,
  getActiveWorkspace,
  getReopenLast,
  setReopenLast as persistReopenLast,
  onWorkspacesChanged,
  openWorkspace,
  openFolderAsWorkspace,
  removeWorkspace,
  setWorkspacePinned,
  sortWorkspaces,
  type ActiveWorkspace,
  type Workspace,
} from '@quantsuite/core'

/** Kept as an alias: the shell's pages have called it this since PLAN-V2. */
export type SuiteWorkspace = Workspace

export function useWorkspaces() {
  const list = useState<Workspace[]>('qss-ws-list', () => [])
  const active = useState<ActiveWorkspace | null>('qss-ws-active', () => null)
  const reopenLast = useState<boolean>('qss-ws-reopen', () => false)
  const ready = useState<boolean>('qss-ws-ready', () => false)
  /** Surfaced by the picker — a failed folder dialog is otherwise invisible. */
  const error = useState<string | null>('qss-ws-error', () => null)
  const subscribed = useState<boolean>('qss-ws-subscribed', () => false)

  /** Pinned first, then most recently opened. */
  const sorted = computed(() => sortWorkspaces(list.value))

  async function refresh() {
    list.value = await listWorkspaces()
    active.value = await getActiveWorkspace()
  }

  async function load() {
    try {
      await refresh()
      reopenLast.value = await getReopenLast()

      // A workspace opened anywhere — this window's picker, QuantCode's
      // explorer, another window — lands here. Subscribed once per session:
      // `useState` survives every component that calls this composable.
      if (!subscribed.value) {
        subscribed.value = true
        onWorkspacesChanged(() => {
          void refresh()
        })
      }
    } catch (e) {
      console.error('[shell] loading workspaces failed', e)
      error.value = e instanceof Error ? e.message : String(e)
    }
    ready.value = true
  }

  /** Open a workspace: bump recency, persist, mark active, tell the suite. */
  async function open(ws: Workspace) {
    error.value = null
    try {
      await openWorkspace(ws)
      await refresh()
      return true
    } catch (e) {
      console.error('[shell] opening workspace failed', e)
      error.value = e instanceof Error ? e.message : String(e)
      return false
    }
  }

  /** Folder dialog → open. Returns false if cancelled or failed. */
  async function openFolder() {
    error.value = null
    try {
      const opened = await openFolderAsWorkspace()
      if (!opened) return false
      await refresh()
      return true
    } catch (e) {
      console.error('[shell] opening folder failed', e)
      error.value = e instanceof Error ? e.message : String(e)
      return false
    }
  }

  async function togglePin(id: string) {
    const ws = list.value.find((w) => w.id === id)
    if (!ws) return
    try {
      await setWorkspacePinned(id, !ws.pinned)
      await refresh()
    } catch (e) {
      console.error('[shell] pinning workspace failed', e)
    }
  }

  /** Forget a saved workspace. Does not touch the folder on disk. */
  async function remove(id: string) {
    try {
      await removeWorkspace(id)
      await refresh()
    } catch (e) {
      console.error('[shell] removing workspace failed', e)
    }
  }

  async function setReopenLast(v: boolean) {
    reopenLast.value = v
    try {
      await persistReopenLast(v)
    } catch (e) {
      console.error('[shell] saving reopenLast failed', e)
    }
  }

  return { list, sorted, active, reopenLast, ready, error, load, refresh, open, openFolder, togglePin, remove, setReopenLast }
}
