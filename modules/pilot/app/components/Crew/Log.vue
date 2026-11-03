<script setup lang="ts">
/**
 * A crew's message log (round 2, card 07) — the message bus: every
 * dispatch, question, answer, progress note, result, failure, gate and
 * decision, with time, sender and receiver, whole. Filter by task, copy
 * it all. Refreshes when the crew changes (its `pilot-crew` event).
 */
import { computed, onMounted, ref, watch } from 'vue'
import { MessagesSquare } from 'lucide-vue-next'
import { useCrewStore } from '#pilot/stores/crew'
import { kindLabel, logText, logTime, party } from '#pilot/utils/crew'
import type { CrewMessage } from '#pilot/types'

const props = defineProps<{ crewId: string }>()
const crews = useCrewStore()
const snap = computed(() => crews.crews[props.crewId] ?? null)
const messages = ref<CrewMessage[]>([])
const task = ref<string>('')
const copied = ref(false)

async function load() {
  messages.value = await crews.log(props.crewId)
}
watch(() => snap.value?.crew.rev, () => void load())
const shown = computed(() => (task.value ? messages.value.filter((m) => m.taskId === task.value) : messages.value))

function close() {
  crews.logOpen = null
}
async function copy() {
  if (!snap.value) return
  try {
    await navigator.clipboard.writeText(logText(shown.value, snap.value))
    copied.value = true
    setTimeout(() => (copied.value = false), 1500)
  } catch {
    /* clipboard denied */
  }
}
const root = ref<HTMLElement | null>(null)
onMounted(() => {
  void load()
  root.value?.focus()
})
function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    close()
  }
}
</script>

<template>
  <div class="qp-cl-veil" @pointerdown.self="close">
    <div ref="root" class="qp-cl" role="dialog" aria-modal="true" aria-labelledby="qp-cl-title" tabindex="-1" @keydown="onKey">
      <div class="qp-cl-head">
        <h3 id="qp-cl-title" class="qp-cl-title"><MessagesSquare :size="14" /> {{ snap?.crew.objective || 'Crew' }} — message log</h3>
        <select v-model="task" class="qp-cl-filter" aria-label="Show the messages of">
          <option value="">All tasks</option>
          <option v-for="t in snap?.tasks ?? []" :key="t.id" :value="t.id">{{ t.key }}</option>
        </select>
        <button class="qp-btn qp-btn-sm" @click="copy">{{ copied ? 'Copied' : 'Copy' }}</button>
        <button class="qp-btn qp-btn-sm" @click="close">Close</button>
      </div>
      <div class="qp-cl-list">
        <div v-for="m in shown" :key="m.id" class="qp-cl-msg" :class="`k-${m.kind}`">
          <div class="qp-cl-meta">
            <span class="qp-cl-time mono">{{ logTime(m.at) }}</span>
            <span class="qp-cl-who">{{ snap ? party(m.fromId, snap) : m.fromId }} → {{ snap ? party(m.toId, snap) : m.toId }}</span>
            <span class="qp-cl-kind">{{ kindLabel(m.kind) }}</span>
            <span v-if="m.status && m.kind !== 'progress'" class="qp-cl-status">{{ m.status }}</span>
          </div>
          <div class="qp-cl-text">{{ m.text }}</div>
        </div>
        <div v-if="!shown.length" class="qp-cl-empty">No messages yet.</div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.qp-cl-veil { position: fixed; inset: 0; z-index: 1100; display: flex; align-items: center; justify-content: center; background: rgba(0, 0, 0, 0.35); }
.qp-cl {
  width: min(960px, calc(100% - 48px));
  height: calc(100% - 64px);
  display: flex;
  flex-direction: column;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 12px;
  background: var(--qss-bg-raised, #1f1f25);
  box-shadow: 0 16px 40px rgba(0, 0, 0, 0.5);
  color: var(--qss-text, #d4d4d8);
  outline: none;
}
.qp-cl-head { display: flex; align-items: center; gap: 8px; padding: 12px 16px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); }
.qp-cl-title { flex: 1; min-width: 0; display: flex; align-items: center; gap: 8px; margin: 0; font-size: 14px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-cl-filter { height: 24px; padding: 0 6px; border: 1px solid var(--qss-border-subtle, #35353d); border-radius: 6px; background: var(--qss-bg, #17171c); color: var(--qss-text, #d4d4d8); font: inherit; font-size: 12px; }
.qp-cl-list { flex: 1; min-height: 0; overflow-y: auto; padding: 8px 16px 16px; display: flex; flex-direction: column; gap: 2px; }
.qp-cl-msg { padding: 6px 8px; border-radius: 6px; }
.qp-cl-msg:hover { background: var(--qss-bg-hover, #313139); }
.qp-cl-meta { display: flex; align-items: baseline; gap: 10px; font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-cl-time { flex: none; }
.qp-cl-who { color: var(--qss-text-secondary, #9a9aa5); }
.qp-cl-kind { font-weight: 600; }
.k-ask .qp-cl-kind,
.k-gate .qp-cl-kind,
.k-escalation .qp-cl-kind { color: var(--qss-warning, #d29a3f); }
.k-done .qp-cl-kind,
.k-decision .qp-cl-kind { color: var(--qss-success, #34d399); }
.k-failed .qp-cl-kind { color: var(--qss-error, #f87171); }
.qp-cl-text { margin-top: 2px; font-size: 12.5px; line-height: 1.5; white-space: pre-wrap; overflow-wrap: anywhere; }
.k-progress .qp-cl-text { color: var(--qss-text-secondary, #9a9aa5); }
.qp-cl-empty { padding: 16px 0; font-size: 12px; color: var(--qss-text-muted, #6e6e7a); }
</style>
