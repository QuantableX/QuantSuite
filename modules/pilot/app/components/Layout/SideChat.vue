<script setup lang="ts">
/**
 * The side chat beside the main thread (round 2, card 10): its own
 * conversation and composer (model, effort, permissions, queue, approvals,
 * Stop) in a sheet over the right edge of the conversation. The main run
 * goes on untouched. Header actions work on the latest answer: Insert into
 * main (a quote chip in the main composer), Open as full thread, New side
 * chat, Close. Escape returns to the main composer and stops nothing.
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { CornerDownLeft, Maximize2, MessageSquarePlus, X } from 'lucide-vue-next'
import { usePilotStore } from '#pilot/stores/pilot'
import { useThreadStore } from '#pilot/stores/thread'
import { useSideStore } from '#pilot/stores/side'

const store = usePilotStore()
const threads = useThreadStore()
const side = useSideStore()
const sideId = computed(() => side.open?.sideId ?? '')
const main = computed(() => store.sessions.find((s) => s.id === side.open?.mainId) ?? null)
const lastAnswer = computed(() => {
  const items = threads.views[sideId.value]?.items ?? []
  return [...items].reverse().find((i) => i.kind === 'assistant' && !i.parent && String(i.data.text ?? '').trim() && !i.data.streaming) ?? null
})
const notice = ref('')
function insert() {
  const a = lastAnswer.value
  if (!a) return
  side.insertIntoMain(sideId.value, a.id, String(a.data.text))
  notice.value = 'Added to the main composer as a quote.'
  setTimeout(() => (notice.value = ''), 2200)
  focusMain()
}
function focusMain() {
  const boxes = [...document.querySelectorAll<HTMLTextAreaElement>('.qp-composer textarea')]
  boxes.find((b) => !b.closest('.qp-side-sheet'))?.focus()
}
const root = ref<HTMLElement | null>(null)
function onKey(e: KeyboardEvent) {
  if (e.key !== 'Escape' || !root.value?.contains(e.target as Node)) return
  // A menu inside the side chat closes first; then Escape goes back to the main composer.
  if (e.defaultPrevented) return
  e.preventDefault()
  focusMain()
}
onMounted(() => {
  window.addEventListener('keydown', onKey)
  setTimeout(() => root.value?.querySelector<HTMLTextAreaElement>('.qp-composer textarea')?.focus(), 300)
})
onBeforeUnmount(() => window.removeEventListener('keydown', onKey))
const host = { kind: 'pilot-side' as const }
</script>

<template>
  <aside ref="root" class="qp-side-sheet" role="complementary" aria-label="Side chat" data-module="pilot">
    <header class="qp-ss-head">
      <div class="qp-ss-title">
        <span class="qp-ss-name">Side chat</span>
        <span class="qp-ss-main" :title="main?.title">beside “{{ main?.title || 'New thread' }}”</span>
      </div>
      <button class="qp-ss-btn" :disabled="!lastAnswer" title="Insert the latest answer into the main composer as a quote" @click="insert"><CornerDownLeft :size="13" /></button>
      <button class="qp-ss-btn" title="Open as a full thread (it leaves the main thread)" @click="side.openAsFull(sideId)"><Maximize2 :size="13" /></button>
      <button class="qp-ss-btn" title="New side chat" @click="main && side.startNew(main.id)"><MessageSquarePlus :size="13" /></button>
      <button class="qp-ss-btn" title="Close (the side chat stays under its main thread)" @click="side.close()"><X :size="14" /></button>
    </header>
    <div v-if="notice || side.error" class="qp-ss-note" role="status">{{ notice || side.error }}</div>
    <PilotEmbedChat :key="sideId" :thread-id="sideId" :host="host" class="qp-ss-chat" />
  </aside>
</template>

<style scoped>
.qp-side-sheet {
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  z-index: 30;
  width: min(420px, 48%);
  display: flex;
  flex-direction: column;
  border-left: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg, #18181e);
  box-shadow: -12px 0 32px rgba(0, 0, 0, 0.35);
  animation: qp-ss-in 0.16s ease-out;
}
@keyframes qp-ss-in {
  from { transform: translateX(12px); opacity: 0; }
  to { transform: none; opacity: 1; }
}
@media (prefers-reduced-motion: reduce) {
  .qp-side-sheet { animation: none; }
}
.qp-ss-head { display: flex; align-items: center; gap: 2px; padding: 8px 8px 8px 12px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); }
.qp-ss-title { flex: 1; min-width: 0; display: flex; flex-direction: column; }
.qp-ss-name { font-size: 12.5px; font-weight: 600; color: var(--qss-text, #d4d4d8); }
.qp-ss-main { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-ss-btn { display: inline-flex; padding: 5px; border: none; border-radius: 6px; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; }
.qp-ss-btn:hover:not(:disabled) { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-ss-btn:disabled { opacity: 0.4; cursor: default; }
.qp-ss-note { padding: 5px 12px; font-size: 11.5px; color: var(--qss-text-secondary, #9a9aa5); border-bottom: 1px solid var(--qss-border-subtle, #35353d); }
.qp-ss-chat { flex: 1; min-height: 0; }
</style>
