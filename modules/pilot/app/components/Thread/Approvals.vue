<script setup lang="ts">
/**
 * What the agent waits on, right above the message box (docs/PLAN-QUANTPILOT.md,
 * card 6; PandaCode's approval bar, T3 Code's question panel).
 *
 * - Approvals: what is asked (command with its input, file edit, MCP tool,
 *   web fetch) and who asks (the agent or a subagent). Allow, Allow for this
 *   session, Deny; with several, Approve all. A focused request: Enter
 *   allows, Escape moves to Deny. Typing a reply in the box and sending it
 *   declines the first request with that text (Composer.vue).
 * - Questions: one at a time with its options (single or multi select),
 *   an "Other" answer, Back / Next / Submit, Dismiss. Digits pick options.
 * - A plan waiting for review is a card in the conversation; here it is a
 *   one-line pointer to it.
 */
import { computed, nextTick, reactive, ref, watch } from 'vue'
import { Bot, ChevronRight, FileText, Globe, ListChecks, Pencil, Plug, SquareTerminal, Wrench } from 'lucide-vue-next'
import { useThreadStore } from '#pilot/stores/thread'
import { usePilotStore } from '#pilot/stores/pilot'
import type { ThreadItem } from '#pilot/types'

const props = defineProps<{ threadId: string }>()
const emit = defineEmits<{ done: [] }>()
const threads = useThreadStore()
const store = usePilotStore()

const pending = computed(() => threads.pending(props.threadId))
const approvals = computed(() => pending.value.filter((i) => i.kind === 'approval'))
const question = computed(() => pending.value.find((i) => i.kind === 'question') ?? null)
/** A Codex proposal waits on the newest turn while nothing runs — PlanCard's rule (card 24). */
const proposal = computed(() => {
  const v = threads.view(props.threadId)
  if (!v || v.turnId) return null
  const last = v.items.at(-1)?.turnId
  return [...v.items].reverse().find((i) => i.kind === 'plan' && i.data.status === 'proposed' && i.turnId === last) ?? null
})
const plan = computed(() => pending.value.find((i) => i.kind === 'plan') ?? proposal.value)
const thread = computed(() => store.sessions.find((s) => s.id === props.threadId) ?? null)
const agent = computed(() => store.adapterLabel(thread.value?.provider || 'claude'))

const busy = ref<string | null>(null)
const expanded = reactive<Record<string, boolean>>({})
const rows = ref<HTMLElement[]>([])

const ICONS: Record<string, unknown> = { command: SquareTerminal, read: FileText, search: FileText, edit: Pencil, web: Globe, agent: Bot, todo: ListChecks, mcp: Plug }
function verb(category: string): string {
  switch (category) {
    case 'command':
      return 'run'
    case 'edit':
      return 'edit'
    case 'read':
    case 'search':
      return 'read'
    case 'web':
      return 'fetch'
    default:
      return 'use'
  }
}

async function answer(item: ThreadItem, a: Parameters<typeof threads.answer>[2]) {
  busy.value = item.id
  await threads.answer(props.threadId, String(item.data.requestId), a)
  busy.value = null
}

async function approveAll() {
  for (const a of [...approvals.value]) await answer(a, { decision: 'allow' })
  emit('done')
}

function onRowKey(e: KeyboardEvent, item: ThreadItem, i: number) {
  if (e.target !== e.currentTarget) return
  if (e.key === 'Enter') {
    e.preventDefault()
    void answer(item, { decision: 'allow' })
  } else if (e.key === 'Escape') {
    e.preventDefault()
    rows.value[i]?.querySelector<HTMLButtonElement>('.qp-ap-deny')?.focus()
  }
}

/** A new request with an empty box: take focus so Enter answers it. */
watch(
  () => approvals.value.length,
  (n, old) => {
    if (n > (old ?? 0)) {
      const active = document.activeElement as HTMLTextAreaElement | null
      const boxIsEmpty = active?.classList.contains('qp-c-input') && !active.value
      if (boxIsEmpty || active === document.body) void nextTick(() => rows.value[0]?.focus())
    }
  }
)

// ── Questions ──
interface Q {
  question: string
  header?: string
  options?: { label: string; description?: string }[]
  multiSelect?: boolean
}
const questions = computed<Q[]>(() => question.value?.data.questions ?? [])
const step = ref(0)
const picked = reactive<Record<string, string[]>>({})
const other = reactive<Record<string, string>>({})
watch(question, () => {
  step.value = 0
  for (const k of Object.keys(picked)) delete picked[k]
  for (const k of Object.keys(other)) delete other[k]
})
const current = computed(() => questions.value[step.value] ?? null)

function toggle(q: Q, label: string) {
  const cur = picked[q.question] ?? []
  picked[q.question] = q.multiSelect ? (cur.includes(label) ? cur.filter((l) => l !== label) : [...cur, label]) : [label]
  if (!q.multiSelect) other[q.question] = ''
}
function answered(q: Q): boolean {
  return (picked[q.question]?.length ?? 0) > 0 || Boolean(other[q.question]?.trim())
}
const allAnswered = computed(() => questions.value.length > 0 && questions.value.every(answered))

function submitQuestion() {
  if (!question.value || !allAnswered.value) return
  const answers: Record<string, string> = {}
  for (const q of questions.value) {
    const parts = [...(picked[q.question] ?? [])]
    if (other[q.question]?.trim()) parts.push(other[q.question].trim())
    answers[q.question] = parts.join(', ')
  }
  void answer(question.value, { decision: 'answers', answers })
  emit('done')
}

function onQuestionKey(e: KeyboardEvent) {
  const q = current.value
  if (!q || (e.target as HTMLElement).tagName === 'INPUT') return
  if (/^[1-9]$/.test(e.key) && q.options?.[Number(e.key) - 1]) {
    e.preventDefault()
    toggle(q, q.options[Number(e.key) - 1].label)
  } else if (e.key === 'Enter') {
    e.preventDefault()
    if (step.value < questions.value.length - 1) step.value++
    else submitQuestion()
  }
}

function showPlan() {
  if (!plan.value) return
  document.querySelector(`[data-item-id="${plan.value.id}"]`)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
}
</script>

<template>
  <div v-if="pending.length || proposal" class="qp-ap" role="region" aria-label="Waiting for you">
    <div
      v-for="(a, i) in approvals"
      :key="a.id"
      ref="rows"
      class="qp-ap-row"
      tabindex="0"
      :aria-label="`${agent} wants to ${verb(a.data.category)} ${a.data.title}. Enter allows, Escape moves to Deny.`"
      @keydown="onRowKey($event, a, i)"
    >
      <div class="qp-ap-line">
        <component :is="ICONS[a.data.category] ?? Wrench" :size="14" class="qp-ap-icon" />
        <span class="qp-ap-who">{{ a.parent ? 'A subagent' : agent }} wants to {{ verb(a.data.category) }}</span>
        <code class="qp-ap-title" :title="a.data.title">{{ a.data.title || a.data.toolName }}</code>
        <button v-if="a.data.input && a.data.input !== a.data.title" class="qp-ap-more" :aria-expanded="!!expanded[a.id]" @click="expanded[a.id] = !expanded[a.id]">
          <ChevronRight :size="12" :class="{ open: expanded[a.id] }" /> Details
        </button>
      </div>
      <pre v-if="expanded[a.id]" class="qp-ap-pre">{{ a.data.input }}</pre>
      <div class="qp-ap-actions">
        <button class="qp-btn qp-btn-primary qp-btn-sm" :disabled="busy === a.id" @click="answer(a, { decision: 'allow' })">Allow</button>
        <button
          v-if="a.data.allowSession !== false"
          class="qp-btn qp-btn-sm"
          :disabled="busy === a.id"
          @click="answer(a, { decision: 'allowSession' })"
        >
          Allow for this session
        </button>
        <button class="qp-btn qp-btn-sm qp-ap-deny" :disabled="busy === a.id" @click="answer(a, { decision: 'deny' })">Deny</button>
        <span v-if="i === 0" class="qp-ap-hint">or write why in the box below to decline with feedback</span>
      </div>
    </div>
    <div v-if="approvals.length > 1" class="qp-ap-all">
      <button class="qp-btn qp-btn-sm" @click="approveAll">Approve all {{ approvals.length }}</button>
    </div>

    <div v-if="question && current" class="qp-ap-q" tabindex="0" @keydown="onQuestionKey">
      <div class="qp-ap-q-head">
        <span class="qp-ap-q-tag">{{ current.header || 'Question' }}</span>
        <span v-if="questions.length > 1" class="qp-ap-q-step">{{ step + 1 }} of {{ questions.length }}</span>
      </div>
      <div v-if="question.data.byMessage" class="qp-ap-q-note">The agent asked this for later — your answer goes out as your next message.</div>
      <div class="qp-ap-q-text">{{ current.question }}</div>
      <div class="qp-ap-q-options">
        <button
          v-for="(o, n) in current.options ?? []"
          :key="o.label"
          class="qp-ap-opt"
          :class="{ on: (picked[current.question] ?? []).includes(o.label) }"
          :aria-pressed="(picked[current.question] ?? []).includes(o.label)"
          @click="toggle(current, o.label)"
        >
          <span class="qp-ap-opt-n">{{ n + 1 }}</span>
          <span class="qp-ap-opt-text">
            <span>{{ o.label }}</span>
            <span v-if="o.description" class="qp-ap-opt-desc">{{ o.description }}</span>
          </span>
        </button>
      </div>
      <input
        v-model="other[current.question]"
        class="qp-ap-other"
        :placeholder="current.multiSelect ? 'Something else (added to your picks)' : 'Other — your own answer'"
        @input="!current.multiSelect && other[current.question] && (picked[current.question] = [])"
        @keydown.enter.prevent="step < questions.length - 1 ? step++ : submitQuestion()"
      />
      <div class="qp-ap-actions">
        <button v-if="step > 0" class="qp-btn qp-btn-sm" @click="step--">Back</button>
        <button v-if="step < questions.length - 1" class="qp-btn qp-btn-sm qp-btn-primary" :disabled="!answered(current)" @click="step++">Next</button>
        <button v-else class="qp-btn qp-btn-sm qp-btn-primary" :disabled="!allAnswered" @click="submitQuestion">Answer</button>
        <button class="qp-btn qp-btn-sm" @click="answer(question, { decision: 'dismiss' })">Dismiss</button>
      </div>
    </div>

    <button v-if="plan" class="qp-ap-plan" @click="showPlan">A plan is ready for your review — show it</button>
  </div>
</template>

<style scoped>
.qp-ap { max-width: 780px; width: calc(100% - 48px); margin: 0 auto 6px; display: flex; flex-direction: column; gap: 6px; }
.qp-ap-row,
.qp-ap-q {
  padding: 9px 12px;
  border-radius: 10px;
  border: 1px solid color-mix(in srgb, var(--qss-warning, #d29a3f) 50%, var(--qss-border, #47474f));
  background: color-mix(in srgb, var(--qss-warning, #d29a3f) 6%, var(--qss-bg-raised, #1f1f25));
  display: flex;
  flex-direction: column;
  gap: 7px;
  outline: none;
}
.qp-ap-row:focus-visible,
.qp-ap-q:focus-visible { box-shadow: 0 0 0 1px var(--qss-warning, #d29a3f); }
.qp-ap-line { display: flex; align-items: center; gap: 7px; min-width: 0; font-size: 12.5px; }
.qp-ap-icon { flex-shrink: 0; color: var(--qss-warning, #d29a3f); }
.qp-ap-who { flex-shrink: 0; color: var(--qss-text-secondary, #9a9aa5); }
.qp-ap-title {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--qss-font-mono, ui-monospace, monospace);
  font-size: 12px;
  color: var(--qss-text, #d4d4d8);
}
.qp-ap-more { flex-shrink: 0; margin-left: auto; display: inline-flex; align-items: center; gap: 3px; border: none; background: transparent; color: var(--qss-text-muted, #6e6e7a); font-size: 11.5px; cursor: pointer; }
.qp-ap-more:hover { color: var(--qss-text, #d4d4d8); }
.qp-ap-more .open { transform: rotate(90deg); }
.qp-ap-pre {
  margin: 0;
  max-height: 180px;
  overflow: auto;
  padding: 7px 9px;
  border-radius: 6px;
  background: var(--qss-bg, #18181e);
  font-family: var(--qss-font-mono, ui-monospace, monospace);
  font-size: 11.5px;
  white-space: pre-wrap;
  word-break: break-word;
  user-select: text;
}
.qp-ap-actions { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
.qp-ap-hint { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-ap-all { display: flex; justify-content: flex-end; }
.qp-ap-q-head { display: flex; align-items: center; justify-content: space-between; }
.qp-ap-q-tag { font-size: 11.5px; font-weight: 600; color: var(--qss-warning, #d29a3f); }
.qp-ap-q-note { margin-top: 4px; font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-ap-q-step { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-ap-q-text { font-size: 13px; color: var(--qss-text, #d4d4d8); }
.qp-ap-q-options { display: grid; grid-template-columns: repeat(auto-fill, minmax(170px, 1fr)); gap: 6px; }
.qp-ap-opt {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 7px 9px;
  border-radius: 8px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg, #18181e);
  color: var(--qss-text, #d4d4d8);
  font-size: 12.5px;
  text-align: left;
  cursor: pointer;
}
.qp-ap-opt:hover { background: var(--qss-bg-hover, #313139); }
.qp-ap-opt.on { border-color: var(--qss-accent, #7aa2f7); background: color-mix(in srgb, var(--qss-accent, #7aa2f7) 14%, var(--qss-bg, #18181e)); }
.qp-ap-opt-n { flex-shrink: 0; width: 16px; height: 16px; border-radius: 4px; display: inline-flex; align-items: center; justify-content: center; background: var(--qss-bg-card, #292930); font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 10.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-ap-opt-text { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
.qp-ap-opt-desc { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-ap-other {
  height: 28px;
  padding: 0 9px;
  border-radius: 7px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg, #18181e);
  color: var(--qss-text, #d4d4d8);
  font-size: 12.5px;
  outline: none;
}
.qp-ap-other:focus { border-color: var(--qss-accent, #7aa2f7); }
.qp-ap-plan {
  align-self: flex-start;
  border: 1px solid color-mix(in srgb, var(--qss-warning, #d29a3f) 50%, var(--qss-border, #47474f));
  background: transparent;
  color: var(--qss-warning, #d29a3f);
  font-size: 12px;
  padding: 4px 10px;
  border-radius: 14px;
  cursor: pointer;
}
.qp-ap-plan:hover { background: color-mix(in srgb, var(--qss-warning, #d29a3f) 10%, transparent); }
</style>
