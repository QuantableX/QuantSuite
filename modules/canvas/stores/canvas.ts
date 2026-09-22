import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useWorkspacesStore } from './workspaces'
import type { CanvasState, CanvasTransform, CanvasWindow, WindowPosition } from '../shared/types'

function createDefaultCanvasState(workspaceId: string): CanvasState {
  return {
    workspaceId,
    transform: { x: 0, y: 0, scale: 1 },
    windows: [],
    nextZIndex: 1,
  }
}

export const useCanvasStore = defineStore('canvas/canvas', () => {
  // ---- State ----
  const canvasStates = ref<Map<string, CanvasState>>(new Map())

  // ---- Getters ----
  const activeCanvasState = computed<CanvasState | undefined>(() => {
    const workspacesStore = useWorkspacesStore()
    const id = workspacesStore.contentWorkspaceId
    if (!id) return undefined
    return canvasStates.value.get(id)
  })

  // ---- Actions ----

  /**
   * The folder a workspace id stands for — the canvas state is persisted in
   * private per-workspace storage, keyed by this folder path.
   *
   * The active workspace is checked first because it may be a synthetic entry:
   * `core / workspace.active` is what QuantCode works against, and a registry
   * row that failed to write must not cost the user their canvas.
   */
  function folderFor(workspaceId: string): string | null {
    const workspacesStore = useWorkspacesStore()
    // The General board is not a folder — it lives in memory only.
    if (workspaceId === workspacesStore.GENERAL_CONTENT) return null
    if (workspacesStore.activeWorkspace?.id === workspaceId) {
      return workspacesStore.activeWorkspace.path
    }
    return workspacesStore.workspaces.find(w => w.id === workspaceId)?.path ?? null
  }

  function initCanvas(workspaceId: string): CanvasState {
    if (!canvasStates.value.has(workspaceId)) {
      canvasStates.value.set(workspaceId, createDefaultCanvasState(workspaceId))
    }
    return canvasStates.value.get(workspaceId)!
  }

  function getCanvasState(workspaceId: string): CanvasState | undefined {
    return canvasStates.value.get(workspaceId)
  }

  function updateTransform(workspaceId: string, transform: Partial<CanvasTransform>): void {
    const state = canvasStates.value.get(workspaceId)
    if (!state) return

    Object.assign(state.transform, transform)
    debouncedSaveTransform(workspaceId)
  }

  // Debounce transform saves to avoid excessive disk writes during pan/zoom.
  // One timer PER workspace: a single shared timer let a pan in workspace B
  // cancel A's pending save, and A's camera position was silently lost.
  const saveTransformTimers = new Map<string, ReturnType<typeof setTimeout>>()
  function debouncedSaveTransform(workspaceId: string): void {
    const pending = saveTransformTimers.get(workspaceId)
    if (pending) clearTimeout(pending)
    saveTransformTimers.set(workspaceId, setTimeout(() => {
      saveTransformTimers.delete(workspaceId)
      saveCanvasState(workspaceId)
    }, 500))
  }

  function addWindow(workspaceId: string, window: CanvasWindow): void {
    const state = canvasStates.value.get(workspaceId)
    if (!state) return

    window.zIndex = state.nextZIndex
    state.nextZIndex += 1
    state.windows.push(window)
    saveCanvasState(workspaceId)
  }

  function removeWindow(workspaceId: string, windowId: string): void {
    const state = canvasStates.value.get(workspaceId)
    if (!state) return

    const index = state.windows.findIndex(w => w.id === windowId)
    if (index !== -1) {
      state.windows.splice(index, 1)
      saveCanvasState(workspaceId)
    }
  }

  function updateWindow(workspaceId: string, windowId: string, updates: Partial<Omit<CanvasWindow, 'id'>>): void {
    const state = canvasStates.value.get(workspaceId)
    if (!state) return

    const window = state.windows.find(w => w.id === windowId)
    if (!window) return

    Object.assign(window, updates)
    saveCanvasState(workspaceId)
  }

  function updateWindowPosition(workspaceId: string, windowId: string, position: Partial<WindowPosition>): void {
    const state = canvasStates.value.get(workspaceId)
    if (!state) return

    const window = state.windows.find(w => w.id === windowId)
    if (!window) return

    Object.assign(window.position, position)
  }

  function bringToFront(workspaceId: string, windowId: string): void {
    const state = canvasStates.value.get(workspaceId)
    if (!state) return

    const window = state.windows.find(w => w.id === windowId)
    if (!window) return

    window.zIndex = state.nextZIndex
    state.nextZIndex += 1
  }

  async function loadCanvasState(workspaceId: string): Promise<void> {
    try {
      const folderPath = folderFor(workspaceId)
      if (!folderPath) {
        initCanvas(workspaceId)
        return
      }
      const raw = await invoke<string>('plugin:canvas|load_canvas_state', { folderPath })
      const state = JSON.parse(raw) as CanvasState
      if (state && Array.isArray(state.windows)) {
        // Agent windows were removed from the module; saved workspaces still
        // carry them and would restore as frames with no body.
        state.windows = state.windows.filter(w => (w.type as string) !== 'agent')
        canvasStates.value.set(workspaceId, state)
      } else {
        initCanvas(workspaceId)
      }
    } catch (error) {
      console.error(`Failed to load canvas state for workspace ${workspaceId}:`, error)
      initCanvas(workspaceId)
    }
  }

  async function saveCanvasState(workspaceId: string): Promise<void> {
    const state = canvasStates.value.get(workspaceId)
    if (!state) return

    try {
      const folderPath = folderFor(workspaceId)
      if (!folderPath) return
      await invoke('plugin:canvas|save_canvas_state', { folderPath, data: JSON.stringify(state) })
    } catch (error) {
      console.error(`Failed to save canvas state for workspace ${workspaceId}:`, error)
    }
  }

  return {
    // State
    canvasStates,
    // Getters
    activeCanvasState,
    // Actions
    initCanvas,
    getCanvasState,
    updateTransform,
    addWindow,
    removeWindow,
    updateWindow,
    updateWindowPosition,
    bringToFront,
    loadCanvasState,
    saveCanvasState,
  }
})
