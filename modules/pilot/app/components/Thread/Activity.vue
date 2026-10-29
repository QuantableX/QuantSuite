<script setup lang="ts">
/**
 * The fold between a user message and the answer: "Worked for 1m 12s ·
 * Ran 3 commands, edited 2 files". Collapsed by default; while the turn
 * runs its header shows the live elapsed time and the running tool. Inside,
 * in order: commentary, reasoning, tool groups (≤ 5 calls each, with their
 * own summary), task lists, answered requests.
 */
import { computed, ref } from 'vue'
import { Brain, ChevronRight, ListChecks, Loader2 } from 'lucide-vue-next'
import type { TurnBlock } from '#pilot/utils/timeline'
import { formatDuration } from '#pilot/utils/timeline'

const props = defineProps<{
  block: TurnBlock
  running: boolean
  /** Milliseconds the turn has run (live) or ran. */
  elapsed: number
}>()
const emit = defineEmits<{ notice: [text: string] }>()

const open = ref(false)

const runningTool = computed(() => {
  for (let i = props.block.activity.length - 1; i >= 0; i--) {
    const e = props.block.activity[i]
    if (e.kind === 'tools') {
      const t = [...e.items].reverse().find((it) => it.data.status === 'running')
      if (t) return String(t.data.title || t.data.name || '')
    }
  }
  return ''
})

const headline = computed(() => (props.running ? `Working for ${formatDuration(props.elapsed)}` : `Worked for ${formatDuration(props.elapsed)}`))
const detail = computed(() => (props.running && runningTool.value ? runningTool.value : props.block.summary))

function todoCounts(todos: { status?: string }[]) {
  const done = todos.filter((t) => t.status === 'completed').length
  return `${done}/${todos.length}`
}

function requestLine(kind: string, data: Record<string, any>): string {
  if (kind === 'approval') {
    const verdict = data.status === 'denied' ? 'Denied' : data.status === 'expired' ? 'Expired' : 'Allowed'
    return `${verdict}: ${data.title || data.toolName}`
  }
  if (kind === 'question') return data.status === 'dismissed' ? 'Question dismissed' : 'Question answered'
  return ''
}
</script>

<template>
  <div class="qp-act" :class="{ running }">
    <button class="qp-act-head" :aria-expanded="open" @click="open = !open">
      <ChevronRight class="qp-act-caret" :class="{ open }" :size="13" />
      <Loader2 v-if="running" class="qp-spin" :size="12" />
      <span class="qp-act-title">{{ headline }}</span>
      <span v-if="detail" class="qp-act-detail" :title="detail">{{ detail }}</span>
    </button>
    <div v-if="open" class="qp-act-body">
      <template v-for="e in block.activity" :key="e.id">
        <div v-if="e.kind === 'tools'" class="qp-act-group">
          <div v-if="e.items.length > 1" class="qp-act-group-head">
            {{ e.summary }}<span v-if="e.failed" class="qp-act-failed"> · {{ e.failed }} failed</span>
          </div>
          <PilotThreadToolCall v-for="t in e.items" :key="t.id" :item="t" />
        </div>
        <div v-else-if="e.item.kind === 'assistant'" class="qp-act-text" :class="{ sub: e.item.parent }">
          <PilotThreadMarkdown :source="String(e.item.data.text ?? '')" :streaming="Boolean(e.item.data.streaming)" @notice="(t: string) => emit('notice', t)" />
        </div>
        <div v-else-if="e.item.kind === 'reasoning'" class="qp-act-line">
          <Brain :size="12" />
          <span>{{ e.item.data.streaming ? 'Thinking…' : 'Thought' }}</span>
          <span v-if="e.item.data.estimatedTokens" class="qp-act-num">{{ e.item.data.estimatedTokens }} tokens</span>
          <span v-if="e.item.data.text" class="qp-act-reason">{{ e.item.data.text }}</span>
        </div>
        <div v-else-if="e.item.kind === 'todo'" class="qp-act-todos">
          <div class="qp-act-line">
            <ListChecks :size="12" />
            <span>Tasks</span>
            <span class="qp-act-num">{{ todoCounts(e.item.data.todos ?? []) }}</span>
          </div>
          <ul class="qp-act-todo-list">
            <li v-for="(t, i) in e.item.data.todos ?? []" :key="i" :class="`is-${t.status}`">{{ t.status === 'in_progress' ? t.activeForm || t.content : t.content }}</li>
          </ul>
        </div>
        <div v-else-if="e.item.kind === 'approval' || e.item.kind === 'question'" class="qp-act-line is-muted">
          {{ requestLine(e.item.kind, e.item.data) }}
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.qp-act { margin: 2px 0 8px; }
.qp-act-head {
  display: flex;
  align-items: center;
  gap: 6px;
  max-width: 100%;
  padding: 3px 8px 3px 4px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text-muted, #6e6e7a);
  font-size: 12.5px;
  cursor: pointer;
  text-align: left;
}
.qp-act-head:hover { color: var(--qss-text-secondary, #9a9aa5); background: var(--qss-bg-hover, #313139); }
.qp-act-head:focus-visible { outline: 1px solid var(--qss-accent, #7aa2f7); }
.qp-act-caret { flex-shrink: 0; transition: transform 0.12s ease; }
.qp-act-caret.open { transform: rotate(90deg); }
.qp-act-title { flex-shrink: 0; font-weight: 500; }
.running .qp-act-title { color: var(--qss-text-secondary, #9a9aa5); }
.qp-act-detail { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-act-detail::before { content: '·'; margin-right: 6px; }
.qp-act-body {
  margin: 4px 0 0 10px;
  padding: 2px 0 2px 10px;
  border-left: 1px solid var(--qss-border-subtle, #35353d);
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.qp-act-group { display: flex; flex-direction: column; gap: 1px; }
.qp-act-group-head { font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); padding: 2px 6px; }
.qp-act-failed { color: var(--qss-error, #f87171); }
.qp-act-text { padding: 2px 6px; }
.qp-act-text :deep(.qp-md) { font-size: 12.5px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-act-text.sub { border-left: 2px solid var(--qss-border, #47474f); padding-left: 8px; }
.qp-act-line { display: flex; align-items: center; gap: 6px; padding: 2px 6px; font-size: 12px; color: var(--qss-text-muted, #6e6e7a); min-width: 0; }
.qp-act-line.is-muted { font-style: normal; }
.qp-act-num { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; }
.qp-act-reason { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-act-todo-list { margin: 0 0 0 26px; padding: 0; list-style: none; font-size: 12px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-act-todo-list li { padding: 1px 0; }
.qp-act-todo-list li::before { content: '○'; margin-right: 6px; color: var(--qss-text-muted, #6e6e7a); }
.qp-act-todo-list li.is-in_progress::before { content: '◐'; color: var(--qss-accent, #7aa2f7); }
.qp-act-todo-list li.is-completed { color: var(--qss-text-muted, #6e6e7a); text-decoration: line-through; }
.qp-act-todo-list li.is-completed::before { content: '●'; color: var(--qss-success, #34d399); }
.qp-spin { animation: qp-spin 0.9s linear infinite; flex-shrink: 0; }
@keyframes qp-spin { to { transform: rotate(360deg); } }
</style>
