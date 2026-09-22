<script setup lang="ts">
import { useWorkspacesStore } from '../../../stores/workspaces'
import { useCanvasStore } from '../../../stores/canvas'
import { useAppStore } from '../../../stores/app'
import { v4 as uuidv4 } from 'uuid'
import type { CanvasWindow as CanvasWindowType, WindowType, FilePreviewKind } from '../../../shared/types'
import { useEventListener } from '@vueuse/core'
import { inActiveKeepAliveTree } from '@quantsuite/core'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { AppWindow, Globe, LayoutDashboard, Terminal } from 'lucide-vue-next'
import { terminalHasFocus } from '../../utils/terminalFocus'

const workspacesStore = useWorkspacesStore()
const canvasStore = useCanvasStore()
const appStore = useAppStore()

const canvasRef = ref<HTMLElement | null>(null)

// Share canvas element with child components (e.g. Minimap)
provide('canvasRef', canvasRef)

// Share context menu visibility so browser webviews can hide when menu is open
const contextMenuVisible = computed(() => contextMenu.value.visible)
provide('contextMenuVisible', contextMenuVisible)

// Clipboard for window copy/paste
const copiedWindow = ref<CanvasWindowType | null>(null)
provide('copiedWindow', copiedWindow)

// Pan/zoom state
const isPanning = ref(false)
const isSpacePanning = ref(false)
const spaceHeld = ref(false)
const panStart = ref({ x: 0, y: 0 })
const transformStart = ref({ x: 0, y: 0 })

// Context menu. `x`/`y` are client coords (the window-placement math converts
// them to canvas coords itself); `localX`/`localY` position the menu element,
// which is `absolute` inside the canvas root — `fixed` anchors to the module
// stage since V3 (`.qss-module-root` is a transformed containing block), so
// client coords drew the menu offset by the stage's own origin.
const contextMenu = ref({ visible: false, x: 0, y: 0, localX: 0, localY: 0 })
const contextMenuEl = ref<HTMLElement | null>(null)

const canvasState = computed(() => canvasStore.activeCanvasState)
const transform = computed(() => canvasState.value?.transform ?? { x: 0, y: 0, scale: 1 })
const windows = computed(() => canvasState.value?.windows ?? [])
const workspaceId = computed(() => workspacesStore.contentWorkspaceId)
const zoomPercent = computed(() => Math.round(transform.value.scale * 100))
const tileCount = computed(() => windows.value.length)

const canvasTransformStyle = computed(() => {
  const { x, y, scale } = transform.value
  return `translate(${x}px, ${y}px) scale(${scale})`
})

const dotGridStyle = computed(() => {
  const scale = transform.value.scale
  const size = 20 * scale
  const majorSize = size * 5
  const ox = transform.value.x % size
  const oy = transform.value.y % size
  const majorOx = transform.value.x % majorSize
  const majorOy = transform.value.y % majorSize
  return {
    background: 'var(--qc-bg)',
    backgroundImage: `radial-gradient(circle, var(--qc-dot-grid-bright) 2px, transparent 2px), radial-gradient(circle, var(--qc-dot-grid) 1.5px, transparent 1.5px)`,
    backgroundSize: `${majorSize}px ${majorSize}px, ${size}px ${size}px`,
    backgroundPosition: `${majorOx}px ${majorOy}px, ${ox}px ${oy}px`,
  }
})

// ---- Panning ----
function onMouseDown(e: MouseEvent) {
  // Middle mouse button or space+left click for panning
  if (e.button === 1 || (spaceHeld.value && e.button === 0)) {
    e.preventDefault()
    isPanning.value = true
    if (spaceHeld.value) isSpacePanning.value = true
    panStart.value = { x: e.clientX, y: e.clientY }
    transformStart.value = { x: transform.value.x, y: transform.value.y }
  }
}

function onMouseMove(e: MouseEvent) {
  if (!isPanning.value || !workspaceId.value) return

  const dx = e.clientX - panStart.value.x
  const dy = e.clientY - panStart.value.y

  canvasStore.updateTransform(workspaceId.value, {
    x: transformStart.value.x + dx,
    y: transformStart.value.y + dy,
  })
}

const GRID_SIZE = 20

function snapToGrid(value: number): number {
  return Math.round(value / GRID_SIZE) * GRID_SIZE
}

/**
 * The middle of the canvas in screen pixels — the anchor every camera snap is
 * measured from.
 *
 * Whole pixels on purpose: a fractional half-viewport would make every snapped
 * position fractional too, which blurs the dot grid and window edges at 1:1.
 */
function cameraAnchor(): { x: number; y: number } {
  const rect = canvasRef.value?.getBoundingClientRect()
  return {
    x: Math.round((rect?.width ?? 0) / 2),
    y: Math.round((rect?.height ?? 0) / 2),
  }
}

/**
 * Snap the camera to the grid laid out FROM the middle of the canvas.
 *
 * The grid used to be anchored at the viewport's top-left corner, which made
 * "back to the middle" a position the grid did not contain: centring put the
 * origin marker dead centre, the next pan-end pulled it a few pixels off, and
 * nothing could put it back exactly. Now the centred view IS snap position
 * zero and every other snap is that plus a whole number of cells, so centring
 * and panning agree instead of fighting.
 *
 * Returns the position unchanged while camera snapping is switched off.
 */
function snapCameraTransform(x: number, y: number): { x: number; y: number } {
  if (!appStore.snapCameraToGrid) return { x, y }
  const anchor = cameraAnchor()
  return {
    x: anchor.x + snapToGrid(x - anchor.x),
    y: anchor.y + snapToGrid(y - anchor.y),
  }
}

/** What "back to the middle" means: world origin centred, always on the grid. */
function centeredCameraTransform(): { x: number; y: number } {
  const anchor = cameraAnchor()
  return snapCameraTransform(anchor.x, anchor.y)
}

provide('snapCameraTransform', snapCameraTransform)
provide('centeredCameraTransform', centeredCameraTransform)

function onMouseUp() {
  if (isPanning.value && appStore.snapCameraToGrid && workspaceId.value) {
    canvasStore.updateTransform(
      workspaceId.value,
      snapCameraTransform(transform.value.x, transform.value.y)
    )
  }
  isPanning.value = false
  isSpacePanning.value = false
}

// ---- Zooming ----
function doZoom(e: WheelEvent) {
  if (!workspaceId.value) return

  const currentScale = transform.value.scale
  // Snap to 5% intervals
  const currentPercent = Math.round(currentScale * 100)
  const newPercent = e.deltaY > 0 ? currentPercent - 5 : currentPercent + 5
  const newScale = Math.min(Math.max(newPercent / 100, 0.1), 5)

  // Zoom toward cursor position
  const rect = canvasRef.value?.getBoundingClientRect()
  if (!rect) return

  const cursorX = e.clientX - rect.left
  const cursorY = e.clientY - rect.top

  const scaleChange = newScale / currentScale
  const newX = cursorX - (cursorX - transform.value.x) * scaleChange
  const newY = cursorY - (cursorY - transform.value.y) * scaleChange

  canvasStore.updateTransform(workspaceId.value, {
    x: newX,
    y: newY,
    scale: newScale,
  })
}

function onWheel(e: WheelEvent) {
  if (!workspaceId.value) return

  // Ctrl+wheel is handled by the document-level capture listener below
  if (e.ctrlKey) return

  const target = e.target as HTMLElement
  const overWindow = target.closest('.canvas-window-body')
  if (overWindow) return

  e.preventDefault()
  doZoom(e)
}

// V3 warm cache: the stage keeps this canvas mounted while another module is
// active, so the global listeners below are attached to a reactive target that
// goes null on deactivation — otherwise Ctrl+wheel would swallow every zoom and
// Ctrl+V every paste in the whole suite. The flag is set in BOTH onMounted and
// onActivated: pages load async and mount after the stage's activation flush,
// so onActivated alone would miss the first visit — but that same late mount
// can land in a stage the user has already left, where setting the flag would
// hand canvas the whole suite's Ctrl+wheel and Ctrl+V with no onDeactivated
// left to take them back, hence the guard.
const isActive = ref(false)
const activeDocument = computed(() => (isActive.value ? document : null))
const activeWindow = computed(() => (isActive.value ? window : null))

onMounted(() => { if (inActiveKeepAliveTree()) isActive.value = true })
onActivated(() => { isActive.value = true })
onDeactivated(() => {
  isActive.value = false
  // The keyup that would clear it lands while we are detached.
  spaceHeld.value = false
})

// Document-level capture listener intercepts Ctrl+wheel before WebView2 native zoom
// or Monaco editors can consume it
useEventListener<WheelEvent>(activeDocument, 'wheel', (e) => {
  if (!e.ctrlKey || !workspaceId.value) return
  e.preventDefault()
  e.stopPropagation()
  doZoom(e)
}, { capture: true, passive: false })

// ---- Ctrl+V to paste copied window ----
useEventListener<KeyboardEvent>(activeWindow, 'keydown', (e) => {
  if (e.ctrlKey && e.key === 'v' && !e.shiftKey && !e.altKey) {
    // Only paste window when focus is on canvas or a window header, not inside terminals/inputs
    const target = e.target as HTMLElement
    if (target.closest('.xterm') || target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) return
    if (!copiedWindow.value) return
    e.preventDefault()
    pasteWindow()
  }
})

// ---- Space key for panning ----
useEventListener<KeyboardEvent>(activeWindow, 'keydown', (e) => {
  if (e.code !== 'Space' || e.repeat) return
  if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return
  // A terminal is neither of those and takes a space as a space; arming the pan
  // grab underneath a half-typed command turned the cursor into a hand.
  if (terminalHasFocus(e.target)) return
  spaceHeld.value = true
})

useEventListener<KeyboardEvent>(activeWindow, 'keyup', (e) => {
  if (e.code === 'Space') {
    spaceHeld.value = false
    if (isSpacePanning.value) {
      isPanning.value = false
      isSpacePanning.value = false
    }
  }
})

// ---- Context menu ----
function onContextMenu(e: MouseEvent) {
  e.preventDefault()
  const rect = canvasRef.value?.getBoundingClientRect()
  contextMenu.value = {
    visible: true,
    x: e.clientX,
    y: e.clientY,
    localX: e.clientX - (rect?.left ?? 0),
    localY: e.clientY - (rect?.top ?? 0),
  }
  // Once the menu has a size, pull it back inside the canvas so a right-click
  // near an edge doesn't clip it against the canvas' overflow.
  nextTick(() => {
    const menu = contextMenuEl.value
    if (!menu || !rect) return
    const m = contextMenu.value
    m.localX = Math.max(0, Math.min(m.localX, rect.width - menu.offsetWidth - 4))
    m.localY = Math.max(0, Math.min(m.localY, rect.height - menu.offsetHeight - 4))
  })
}

function closeContextMenu() {
  contextMenu.value.visible = false
}

function createWindow(type: WindowType, centered = false) {
  if (!workspaceId.value) return

  const rect = canvasRef.value?.getBoundingClientRect()
  if (!rect) return

  // Convert screen position to canvas coordinates
  const scale = transform.value.scale

  const titles: Record<WindowType, string> = {
    terminal: 'Terminal',
    diff: 'Diff Viewer',
    spec: 'Workspace Kanban',
    file: 'File',
    browser: 'Browser',
  }

  const sizes: Record<WindowType, { width: number; height: number }> = {
    terminal: { width: 600, height: 400 },
    diff: { width: 700, height: 500 },
    spec: { width: 800, height: 450 },
    file: { width: 500, height: 400 },
    browser: { width: 700, height: 500 },
  }

  const screenX = centered ? (rect.width - sizes[type].width * scale) / 2 : contextMenu.value.x - rect.left
  const screenY = centered ? (rect.height - sizes[type].height * scale) / 2 : contextMenu.value.y - rect.top
  const canvasX = (screenX - transform.value.x) / scale
  const canvasY = (screenY - transform.value.y) / scale

  const win: CanvasWindowType = {
    id: uuidv4(),
    type,
    title: titles[type],
    position: {
      x: canvasX,
      y: canvasY,
      ...sizes[type],
    },
    status: 'idle',
    minimized: false,
    zIndex: 0,
    // No `terminalId`: session ids are minted by the console plugin when it
    // actually opens a PTY, and a made-up one here is indistinguishable from a
    // real one the window is meant to re-attach to.
    ...(type === 'browser' ? { browserConfig: { url: 'http://localhost:3000' } } : {}),
  }

  canvasStore.addWindow(workspaceId.value, win)
  closeContextMenu()
}

// ---- Open file via system dialog ----

async function openFileOnCanvas() {
  const menuX = contextMenu.value.x
  const menuY = contextMenu.value.y
  closeContextMenu()

  try {
    const selected = await openDialog({
      multiple: false,
      directory: false,
    })
    if (selected) {
      createFileWindow(selected as string, menuX, menuY)
    }
  } catch {
    // User cancelled or dialog error
  }
}

// ---- File preview helpers ----

const IMAGE_EXTS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'ico', 'bmp', 'avif'])
const CODE_EXTS = new Set([
  'ts', 'tsx', 'js', 'jsx', 'vue', 'rs', 'py', 'go', 'java', 'c', 'cpp', 'h', 'hpp',
  'css', 'scss', 'less', 'html', 'xml', 'json', 'yaml', 'yml', 'toml', 'sql', 'graphql',
  'sh', 'bash', 'zsh', 'ps1', 'bat', 'rb', 'php', 'swift', 'kt', 'lua', 'r', 'dart',
  'zig', 'nim', 'ex', 'exs', 'erl', 'hrl', 'clj', 'scala', 'tf', 'hcl',
  'pine',
])

function detectPreviewKind(filePath: string): FilePreviewKind {
  const ext = filePath.split('.').pop()?.toLowerCase() ?? ''
  if (IMAGE_EXTS.has(ext)) return 'image'
  if (ext === 'md' || ext === 'mdx') return 'markdown'
  if (CODE_EXTS.has(ext)) return 'code'
  // Try text for common config/dotfiles
  if (['txt', 'log', 'env', 'gitignore', 'editorconfig', 'prettierrc', 'eslintrc'].includes(ext)) return 'text'
  // Files without extension or unknown — try text
  if (!ext || ext === filePath) return 'text'
  return 'binary'
}

function detectLanguage(filePath: string): string {
  const ext = filePath.split('.').pop()?.toLowerCase() ?? ''
  const map: Record<string, string> = {
    ts: 'typescript', tsx: 'typescriptreact', js: 'javascript', jsx: 'javascriptreact',
    vue: 'vue', html: 'html', css: 'css', scss: 'scss', json: 'json', md: 'markdown',
    yaml: 'yaml', yml: 'yaml', toml: 'toml', rs: 'rust', py: 'python', go: 'go',
    sh: 'shell', sql: 'sql', xml: 'xml', svg: 'xml', pine: 'javascript',
  }
  return map[ext] ?? 'plaintext'
}

function createFileWindow(filePath: string, screenX: number, screenY: number) {
  if (!workspaceId.value) return

  const rect = canvasRef.value?.getBoundingClientRect()
  if (!rect) return

  const scale = transform.value.scale
  const canvasX = (screenX - rect.left - transform.value.x) / scale
  const canvasY = (screenY - rect.top - transform.value.y) / scale

  const fileName = filePath.split(/[/\\]/).pop() ?? filePath
  const previewKind = detectPreviewKind(filePath)

  const isImage = previewKind === 'image'

  const win: CanvasWindowType = {
    id: uuidv4(),
    type: 'file',
    title: fileName,
    position: {
      x: canvasX,
      y: canvasY,
      width: isImage ? 450 : 500,
      height: isImage ? 350 : 400,
    },
    status: 'idle',
    minimized: false,
    zIndex: 0,
    fileConfig: {
      filePath,
      previewKind,
      language: detectLanguage(filePath),
    },
  }

  canvasStore.addWindow(workspaceId.value, win)
}

// ---- Drag & drop from file explorer ----

const isDragOver = ref(false)

function onDragOver(e: DragEvent) {
  if (e.dataTransfer?.types.includes('application/x-quantcode-file')) {
    e.preventDefault()
    e.dataTransfer.dropEffect = 'copy'
    isDragOver.value = true
  }
}

function onDragLeave() {
  isDragOver.value = false
}

function onDrop(e: DragEvent) {
  isDragOver.value = false
  if (!e.dataTransfer) return

  const filePath = e.dataTransfer.getData('application/x-quantcode-file')
  if (filePath) {
    e.preventDefault()
    createFileWindow(filePath, e.clientX, e.clientY)
  }
}

// Listen for "Open on Canvas" events from the file explorer
useEventListener(window, 'qc-open-file-on-canvas', ((e: CustomEvent) => {
  const filePath = e.detail?.filePath
  if (!filePath) return

  // Place near center of the visible canvas area
  const rect = canvasRef.value?.getBoundingClientRect()
  if (!rect) return
  const centerX = rect.left + rect.width / 2
  const centerY = rect.top + rect.height / 2

  createFileWindow(filePath, centerX, centerY)
}) as EventListener)

// ---- Window copy/paste ----
function pasteWindow() {
  if (!copiedWindow.value || !workspaceId.value) return

  const source = copiedWindow.value

  const win: CanvasWindowType = {
    id: uuidv4(),
    type: source.type,
    title: source.title,
    position: {
      x: source.position.x,
      y: source.position.y,
      width: source.position.width,
      height: source.position.height,
    },
    status: 'idle',
    minimized: false,
    zIndex: 0,
    // A pasted terminal is a new shell, so it carries no session — the copy's
    // own id belongs to the copy, and inventing a third opens neither.
    ...(source.type === 'terminal' && source.shellType ? { shellType: source.shellType } : {}),
    ...(source.type === 'file' && source.fileConfig
      ? { fileConfig: { ...source.fileConfig } }
      : {}),
    ...(source.type === 'browser' && source.browserConfig
      ? { browserConfig: { ...source.browserConfig } }
      : {}),
  }

  canvasStore.addWindow(workspaceId.value, win)
}

provide('pasteWindow', pasteWindow)

// ---- Terminal pane actions ----

/** Space left between a window and the neighbour spawned off it. */
const NEIGHBOUR_GAP = 20

/**
 * The canvas answer to a terminal pane's "split": a second terminal window laid
 * against the source's right or bottom edge, same size, same shell.
 *
 * A split inside a canvas window would divide 600×400 into two unusable halves
 * inside a frame the user can already drag to any size. Out here the window
 * manager is the mouse, so the honest reading of "split right" is "another one,
 * to the right" — and the entry stops being a menu item that does nothing.
 */
function spawnTerminalBeside(
  source: CanvasWindowType,
  axis: 'row' | 'col',
  before = false
) {
  if (!workspaceId.value) return

  // `before` is what separates "split left" from "split right": the same axis,
  // the other edge. The offset is the source's own size plus the gap, so the two
  // sit flush the way panes in a split do.
  const step = (axis === 'row' ? source.position.width : source.position.height) + NEIGHBOUR_GAP
  const offset = before ? -step : step

  const win: CanvasWindowType = {
    id: uuidv4(),
    type: 'terminal',
    title: 'Terminal',
    position: {
      x: source.position.x + (axis === 'row' ? offset : 0),
      y: source.position.y + (axis === 'col' ? offset : 0),
      width: source.position.width,
      height: source.position.height,
    },
    status: 'idle',
    minimized: false,
    zIndex: 0,
    // Inherited so the split skips the picker: splitting a PowerShell is a
    // request for another PowerShell, not for the question again.
    ...(source.shellType ? { shellType: source.shellType } : {}),
  }

  canvasStore.addWindow(workspaceId.value, win)
}

provide('spawnTerminalBeside', spawnTerminalBeside)

/** The on-screen rectangle in canvas coordinates — what "fill the view" means. */
function viewportBox() {
  const rect = canvasRef.value?.getBoundingClientRect()
  if (!rect) return null
  const { x, y, scale } = transform.value
  return {
    x: -x / scale,
    y: -y / scale,
    width: rect.width / scale,
    height: rect.height / scale,
  }
}

provide('canvasViewportBox', viewportBox)

useEventListener(window, 'click', () => {
  if (contextMenu.value.visible) closeContextMenu()
})
</script>

<template>
  <div
    ref="canvasRef"
    class="w-full h-full relative overflow-hidden"
    :class="{ 'cursor-grab': spaceHeld && !isPanning, 'cursor-grabbing': isPanning }"
    :style="dotGridStyle"
    @mousedown="onMouseDown"
    @mousemove="onMouseMove"
    @mouseup="onMouseUp"
    @mouseleave="onMouseUp"
    @wheel="onWheel"
    @contextmenu="onContextMenu"
    @dragover="onDragOver"
    @dragleave="onDragLeave"
    @drop="onDrop"
  >
    <div v-if="!windows.length && workspaceId" class="canvas-start" @mousedown.stop @wheel.stop>
      <QEmptyState title="Empty canvas" description="Add a window or drop a file here.">
        <template #icon><AppWindow :size="24" /></template>
        <button class="canvas-start-action" @click="createWindow('terminal', true)"><Terminal :size="16" /> Terminal</button>
        <button class="canvas-start-action" @click="createWindow('spec', true)"><LayoutDashboard :size="16" /> Kanban</button>
        <button class="canvas-start-action" @click="createWindow('browser', true)"><Globe :size="16" /> Browser</button>
      </QEmptyState>
    </div>

    <!-- Drag-drop overlay -->
    <div
      v-if="isDragOver"
      class="absolute inset-0 z-30 pointer-events-none flex items-center justify-center"
      :style="{ background: 'rgba(59, 130, 246, 0.08)', border: '2px dashed rgba(59, 130, 246, 0.4)' }"
    >
      <span class="text-blue-400 text-xs font-medium px-3 py-1.5 rounded" :style="{ background: 'var(--qc-bg-header)' }">
        Drop file to open on canvas
      </span>
    </div>

    <!-- Zoom / tile count overlay -->
    <div class="canvas-status">
      {{ zoomPercent }}% <span>·</span> {{ tileCount }} {{ tileCount === 1 ? 'window' : 'windows' }}
    </div>

    <!-- Windows container - transformed -->
    <div
      class="absolute top-0 left-0 origin-top-left"
      :style="{ transform: canvasTransformStyle }"
    >
      <!-- Origin marker — world (0,0), the canvas' fixed reference point.
           Counter-scaled so it keeps its screen size at every zoom; the
           minimap's double-click centers the view back on it. -->
      <div
        class="absolute top-0 left-0 pointer-events-none select-none"
        :style="{ transform: `translate(-50%, -50%) scale(${1 / transform.scale})` }"
      >
        <svg width="37" height="37" viewBox="0 0 37 37" fill="none">
          <path
            d="M18.5 1 V13 M18.5 24 V36 M1 18.5 H13 M24 18.5 H36"
            :stroke="'color-mix(in srgb, var(--qc-text-dim) 65%, transparent)'"
            stroke-width="1"
            stroke-linecap="round"
          />
          <circle cx="18.5" cy="18.5" r="2" :fill="'color-mix(in srgb, var(--qc-text-dim) 80%, transparent)'" />
        </svg>
      </div>

      <CanvasCanvasWindow
        v-for="win in windows"
        :key="win.id"
        :window="win"
      />
    </div>

    <!-- Minimap -->
    <CanvasCanvasMinimap />

    <!-- Context menu -->
    <div
      v-if="contextMenu.visible"
      ref="contextMenuEl"
      class="absolute z-[9999] rounded-lg shadow-2xl py-1 min-w-[180px]"
      :style="{ left: contextMenu.localX + 'px', top: contextMenu.localY + 'px', background: 'var(--qc-bg-header)', border: '1px solid var(--qc-border)' }"
      @click.stop
    >
      <button
        class="w-full text-left px-4 py-2 text-xs transition-colors flex items-center gap-2 hover:brightness-125"
        :style="{ color: 'var(--qc-text)' }"
        @click="createWindow('terminal')"
      >
        <span class="text-[#22d3ee]">&#9645;</span> New Terminal
      </button>
      <button
        class="w-full text-left px-4 py-2 text-xs transition-colors flex items-center gap-2 hover:brightness-125"
        :style="{ color: 'var(--qc-text)' }"
        @click="createWindow('diff')"
      >
        <span class="text-green-400">&#177;</span> New Diff Viewer
      </button>
      <button
        class="w-full text-left px-4 py-2 text-xs transition-colors flex items-center gap-2 hover:brightness-125"
        :style="{ color: 'var(--qc-text)' }"
        @click="createWindow('spec')"
      >
        <span class="text-blue-400">&#9635;</span> New Kanban Board
      </button>
      <button
        class="w-full text-left px-4 py-2 text-xs transition-colors flex items-center gap-2 hover:brightness-125"
        :style="{ color: 'var(--qc-text)' }"
        @click="createWindow('browser')"
      >
        <span class="text-sky-400">&#9741;</span> New Browser
      </button>
      <div :style="{ borderTop: '1px solid var(--qc-border)', margin: '4px 0' }" />
      <button
        class="w-full text-left px-4 py-2 text-xs transition-colors flex items-center gap-2 hover:brightness-125"
        :style="{ color: 'var(--qc-text)' }"
        @click="openFileOnCanvas"
      >
        <span class="text-purple-400">&#128196;</span> Open File on Canvas...
      </button>
      <template v-if="copiedWindow">
        <div :style="{ borderTop: '1px solid var(--qc-border)', margin: '4px 0' }" />
        <button
          class="w-full text-left px-4 py-2 text-xs transition-colors flex items-center gap-2 hover:brightness-125"
          :style="{ color: 'var(--qc-text)' }"
          @click="pasteWindow(); closeContextMenu()"
        >
          <span class="text-emerald-400">&#9112;</span> Paste Window
        </button>
      </template>
    </div>
  </div>
</template>

<style scoped>
.canvas-start { position: absolute; inset: 50% auto auto 50%; transform: translate(-50%, -50%); z-index: 2; width: min(500px, calc(100% - 48px)); border: 1px solid var(--qc-border); border-radius: 20px; background: var(--qc-bg-header); box-shadow: 0 18px 60px rgb(0 0 0 / .12); }
.canvas-start-action { display: inline-flex; align-items: center; gap: 8px; padding: 10px 14px; border: 1px solid var(--qc-border); border-radius: 9px; background: var(--qc-bg-surface); color: var(--qc-text); font-size: 12px; cursor: pointer; }
.canvas-start-action:hover { background: var(--qss-bg-hover); }
.canvas-status { position: absolute; top: 16px; left: 50%; transform: translateX(-50%); z-index: 10; display: flex; gap: 10px; padding: 6px 12px; border: 1px solid var(--qc-border-subtle); border-radius: 8px; background: var(--qc-bg-header); color: var(--qc-text-muted); font-size: 11px; font-variant-numeric: tabular-nums; pointer-events: none; }
</style>

<style scoped>
</style>
