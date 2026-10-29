<script setup lang="ts">
/**
 * A request waiting on the user, in the timeline: a tool approval, a
 * question, or a plan. Card 6 moves approvals into the bar above the
 * composer; until then they are answered here.
 */
import { computed, reactive, ref } from 'vue'
import { useThreadStore } from '#pilot/stores/thread'
import type { ThreadItem } from '#pilot/types'

const props = defineProps<{ item: ThreadItem }>()
const threads = useThreadStore()
const busy = ref(false)
const feedback = ref('')

const data = computed(() => props.item.data)
const questions = computed<{ question: string; header?: string; options?: { label: string; description?: string }[]; multiSelect?: boolean }[]>(
  () => data.value.questions ?? []
)
const picked = reactive<Record<string, string[]>>({})

async function reply(answer: Parameters<typeof threads.answer>[2]) {
  busy.value = true
  await threads.answer(props.item.threadId, String(data.value.requestId), answer)
  busy.value = false
}

function toggle(q: string, label: string, multi: boolean) {
  const cur = picked[q] ?? []
  picked[q] = multi ? (cur.includes(label) ? cur.filter((l) => l !== label) : [...cur, label]) : [label]
}

const complete = computed(() => questions.value.every((q) => (picked[q.question] ?? []).length > 0))

function submitAnswers() {
  const answers: Record<string, string | string[]> = {}
  for (const q of questions.value) {
    const p = picked[q.question] ?? []
    answers[q.question] = q.multiSelect ? p : p[0] ?? ''
  }
  void reply({ decision: 'answers', answers })
}
</script>

<template>
  <div class="qp-req" :class="`is-${item.kind}`">
    <template v-if="item.kind === 'approval'">
      <div class="qp-req-head">Allow {{ data.toolName }}?</div>
      <div class="qp-req-title" :title="data.title">{{ data.title }}</div>
      <pre v-if="data.input && data.input !== data.title" class="qp-req-pre">{{ data.input }}</pre>
      <div class="qp-req-actions">
        <button class="qp-btn qp-btn-primary qp-btn-sm" :disabled="busy" @click="reply({ decision: 'allow' })">Allow</button>
        <button class="qp-btn qp-btn-sm" :disabled="busy" @click="reply({ decision: 'allowSession' })">Allow for this session</button>
        <input v-model="feedback" class="qp-req-input" placeholder="Why not? (optional)" @keydown.enter="reply({ decision: 'deny', message: feedback })" />
        <button class="qp-btn qp-btn-sm qp-btn-danger" :disabled="busy" @click="reply({ decision: 'deny', message: feedback })">Deny</button>
      </div>
    </template>

    <template v-else-if="item.kind === 'question'">
      <div v-for="q in questions" :key="q.question" class="qp-req-q">
        <div class="qp-req-head">{{ q.header || 'Question' }}</div>
        <div class="qp-req-title">{{ q.question }}</div>
        <div class="qp-req-options">
          <button
            v-for="o in q.options ?? []"
            :key="o.label"
            class="qp-req-option"
            :class="{ on: (picked[q.question] ?? []).includes(o.label) }"
            :title="o.description"
            @click="toggle(q.question, o.label, Boolean(q.multiSelect))"
          >
            {{ o.label }}
            <span v-if="o.description" class="qp-req-option-desc">{{ o.description }}</span>
          </button>
        </div>
      </div>
      <div class="qp-req-actions">
        <button class="qp-btn qp-btn-primary qp-btn-sm" :disabled="busy || !complete" @click="submitAnswers">Answer</button>
        <button class="qp-btn qp-btn-sm" :disabled="busy" @click="reply({ decision: 'dismiss' })">Dismiss</button>
      </div>
    </template>

    <template v-else-if="item.kind === 'plan'">
      <div class="qp-req-head">Plan ready</div>
      <PilotThreadMarkdown :source="String(data.plan ?? '')" />
      <div class="qp-req-actions">
        <button class="qp-btn qp-btn-primary qp-btn-sm" :disabled="busy" @click="reply({ decision: 'allow' })">Implement</button>
        <input v-model="feedback" class="qp-req-input" placeholder="What should change?" @keydown.enter="reply({ decision: 'deny', message: feedback || 'Keep planning.' })" />
        <button class="qp-btn qp-btn-sm" :disabled="busy" @click="reply({ decision: 'deny', message: feedback || 'Keep planning.' })">Keep planning</button>
      </div>
    </template>
  </div>
</template>

<style scoped>
.qp-req {
  margin: 4px 0 10px;
  padding: 10px 12px;
  border: 1px solid color-mix(in srgb, var(--qss-warning, #d29a3f) 45%, var(--qss-border, #47474f));
  border-radius: 10px;
  background: color-mix(in srgb, var(--qss-warning, #d29a3f) 5%, var(--qss-bg-raised, #1f1f25));
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.qp-req.is-plan { border-color: var(--qss-border, #47474f); background: var(--qss-bg-raised, #1f1f25); }
.qp-req-head { font-size: 12px; font-weight: 600; color: var(--qss-warning, #d29a3f); }
.is-plan .qp-req-head { color: var(--qss-text, #d4d4d8); }
.qp-req-title { font-size: 13px; color: var(--qss-text, #d4d4d8); overflow-wrap: anywhere; }
.qp-req-pre {
  margin: 0;
  max-height: 200px;
  overflow: auto;
  padding: 6px 8px;
  border-radius: 6px;
  background: var(--qss-bg, #18181e);
  font-family: var(--qss-font-mono, ui-monospace, monospace);
  font-size: 11.5px;
  white-space: pre-wrap;
  word-break: break-word;
}
.qp-req-actions { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
.qp-req-input {
  flex: 1;
  min-width: 140px;
  height: 24px;
  padding: 0 8px;
  border-radius: 6px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg, #18181e);
  color: var(--qss-text, #d4d4d8);
  font-size: 12px;
  outline: none;
}
.qp-req-input:focus { border-color: var(--qss-accent, #7aa2f7); }
.qp-req-q { display: flex; flex-direction: column; gap: 4px; }
.qp-req-options { display: flex; flex-wrap: wrap; gap: 6px; }
.qp-req-option {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 1px;
  padding: 5px 10px;
  border-radius: 8px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg, #18181e);
  color: var(--qss-text, #d4d4d8);
  font-size: 12.5px;
  cursor: pointer;
  text-align: left;
}
.qp-req-option:hover { background: var(--qss-bg-hover, #313139); }
.qp-req-option.on { border-color: var(--qss-accent, #7aa2f7); background: color-mix(in srgb, var(--qss-accent, #7aa2f7) 14%, var(--qss-bg, #18181e)); }
.qp-req-option-desc { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
</style>
