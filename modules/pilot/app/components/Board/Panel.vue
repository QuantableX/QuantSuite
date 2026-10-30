<script setup lang="ts">
/**
 * A thread beside the board (card 21): its conversation and composer, so a
 * card's agent can be followed and answered without leaving the board. It
 * is the open thread (the store's active one); "Open in conversation"
 * switches the main area over to it. The left edge resizes it.
 */
import { computed, onBeforeUnmount, ref } from 'vue'
import { Maximize2, X } from 'lucide-vue-next'
import { usePilotStore } from '#pilot/stores/pilot'
import { useAppStore } from '#pilot/stores/app'

const props = defineProps<{ threadId: string }>()
const emit = defineEmits<{ close: [] }>()
const store = usePilotStore()
const app = useAppStore()
const thread = computed(() => store.sessions.find((s) => s.id === props.threadId) ?? null)

const WIDTH_KEY = 'qp-board-panel-w'
function readWidth() {
  try {
    return Number(localStorage.getItem(WIDTH_KEY)) || 520
  } catch {
    return 520
  }
}
const width = ref(readWidth())
const root = ref<HTMLElement | null>(null)

let stopResize: (() => void) | null = null
function startResize(e: PointerEvent) {
  if (e.button !== 0) return
  e.preventDefault()
  const startX = e.clientX
  const startW = width.value
  // The board keeps at least a column's room.
  const room = (root.value?.parentElement?.clientWidth ?? 1200) - 300
  const move = (ev: PointerEvent) => {
    width.value = Math.round(Math.min(Math.max(startW + startX - ev.clientX, 360), Math.max(room, 360)))
  }
  const up = () => {
    stopResize?.()
    try { localStorage.setItem(WIDTH_KEY, String(width.value)) } catch { /* private mode */ }
  }
  window.addEventListener('pointermove', move, true)
  window.addEventListener('pointerup', up, true)
  window.addEventListener('pointercancel', up, true)
  stopResize = () => {
    window.removeEventListener('pointermove', move, true)
    window.removeEventListener('pointerup', up, true)
    window.removeEventListener('pointercancel', up, true)
    stopResize = null
  }
}
onBeforeUnmount(() => stopResize?.())
</script>

<template>
  <aside ref="root" class="qp-bpanel" :style="{ width: `${width}px` }" aria-label="Thread beside the board">
    <div class="qp-bpanel-edge" title="Drag to resize" @pointerdown="startResize" />
    <header class="qp-bpanel-head">
      <span class="qp-bpanel-title" :title="thread?.title || undefined">{{ thread?.title || 'New thread' }}</span>
      <span v-if="thread" class="qp-bpanel-chip">{{ store.adapterLabel(thread.provider) }}</span>
      <div class="qp-bpanel-spacer" />
      <button class="qp-bpanel-icon" title="Open in the conversation view" aria-label="Open in the conversation view" @click="app.setView('conversation')">
        <Maximize2 :size="13" />
      </button>
      <button class="qp-bpanel-icon" title="Close the panel" aria-label="Close the panel" @click="emit('close')">
        <X :size="14" />
      </button>
    </header>
    <div class="qp-bpanel-body">
      <PilotThreadConversation v-if="thread" :thread-id="threadId" />
      <p v-else class="qp-bpanel-gone">This thread was deleted.</p>
    </div>
  </aside>
</template>

<style scoped>
.qp-bpanel {
  position: relative;
  flex-shrink: 0;
  min-width: 360px;
  max-width: calc(100% - 300px);
  display: flex;
  flex-direction: column;
  min-height: 0;
  border-left: 1px solid var(--qss-border-subtle, #35353d);
  background: var(--qss-bg, #18181e);
}
.qp-bpanel-edge {
  position: absolute;
  top: 0;
  bottom: 0;
  left: -3px;
  width: 6px;
  cursor: col-resize;
  z-index: 2;
  touch-action: none;
}
.qp-bpanel-edge:hover { background: color-mix(in srgb, var(--qss-accent, #7c8cff) 35%, transparent); }
.qp-bpanel-head {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 8px;
  height: 36px;
  padding: 0 8px 0 12px;
  border-bottom: 1px solid var(--qss-border-subtle, #35353d);
}
.qp-bpanel-title {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--qss-text, #d4d4d8);
  font-size: 12.5px;
  font-weight: 600;
}
.qp-bpanel-chip {
  flex-shrink: 0;
  font-size: 11px;
  color: var(--qss-text-secondary, #9a9aa5);
  border: 1px solid var(--qss-border-subtle, #35353d);
  border-radius: 999px;
  padding: 1px 8px;
}
.qp-bpanel-spacer { flex: 1; }
.qp-bpanel-icon {
  display: inline-flex;
  padding: 4px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text-muted, #6e6e7a);
  cursor: pointer;
}
.qp-bpanel-icon:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-bpanel-body { flex: 1; min-height: 0; display: flex; flex-direction: column; }
.qp-bpanel-gone { margin: 24px; font-size: 12.5px; color: var(--qss-text-muted, #6e6e7a); }
</style>
