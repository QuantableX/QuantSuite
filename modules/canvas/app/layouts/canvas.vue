<script setup lang="ts">
import { useWorkspacesStore } from '#canvas-root/stores/workspaces'
import { useCanvasStore } from '#canvas-root/stores/canvas'
import { useAppStore } from '#canvas-root/stores/app'
import { useEventListener } from '@vueuse/core'
import { terminalHasFocus } from '#canvas/utils/terminalFocus'
import { bus, inActiveKeepAliveTree } from '@quantsuite/core'

const workspacesStore = useWorkspacesStore()
const canvasStore = useCanvasStore()
const appStore = useAppStore()

/**
 * Open a file requested from outside the module (E5): the shell palette's
 * file results emit `core.file.open` rather than reaching into this store.
 * Modules talk through the bus, never to the shell (ARCHITECTURE.md §4).
 */
async function openRequestedFile(path: string | undefined) {
  if (!path) return
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const content = await invoke<string>('plugin:canvas|read_file', { path })
    appStore.openTab(path, content)
  } catch {
    appStore.openTab(path, `// Could not read ${path}\n`)
  }
  if (!appStore.editorVisible) appStore.toggleEditor()
}

/**
 * The canvas follows the open workspace, wherever the switch came from — the
 * dashboard, the explorer's switcher, Ctrl+O, another window. There is one
 * registry now (docs/PLAN-WORKSPACES.md), so there is nothing left to "adopt":
 * the store reads the same `core.db` the shell writes, and this watcher only
 * has to move the canvas state along with it.
 *
 * The outgoing workspace is saved first; the incoming one is read from disk
 * only on its first visit, because the in-memory state is the newer of the two
 * after that.
 */
watch(
  () => workspacesStore.contentWorkspaceId,
  async (id, previous) => {
    if (previous && previous !== id) await canvasStore.saveCanvasState(previous)
    if (!id) return
    if (!canvasStore.canvasStates.has(id)) {
      canvasStore.initCanvas(id)
      await canvasStore.loadCanvasState(id)
    }
  }
)

let offFileOpen: (() => void) | undefined

onMounted(async () => {
  // Apply saved theme and font size
  appStore.applyTheme(appStore.theme)
  appStore.applyFontSize(appStore.fontSize)

  // Pinia survives module switches; `ensureLoaded` reads the registry on the
  // very first mount and subscribes for later changes. Re-reading on every
  // switch was part of the "loads forever" on entry.
  await workspacesStore.ensureLoaded()

  // First visit: the watcher above only fires on a *change*, so the workspace
  // that was already open when this layout mounted needs its state loaded here.
  const active = workspacesStore.contentWorkspaceId
  if (active && !canvasStore.canvasStates.has(active)) {
    canvasStore.initCanvas(active)
    await canvasStore.loadCanvasState(active)
  }

  offFileOpen = bus.on<{ path: string }>('core.file.open', (event) => {
    void openRequestedFile(event.payload.path)
  })
})

onUnmounted(() => {
  offFileOpen?.()
})

// V3 warm cache: the stage keeps this layout mounted while another module is
// active, so the shortcut listener hangs off a reactive target that goes null
// on deactivation — otherwise Ctrl+B/Ctrl+Shift+B/Ctrl+J/Ctrl+Tab/Ctrl+O keep
// firing and preventDefault-ing suite-wide (Ctrl+B collides with QuantNotes' own).
// The flag is set in BOTH onMounted and onActivated: layouts load async and
// mount after the stage's activation flush, so onActivated alone would miss
// the first visit — but that same late mount can land in a stage the user has
// already left, where setting the flag would arm canvas's keys inside another
// module with no onDeactivated left to clear them, hence the guard.
const isActive = ref(false)
const activeWindow = computed(() => (isActive.value ? window : null))

onMounted(() => { if (inActiveKeepAliveTree()) isActive.value = true })
onActivated(() => { isActive.value = true })
onDeactivated(() => { isActive.value = false })

useEventListener<KeyboardEvent>(activeWindow, 'keydown', (e) => {
  // Every single-modifier binding below is also a shell binding, and the shell
  // was here first: Ctrl+B is the tmux prefix, Ctrl+O submits a line in
  // readline, Ctrl+J is a newline. The panel toggles are still reachable from
  // anywhere else on the canvas, so the terminal wins the tie.
  // Ctrl+Shift+B is not shared with anything and stays unconditional.
  const inTerminal = terminalHasFocus(e.target)

  // Ctrl+B: toggle file explorer
  if (e.ctrlKey && !e.shiftKey && e.key === 'b' && !inTerminal) {
    e.preventDefault()
    appStore.toggleFileExplorer()
  }
  // Ctrl+Shift+B: toggle editor panel
  if (e.ctrlKey && e.shiftKey && e.key === 'B') {
    e.preventDefault()
    appStore.toggleEditor()
  }
  // Ctrl+J: toggle notes bar
  if (e.ctrlKey && !e.shiftKey && e.key === 'j' && !inTerminal) {
    e.preventDefault()
    appStore.toggleNotesBar()
  }
  // Ctrl+Tab: cycle editor tabs
  if (e.ctrlKey && e.key === 'Tab' && !inTerminal) {
    e.preventDefault()
    const tabs = appStore.activeEditorTabs
    if (tabs.length <= 1) return
    const currentIndex = tabs.findIndex(t => t.id === appStore.activeTabId)
    const nextIndex = (currentIndex + 1) % tabs.length
    appStore.setActiveTab(tabs[nextIndex].id)
  }
  // Ctrl+O: open workspace (dispatches custom event for WorkspaceSwitcher to handle)
  if (e.ctrlKey && e.key === 'o' && !inTerminal) {
    e.preventDefault()
    window.dispatchEvent(new CustomEvent('quantcode:open-workspace'))
  }
})
</script>

<template>
  <!--
    Two changes from the standalone app.vue this was made from:

    - `<slot />` instead of `<NuxtPage />`. The shell already renders
      `<NuxtLayout><NuxtPage/></NuxtLayout>`; leaving NuxtPage here nested a
      second page render inside the layout.
    - `h-full` instead of `min-h-screen`. `min-h-screen` is `min-height: 100vh`
      wearing a utility class — it forced the module to be at least a full
      viewport tall inside an already-constrained container, which is what made
      the whole thing overflow.
  -->
  <div class="h-full overflow-hidden select-none" style="background: var(--qc-bg); color: var(--qc-text);">
    <slot />
  </div>
</template>
