<script setup lang="ts">
/**
 * The inspector split (card 11): a pointer-dragged edge on the left, an
 * icon tab bar, the tab's content. Ctrl+Alt+B toggles it, Ctrl+D the Diff
 * tab. Wide content lives here, never in the 220 px side panels.
 */
import { onBeforeUnmount, ref } from 'vue'
import { GitCompare, X } from 'lucide-vue-next'
import { useInspectorStore } from '#pilot/stores/inspector'

defineProps<{ threadId: string }>()
const inspector = useInspectorStore()
const root = ref<HTMLElement | null>(null)
let drag: { startX: number; startW: number; max: number } | null = null

function onDown(e: PointerEvent) {
  if (e.button !== 0) return
  const host = root.value?.parentElement
  drag = { startX: e.clientX, startW: inspector.width, max: (host?.clientWidth ?? 1200) - 360 }
  ;(e.target as HTMLElement).setPointerCapture(e.pointerId)
  e.preventDefault()
}
function onMove(e: PointerEvent) {
  if (!drag) return
  const w = drag.startW + (drag.startX - e.clientX)
  inspector.setWidth(Math.max(320, Math.min(drag.max, w)))
}
function onUp() {
  drag = null
}
onBeforeUnmount(() => (drag = null))
</script>

<template>
  <aside ref="root" class="qp-insp" :style="{ width: `${inspector.width}px` }">
    <div class="qp-insp-edge" title="Drag to resize" @pointerdown="onDown" @pointermove="onMove" @pointerup="onUp" @pointercancel="onUp" />
    <div class="qp-insp-tabs" role="tablist">
      <button class="qp-insp-tab" role="tab" :aria-selected="inspector.tab === 'diff'" :class="{ on: inspector.tab === 'diff' }" title="Diff (Ctrl+D)" @click="inspector.tab = 'diff'">
        <GitCompare :size="14" /> Diff
      </button>
      <button class="qp-insp-close" title="Close the inspector (Ctrl+Alt+B)" @click="inspector.toggle(false)"><X :size="14" /></button>
    </div>
    <div class="qp-insp-body">
      <PilotInspectorDiff v-if="inspector.tab === 'diff'" :thread-id="threadId" />
    </div>
  </aside>
</template>

<style scoped>
.qp-insp {
  position: relative;
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
  min-width: 320px;
  min-height: 0;
  border-left: 1px solid var(--qss-border-subtle, #35353d);
  background: var(--qss-bg, #18181e);
}
.qp-insp-edge { position: absolute; left: -3px; top: 0; bottom: 0; width: 6px; cursor: col-resize; z-index: 5; }
.qp-insp-edge:hover { background: color-mix(in srgb, var(--qss-accent, #7aa2f7) 30%, transparent); }
.qp-insp-tabs { display: flex; align-items: center; gap: 2px; padding: 6px 8px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); }
.qp-insp-tab {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 3px 9px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text-secondary, #9a9aa5);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}
.qp-insp-tab.on { background: var(--qss-bg-card, #292930); color: var(--qss-text, #d4d4d8); }
.qp-insp-close { margin-left: auto; display: inline-flex; padding: 3px; border: none; border-radius: 5px; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; }
.qp-insp-close:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-insp-body { flex: 1; min-height: 0; display: flex; flex-direction: column; }
</style>
