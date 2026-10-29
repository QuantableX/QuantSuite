<script setup lang="ts">
/**
 * A plan waiting for review (Claude's ExitPlanMode; card 6). Implement
 * accepts it and switches the thread to the mode the work should run in —
 * Accept edits or Full access, live on the running process. Keep planning
 * declines it with what should change.
 */
import { ref } from 'vue'
import { ClipboardList } from 'lucide-vue-next'
import { useThreadStore } from '#pilot/stores/thread'
import { usePilotStore } from '#pilot/stores/pilot'
import type { Mode, ThreadItem } from '#pilot/types'

const props = defineProps<{ item: ThreadItem }>()
const emit = defineEmits<{ notice: [text: string] }>()
const threads = useThreadStore()
const store = usePilotStore()
const busy = ref(false)
const feedback = ref('')

async function implement(mode: Mode) {
  busy.value = true
  await threads.answer(props.item.threadId, String(props.item.data.requestId), { decision: 'allow' })
  await store.update(props.item.threadId, { mode })
  busy.value = false
}

async function keepPlanning() {
  busy.value = true
  const message = feedback.value.trim() || 'Keep planning — the plan is not ready yet.'
  await threads.answer(props.item.threadId, String(props.item.data.requestId), { decision: 'deny', message })
  busy.value = false
}
</script>

<template>
  <div class="qp-plancard" :data-item-id="item.id">
    <div class="qp-plancard-head"><ClipboardList :size="14" /> Plan ready for review</div>
    <PilotThreadMarkdown :source="String(item.data.plan ?? '')" @notice="(t: string) => emit('notice', t)" />
    <div class="qp-plancard-actions">
      <button class="qp-btn qp-btn-primary qp-btn-sm" :disabled="busy" title="Accept the plan; file edits run without asking" @click="implement('edits')">Implement — accept edits</button>
      <button class="qp-btn qp-btn-sm" :disabled="busy" title="Accept the plan; commands and edits run without asking" @click="implement('full')">Implement — full access</button>
    </div>
    <div class="qp-plancard-keep">
      <input v-model="feedback" class="qp-plancard-input" placeholder="What should change? (optional)" @keydown.enter.prevent="keepPlanning" />
      <button class="qp-btn qp-btn-sm" :disabled="busy" @click="keepPlanning">Keep planning</button>
    </div>
  </div>
</template>

<style scoped>
.qp-plancard {
  margin: 6px 0 10px;
  padding: 12px 14px;
  border-radius: 10px;
  border: 1px solid color-mix(in srgb, var(--qss-warning, #d29a3f) 50%, var(--qss-border, #47474f));
  background: var(--qss-bg-raised, #1f1f25);
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.qp-plancard-head { display: flex; align-items: center; gap: 6px; font-size: 12.5px; font-weight: 600; color: var(--qss-warning, #d29a3f); }
.qp-plancard-actions,
.qp-plancard-keep { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
.qp-plancard-input {
  flex: 1;
  min-width: 180px;
  height: 26px;
  padding: 0 9px;
  border-radius: 7px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg, #18181e);
  color: var(--qss-text, #d4d4d8);
  font-size: 12.5px;
  outline: none;
}
.qp-plancard-input:focus { border-color: var(--qss-accent, #7aa2f7); }
</style>
