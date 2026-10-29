<script setup lang="ts">
/**
 * The thread panel (QRightPanel, 220px): the face and one word for what the
 * agent does, then what the conversation does not say at a glance —
 * threads that need you, the model, context fill, cost, tokens, the
 * subscription windows, the files this thread wrote, the last error.
 * Sans labels, mono only for numbers (the agent-UI register).
 */
import { computed } from 'vue'
import { useThreadStore } from '#pilot/stores/thread'
import { usePilotStore } from '#pilot/stores/pilot'
import { basename, dirname, fmtCost, fmtTokens } from '#pilot/utils/format'
import { formatDuration } from '#pilot/utils/timeline'

const props = defineProps<{ threadId: string }>()
const threads = useThreadStore()
const store = usePilotStore()

const view = computed(() => threads.view(props.threadId))
const thread = computed(() => store.sessions.find((s) => s.id === props.threadId) ?? null)

const faceState = computed(() => {
  switch (view.value?.state) {
    case 'running':
    case 'starting':
      return 'working'
    case 'waiting':
      return 'waiting'
    case 'error':
    case 'limited':
      return 'error'
    default:
      return 'idle'
  }
})

const stateLabel = computed(() => {
  switch (view.value?.state) {
    case 'running':
    case 'starting':
      return 'working'
    case 'waiting':
      return 'needs you'
    case 'error':
      return 'error'
    case 'limited':
      return 'usage limit'
    case 'interrupted':
      return 'stopped'
    default:
      return thread.value?.providerSessionId ? 'idle' : 'ready'
  }
})

/** The running tool, or the request waiting on the user. */
const doing = computed(() => {
  const items = view.value?.items ?? []
  for (let i = items.length - 1; i >= 0 && i >= items.length - 40; i--) {
    const it = items[i]
    if ((it.kind === 'approval' || it.kind === 'question' || it.kind === 'plan') && it.data.status === 'pending') {
      return it.kind === 'approval' ? `Allow ${it.data.toolName}?` : it.kind === 'plan' ? 'A plan waits for you' : 'A question waits for you'
    }
    if (it.kind === 'tool' && it.data.status === 'running') return String(it.data.title || it.data.name)
  }
  return ''
})

const needsYou = computed(() =>
  store.sessions.filter((s) => s.id !== props.threadId && threads.views[s.id]?.state === 'waiting')
)

const model = computed(() => view.value?.model || thread.value?.model || '')
const usage = computed(() => view.value?.usage)
const contextPct = computed(() => {
  const u = usage.value
  if (!u?.contextTokens || !u.contextWindow) return null
  return Math.max(0, Math.min(100, (u.contextTokens / u.contextWindow) * 100))
})
const tokensLabel = computed(() => {
  const u = usage.value
  if (!u || (!u.inputTokens && !u.outputTokens)) return ''
  return `${fmtTokens(u.inputTokens)} in · ${fmtTokens(u.outputTokens)} out`
})

const LIMIT_NAMES: Record<string, string> = { five_hour: '5-hour', seven_day: 'Weekly', seven_day_opus: 'Weekly (Opus)' }
const limits = computed(() =>
  (view.value?.rateLimits ?? [])
    .filter((w) => w.usedPct != null)
    .map((w) => ({
      name: LIMIT_NAMES[w.kind] ?? w.kind.replace(/_/g, ' '),
      pct: Math.round(w.usedPct ?? 0),
      reset: w.resetsAt ? formatDuration(Math.max(0, w.resetsAt - Date.now())) : '',
    }))
)

const files = computed(() => {
  const seen = new Set<string>()
  const out: string[] = []
  const items = view.value?.items ?? []
  for (let i = items.length - 1; i >= 0; i--) {
    const p = items[i].kind === 'tool' ? items[i].data.path : null
    if (typeof p === 'string' && !seen.has(p)) {
      seen.add(p)
      out.push(p)
      if (out.length >= 40) break
    }
  }
  return out
})

async function copyPath(p: string) {
  try {
    await navigator.clipboard.writeText(p)
  } catch {
    /* clipboard denied */
  }
}
</script>

<template>
  <div class="qp-panel">
    <div class="qp-hero">
      <div class="qp-hero-face"><PilotFace :state="faceState" :cheer="view?.cheer ?? 0" /></div>
      <div class="qp-hero-state" :class="`is-${faceState}`">{{ stateLabel }}</div>
      <div v-if="doing" class="qp-hero-doing" :title="doing">{{ doing }}</div>
    </div>

    <section v-if="needsYou.length" class="qp-block">
      <h4 class="qp-block-title is-warn">Needs you</h4>
      <button v-for="s in needsYou" :key="s.id" class="qp-need" @click="store.openSession(s.id)">
        <span class="qp-need-title">{{ s.title || 'New thread' }}</span>
        <span class="qp-need-sub">{{ s.contextName }}</span>
      </button>
    </section>

    <section class="qp-block">
      <h4 class="qp-block-title">Vitals</h4>
      <div class="qp-vital">
        <span class="qp-vital-k">Agent</span>
        <span class="qp-vital-v">{{ store.adapterLabel(thread?.provider || 'claude') }}</span>
      </div>
      <div v-if="model" class="qp-vital">
        <span class="qp-vital-k">Model</span>
        <span class="qp-vital-v" :title="model">{{ model }}</span>
      </div>
      <div v-if="contextPct !== null" class="qp-vital is-bar">
        <span class="qp-vital-k">Context</span>
        <span class="qp-vital-v mono">{{ Math.round(contextPct) }}%</span>
        <span class="qp-bar"><span class="qp-bar-fill" :class="{ hot: contextPct >= 80 }" :style="{ width: `${contextPct}%` }" /></span>
      </div>
      <div v-if="usage?.costUsd" class="qp-vital">
        <span class="qp-vital-k">Cost</span>
        <span class="qp-vital-v mono">{{ fmtCost(usage.costUsd) }}</span>
      </div>
      <div v-if="tokensLabel" class="qp-vital">
        <span class="qp-vital-k">Tokens</span>
        <span class="qp-vital-v mono">{{ tokensLabel }}</span>
      </div>
      <div v-for="l in limits" :key="l.name" class="qp-vital is-bar">
        <span class="qp-vital-k">{{ l.name }}</span>
        <span class="qp-vital-v mono" :title="l.reset ? `Resets in ${l.reset}` : undefined">{{ l.pct }}%</span>
        <span class="qp-bar"><span class="qp-bar-fill" :class="{ hot: l.pct >= 80 }" :style="{ width: `${l.pct}%` }" /></span>
      </div>
    </section>

    <section v-if="files.length" class="qp-block qp-files">
      <h4 class="qp-block-title">Files</h4>
      <button v-for="p in files" :key="p" class="qp-file" :title="`${p} — click to copy the path`" @click="copyPath(p)">
        <span class="qp-file-name">{{ basename(p) }}</span>
        <span class="qp-file-dir">{{ dirname(p) }}</span>
      </button>
    </section>

    <section v-if="thread?.lastError && view?.state === 'error'" class="qp-block">
      <h4 class="qp-block-title is-err">Last error</h4>
      <div class="qp-err">{{ thread.lastError }}</div>
    </section>
  </div>
</template>

<style scoped>
.qp-panel { display: flex; flex-direction: column; height: 100%; min-height: 0; overflow-y: auto; }
.qp-hero { display: flex; flex-direction: column; align-items: center; gap: 4px; padding: 16px 12px 12px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); }
.qp-hero-face { width: 92px; }
.qp-hero-state { font-size: 12.5px; font-weight: 600; color: var(--qss-text-secondary, #9a9aa5); }
.qp-hero-state.is-working { color: var(--qss-success, #34d399); }
.qp-hero-state.is-waiting { color: var(--qss-warning, #d29a3f); }
.qp-hero-state.is-error { color: var(--qss-error, #f87171); }
.qp-hero-doing { max-width: 100%; font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-block { padding: 10px 12px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); display: flex; flex-direction: column; gap: 5px; }
.qp-block-title { margin: 0 0 2px; font-size: 12px; font-weight: 600; color: var(--qss-text-secondary, #9a9aa5); }
.qp-block-title.is-warn { color: var(--qss-warning, #d29a3f); }
.qp-block-title.is-err { color: var(--qss-error, #f87171); }
.qp-need { display: flex; flex-direction: column; gap: 1px; padding: 5px 7px; border-radius: 6px; border: 1px solid color-mix(in srgb, var(--qss-warning, #d29a3f) 40%, var(--qss-border, #47474f)); background: transparent; color: var(--qss-text, #d4d4d8); text-align: left; cursor: pointer; }
.qp-need:hover { background: var(--qss-bg-hover, #313139); }
.qp-need-title { font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-need-sub { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-vital { display: grid; grid-template-columns: auto 1fr; align-items: baseline; gap: 2px 8px; font-size: 12px; }
.qp-vital.is-bar { grid-template-columns: auto 1fr; }
.qp-vital-k { color: var(--qss-text-muted, #6e6e7a); }
.qp-vital-v { text-align: right; color: var(--qss-text, #d4d4d8); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-vital-v.mono { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11.5px; }
.qp-bar { grid-column: 1 / -1; height: 3px; border-radius: 2px; background: var(--qss-bg-card, #292930); overflow: hidden; }
.qp-bar-fill { display: block; height: 100%; background: var(--qss-text-secondary, #9a9aa5); }
.qp-bar-fill.hot { background: var(--qss-warning, #d29a3f); }
.qp-file { display: flex; flex-direction: column; gap: 0; padding: 3px 6px; border: none; border-radius: 5px; background: transparent; text-align: left; cursor: pointer; min-width: 0; }
.qp-file:hover { background: var(--qss-bg-hover, #313139); }
.qp-file-name { font-size: 12px; color: var(--qss-text, #d4d4d8); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-file-dir { font-size: 10.5px; color: var(--qss-text-muted, #6e6e7a); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left; }
.qp-err { font-size: 11.5px; color: var(--qss-text-secondary, #9a9aa5); white-space: pre-wrap; overflow-wrap: anywhere; max-height: 160px; overflow-y: auto; user-select: text; }
</style>
