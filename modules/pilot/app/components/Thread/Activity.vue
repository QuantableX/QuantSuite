<script setup lang="ts">
/**
 * The fold between a user message and the answer: "Worked for 1m 12s ·
 * Ran 3 commands, edited 2 files". Collapsed by default; while the turn
 * runs its header shows the live elapsed time and the running tool. Inside,
 * in order: commentary, reasoning, tool groups (≤ 5 calls each, with their
 * own summary), answered requests. The task list has its own row (Tasks.vue).
 */
import { computed, ref } from 'vue'
import { Brain, ChevronRight, Loader2 } from 'lucide-vue-next'
import type { TurnBlock } from '#pilot/utils/timeline'
import type { ThreadItem } from '#pilot/types'
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

/** The images and files sent with a question's answers (parity 17), as a message's attachments. */
function answerFiles(item: ThreadItem): Record<string, any>[] {
  const files = item.data.replyFiles as Record<string, Record<string, any>[]> | undefined
  return files ? Object.values(files).flat() : []
}
function filesItem(item: ThreadItem): ThreadItem {
  return { ...item, kind: 'user', data: { text: '', attachments: answerFiles(item).map((f) => ({ kind: f.kind, name: f.name, path: f.path, thumb: f.thumb ?? null })) } }
}

function requestLine(kind: string, data: Record<string, any>): string {
  if (kind === 'approval') {
    const verdict = data.status === 'denied' ? 'Denied' : data.status === 'expired' ? 'Expired' : 'Allowed'
    return `${verdict}: ${data.title || data.toolName}`
  }
  if (kind === 'question' && data.elicitation) {
    const who = data.elicitation.server || 'An MCP server'
    const what = data.elicitation.mode === 'url' ? 'web step' : 'form'
    switch (data.status) {
      case 'accepted':
        return data.elicitation.mode === 'url' ? `${who}: web step finished` : `${who}: form sent`
      case 'declined':
        return `${who}: ${what} declined`
      case 'expired':
        return `${who}: ${what} expired`
      default:
        return `${who}: ${what} cancelled`
    }
  }
  if (kind === 'question') {
    if (data.status === 'dismissed') return 'Question dismissed'
    if (data.status === 'expired') return 'Question expired'
    const n = Object.values((data.replyFiles ?? {}) as Record<string, unknown[]>).flat().length
    return n ? `Question answered · ${n} file${n === 1 ? '' : 's'}` : 'Question answered'
  }
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
        <div v-else-if="e.item.kind === 'assistant'" class="qp-act-text" :class="{ sub: e.item.parent }" :data-item-id="e.item.id">
          <PilotThreadMarkdown :source="String(e.item.data.text ?? '')" :streaming="Boolean(e.item.data.streaming)" :cite="{ threadId: e.item.threadId, itemId: e.item.id }" @notice="(t: string) => emit('notice', t)" />
        </div>
        <div v-else-if="e.item.kind === 'reasoning'" class="qp-act-line">
          <Brain :size="12" />
          <span>{{ e.item.data.streaming ? 'Thinking…' : 'Thought' }}</span>
          <span v-if="e.item.data.estimatedTokens" class="qp-act-num">{{ e.item.data.estimatedTokens }} tokens</span>
          <span v-if="e.item.data.text" class="qp-act-reason">{{ e.item.data.text }}</span>
        </div>
        <div v-else-if="e.item.kind === 'approval' || e.item.kind === 'question'" class="qp-act-line is-muted">
          {{ requestLine(e.item.kind, e.item.data) }}
        </div>
        <div v-if="e.kind !== 'tools' && e.item.kind === 'question' && answerFiles(e.item).length" class="qp-act-files">
          <PilotThreadUserMessage :item="filesItem(e.item)" @notice="(t: string) => emit('notice', t)" />
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
.qp-act-caret { flex-shrink: 0; transition: transform calc(var(--qp-motion, 160ms) * 0.75) ease; }
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
.qp-spin { animation: qp-spin 0.9s linear infinite; flex-shrink: 0; }
@keyframes qp-spin { to { transform: rotate(360deg); } }
.qp-act-files { margin: 2px 0 4px 18px; }
</style>
