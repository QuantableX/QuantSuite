<script setup lang="ts">
/**
 * The Agent tab (card 16): what one subagent did, read-only — what it was
 * asked, its text and reasoning, its newest tool calls (the 16 newest; the
 * agents it started count in) and its report. Previews stay bounded at
 * 32 KiB. Picked from Agents in the thread details or a subagent's call.
 */
import { computed } from 'vue'
import { Bot, Brain, Loader2 } from 'lucide-vue-next'
import { useThreadStore } from '#pilot/stores/thread'
import { useInspectorStore } from '#pilot/stores/inspector'
import { AGENT_TOOLS_MAX, bounded, descendants, subagents } from '#pilot/utils/timeline'

const props = defineProps<{ threadId: string }>()
const threads = useThreadStore()
const inspector = useInspectorStore()

const items = computed(() => threads.view(props.threadId)?.items ?? [])
const callId = computed(() => inspector.agent[props.threadId] ?? '')
const agent = computed(() => subagents(items.value).find((a) => a.callId === callId.value) ?? null)

const prompt = computed(() => {
  const raw = agent.value?.item.data.input
  if (!raw) return ''
  try {
    const input = typeof raw === 'string' ? JSON.parse(raw) : raw
    return bounded(String(input.prompt ?? input.message ?? input.task ?? ''))
  } catch {
    return bounded(String(raw))
  }
})
const report = computed(() => bounded(String(agent.value?.item.data.output ?? '')))

/** Its own entries from the cut on: everything after the 16th newest tool call. */
const transcript = computed(() => {
  if (!agent.value) return { entries: [], hidden: 0 }
  const all = descendants(items.value, agent.value.callId).filter((i) => i.kind === 'tool' || i.kind === 'reasoning' || (i.kind === 'assistant' && String(i.data.text ?? '').trim()))
  const tools = all.filter((i) => i.kind === 'tool')
  if (tools.length <= AGENT_TOOLS_MAX) return { entries: all, hidden: 0 }
  const first = tools[tools.length - AGENT_TOOLS_MAX]
  return { entries: all.slice(all.indexOf(first)), hidden: tools.length - AGENT_TOOLS_MAX }
})
</script>

<template>
  <div class="qp-agentv">
    <div v-if="!agent" class="qp-agentv-empty">
      <Bot :size="18" />
      <span>Pick an agent under Agents in the thread details, or open one from its call in the conversation.</span>
    </div>
    <template v-else>
      <div class="qp-agentv-head">
        <Loader2 v-if="agent.status === 'running'" class="qp-spin" :size="13" />
        <Bot v-else :size="13" />
        <span class="qp-agentv-title" :title="agent.title">{{ agent.title }}</span>
        <span class="qp-agentv-meta"><template v-if="agent.type">{{ agent.type }} · </template><span class="mono">{{ agent.tools }}</span> tool{{ agent.tools === 1 ? '' : 's' }}</span>
      </div>
      <div class="qp-agentv-body">
        <details v-if="prompt" class="qp-agentv-prompt">
          <summary>What it was asked</summary>
          <pre class="qp-agentv-pre">{{ prompt }}</pre>
        </details>
        <div v-if="transcript.hidden" class="qp-agentv-hidden">{{ transcript.hidden }} earlier tool call{{ transcript.hidden === 1 ? '' : 's' }} not shown</div>
        <template v-for="e in transcript.entries" :key="e.id">
          <PilotThreadToolCall v-if="e.kind === 'tool'" :item="e" />
          <div v-else-if="e.kind === 'reasoning'" class="qp-agentv-line">
            <Brain :size="12" />
            <span>{{ e.data.streaming ? 'Thinking…' : 'Thought' }}</span>
            <span v-if="e.data.text" class="qp-agentv-reason">{{ e.data.text }}</span>
          </div>
          <div v-else class="qp-agentv-text">
            <PilotThreadMarkdown :source="bounded(String(e.data.text ?? ''))" :streaming="Boolean(e.data.streaming)" />
          </div>
        </template>
        <div v-if="!transcript.entries.length && agent.status === 'running'" class="qp-agentv-line">Starting…</div>
        <div v-if="report && agent.status !== 'running'" class="qp-agentv-report">
          <div class="qp-agentv-label">Its report</div>
          <PilotThreadMarkdown :source="report" />
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.qp-agentv { flex: 1; min-height: 0; display: flex; flex-direction: column; }
.qp-agentv-empty { margin: auto; max-width: 280px; display: flex; flex-direction: column; align-items: center; gap: 8px; text-align: center; font-size: 12px; line-height: 1.5; color: var(--qss-text-muted, #6e6e7a); }
.qp-agentv-head { display: flex; align-items: center; gap: 7px; padding: 8px 12px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); font-size: 12.5px; color: var(--qss-text, #d4d4d8); }
.qp-agentv-title { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 600; }
.qp-agentv-meta { margin-left: auto; flex-shrink: 0; font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-agentv-meta .mono { font-family: var(--qss-font-mono, ui-monospace, monospace); }
.qp-agentv-body { flex: 1; min-height: 0; overflow-y: auto; padding: 10px 12px 16px; display: flex; flex-direction: column; gap: 6px; }
.qp-agentv-prompt summary { cursor: pointer; font-size: 12px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-agentv-pre {
  margin: 6px 0 0;
  max-height: 260px;
  overflow: auto;
  padding: 8px 10px;
  border-radius: 6px;
  border: 1px solid var(--qss-border-subtle, #35353d);
  background: var(--qss-bg-raised, #1f1f25);
  font-family: var(--qss-font-mono, ui-monospace, monospace);
  font-size: 11.5px;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  user-select: text;
}
.qp-agentv-hidden { font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-agentv-line { display: flex; align-items: center; gap: 6px; padding: 2px 6px; font-size: 12px; color: var(--qss-text-muted, #6e6e7a); min-width: 0; }
.qp-agentv-reason { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-agentv-text { padding: 2px 6px; }
.qp-agentv-text :deep(.qp-md) { font-size: 12.5px; }
.qp-agentv-report { margin-top: 6px; padding: 10px 12px; border-radius: 8px; border: 1px solid var(--qss-border-subtle, #35353d); background: var(--qss-bg-raised, #1f1f25); }
.qp-agentv-label { font-size: 11.5px; font-weight: 600; color: var(--qss-text-secondary, #9a9aa5); margin-bottom: 4px; }
.qp-spin { animation: qp-spin 0.9s linear infinite; flex-shrink: 0; }
@keyframes qp-spin { to { transform: rotate(360deg); } }
</style>
