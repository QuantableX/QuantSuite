<script setup lang="ts">
/**
 * Bob as a companion (round 2, card 05; Settings → Appearance → Bob The
 * Blob → Companion): a small Bob over QuantPilot that shows the open
 * thread's mood. Drag him anywhere (pointer events): he leans into the drag,
 * is wide-eyed while held and > < when swung fast, lands with a squash and
 * stays where he is put (kept as fractions of the window, so a resize keeps
 * him on screen).
 */
import { computed, ref } from 'vue'
import { useMascotStore } from '#pilot/stores/mascot'
import { usePilotStore } from '#pilot/stores/pilot'
import { useThreadStore } from '#pilot/stores/thread'
import { useSidebarStore } from '#pilot/stores/sidebar'
import type { LiveState } from '#pilot/types'

const mascot = useMascotStore()
const store = usePilotStore()
const threads = useThreadStore()
const sb = useSidebarStore()

const view = computed(() => (store.activeId ? threads.view(store.activeId) : null))
const state = computed<LiveState>(() => {
  switch (view.value?.state) {
    case 'running':
    case 'starting':
      return 'working'
    case 'waiting':
      return 'waiting'
    case 'error':
    case 'limited':
      return 'error'
    default:
      return store.activeId ? 'idle' : 'offline'
  }
})
const busy = computed(() => store.sessions.filter((s) => sb.statusOf(s.id) === 'working').length)

const SIZE = 60
const root = ref<HTMLElement | null>(null)
const drag = ref<{ id: number; dx: number; dy: number; x: number; y: number; lean: number; lastX: number } | null>(null)
const pos = computed(() => mascot.prefs.companionAt)

function host(): DOMRect | null {
  return root.value?.parentElement?.getBoundingClientRect() ?? null
}
function onDown(e: PointerEvent) {
  if (e.button !== 0) return
  const r = root.value!.getBoundingClientRect()
  drag.value = { id: e.pointerId, dx: e.clientX - r.left, dy: e.clientY - r.top, x: r.left, y: r.top, lean: 0, lastX: e.clientX }
  try {
    root.value!.setPointerCapture(e.pointerId)
  } catch {
    /* synthetic pointer */
  }
}
function onMove(e: PointerEvent) {
  const d = drag.value
  if (!d || d.id !== e.pointerId) return
  const vx = e.clientX - d.lastX
  drag.value = { ...d, x: e.clientX - d.dx, y: e.clientY - d.dy, lean: Math.max(-18, Math.min(18, d.lean * 0.6 + vx * 1.5)), lastX: e.clientX }
}
function onUp(e: PointerEvent) {
  const d = drag.value
  if (!d || d.id !== e.pointerId) return
  drag.value = null
  const h = host()
  if (!h) return
  const x = (d.x - h.left + SIZE / 2) / h.width
  const y = (d.y - h.top + SIZE / 2) / h.height
  mascot.set({ companionAt: { x, y } })
}
/** Held, or swung fast: Bob reacts (Face.vue). */
const carried = computed(() => (drag.value ? (Math.abs(drag.value.lean) > 15 ? 'fast' : 'held') : 'no'))
const style = computed(() => {
  const d = drag.value
  if (d) {
    const h = host()
    return { left: `${d.x - (h?.left ?? 0)}px`, top: `${d.y - (h?.top ?? 0)}px`, transform: `rotate(${d.lean}deg)` }
  }
  return { left: `calc(${pos.value.x * 100}% - ${SIZE / 2}px)`, top: `calc(${pos.value.y * 100}% - ${SIZE / 2}px)` }
})
</script>

<template>
  <div
    ref="root"
    class="qp-companion"
    :class="{ dragging: !!drag }"
    :data-own-motion="mascot.prefs.motion === 'still' ? undefined : ''"
    :style="style"
    :title="`${mascot.prefs.name || 'Bob'} — drag me anywhere`"
    @pointerdown="onDown"
    @pointermove="onMove"
    @pointerup="onUp"
    @pointercancel="onUp"
  >
    <PilotFace :state="state" :cheer="view?.cheer ?? 0" :busy="busy" :agent="store.active?.provider ?? null" :nod="store.activeId ? mascot.nods[store.activeId] ?? 0 : 0" :carried="carried" />
  </div>
</template>

<style scoped>
.qp-companion {
  position: absolute;
  z-index: 60;
  width: 60px;
  height: 60px;
  cursor: grab;
  touch-action: none;
  transition: transform 0.35s cubic-bezier(0.34, 1.45, 0.64, 1);
}
.qp-companion.dragging { cursor: grabbing; transition: none; }
</style>
