<script setup lang="ts">
definePageMeta({ layout: 'canvas' })

import { useAppStore } from '#canvas-root/stores/app'
import { useWorkspacesStore } from '#canvas-root/stores/workspaces'
import { bus, qs } from '@quantsuite/core'

const appStore = useAppStore()
const workspacesStore = useWorkspacesStore()

// ── The footer dock (CodeDock — the suite-wide Problems/Terminal/Search bar).
// `notesBarVisible` keeps its name and all its toggles (header, pill, Ctrl+J);
// it is simply the dock's open flag now. The bar replaced the notes strip on
// 2026-08-27 — notes live on in the suite drawer's Notes tab.
const dockTab = ref<'problems' | 'terminal' | 'search'>('terminal')
/** The module is on screen — the dock terminal must not draw while hidden. */
const moduleActive = ref(true)
onActivated(() => (moduleActive.value = true))
onDeactivated(() => (moduleActive.value = false))

function onDockPanel(panel: 'problems' | 'terminal' | 'search' | null) {
  if (panel) dockTab.value = panel
  else appStore.toggleNotesBar()
}

/** A problems/search hit: open the file in this module's own editor panel. */
async function revealFromDock(target: { path: string; line: number }) {
  try {
    const content = await qs.files.readFile(target.path)
    appStore.openTab(target.path, content)
    if (!appStore.editorVisible) appStore.toggleEditor()
  } catch {
    // Unreadable here — hand it to whoever listens (QuantCode does).
    void bus.emit('core.file.open', { path: target.path })
  }
}


/**
 * A file activated in the explorer. The explorer has already read it — this
 * side only decides where it lands, which is what lets QuantCanvas,
 * QuantConsole and QuantCode share one explorer (docs/PLAN-QUANTSPACE.md).
 */
function openFromExplorer(file: { path: string; content: string; kind: 'text' | 'image' }) {
  appStore.openTab(file.path, file.content)
  if (!appStore.editorVisible) appStore.toggleEditor()
}

const fileExplorerVisible = computed(() => appStore.fileExplorerVisible)
const editorVisible = computed(() => appStore.editorVisible)

const DEFAULT_SIDEBAR_WIDTH = 220
const MIN_SIDEBAR_WIDTH = 120
const SNAP_THRESHOLD = 20 // px â€“ sidebar snaps when within this distance
const TOGGLE_BTN_WIDTH = 51 // sidebar toggle button width in titlebar
const SNAP_WIDTH = DEFAULT_SIDEBAR_WIDTH + TOGGLE_BTN_WIDTH

const initialWidth = computed(() => appStore.sidebarInitialWidth === 'wide' ? SNAP_WIDTH : DEFAULT_SIDEBAR_WIDTH)
const leftWidth = ref(appStore.sidebarInitialWidth === 'wide' ? SNAP_WIDTH : DEFAULT_SIDEBAR_WIDTH)
const rightWidth = ref(appStore.sidebarInitialWidth === 'wide' ? SNAP_WIDTH : DEFAULT_SIDEBAR_WIDTH)

watch(initialWidth, (w) => {
  leftWidth.value = w
  rightWidth.value = w
})

provide('rightSidebarWidth', rightWidth)

// Provide viewport insets so child components (e.g. browser webview) can clip
// to the visible canvas area, excluding sidebar overlays
const viewportInsets = computed(() => ({
  left: fileExplorerVisible.value ? leftWidth.value : 0,
  right: editorVisible.value ? rightWidth.value : 0,
}))
provide('viewportInsets', viewportInsets)

// ---- Resize logic ----
const resizing = ref<'left' | 'right' | null>(null)
const resizeStartX = ref(0)
const resizeStartWidth = ref(0)

function startResize(side: 'left' | 'right', e: MouseEvent) {
  e.preventDefault()
  resizing.value = side
  resizeStartX.value = e.clientX
  resizeStartWidth.value = side === 'left' ? leftWidth.value : rightWidth.value
  document.addEventListener('mousemove', onResizeMove)
  document.addEventListener('mouseup', onResizeEnd)
  document.body.style.cursor = 'col-resize'
  document.body.style.userSelect = 'none'
}

// The notes bar's own resize moved into the shared QDock chrome — its grip
// drags and double-click-resets the height via `update:height`.

function onResizeMove(e: MouseEvent) {
  if (!resizing.value) return
  const dx = e.clientX - resizeStartX.value
  const delta = resizing.value === 'left' ? dx : -dx
  const maxWidth = Math.floor(window.innerWidth * (resizing.value === 'left' ? 0.25 : 0.50))
  let newWidth = Math.min(maxWidth, Math.max(MIN_SIDEBAR_WIDTH, resizeStartWidth.value + delta))
  // Snap to default width or default + toggle button width when near either
  if (Math.abs(newWidth - DEFAULT_SIDEBAR_WIDTH) < SNAP_THRESHOLD) {
    newWidth = DEFAULT_SIDEBAR_WIDTH
  } else if (Math.abs(newWidth - SNAP_WIDTH) < SNAP_THRESHOLD) {
    newWidth = SNAP_WIDTH
  }
  if (resizing.value === 'left') {
    leftWidth.value = newWidth
  } else {
    rightWidth.value = newWidth
  }
}

function onResizeEnd() {
  resizing.value = null
  document.removeEventListener('mousemove', onResizeMove)
  document.removeEventListener('mouseup', onResizeEnd)
  document.body.style.cursor = ''
  document.body.style.userSelect = ''
}

function resetWidth(side: 'left' | 'right') {
  if (side === 'left') {
    leftWidth.value = initialWidth.value
  } else {
    rightWidth.value = initialWidth.value
  }
}

// Settings moved to the unified QSettingsModal (V3): the header gear opens
// it, and this module's sections are registered in
// `app/plugins/canvas-settings.client.ts`.
</script>

<template>
  <!-- Was `h-screen w-screen`: viewport units in utility-class clothing. The
       module fills the box the shell gives it. -->
  <div class="flex flex-col h-full w-full overflow-hidden">
    <!-- Title Bar -->
    <CanvasWorkspaceSwitcher />

    <!-- Main Content -->
    <!-- A column, not a row: the only in-flow children are the canvas and the
         footer dock (the side panels are absolute overlays), and the panels'
         `bottom-0` now reaches the window's bottom edge — full-height sidebars
         BESIDE the dock, exactly QuantCode's geometry. -->
    <div class="flex flex-col flex-1 min-h-0 relative">
      <!-- File Explorer Sidebar (overlay) -->
      <transition name="slide-left">
        <div
          v-if="fileExplorerVisible"
          class="absolute top-0 left-0 bottom-0 overflow-hidden z-20"
          :style="{ width: leftWidth + 'px', borderRight: '1px solid var(--qc-border)', background: 'var(--qc-bg)' }"
        >
          <CanvasSidebarFileExplorer
            host="canvas"
            :selected-path="appStore.activeTab?.filePath ?? null"
            :refresh-key="appStore.fileExplorerRefreshKey"
            @select="workspacesStore.setExplorerSelection"
            @open="openFromExplorer"
            @deleted="appStore.notifyFileDeleted"
          />
          <!-- Left resize handle -->
          <div
            class="resize-handle resize-handle-right"
            @mousedown="startResize('left', $event)"
            @dblclick="resetWidth('left')"
          />
        </div>
      </transition>

      <!-- Canvas (always present, fills remaining space) -->
      <div class="flex-1 min-h-0 min-w-0 relative">
        <CanvasCanvasInfinityCanvas />
      </div>

      <!-- The footer dock — the SAME bar every QuantSpace module carries
           (CodeDock: Problems / Terminal / Search). Inset by the panels'
           widths, so it spans only the space BETWEEN them, QuantCode-style;
           the margins animate with the panels' own slide. -->
      <CodeDock
        class="canvas-dock"
        :style="{
          marginLeft: (fileExplorerVisible ? leftWidth : 0) + 'px',
          marginRight: (editorVisible ? rightWidth : 0) + 'px',
        }"
        :panel="appStore.notesBarVisible ? dockTab : null"
        :root="workspacesStore.contentWorkspace?.path ?? null"
        :visible="moduleActive"
        @update:panel="onDockPanel"
        @reveal="revealFromDock"
      />

      <!-- Editor Panel Sidebar (overlay) -->
      <transition name="slide-right">
        <div
          v-if="editorVisible"
          class="absolute top-0 right-0 bottom-0 overflow-hidden z-20"
          :style="{ width: rightWidth + 'px', borderLeft: '1px solid var(--qc-border)', background: 'var(--qc-bg)' }"
        >
          <!-- Right resize handle -->
          <div
            class="resize-handle resize-handle-left"
            @mousedown="startResize('right', $event)"
            @dblclick="resetWidth('right')"
          />
          <CanvasSidebarEditorPanel />
        </div>
      </transition>
    </div>

  </div>

</template>

<style scoped>
/* The shared dock: in flow at the bottom of the panel container, inset by the
   side panels' widths. Below the overlays (z-20) in stacking — the margins
   keep it clear of them — and the margin animates with their slide. */
.canvas-dock {
  position: relative;
  z-index: 10;
  transition: margin-left 0.25s cubic-bezier(0.4, 0, 0.2, 1), margin-right 0.25s cubic-bezier(0.4, 0, 0.2, 1);
}

.slide-left-enter-active,
.slide-left-leave-active {
  transition: transform 0.25s cubic-bezier(0.4, 0, 0.2, 1), opacity 0.25s cubic-bezier(0.4, 0, 0.2, 1);
}
.slide-left-enter-from,
.slide-left-leave-to {
  transform: translateX(-100%);
  opacity: 0;
}

.slide-right-enter-active,
.slide-right-leave-active {
  transition: transform 0.25s cubic-bezier(0.4, 0, 0.2, 1), opacity 0.25s cubic-bezier(0.4, 0, 0.2, 1);
}
.slide-right-enter-from,
.slide-right-leave-to {
  transform: translateX(100%);
  opacity: 0;
}

.resize-handle {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 5px;
  cursor: col-resize;
  z-index: 10;
  transition: background 0.15s;
}

.resize-handle:hover,
.resize-handle:active {
  background: var(--qc-text-dim);
  opacity: 0.4;
}

.resize-handle-right {
  right: 0;
}

.resize-handle-left {
  left: 0;
}

.resize-handle-h {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 5px;
  cursor: row-resize;
  z-index: 10;
  transition: background 0.15s;
}

.resize-handle-h:hover,
.resize-handle-h:active {
  background: var(--qc-text-dim);
  opacity: 0.4;
}

/* â”€â”€ Settings Modal â”€â”€ */

</style>
