<script setup lang="ts">
/**
 * A plan from plan mode (cards 6, 16). Claude's waits for review
 * (ExitPlanMode): Implement accepts it and switches the thread to the mode
 * the work should run in — Accept edits or Full access, live on the running
 * process — and Keep planning declines it with what should change. Codex's
 * is a proposal on the thread's newest turn: Implement switches the mode and
 * sends the plan back to be carried out (T3 Code's "PLEASE IMPLEMENT THIS
 * PLAN"), Keep planning sends what should change. Every plan copies.
 */
import { computed, ref } from 'vue'
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

const plan = computed(() => String(props.item.data.plan ?? ''))
const status = computed(() => String(props.item.data.status ?? ''))
const proposed = computed(() => status.value === 'proposed')
/** A proposal is answered by the next message: only the newest turn's, while nothing runs. */
const actionable = computed(() => {
  if (status.value === 'pending') return true
  if (!proposed.value) return false
  const view = threads.view(props.item.threadId)
  return Boolean(view && !view.turnId && view.items.at(-1)?.turnId === props.item.turnId)
})
const label = computed(() => {
  switch (status.value) {
    case 'approved':
      return 'Plan · implementing'
    case 'rejected':
      return 'Plan · kept planning'
    case 'expired':
      return 'Plan · expired'
    default:
      return 'Plan'
  }
})

async function implement(mode: Mode) {
  busy.value = true
  try {
    if (proposed.value) {
      await store.update(props.item.threadId, { mode })
      await threads.send(props.item.threadId, `PLEASE IMPLEMENT THIS PLAN:\n${plan.value.trim()}`)
    } else {
      await threads.answer(props.item.threadId, String(props.item.data.requestId), { decision: 'allow' })
      await store.update(props.item.threadId, { mode })
    }
  } finally {
    busy.value = false
  }
}

async function keepPlanning() {
  const text = feedback.value.trim()
  if (proposed.value && !text) return
  busy.value = true
  try {
    if (proposed.value) {
      if (await threads.send(props.item.threadId, text)) feedback.value = ''
    } else {
      await threads.answer(props.item.threadId, String(props.item.data.requestId), { decision: 'deny', message: text || 'Keep planning — the plan is not ready yet.' })
    }
  } finally {
    busy.value = false
  }
}

async function copy() {
  try {
    await navigator.clipboard.writeText(plan.value)
    emit('notice', 'Plan copied')
  } catch {
    /* clipboard denied — nothing to do */
  }
}
</script>

<template>
  <div class="qp-plancard" :class="{ waiting: actionable }" :data-item-id="item.id">
    <div class="qp-plancard-head">
      <ClipboardList :size="14" />
      <span>{{ actionable ? (proposed ? 'Plan proposed' : 'Plan ready for review') : label }}</span>
      <button class="qp-plancard-copy" title="Copy the plan as Markdown" @click="copy">Copy</button>
    </div>
    <PilotThreadMarkdown :source="plan" @notice="(t: string) => emit('notice', t)" />
    <template v-if="actionable">
      <div class="qp-plancard-actions">
        <button class="qp-btn qp-btn-primary qp-btn-sm" :disabled="busy" title="Accept the plan; file edits run without asking" @click="implement('edits')">Implement — accept edits</button>
        <button class="qp-btn qp-btn-sm" :disabled="busy" title="Accept the plan; commands and edits run without asking" @click="implement('full')">Implement — full access</button>
      </div>
      <div class="qp-plancard-keep">
        <input v-model="feedback" class="qp-plancard-input" :placeholder="proposed ? 'What should change?' : 'What should change? (optional)'" @keydown.enter.prevent="keepPlanning" />
        <button class="qp-btn qp-btn-sm" :disabled="busy || (proposed && !feedback.trim())" @click="keepPlanning">Keep planning</button>
      </div>
    </template>
  </div>
</template>

<style scoped>
.qp-plancard {
  margin: 6px 0 10px;
  padding: 12px 14px;
  border-radius: 10px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg-raised, #1f1f25);
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.qp-plancard.waiting { border-color: color-mix(in srgb, var(--qss-warning, #d29a3f) 50%, var(--qss-border, #47474f)); }
.qp-plancard-head { display: flex; align-items: center; gap: 6px; font-size: 12.5px; font-weight: 600; color: var(--qss-text-secondary, #9a9aa5); }
.qp-plancard.waiting .qp-plancard-head { color: var(--qss-warning, #d29a3f); }
.qp-plancard-copy {
  margin-left: auto;
  padding: 1px 7px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--qss-text-muted, #6e6e7a);
  font: inherit;
  font-size: 11px;
  font-weight: 400;
  cursor: pointer;
}
.qp-plancard-copy:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
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
