<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import { qs } from '@quantsuite/core'
import { Webview } from '@tauri-apps/api/webview'
import { open } from '@tauri-apps/plugin-dialog'
import { pictureDir, join } from '@tauri-apps/api/path'
import { useWorkspacesStore } from '../../../stores/workspaces'
import { useCanvasStore } from '../../../stores/canvas'
import { useAppStore } from '../../../stores/app'
import type { CanvasWindow as CanvasWindowType, WindowPosition, WindowStatus } from '../../../shared/types'

const props = defineProps<{
  window: CanvasWindowType
}>()

const workspacesStore = useWorkspacesStore()
const canvasStore = useCanvasStore()
const appStore = useAppStore()

const GRID_SIZE = 20

function snapToGrid(value: number): number {
  return Math.round(value / GRID_SIZE) * GRID_SIZE
}

const workspaceId = computed(() => workspacesStore.contentWorkspaceId!)

// Drag state
const isDragging = ref(false)
const dragStart = ref({ x: 0, y: 0 })
const dragPositionStart = ref({ x: 0, y: 0 })

// Resize state
const isResizing = ref(false)
const resizeHandle = ref('')
const resizeStart = ref({ x: 0, y: 0 })
const resizeDimStart = ref({ x: 0, y: 0, width: 0, height: 0 })

const MIN_WIDTH = 300
const MIN_HEIGHT = 200

const windowStyle = computed(() => ({
  position: 'absolute' as const,
  left: `${props.window.position.x}px`,
  top: `${props.window.position.y}px`,
  width: `${props.window.position.width}px`,
  height: props.window.minimized ? 'auto' : `${props.window.position.height}px`,
  zIndex: props.window.zIndex,
  background: 'var(--qc-bg-window)',
  border: '1px solid var(--qc-border)',
}))

// Lazy mount gate for the body: a window restored from disk in minimized state
// must not spin up its child (PTY, tab webviews) unseen. Flips
// once the window is first shown and never goes back — from then on minimizing
// only hides the body.
const bodyMounted = ref(!props.window.minimized)

watch(() => props.window.minimized, (minimized) => {
  if (!minimized) bodyMounted.value = true
})

const statusColor = computed(() => {
  const colors: Record<WindowStatus, string> = {
    idle: '#57606f',
    thinking: '#ffa502',
    live: '#2ed573',
    error: '#ff4757',
    minimized: '#57606f',
  }
  return colors[props.window.status]
})

const typeIcon = computed(() => {
  const icons: Record<string, string> = {
    terminal: '\u2588',
    diff: '\u00B1',
    spec: '\u25A3',
    file: '\u25A0',
    browser: '\u260D',
  }
  return icons[props.window.type] ?? '\u25C7'
})

const windowGridLabel = computed(() => {
  const columns = Math.max(1, Math.round(props.window.position.width / GRID_SIZE))
  const rows = Math.max(1, Math.round(props.window.position.height / GRID_SIZE))
  return `${columns}×${rows}`
})

function bringToFront() {
  canvasStore.bringToFront(workspaceId.value, props.window.id)
}

// ---- Drag handling ----
function startDrag(e: MouseEvent) {
  if (e.button !== 0) return
  e.preventDefault()
  e.stopPropagation()

  // Focus the header so Ctrl+C works for copying
  ;(e.currentTarget as HTMLElement)?.focus()

  bringToFront()
  isDragging.value = true
  dragStart.value = { x: e.clientX, y: e.clientY }
  dragPositionStart.value = { x: props.window.position.x, y: props.window.position.y }

  window.addEventListener('mousemove', onDragMove)
  window.addEventListener('mouseup', onDragEnd)
}

function onDragMove(e: MouseEvent) {
  if (!isDragging.value) return

  const canvasState = canvasStore.activeCanvasState
  const scale = canvasState?.transform.scale ?? 1

  const dx = (e.clientX - dragStart.value.x) / scale
  const dy = (e.clientY - dragStart.value.y) / scale

  canvasStore.updateWindowPosition(workspaceId.value, props.window.id, {
    x: dragPositionStart.value.x + dx,
    y: dragPositionStart.value.y + dy,
  })
}

function onDragEnd() {
  isDragging.value = false
  window.removeEventListener('mousemove', onDragMove)
  window.removeEventListener('mouseup', onDragEnd)

  // Snap to grid if enabled
  if (appStore.snapToGrid) {
    canvasStore.updateWindowPosition(workspaceId.value, props.window.id, {
      x: snapToGrid(props.window.position.x),
      y: snapToGrid(props.window.position.y),
    })
  }

  canvasStore.saveCanvasState(workspaceId.value)
}

// ---- Resize handling ----
function startResize(handle: string, e: MouseEvent) {
  e.preventDefault()
  e.stopPropagation()

  bringToFront()
  isResizing.value = true
  resizeHandle.value = handle
  resizeStart.value = { x: e.clientX, y: e.clientY }
  resizeDimStart.value = {
    x: props.window.position.x,
    y: props.window.position.y,
    width: props.window.position.width,
    height: props.window.position.height,
  }

  window.addEventListener('mousemove', onResizeMove)
  window.addEventListener('mouseup', onResizeEnd)
}

function onResizeMove(e: MouseEvent) {
  if (!isResizing.value) return

  const canvasState = canvasStore.activeCanvasState
  const scale = canvasState?.transform.scale ?? 1

  const dx = (e.clientX - resizeStart.value.x) / scale
  const dy = (e.clientY - resizeStart.value.y) / scale
  const h = resizeHandle.value

  let newX = resizeDimStart.value.x
  let newY = resizeDimStart.value.y
  let newW = resizeDimStart.value.width
  let newH = resizeDimStart.value.height

  if (h.includes('e')) newW = Math.max(MIN_WIDTH, resizeDimStart.value.width + dx)
  if (h.includes('s')) newH = Math.max(MIN_HEIGHT, resizeDimStart.value.height + dy)
  if (h.includes('w')) {
    const proposedW = resizeDimStart.value.width - dx
    if (proposedW >= MIN_WIDTH) {
      newW = proposedW
      newX = resizeDimStart.value.x + dx
    }
  }
  if (h.includes('n')) {
    const proposedH = resizeDimStart.value.height - dy
    if (proposedH >= MIN_HEIGHT) {
      newH = proposedH
      newY = resizeDimStart.value.y + dy
    }
  }

  canvasStore.updateWindowPosition(workspaceId.value, props.window.id, {
    x: newX,
    y: newY,
    width: newW,
    height: newH,
  })
}

function onResizeEnd() {
  isResizing.value = false
  window.removeEventListener('mousemove', onResizeMove)
  window.removeEventListener('mouseup', onResizeEnd)

  // Snap to grid if enabled
  if (appStore.snapToGrid) {
    canvasStore.updateWindowPosition(workspaceId.value, props.window.id, {
      x: snapToGrid(props.window.position.x),
      y: snapToGrid(props.window.position.y),
      width: snapToGrid(props.window.position.width),
      height: snapToGrid(props.window.position.height),
    })
  }

  canvasStore.saveCanvasState(workspaceId.value)
}

function toggleMinimize() {
  canvasStore.updateWindow(workspaceId.value, props.window.id, {
    minimized: !props.window.minimized,
  })
}

// ---- Zoom ----

const canvasViewportBox = inject<() => WindowPosition | null>('canvasViewportBox', () => null)

/** Where to put the window back; non-null exactly while it is filling the view. */
const restoreBox = ref<WindowPosition | null>(null)

/** Breathing room so a zoomed window still reads as a window on a canvas. */
const ZOOM_MARGIN = 24

function toggleZoom() {
  if (restoreBox.value) {
    canvasStore.updateWindowPosition(workspaceId.value, props.window.id, restoreBox.value)
    restoreBox.value = null
    canvasStore.saveCanvasState(workspaceId.value)
    return
  }

  const box = canvasViewportBox()
  if (!box) return

  restoreBox.value = { ...props.window.position }
  canvasStore.updateWindowPosition(workspaceId.value, props.window.id, {
    x: box.x + ZOOM_MARGIN,
    y: box.y + ZOOM_MARGIN,
    width: Math.max(MIN_WIDTH, box.width - ZOOM_MARGIN * 2),
    height: Math.max(MIN_HEIGHT, box.height - ZOOM_MARGIN * 2),
  })
  bringToFront()
  canvasStore.saveCanvasState(workspaceId.value)
}

// A dragged or resized window is no longer the one that was zoomed, so the
// stored box would restore it to somewhere it has not been for a while.
watch(
  () => [props.window.position.x, props.window.position.y, props.window.position.width, props.window.position.height],
  () => {
    if (isDragging.value || isResizing.value) restoreBox.value = null
  }
)

async function closeWindow() {
  // Kill the terminal process when the user explicitly closes the window. The
  // session belongs to the console plugin since P4.5 — one emulator for the
  // whole suite (docs/PLAN-CONSOLE.md).
  if (props.window.type === 'terminal' && props.window.terminalId) {
    try {
      await qs.console.close(props.window.terminalId)
    } catch {
      // Terminal may have already exited
    }
  }
  // Destroy all tab webviews when user explicitly closes the browser window
  if (props.window.type === 'browser') {
    const sanitizedId = props.window.id.replace(/[^a-zA-Z0-9_-]/g, '_')
    const tabs = props.window.browserConfig?.tabs ?? []
    for (const tab of tabs) {
      const tabLabel = `browser-${sanitizedId}-${tab.id.replace(/[^a-zA-Z0-9_-]/g, '_')}`
      try {
        const wv = await Webview.getByLabel(tabLabel)
        if (wv) await wv.close()
      } catch { /* ignore */ }
    }
    // Legacy single-webview label fallback
    const legacyLabel = `browser-${sanitizedId}`
    try {
      const wv = await Webview.getByLabel(legacyLabel)
      if (wv) await wv.close()
    } catch { /* ignore */ }
  }
  canvasStore.removeWindow(workspaceId.value, props.window.id)
}

/** The terminal body's handle, for the actions the header drives. */
const terminal = ref<{ prefill: (text: string) => void } | null>(null)

async function attachFile() {
  if (props.window.type !== 'terminal') return

  try {
    // Default to Screenshots folder on Windows
    let defaultPath: string | undefined
    try {
      const pictures = await pictureDir()
      defaultPath = await join(pictures, 'Screenshots')
    } catch {
      // Fall back to no default path
    }

    const selected = await open({
      defaultPath,
      multiple: false,
      filters: [
        { name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp'] },
        { name: 'All Files', extensions: ['*'] },
      ],
    })

    if (selected) {
      // Into the pane's input, not down the PTY. Raw bytes bypass the block
      // engine, so the shell's line and the block's recorded command drift
      // apart and the block ends up describing a command nobody ran.
      terminal.value?.prefill(`"${selected}" `)
    }
  } catch {
    // Dialog cancelled or error
  }
}

// ---- Copy window ----
const copiedWindow = inject<Ref<CanvasWindowType | null>>('copiedWindow')

function copyWindow() {
  if (copiedWindow) {
    copiedWindow.value = { ...props.window, position: { ...props.window.position } }
  }
}

function onWindowClick() {
  bringToFront()
}

// The frame owns closing and zooming; the body only knows when they were asked
// for. Handed down rather than emitted back up so the terminal's pane menu and
// the header's own buttons run the same two functions.
provide('canvasWindowActions', { close: closeWindow, toggleZoom })
</script>

<template>
  <!-- No `overflow-hidden` on the frame: it clipped the outer half of every
       resize handle, leaving ~6px inside the edge — over the terminal body
       that strip is nearly impossible to hit, so windows could only be
       resized at the header. Header and body carry their own rounding
       instead, and the handles straddle the edge like a real window
       manager's. -->
  <div
    :data-window-id="window.id"
    :style="windowStyle"
    class="rounded-lg shadow-2xl flex flex-col group"
    @mousedown="onWindowClick"
    @contextmenu.stop
  >
    <!-- Header -->
    <div
      class="relative h-8 flex items-center px-3 flex-shrink-0 cursor-move select-none outline-none"
      :class="window.minimized ? 'rounded-lg' : 'rounded-t-lg'"
      :style="{ background: 'var(--qc-bg-header)', borderBottom: '1px solid var(--qc-border)' }"
      tabindex="0"
      @mousedown="startDrag"
      @keydown.ctrl.c.prevent.stop="copyWindow"
    >
      <!-- Icon + Title -->
      <div class="min-w-0 max-w-[calc(50%-56px)] flex items-center gap-2">
        <span class="text-xs text-[#a0a0a8] flex-shrink-0">{{ typeIcon }}</span>
        <span class="text-xs font-medium truncate" :style="{ color: 'var(--qc-text)' }">{{ window.type === 'spec' && window.title === 'Spec Dashboard' ? 'Workspace Kanban' : window.title }}</span>
      </div>

      <!-- Grid dimensions -->
      <span
        class="absolute left-1/2 -translate-x-1/2 pointer-events-none z-10 text-[10px] font-mono tracking-wide px-1.5 py-0.5 rounded"
        :style="{
          color: 'var(--qc-text-dim)',
          background: 'color-mix(in srgb, var(--qc-bg-window) 88%, transparent)',
          border: '1px solid var(--qc-border)',
        }"
      >
        {{ windowGridLabel }}
      </span>

      <div class="ml-auto flex items-center gap-1">
        <!-- Attach file (terminal only) -->
        <button
          v-if="window.type === 'terminal'"
          class="w-5 h-5 flex items-center justify-center transition-colors rounded"
          :style="{ color: 'var(--qc-text-muted)' }"
          @mousedown.stop
          @click.stop="attachFile"
          title="Attach file"
        >
          <svg class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48" />
          </svg>
        </button>

        <!-- Status badge -->
        <span
          class="w-2 h-2 rounded-full flex-shrink-0"
          :style="{ backgroundColor: statusColor }"
          :title="window.status"
        />

        <!-- Copy -->
        <button
          class="w-5 h-5 flex items-center justify-center transition-colors rounded text-xs"
          :style="{ color: 'var(--qc-text-muted)' }"
          @mousedown.stop
          @click.stop="copyWindow"
          title="Copy window"
        >
          <svg class="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <rect x="9" y="9" width="13" height="13" rx="2" ry="2" /><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
          </svg>
        </button>

        <!-- Minimize -->
        <button
          class="w-5 h-5 flex items-center justify-center transition-colors rounded text-xs"
          :style="{ color: 'var(--qc-text-muted)' }"
          @mousedown.stop
          @click.stop="toggleMinimize"
          title="Minimize"
        >
          &#8722;
        </button>

        <!-- Close -->
        <button
          class="w-5 h-5 flex items-center justify-center text-red-400/60 hover:text-red-400 transition-colors rounded text-xs"
          @mousedown.stop
          @click.stop="closeWindow"
          title="Close"
        >
          &#10005;
        </button>
      </div>
    </div>

    <!-- Body — two gates, both needed:
         v-if mounts the child the first time the window is shown, so a window
         saved in minimized state stays cold on load (no PTY, no tab webviews
         for something nobody sees).
         v-show handles minimizing afterwards: destroying the child would close
         the browser tab webviews and drop their page state; they keep
         themselves alive while hidden, the same way TerminalWindow keeps its
         PTY. -->
    <div
      v-if="bodyMounted"
      v-show="!window.minimized"
      class="flex-1 min-h-0 overflow-hidden rounded-b-lg canvas-window-body"
    >
      <CanvasWindowsTerminalWindow
        v-if="window.type === 'terminal'"
        ref="terminal"
        :window="window"
      />
      <CanvasWindowsDiffWindow
        v-else-if="window.type === 'diff'"
        :window="window"
      />
      <CanvasWindowsKanbanWindow
        v-else-if="window.type === 'spec'"
        :window="window"
      />
      <CanvasWindowsFileWindow
        v-else-if="window.type === 'file'"
        :window="window"
      />
      <CanvasWindowsBrowserWindow
        v-else-if="window.type === 'browser'"
        :window="window"
      />
    </div>

    <!-- Resize handles (hidden when minimized). 12px straddling each edge —
         half outside, half inside — and above every window overlay (the shell
         picker is z-20 and used to swallow edge clicks on fresh terminals). -->
    <template v-if="!window.minimized">
      <!-- Edge handles -->
      <div class="absolute z-30 -top-1.5 left-3 right-3 h-3 cursor-n-resize" @mousedown="startResize('n', $event)" />
      <div class="absolute z-30 -bottom-1.5 left-3 right-3 h-3 cursor-s-resize" @mousedown="startResize('s', $event)" />
      <div class="absolute z-30 -left-1.5 top-3 bottom-3 w-3 cursor-w-resize" @mousedown="startResize('w', $event)" />
      <div class="absolute z-30 -right-1.5 top-3 bottom-3 w-3 cursor-e-resize" @mousedown="startResize('e', $event)" />
      <!-- Corner handles -->
      <div class="absolute z-30 -top-1.5 -left-1.5 w-5 h-5 cursor-nw-resize" @mousedown="startResize('nw', $event)" />
      <div class="absolute z-30 -top-1.5 -right-1.5 w-5 h-5 cursor-ne-resize" @mousedown="startResize('ne', $event)" />
      <div class="absolute z-30 -bottom-1.5 -left-1.5 w-5 h-5 cursor-sw-resize" @mousedown="startResize('sw', $event)" />
      <div class="absolute z-30 -bottom-1.5 -right-1.5 w-5 h-5 cursor-se-resize" @mousedown="startResize('se', $event)" />
    </template>
  </div>
</template>
