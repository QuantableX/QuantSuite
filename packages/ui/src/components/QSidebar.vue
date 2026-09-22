<script setup lang="ts">
/**
 * The module left sidebar (V3) — 220px, the suite's one sidebar shell.
 *
 * Two modes, covering both behaviors the modules already had:
 *  - in-flow (default): a flex column child that collapses to width 0
 *    (notes/algo behavior).
 *  - overlay: absolutely positioned over the content, resizable with snap
 *    points and a drag handle (QuantCode behavior). The module panel's
 *    `translateZ(0)` makes `position: absolute` anchor to it.
 *
 * Slots: `header`, default (scrolling body), `footer`.
 * `storageKey` persists width + open state in localStorage.
 */
import { usePanelResize } from '../composables/usePanelResize'

const props = withDefaults(
  defineProps<{
    modelValue?: boolean
    width?: number
    resizable?: boolean
    minWidth?: number
    maxWidthFraction?: number
    snapPoints?: number[]
    overlay?: boolean
    storageKey?: string
  }>(),
  {
    modelValue: true,
    width: 220,
    resizable: false,
    minWidth: 120,
    maxWidthFraction: 0.25,
    snapPoints: () => [220, 271],
    overlay: false,
    storageKey: undefined,
  }
)
const emit = defineEmits<{
  (e: 'update:modelValue', open: boolean): void
  (e: 'update:width', width: number): void
}>()

// ---- Resize (overlay mode) + persistence, shared with QRightPanel ----
const { currentWidth, resizing, startResize, resetWidth } = usePanelResize({
  prefix: 'qss-sidebar',
  direction: 1,
  defaultWidth: () => props.width,
  minWidth: () => props.minWidth,
  maxWidth: () => Math.floor(window.innerWidth * props.maxWidthFraction),
  snapPoints: () => props.snapPoints,
  storageKey: () => props.storageKey,
  resizable: () => props.resizable,
  open: () => props.modelValue,
  setWidth: (w) => emit('update:width', w),
  setOpen: (open) => emit('update:modelValue', open),
})
</script>

<template>
  <!-- `:css` is bound, not just `:name`: an in-flow panel has no slide
       animation, but `.qsb` carries a `transition` for its resize, and Vue
       then waits for a `transitionend` that never fires — the panel sticks
       in `v-leave-active` and never disappears. Turning CSS transitions off
       for the in-flow case makes closing instant, which is what it should
       have been. -->
  <Transition :name="overlay ? 'qsb-slide' : undefined" :css="overlay">
    <aside
      v-if="modelValue"
      class="qsb"
      :class="{ 'qsb--overlay': overlay, 'qsb--resizing': resizing }"
      :style="{ width: currentWidth + 'px' }"
    >
      <div v-if="$slots.header" class="qsb-header">
        <slot name="header" />
      </div>
      <div class="qsb-body">
        <slot />
      </div>
      <div v-if="$slots.footer" class="qsb-footer">
        <slot name="footer" />
      </div>
      <div
        v-if="resizable"
        class="qsb-resize"
        @mousedown="startResize"
        @dblclick="resetWidth"
      />
    </aside>
  </Transition>
</template>

<style scoped>
.qsb {
  position: relative;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  border-right: 1px solid var(--qss-border-subtle);
  background: var(--qss-module-chrome, var(--qss-bg-raised));
}

.qsb--overlay {
  position: absolute;
  top: 0;
  left: 0;
  bottom: 0;
  z-index: 20;
  height: auto;
  overflow: hidden;
}

.qsb-header {
  flex-shrink: 0;
}

.qsb-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}

.qsb-footer {
  flex-shrink: 0;
  margin-top: auto;
}

.qsb-resize {
  position: absolute;
  top: 0;
  right: -2px;
  bottom: 0;
  width: 5px;
  cursor: col-resize;
  z-index: 5;
}
.qsb-resize:hover {
  background: var(--qss-text-muted);
  opacity: 0.4;
}

.qsb-slide-enter-active {
  transition: transform var(--qss-dur-base) var(--qss-ease-out), opacity var(--qss-dur-base) var(--qss-ease-out);
}
.qsb-slide-leave-active {
  transition: transform var(--qss-dur-fast) var(--qss-ease-in), opacity var(--qss-dur-fast) var(--qss-ease-in);
}
.qsb-slide-enter-from,
.qsb-slide-leave-to {
  transform: translateX(-100%);
  opacity: 0;
}
.qsb--resizing {
  transition: none !important;
}

@media (prefers-reduced-motion: reduce) {
  .qsb-slide-enter-active,
  .qsb-slide-leave-active {
    transition-duration: 1ms;
  }
}
</style>
