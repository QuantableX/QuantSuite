<script setup lang="ts">
/**
 * The split layout — `ConsoleSplitTree` (docs/PLAN-CONSOLE.md §5, phase P3).
 *
 * Renders one `PaneNode` and, for a split, itself once per child. The tree lives
 * in the store; this component only lays it out and reports where a divider was
 * dragged, which is why a drag emits fractions rather than mutating anything.
 *
 * **It no longer contains the panes.** The leaf slot is an empty placeholder and
 * the page draws the panes over it, positioned from the same fractions plus the
 * dividers' pixels (`paneRects` in the store). Moving a pane between component
 * instances — which is what a split did while they lived in here — makes Vue
 * destroy and rebuild it, and a rebuilt pane has no xterm and nothing a
 * full-screen program had drawn. So this component owns the geometry and the
 * dividers; the seams it leaves between the tiles are what the pointer finds.
 *
 * Three decisions worth knowing before editing:
 *
 *   - **Every child is `flex-basis: 0` plus `flex-grow: <fraction>`.** The
 *     alternative — `flex: 0 0 <fraction * 100>%` — has to subtract each child's
 *     share of the dividers from its own percentage, and gets it wrong the moment
 *     a split has three children or a divider is hidden by zoom. With a basis of
 *     zero the dividers eat their 4px out of the free space *before* the grow
 *     factors divide it, so the arithmetic disappears.
 *   - **Zoom hides, it never unmounts.** A pane that unmounts mid-command loses
 *     its xterm instance and its scroll position, and the session behind it would
 *     be re-attached for nothing. So the siblings of a zoomed pane get
 *     `display: none` and stay in the DOM (see `boxes`) — and the page keeps the
 *     zoomed *tile* at the full box, from the same rule.
 */
import { computed, onUnmounted, ref, type CSSProperties } from 'vue'
import type { PaneNode } from '../types/multiplex'

/**
 * Recursion resolves through Nuxt's global registry (`prefix: 'Console'`), but the
 * name is declared here as well so the component still finds itself if it is ever
 * imported directly — an unresolved self-reference renders nothing and only logs.
 */
defineOptions({ name: 'ConsoleSplitTree' })

const props = defineProps<{
  node: PaneNode
  /** When set, only this leaf is shown, filling the whole box. */
  zoomedPaneKey: string | null
}>()

const emit = defineEmits<{
  /** A divider was dragged: new fractions for that split node, summing to 1. */
  (e: 'resize', payload: { id: string; sizes: number[] }): void
}>()

/**
 * The leaf slot carries no props. It used to hand out `paneKey`/`active`, from
 * the days the panes rendered in here — the page draws a bare placeholder now
 * (its panes are absolutely positioned from `paneRects`) and forwarding data
 * nobody reads was only a second contract to keep in sync (q1 #13).
 */
defineSlots<{
  leaf(): unknown
}>()

/** Divider hit area. Wider than the 1px it draws — a 1px target is unhittable. */
const DIVIDER_PX = 4
/** No drag may leave a pane smaller than this; below it a terminal is useless. */
const MIN_PANE_PX = 120

const containerEl = ref<HTMLElement | null>(null)

const splitNode = computed(() => (props.node.kind === 'split' ? props.node : null))
/** Empty for a split — the template branches on truthiness, not on `kind`. */
const leafPaneKey = computed(() => (props.node.kind === 'leaf' ? props.node.paneKey : ''))

// ── tree queries ─────────────────────────────────────────────────────────────

function containsPaneKey(node: PaneNode, paneKey: string): boolean {
  if (node.kind === 'leaf') return node.paneKey === paneKey
  return node.children.some((child) => containsPaneKey(child, paneKey))
}

// ── sizes ────────────────────────────────────────────────────────────────────

/**
 * The split's fractions, normalized. Malformed input falls back to equal shares
 * rather than throwing: a tree that arrives from a restored session with one
 * fraction too few would otherwise take the whole console down, and equal shares
 * are a layout the user can fix with one drag.
 */
const sizes = computed<number[]>(() => {
  const split = splitNode.value
  if (!split) return []
  const count = split.children.length
  if (count === 0) return []

  const equal = Array.from({ length: count }, () => 1 / count)
  const raw = split.sizes
  if (!Array.isArray(raw) || raw.length !== count) return equal

  let sum = 0
  for (const value of raw) {
    // Also catches a non-number that slipped past the type at runtime.
    if (!Number.isFinite(value) || value <= 0) return equal
    sum += value
  }
  return raw.map((value) => value / sum)
})

/**
 * Fractions the drag is currently painting.
 *
 * The store is the source of truth, but the pointer must not wait for a
 * round-trip through it — and if the page ever forgets to handle `resize`, this
 * keeps the drag itself smooth instead of a divider that refuses to move.
 */
const dragSizes = ref<number[] | null>(null)
const effectiveSizes = computed(() => dragSizes.value ?? sizes.value)

/** Index of the divider being dragged, or -1. */
const dragIndex = ref(-1)

/** Which child holds the zoomed leaf, or -1 when zoom does not apply here. */
const zoomIndex = computed(() => {
  const split = splitNode.value
  const target = props.zoomedPaneKey
  if (!split || !target) return -1
  // A stale key that matches nothing must not blank the layout, so an index of
  // -1 is treated as "not zoomed" everywhere below.
  return split.children.findIndex((child) => containsPaneKey(child, target))
})

interface ChildBox {
  node: PaneNode
  style: CSSProperties
}

const boxes = computed<ChildBox[]>(() => {
  const split = splitNode.value
  if (!split) return []
  const zoom = zoomIndex.value
  const fractions = effectiveSizes.value

  return split.children.map((child, index) => {
    if (zoom >= 0 && index !== zoom) {
      // Hidden, not removed: the pane keeps its DOM, its xterm instance and its
      // scroll offset, and reports 0x0 while hidden — which the pane's own
      // guard already refuses to send to the PTY.
      return { node: child, style: { display: 'none' } }
    }
    const grow = zoom >= 0 ? 1 : (fractions[index] ?? 1 / split.children.length)
    return { node: child, style: { flexGrow: String(grow) } }
  })
})

/** No dividers while zoomed — there is nothing beside the zoomed pane to resize. */
const dividersVisible = computed(() => zoomIndex.value < 0)

// ── drag ─────────────────────────────────────────────────────────────────────

interface DragState {
  pointerId: number
  /** Divider between child `index` and `index + 1`. */
  index: number
  /** Pointer coordinate along the split's axis when the drag began. */
  startPos: number
  startA: number
  startB: number
  /** Pixels the fractions divide: the box minus the dividers' fixed share. */
  available: number
  /** `MIN_PANE_PX` expressed against `available`. */
  minFraction: number
}

let drag: DragState | null = null
let frame = 0
let pending: number[] | null = null

/** Force the sum to exactly 1; float division alone lands on 0.9999999999999998. */
function toUnitSum(values: number[]): number[] {
  if (values.length === 0) return values
  const sum = values.reduce((a, b) => a + b, 0)
  if (!Number.isFinite(sum) || sum <= 0) return Array.from({ length: values.length }, () => 1 / values.length)
  const out = values.map((value) => value / sum)
  const last = out.length - 1
  out[last] = out[last] + (1 - out.reduce((a, b) => a + b, 0))
  return out
}

function emitSizes(values: number[]) {
  const split = splitNode.value
  if (!split) return
  emit('resize', { id: split.id, sizes: toUnitSum(values) })
}

/**
 * Coalesce to one emit per frame. Emitting on every `pointermove` is the same
 * payload shape, but each one re-renders the store's subscribers, and a fast
 * mouse produces several moves per frame — work nothing can see.
 */
function scheduleEmit(values: number[]) {
  pending = values
  if (frame) return
  frame = requestAnimationFrame(() => {
    frame = 0
    const next = pending
    pending = null
    if (next) emitSizes(next)
  })
}

function cancelScheduled() {
  if (frame) cancelAnimationFrame(frame)
  frame = 0
  pending = null
}

function onDividerDown(event: PointerEvent, index: number) {
  const split = splitNode.value
  const container = containerEl.value
  if (!split || !container || event.button !== 0) return

  const current = effectiveSizes.value
  const startA = current[index]
  const startB = current[index + 1]
  if (!Number.isFinite(startA) || !Number.isFinite(startB)) return

  const rect = container.getBoundingClientRect()
  const total = split.dir === 'row' ? rect.width : rect.height
  const available = total - (current.length - 1) * DIVIDER_PX
  // A split that has not been laid out yet measures ~0; dividing by that turns
  // the first pointer move into an Infinity fraction.
  if (available < 2) return

  const element = event.currentTarget
  if (!(element instanceof HTMLElement)) return
  try {
    // Capture, so a pointer that leaves the window (or crosses an iframe, or
    // ends up over the xterm canvas) keeps delivering moves here instead of
    // stranding the layout mid-drag.
    element.setPointerCapture(event.pointerId)
  } catch {
    return
  }

  const pair = startA + startB
  drag = {
    pointerId: event.pointerId,
    index,
    startPos: split.dir === 'row' ? event.clientX : event.clientY,
    startA,
    startB,
    available,
    // In a box too small for two minimums the divider clamps to the middle of
    // the pair instead of to a minimum it cannot honour on both sides.
    minFraction: Math.min(MIN_PANE_PX / available, pair / 2),
  }
  dragIndex.value = index
  dragSizes.value = [...current]
  // Keeps the drag from selecting the block text underneath. `dblclick` is not a
  // compatibility mouse event and still fires, so the reset below survives this.
  event.preventDefault()
}

function onDividerMove(event: PointerEvent) {
  const split = splitNode.value
  if (!drag || !split || event.pointerId !== drag.pointerId) return

  const current = dragSizes.value
  if (!current || current.length !== split.children.length) {
    // The tree changed under the drag (a pane closed elsewhere); stop rather
    // than write fractions against children that no longer exist.
    endDrag(false)
    return
  }

  const pos = split.dir === 'row' ? event.clientX : event.clientY
  const pair = drag.startA + drag.startB
  const shift = (pos - drag.startPos) / drag.available

  let a = drag.startA + shift
  if (a < drag.minFraction) a = drag.minFraction
  if (a > pair - drag.minFraction) a = pair - drag.minFraction

  const next = [...current]
  next[drag.index] = a
  next[drag.index + 1] = pair - a
  dragSizes.value = next
  scheduleEmit(next)
}

function endDrag(commit: boolean) {
  if (!drag) return
  cancelScheduled()
  const final = dragSizes.value
  drag = null
  dragIndex.value = -1
  dragSizes.value = null
  // The final value goes out even when the last frame already carried it: the
  // pointer may have moved after that frame, and the store must end up with
  // what the user actually sees.
  if (commit && final) emitSizes(final)
}

function onDividerUp(event: PointerEvent) {
  if (!drag || event.pointerId !== drag.pointerId) return
  endDrag(true)
}

/** Lost capture without a pointerup (element removed, browser took over). */
function onCaptureLost(event: PointerEvent) {
  if (!drag || event.pointerId !== drag.pointerId) return
  endDrag(true)
}

/** Double-click resets the split — the cheapest way back from a bad drag. */
function resetSizes() {
  const split = splitNode.value
  if (!split) return
  const count = split.children.length
  if (count < 2) return
  dragSizes.value = null
  emit('resize', { id: split.id, sizes: toUnitSum(Array.from({ length: count }, () => 1)) })
}

// ── focus ────────────────────────────────────────────────────────────────────

onUnmounted(cancelScheduled)
</script>

<template>
  <!-- Nothing at all for a split whose `children` array is empty: an empty box
       would still claim its grow share of the layout. -->
  <div
    v-if="leafPaneKey || boxes.length"
    ref="containerEl"
    class="console-split-node"
    :class="{
      'is-row': splitNode?.dir === 'row',
      'is-col': splitNode?.dir === 'col',
      'is-dragging': dragIndex >= 0,
    }"
  >
    <!-- A leaf: the page fills the box through the slot (a placeholder — the
         panes themselves are positioned by the page from `paneRects`). -->
    <slot v-if="leafPaneKey" name="leaf" />

    <!-- A split. An empty `children` array renders neither box nor divider. -->
    <template v-for="(box, index) in boxes" :key="box.node.id">
      <ConsoleSplitTree
        :node="box.node"
        :zoomed-pane-key="zoomedPaneKey"
        :style="box.style"
        @resize="emit('resize', $event)"
      >
        <!-- Forwarded explicitly: without this every nested split renders empty. -->
        <template #leaf>
          <slot name="leaf" />
        </template>
      </ConsoleSplitTree>

      <div
        v-if="dividersVisible && index < boxes.length - 1"
        class="console-split-divider"
        :class="{ 'is-active': dragIndex === index }"
        role="separator"
        :aria-orientation="splitNode?.dir === 'row' ? 'vertical' : 'horizontal'"
        @pointerdown="onDividerDown($event, index)"
        @pointermove="onDividerMove"
        @pointerup="onDividerUp"
        @pointercancel="onDividerUp"
        @lostpointercapture="onCaptureLost"
        @dblclick="resetSizes"
      >
        <span class="console-split-grip" aria-hidden="true" />
      </div>
    </template>
  </div>
</template>

<style scoped>
.console-split-node {
  position: relative;
  box-sizing: border-box;
  /* Basis zero, grow carries the fraction (see the header): the dividers take
     their 4px out of the free space before the fractions divide it. `width` and
     `height` only matter for the outermost node, where the page's container may
     be a plain block rather than a flex parent. */
  flex-grow: 1;
  flex-shrink: 1;
  flex-basis: 0;
  width: 100%;
  height: 100%;
  /* Without these a pane's content sets a min-content floor and refuses to
     shrink — the divider then drags against a pane that will not give way. */
  min-width: 0;
  min-height: 0;
  overflow: hidden;
}

.console-split-node.is-row {
  display: flex;
  flex-direction: row;
}
.console-split-node.is-col {
  display: flex;
  flex-direction: column;
}

.console-split-divider {
  flex: 0 0 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  /* Or the browser scrolls/pans the pane instead of resizing on touch. */
  touch-action: none;
  background: transparent;
}
.is-row > .console-split-divider {
  cursor: col-resize;
}
.is-col > .console-split-divider {
  cursor: row-resize;
}

.console-split-grip {
  background: var(--qss-border-subtle);
  transition: background var(--qss-dur-fast) var(--qss-ease-out);
}
/* 2px drawn inside a 4px hit area: the reference draws the seam at 2px and
   widens only what the pointer has to find. A 1px hairline between two panes of
   the same colour reads as a rendering artefact rather than an edge. */
.is-row > .console-split-divider > .console-split-grip {
  width: 2px;
  height: 100%;
}
.is-col > .console-split-divider > .console-split-grip {
  width: 100%;
  height: 2px;
}
.console-split-divider:hover > .console-split-grip {
  background: color-mix(in srgb, var(--qss-accent) 55%, var(--qss-border));
}
.console-split-divider.is-active > .console-split-grip {
  background: var(--qss-accent);
  /* No transition while dragging: the grip must sit exactly where the pointer
     is, not lag 140ms behind it. */
  transition: none;
}

/* While dragging, the whole split keeps the resize cursor and stops selecting
   text — the pointer spends the drag over a pane, not over the divider. */
.console-split-node.is-dragging {
  user-select: none;
}
.console-split-node.is-dragging.is-row {
  cursor: col-resize;
}
.console-split-node.is-dragging.is-col {
  cursor: row-resize;
}
</style>
