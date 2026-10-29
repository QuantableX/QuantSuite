<script setup lang="ts">
/**
 * The main column for one thread: the timeline and the composer, the hero
 * while the thread is empty, and the banners that belong above the box —
 * a failed send, a V2 session continuing as a chat, a passing notice.
 */
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useThreadStore } from '#pilot/stores/thread'
import { usePilotStore } from '#pilot/stores/pilot'

const props = defineProps<{ threadId: string }>()
const threads = useThreadStore()
const store = usePilotStore()

const view = computed(() => threads.view(props.threadId))
const thread = computed(() => store.sessions.find((s) => s.id === props.threadId) ?? null)
const empty = computed(() => Boolean(view.value?.loaded) && !view.value?.items.length)
const isTerminalRow = computed(() => thread.value?.kind === 'terminal')

watch(
  () => props.threadId,
  (id) => void threads.open(id),
  { immediate: true }
)

const notice = ref('')
let noticeTimer: ReturnType<typeof setTimeout> | null = null
function showNotice(text: string) {
  notice.value = text
  if (noticeTimer) clearTimeout(noticeTimer)
  noticeTimer = setTimeout(() => (notice.value = ''), 2400)
}
onBeforeUnmount(() => {
  if (noticeTimer) clearTimeout(noticeTimer)
})

const faceState = computed(() => {
  switch (view.value?.state) {
    case 'running':
    case 'starting':
      return 'working'
    case 'waiting':
      return 'waiting'
    case 'error':
      return 'error'
    default:
      return 'idle'
  }
})
</script>

<template>
  <div class="qp-conv">
    <div v-if="empty" class="qp-hero">
      <div class="qp-hero-face"><PilotFace :state="faceState" :cheer="view?.cheer ?? 0" /></div>
      <h2 class="qp-hero-title">What should we build in {{ thread?.contextName || 'General' }}?</h2>
      <p v-if="isTerminalRow" class="qp-hero-text">
        This session started in the terminal. Its earlier messages stay in {{ store.adapterLabel(thread?.provider || 'claude') }}'s own history — write here and the agent continues with them, or open the terminal again.
        <button class="qp-link" @click="store.openTerminal(threadId)">Open terminal</button>
      </p>
      <p v-else class="qp-hero-text">{{ thread?.cwd }}</p>
    </div>
    <PilotThreadTimeline v-else :thread-id="threadId" @notice="showNotice" />

    <div class="qp-conv-foot">
      <div v-if="view?.error" class="qp-conv-banner is-error">
        <span class="qp-conv-banner-text">{{ view.error }}</span>
        <button class="qp-link" @click="view.error = null">Dismiss</button>
      </div>
      <div v-if="notice" class="qp-conv-banner is-notice">{{ notice }}</div>
      <PilotThreadComposer :thread-id="threadId" />
    </div>
  </div>
</template>

<style scoped>
.qp-conv { flex: 1; min-height: 0; display: flex; flex-direction: column; }
.qp-hero {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 24px;
  text-align: center;
}
.qp-hero-face { width: 120px; margin-bottom: 4px; }
.qp-hero-title { margin: 0; font-size: 18px; font-weight: 600; color: var(--qss-text, #d4d4d8); }
.qp-hero-text { margin: 0; max-width: 520px; font-size: 12.5px; line-height: 1.55; color: var(--qss-text-muted, #6e6e7a); overflow-wrap: anywhere; }
.qp-conv-foot { flex-shrink: 0; }
.qp-conv-banner {
  max-width: 780px;
  margin: 0 auto 6px;
  padding: 6px 10px;
  border-radius: 8px;
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 12px;
  width: calc(100% - 48px);
}
.qp-conv-banner-text { flex: 1; min-width: 0; white-space: pre-wrap; overflow-wrap: anywhere; }
.qp-conv-banner.is-error {
  color: var(--qss-error, #f87171);
  border: 1px solid color-mix(in srgb, var(--qss-error, #f87171) 40%, var(--qss-border, #47474f));
  background: color-mix(in srgb, var(--qss-error, #f87171) 6%, transparent);
}
.qp-conv-banner.is-notice { color: var(--qss-text-secondary, #9a9aa5); justify-content: center; }
</style>
