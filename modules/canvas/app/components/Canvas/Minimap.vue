<script setup lang="ts">
import { useCanvasStore } from '../../../stores/canvas'
import { useWorkspacesStore } from '../../../stores/workspaces'
import { useAppStore } from '../../../stores/app'

const canvasStore = useCanvasStore()
const workspacesStore = useWorkspacesStore()
const appStore = useAppStore()

const rightSidebarWidth = inject<Ref<number>>('rightSidebarWidth', ref(220))

const expanded = ref(true)

const minimapRef = ref<HTMLElement | null>(null)
const canvasRef = inject<Ref<HTMLElement | null>>('canvasRef', ref(null))

const MINIMAP_W = 200
const MINIMAP_H = 150
const PADDING = 20

const workspaceId = computed(() => workspacesStore.contentWorkspaceId)
const canvasState = computed(() => canvasStore.activeCanvasState)
const windows = computed(() => canvasState.value?.windows ?? [])
const transform = computed(() => canvasState.value?.transform ?? { x: 0, y: 0, scale: 1 })

// Track actual canvas container size
const containerSize = ref({ width: 800, height: 600 })

function updateContainerSize() {
  const el = canvasRef?.value
  if (el) {
    containerSize.value = { width: el.clientWidth, height: el.clientHeight }
  }
}

let resizeObserver: ResizeObserver | null = null

watch(canvasRef, (el) => {
  resizeObserver?.disconnect()
  if (el) {
    updateContainerSize()
    resizeObserver = new ResizeObserver(updateContainerSize)
    resizeObserver.observe(el)
  }
}, { immediate: true })

onUnmounted(() => {
  resizeObserver?.disconnect()
})

// Compute bounding box that includes all windows AND the current viewport
const bounds = computed(() => {
  const scale = transform.value.scale
  const viewX = -transform.value.x / scale
  const viewY = -transform.value.y / scale
  const viewW = containerSize.value.width / scale
  const viewH = containerSize.value.height / scale

  let minX = viewX
  let minY = viewY
  let maxX = viewX + viewW
  let maxY = viewY + viewH

  for (const w of windows.value) {
    minX = Math.min(minX, w.position.x)
    minY = Math.min(minY, w.position.y)
    maxX = Math.max(maxX, w.position.x + w.position.width)
    maxY = Math.max(maxY, w.position.y + w.position.height)
  }

  // Add some padding
  const rangeX = maxX - minX || 400
  const rangeY = maxY - minY || 300
  const padX = rangeX * 0.15
  const padY = rangeY * 0.15
  return {
    minX: minX - padX,
    minY: minY - padY,
    maxX: maxX + padX,
    maxY: maxY + padY,
  }
})

const worldWidth = computed(() => bounds.value.maxX - bounds.value.minX)
const worldHeight = computed(() => bounds.value.maxY - bounds.value.minY)
const minimapScale = computed(() => {
  const sw = (MINIMAP_W - PADDING * 2) / worldWidth.value
  const sh = (MINIMAP_H - PADDING * 2) / worldHeight.value
  return Math.min(sw, sh)
})

/**
 * Top-left of the scaled world box inside the minimap.
 *
 * The scale fits the world to whichever axis is tighter, so the other axis is
 * left with slack. Offsetting by PADDING alone spent all of that slack on one
 * side — the map hugged the top edge with a dead band below it. Splitting the
 * slack centres the world on both axes; on the constraining axis it works out
 * to PADDING, so the fitted axis is unchanged.
 */
const contentOffset = computed(() => ({
  x: (MINIMAP_W - worldWidth.value * minimapScale.value) / 2,
  y: (MINIMAP_H - worldHeight.value * minimapScale.value) / 2,
}))

function toMinimapCoords(x: number, y: number) {
  return {
    x: contentOffset.value.x + (x - bounds.value.minX) * minimapScale.value,
    y: contentOffset.value.y + (y - bounds.value.minY) * minimapScale.value,
  }
}

// Window rectangles on the minimap
const windowRects = computed(() =>
  windows.value.map((w) => {
    const pos = toMinimapCoords(w.position.x, w.position.y)
    // Different gray intensities per window type — theme-adaptive via --qc-text
    const mixPcts: Record<string, number> = {
      terminal: 65,
      diff: 40,
      spec: 55,
    }
    return {
      id: w.id,
      x: pos.x,
      y: pos.y,
      width: w.position.width * minimapScale.value,
      height: w.position.height * minimapScale.value,
      mix: mixPcts[w.type] ?? 35,
    }
  })
)

// Viewport rectangle
const viewportRect = computed(() => {
  const scale = transform.value.scale
  const cw = containerSize.value.width
  const ch = containerSize.value.height

  // The visible area in world coordinates
  const viewX = -transform.value.x / scale
  const viewY = -transform.value.y / scale
  const viewW = cw / scale
  const viewH = ch / scale

  const pos = toMinimapCoords(viewX, viewY)
  return {
    x: pos.x,
    y: pos.y,
    width: viewW * minimapScale.value,
    height: viewH * minimapScale.value,
  }
})

/**
 * The canvas' own camera snap, so a minimap jump lands where a pan would.
 *
 * Both are laid out from the middle of the canvas, which is what makes
 * centring reachable: the canvas owns the grid, this only asks it.
 */
const snapCameraTransform = inject<(x: number, y: number) => { x: number; y: number }>(
  'snapCameraTransform',
  (x, y) => ({ x, y })
)

/** The centred camera position — world (0,0) dead centre, on the snap grid. */
const centeredCameraTransform = inject<() => { x: number; y: number }>(
  'centeredCameraTransform',
  () => ({ x: containerSize.value.width / 2, y: containerSize.value.height / 2 })
)

function onMinimapClick(e: MouseEvent) {
  if (!workspaceId.value || !minimapRef.value) return

  const rect = minimapRef.value.getBoundingClientRect()
  const clickX = e.clientX - rect.left
  const clickY = e.clientY - rect.top

  // Convert minimap coords to world coords — the inverse of toMinimapCoords,
  // so it has to read the same centred offset, not PADDING.
  const worldX = (clickX - contentOffset.value.x) / minimapScale.value + bounds.value.minX
  const worldY = (clickY - contentOffset.value.y) / minimapScale.value + bounds.value.minY

  // Center the viewport on this point
  const cw = containerSize.value.width
  const ch = containerSize.value.height
  const scale = transform.value.scale

  canvasStore.updateTransform(
    workspaceId.value,
    snapCameraTransform(-worldX * scale + cw / 2, -worldY * scale + ch / 2)
  )
}

/** The canvas origin (world 0,0) on the minimap, for the crosshair. */
const originPos = computed(() => toMinimapCoords(0, 0))

/**
 * Double-click: back to the middle — world (0,0) centred in the view, at the
 * current zoom. The two single clicks that precede a dblclick have already
 * panned via onMinimapClick; this overwrites their result, so no guard needed.
 *
 * Exact, not approximate: the snap grid is measured from this very position,
 * so centring can never be rounded away from the centre.
 */
function centerOrigin() {
  if (!workspaceId.value) return
  canvasStore.updateTransform(workspaceId.value, centeredCameraTransform())
}
</script>

<template>
  <!-- data-canvas-overlay: browser windows are native webviews floating above
       all HTML, so they cut this rect out of themselves to keep the map on top
       (BrowserWindow.vue, computeObscuringRects). -->
  <div v-if="appStore.canvasMinimap" data-canvas-overlay class="absolute bottom-3 z-10 transition-[right] duration-250 ease-[cubic-bezier(0.4,0,0.2,1)]" :style="{ right: appStore.editorVisible ? (rightSidebarWidth + 12) + 'px' : '12px' }">
    <!-- Toggle button (minimized state) -->
    <button
      v-if="!expanded"
      class="rounded px-2 py-1 text-[10px] transition-colors"
      :style="{ background: 'var(--qc-bg-header)', border: '1px solid var(--qc-border)', color: 'var(--qc-text-muted)' }"
      @click="expanded = true"
    >
      Map
    </button>

    <!-- Minimap (expanded state) -->
    <div
      v-if="expanded"
      ref="minimapRef"
      class="rounded-lg overflow-hidden cursor-pointer relative"
      :style="{ width: MINIMAP_W + 'px', height: MINIMAP_H + 'px', background: 'color-mix(in srgb, var(--qc-bg-titlebar) 90%, transparent)', border: '1px solid var(--qc-border)' }"
      title="Click: jump there · Double-click: back to center"
      @click="onMinimapClick"
      @dblclick="centerOrigin"
    >
      <!-- Close/minimize button -->
      <button
        class="absolute top-1 right-1 z-10 w-4 h-4 flex items-center justify-center text-[8px] transition-colors"
        :style="{ color: 'var(--qc-text-muted)' }"
        @click.stop="expanded = false"
      >
        &#10005;
      </button>

      <!-- Window rectangles -->
      <div
        v-for="wr in windowRects"
        :key="wr.id"
        class="absolute rounded-sm opacity-70"
        :style="{
          left: wr.x + 'px',
          top: wr.y + 'px',
          width: Math.max(4, wr.width) + 'px',
          height: Math.max(3, wr.height) + 'px',
          backgroundColor: `color-mix(in srgb, var(--qc-text) ${wr.mix}%, transparent)`,
        }"
      />

      <!-- Origin crosshair — world (0,0); the container clips it away when the
           view is too far out for it to fit. -->
      <svg
        class="absolute pointer-events-none"
        :style="{ left: (originPos.x - 4.5) + 'px', top: (originPos.y - 4.5) + 'px' }"
        width="9" height="9" viewBox="0 0 9 9" fill="none"
      >
        <path
          d="M4.5 0 V9 M0 4.5 H9"
          :stroke="'color-mix(in srgb, var(--qc-text-dim) 85%, transparent)'"
          stroke-width="1"
        />
      </svg>

      <!-- Viewport rectangle -->
      <div
        class="absolute border border-white/30 rounded-sm pointer-events-none"
        :style="{
          left: viewportRect.x + 'px',
          top: viewportRect.y + 'px',
          width: viewportRect.width + 'px',
          height: viewportRect.height + 'px',
        }"
      />
    </div>
  </div>
</template>
