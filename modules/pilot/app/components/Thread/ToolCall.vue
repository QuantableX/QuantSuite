<script setup lang="ts">
/**
 * One tool call: an icon for its kind, the one-line title (the command, the
 * path, the pattern), its status. Expands to the input and the output —
 * both already bounded by the runtime (4 KiB in, 8 KiB out, head and tail).
 * A subagent's call also opens what it did in the inspector (card 16).
 */
import { computed, ref } from 'vue'
import { Bot, Check, ChevronRight, FileText, Globe, ListChecks, Loader2, Pencil, Plug, Search, SquareTerminal, Wrench, X } from 'lucide-vue-next'
import type { ThreadItem } from '#pilot/types'
import { useInspectorStore } from '#pilot/stores/inspector'

const props = defineProps<{ item: ThreadItem }>()
const open = ref(false)

const ICONS: Record<string, unknown> = {
  command: SquareTerminal,
  read: FileText,
  search: Search,
  edit: Pencil,
  web: Globe,
  agent: Bot,
  todo: ListChecks,
  mcp: Plug,
}

const icon = computed(() => ICONS[String(props.item.data.category)] ?? Wrench)
const status = computed(() => String(props.item.data.status ?? 'running'))
const title = computed(() => String(props.item.data.title || props.item.data.name || 'Tool'))
const name = computed(() => String(props.item.data.name ?? ''))
const input = computed(() => String(props.item.data.input ?? ''))
const output = computed(() => String(props.item.data.output ?? ''))
const hasBody = computed(() => Boolean(input.value || output.value))
const agentCall = computed(() => (props.item.data.category === 'agent' && props.item.data.callId ? String(props.item.data.callId) : ''))
const inspector = useInspectorStore()
const statusLabel = computed(() => {
  switch (status.value) {
    case 'running':
      return 'Running'
    case 'failed':
      return 'Failed'
    case 'denied':
      return 'Denied'
    case 'interrupted':
      return 'Stopped'
    default:
      return 'Done'
  }
})
</script>

<template>
  <div class="qp-tool" :class="`is-${status}`">
    <button class="qp-tool-row" :disabled="!hasBody" :aria-expanded="open" @click="open = !open">
      <ChevronRight class="qp-tool-caret" :class="{ open }" :size="12" />
      <component :is="icon" class="qp-tool-icon" :size="13" />
      <span class="qp-tool-title" :class="{ mono: item.data.category === 'command' || item.data.category === 'read' || item.data.category === 'edit' }" :title="title">{{ title }}</span>
      <span v-if="name && name !== title" class="qp-tool-name">{{ name }}</span>
      <span class="qp-tool-status" :title="statusLabel">
        <Loader2 v-if="status === 'running'" class="qp-spin" :size="12" />
        <X v-else-if="status === 'failed' || status === 'denied'" :size="12" />
        <Check v-else-if="status === 'completed'" :size="12" />
        <span v-else class="qp-tool-stopped">stopped</span>
      </span>
    </button>
    <div v-if="open && hasBody" class="qp-tool-body">
      <button v-if="agentCall" class="qp-tool-open" @click="inspector.openAgent(item.threadId, agentCall)"><Bot :size="12" /> Open what it did</button>
      <div v-if="input" class="qp-tool-section">
        <div class="qp-tool-label">Input</div>
        <pre class="qp-tool-pre">{{ input }}</pre>
      </div>
      <div v-if="output" class="qp-tool-section">
        <div class="qp-tool-label">Output</div>
        <pre class="qp-tool-pre" :class="{ err: item.data.isError }">{{ output }}</pre>
      </div>
    </div>
  </div>
</template>

<style scoped>
.qp-tool { min-width: 0; }
.qp-tool-open {
  align-self: flex-start;
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 2px 8px;
  border: 1px solid var(--qss-border-subtle, #35353d);
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text-secondary, #9a9aa5);
  font: inherit;
  font-size: 11.5px;
  cursor: pointer;
}
.qp-tool-open:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-tool-row {
  display: flex;
  align-items: center;
  gap: 7px;
  width: 100%;
  min-width: 0;
  padding: 3px 6px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text-secondary, #9a9aa5);
  font-size: 12.5px;
  text-align: left;
  cursor: pointer;
}
.qp-tool-row:hover:not(:disabled) { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-tool-row:disabled { cursor: default; }
.qp-tool-row:focus-visible { outline: 1px solid var(--qss-accent, #7aa2f7); outline-offset: -1px; }
.qp-tool-caret { flex-shrink: 0; opacity: 0.6; transition: transform 0.12s ease; }
.qp-tool-caret.open { transform: rotate(90deg); }
.qp-tool-row:disabled .qp-tool-caret { visibility: hidden; }
.qp-tool-icon { flex-shrink: 0; opacity: 0.85; }
.qp-tool-title { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-tool-title.mono { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 12px; }
.qp-tool-name { flex-shrink: 0; font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-tool-status { flex-shrink: 0; display: inline-flex; color: var(--qss-text-muted, #6e6e7a); }
.is-completed .qp-tool-status { color: var(--qss-success, #34d399); }
.is-failed .qp-tool-status,
.is-denied .qp-tool-status { color: var(--qss-error, #f87171); }
.qp-tool-stopped { font-size: 11px; }
.qp-spin { animation: qp-spin 0.9s linear infinite; }
@keyframes qp-spin { to { transform: rotate(360deg); } }
.qp-tool-body { margin: 2px 0 6px 25px; display: flex; flex-direction: column; gap: 6px; }
.qp-tool-label { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); margin-bottom: 2px; }
.qp-tool-pre {
  margin: 0;
  max-height: 320px;
  overflow: auto;
  padding: 8px 10px;
  border-radius: 6px;
  border: 1px solid var(--qss-border-subtle, #35353d);
  background: var(--qss-bg-raised, #1f1f25);
  font-family: var(--qss-font-mono, ui-monospace, monospace);
  font-size: 11.5px;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-word;
  user-select: text;
  color: var(--qss-text, #d4d4d8);
}
.qp-tool-pre.err { color: var(--qss-error, #f87171); }
</style>
