/**
 * Drag-to-resize plus width/open persistence for the suite's edge panels (V3).
 *
 * QSidebar and QRightPanel are the same panel mirrored: same pointer
 * bookkeeping, same `localStorage` round-trip, same 20px snap. They carried two
 * copies of it and had already drifted (snap points vs a single default width,
 * a viewport fraction vs a fixed maximum), so the copies live here once and the
 * differences are passed in.
 *
 * Everything variable is a getter: the callers are `<script setup>` components
 * reading their own props, which must stay reactive.
 */

import { onMounted, onUnmounted, ref, watch } from 'vue'

const SNAP_THRESHOLD = 20

export interface PanelResizeOptions {
  /** `localStorage` namespace — `qss-sidebar` / `qss-rpanel`. */
  prefix: string
  /** `1` grows with the cursor (left edge), `-1` against it (right edge). */
  direction: 1 | -1
  /**
   * The width the panel opens at, and the one a double-click returns to.
   *
   * Read as a getter so a module can drive the width from outside, and captured
   * once at setup as the *origin* (see `originWidth`).
   */
  defaultWidth: () => number
  minWidth: () => number
  /** Upper bound in px, re-read per move (the sidebar's is viewport-relative). */
  maxWidth: () => number
  /** Widths the drag snaps to within `SNAP_THRESHOLD`. */
  snapPoints: () => number[]
  storageKey: () => string | undefined
  resizable: () => boolean
  open: () => boolean
  setWidth: (width: number) => void
  setOpen: (open: boolean) => void
}

export function usePanelResize(options: PanelResizeOptions) {
  const currentWidth = ref(options.defaultWidth())

  /**
   * Where a double-click puts the panel back.
   *
   * Captured once, and that is the whole point. Every module binds this panel
   * two-way — `:width="w"` with `@update:width="w = $event"` — so `defaultWidth()`
   * *is* the current width a moment after the first drag, and resetting to it did
   * exactly nothing. The origin is the width the panel was created with, which is
   * the position the user means by "back where it was".
   */
  const originWidth = options.defaultWidth()

  watch(options.defaultWidth, (w) => {
    currentWidth.value = w
  })

  onMounted(() => {
    const key = options.storageKey()
    if (!key) return
    try {
      const raw = localStorage.getItem(`${options.prefix}:${key}`)
      if (raw) {
        const saved = JSON.parse(raw)
        if (typeof saved.width === 'number') {
          currentWidth.value = saved.width
          options.setWidth(saved.width)
        }
        if (typeof saved.open === 'boolean') options.setOpen(saved.open)
      }
    } catch {
      /* ignore */
    }
  })

  function persist() {
    const key = options.storageKey()
    if (!key) return
    try {
      localStorage.setItem(
        `${options.prefix}:${key}`,
        JSON.stringify({ width: currentWidth.value, open: options.open() })
      )
    } catch {
      /* ignore */
    }
  }
  watch(options.open, persist)

  const resizing = ref(false)
  let startX = 0
  let startWidth = 0

  function startResize(e: MouseEvent) {
    if (!options.resizable()) return
    e.preventDefault()
    resizing.value = true
    startX = e.clientX
    startWidth = currentWidth.value
    document.addEventListener('mousemove', onMove)
    document.addEventListener('mouseup', onUp)
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'
  }
  function onMove(e: MouseEvent) {
    if (!resizing.value) return
    let w = Math.min(
      options.maxWidth(),
      Math.max(options.minWidth(), startWidth + options.direction * (e.clientX - startX))
    )
    for (const snap of options.snapPoints()) {
      if (Math.abs(w - snap) < SNAP_THRESHOLD) {
        w = snap
        break
      }
    }
    currentWidth.value = w
    options.setWidth(w)
  }
  function onUp() {
    resizing.value = false
    document.removeEventListener('mousemove', onMove)
    document.removeEventListener('mouseup', onUp)
    document.body.style.cursor = ''
    document.body.style.userSelect = ''
    persist()
  }
  // A panel destroyed mid-drag never sees its mouseup: both document listeners
  // would stay attached and `body` would keep `col-resize` and
  // `user-select: none` — no text selectable anywhere for the rest of the
  // session. Only while dragging, so an ordinary unmount touches no body style.
  onUnmounted(() => {
    if (resizing.value) onUp()
  })

  function resetWidth() {
    currentWidth.value = originWidth
    options.setWidth(originWidth)
    persist()
  }

  return { currentWidth, resizing, startResize, resetWidth, persist }
}
