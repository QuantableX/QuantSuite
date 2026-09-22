<script setup lang="ts">
/**
 * The module right panel (V3) — 220px default, mirrors QSidebar on the right
 * edge. Resizable by default (min 180 / max 560): panels that used to be
 * wider (QuantTerminal's 440px watchlist) stay usable by dragging, while the
 * default geometry matches every other module.
 *
 * `maxWidthFraction` and `snapPoints` mirror QSidebar's and default to *off*, so
 * this panel keeps behaving exactly as it did unless a module asks for the other
 * shape. QuantConsole asks: its right panel is a tools panel and is dragged the
 * way QuantCanvas's is, which is a fraction of the viewport with two snaps.
 */
import { usePanelResize } from '../composables/usePanelResize'

const props = withDefaults(
  defineProps<{
    modelValue?: boolean
    width?: number
    resizable?: boolean
    minWidth?: number
    maxWidth?: number
    /** Upper bound as a share of the viewport. Wins over `maxWidth` when set. */
    maxWidthFraction?: number
    /** Widths the drag snaps to. Defaults to the panel's own default width. */
    snapPoints?: number[]
    overlay?: boolean
    storageKey?: string
  }>(),
  {
    modelValue: true,
    width: 220,
    resizable: true,
    minWidth: 180,
    maxWidth: 560,
    maxWidthFraction: undefined,
    snapPoints: undefined,
    overlay: false,
    storageKey: undefined,
  }
)
const emit = defineEmits<{
  (e: 'update:modelValue', open: boolean): void
  (e: 'update:width', width: number): void
}>()

// ---- Resize + persistence, shared with QSidebar. The default snap point is
// the width the panel was CREATED with, captured once — the same trap
// `originWidth` documents: every module binds `:width` two-way, so
// `props.width` *is* the current width mid-drag, and a snap point read from it
// chases the cursor instead of catching it. ----
const originSnap = [props.width]

const { currentWidth, resizing, startResize, resetWidth } = usePanelResize({
  prefix: 'qss-rpanel',
  direction: -1,
  defaultWidth: () => props.width,
  minWidth: () => props.minWidth,
  maxWidth: () =>
    props.maxWidthFraction
      ? Math.floor(window.innerWidth * props.maxWidthFraction)
      : props.maxWidth,
  snapPoints: () => props.snapPoints ?? originSnap,
  storageKey: () => props.storageKey,
  resizable: () => props.resizable,
  open: () => props.modelValue,
  setWidth: (w) => emit('update:width', w),
  setOpen: (open) => emit('update:modelValue', open),
})
</script>

<template>
  <!-- `:css` is bound, not just `:name`: an in-flow panel has no slide
       animation, but `.qrp` carries a `transition` for its resize, and Vue
       then waits for a `transitionend` that never fires — the panel sticks
       in `v-leave-active` and never disappears. Turning CSS transitions off
       for the in-flow case makes closing instant, which is what it should
       have been. -->
  <Transition :name="overlay ? 'qrp-slide' : undefined" :css="overlay">
    <aside
      v-if="modelValue"
      class="qrp"
      :class="{ 'qrp--overlay': overlay, 'qrp--resizing': resizing }"
      :style="{ width: currentWidth + 'px' }"
    >
      <div
        v-if="resizable"
        class="qrp-resize"
        @mousedown="startResize"
        @dblclick="resetWidth"
      />
      <div v-if="$slots.header" class="qrp-header">
        <slot name="header" />
      </div>
      <div class="qrp-body">
        <slot />
      </div>
    </aside>
  </Transition>
</template>

<style scoped>
.qrp {
  position: relative;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  border-left: 1px solid var(--qss-border-subtle);
  background: var(--qss-module-chrome, var(--qss-bg-raised));
}

.qrp--overlay {
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  z-index: 20;
  height: auto;
  overflow: hidden;
}

.qrp-header {
  flex-shrink: 0;
}

.qrp-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  /* Flex column, so slot content can OPT INTO filling the panel height
     (flex: 1 + min-height: 0 — the orderbook's fixed-center layout needs a
     bounded box). Plain block children stack and scroll exactly as before. */
  display: flex;
  flex-direction: column;
}

.qrp-resize {
  position: absolute;
  top: 0;
  left: -2px;
  bottom: 0;
  width: 5px;
  cursor: col-resize;
  z-index: 5;
}
.qrp-resize:hover {
  background: var(--qss-text-muted);
  opacity: 0.4;
}

.qrp-slide-enter-active {
  transition: transform var(--qss-dur-base) var(--qss-ease-out), opacity var(--qss-dur-base) var(--qss-ease-out);
}
.qrp-slide-leave-active {
  transition: transform var(--qss-dur-fast) var(--qss-ease-in), opacity var(--qss-dur-fast) var(--qss-ease-in);
}
.qrp-slide-enter-from,
.qrp-slide-leave-to {
  transform: translateX(100%);
  opacity: 0;
}
.qrp--resizing {
  transition: none !important;
}

@media (prefers-reduced-motion: reduce) {
  .qrp-slide-enter-active,
  .qrp-slide-leave-active {
    transition-duration: 1ms;
  }
}
</style>
