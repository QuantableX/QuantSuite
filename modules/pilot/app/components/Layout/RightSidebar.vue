<script setup lang="ts">
/**
 * The pilot's panel: the face and one word for what it is doing, then the
 * three things a terminal cannot show —
 *   Needs you   every session whose agent waits on you, wherever it is
 *   Vitals      model, context fill, cost, lines — for the open session
 *   Files       what the agent wrote; a click opens it in QuantCode
 * Sans labels, mono only for numbers (the agent-UI register).
 */
import { computed } from 'vue'
import type { PendingAgentCall } from '@quantsuite/core'
import { usePilotStore } from '#pilot/stores/pilot'
import { basename, dirname, fmtCost, fmtTokens } from '#pilot/utils/format'

const store = usePilotStore()
const session = computed(() => store.active)
const live = computed(() => store.activeLive)

/** QuantMCP's own approval queue (the drawer's Agent tab answers it). */
const brokerQueue = useState<PendingAgentCall[]>('qss-agent-queue', () => [])

const faceState = computed(() => {
  if (!session.value) return store.status ? 'idle' : 'offline'
  return live.value.state
})

const stateLabel = computed(() => {
  if (!session.value) return 'ready'
  switch (live.value.state) {
    case 'thinking':
      return 'thinking'
    case 'working':
      return 'working'
    case 'waiting':
      return 'needs you'
    case 'error':
      return 'error'
    case 'offline':
      return live.value.launching ? 'starting' : session.value.providerSessionId ? 'asleep' : 'ready'
    default:
      return 'idle'
  }
})

/** One line under the state: the running tool, the question, or the error. */
const doing = computed(() => {
  if (!session.value) return ''
  const l = live.value
  if (l.state === 'working' && l.tool) return l.tool
  if (l.state === 'waiting' || l.state === 'error') return l.detail ?? ''
  return ''
})

const vitals = computed(() => live.value.vitals)
const model = computed(() => vitals.value.model || session.value?.model || '')
const hasVitals = computed(
  () =>
    Boolean(model.value) ||
    vitals.value.costUsd !== null ||
    vitals.value.contextPct !== null ||
    vitals.value.linesAdded !== null ||
    vitals.value.inputTokens !== null
)
const contextPct = computed(() => (vitals.value.contextPct === null ? null : Math.max(0, Math.min(100, vitals.value.contextPct))))
const linesLabel = computed(() => {
  const a = vitals.value.linesAdded
  const r = vitals.value.linesRemoved
  if (a === null && r === null) return ''
  return `+${a ?? 0} −${r ?? 0}`
})
const tokensLabel = computed(() => {
  const i = vitals.value.inputTokens
  const o = vitals.value.outputTokens
  if (i === null && o === null) return ''
  return `${fmtTokens(i)} in · ${fmtTokens(o)} out`
})

const showFiles = computed(() => Boolean(session.value && live.value.ptyId))
const agentName = computed(() => (session.value ? store.adapterLabel(session.value.provider) : ''))
</script>

<template>
  <div class="qp-panel">
    <div class="qp-hero">
      <div class="qp-hero-face">
        <PilotFace :state="faceState" :cheer="live.cheer" />
      </div>
      <div class="qp-hero-state" :class="`is-${faceState}`">{{ stateLabel }}</div>
      <div v-if="doing" class="qp-hero-doing" :class="{ err: live.state === 'error' }" :title="doing">{{ doing }}</div>
    </div>

    <section v-if="store.needsYou.length" class="qp-block">
      <h4 class="qp-block-title is-warn">Needs you</h4>
      <button
        v-for="s in store.needsYou"
        :key="s.id"
        class="qp-need"
        :class="{ active: s.id === store.activeId }"
        @click="store.openSession(s.id)"
      >
        <span class="qp-need-title">{{ s.title || 'New session' }}</span>
        <span class="qp-need-sub">{{ store.adapterLabel(s.provider) }} · {{ s.contextName }}</span>
        <span v-if="store.live[s.id]?.detail" class="qp-need-detail">{{ store.live[s.id]?.detail }}</span>
      </button>
    </section>

    <section v-if="session && hasVitals" class="qp-block">
      <h4 class="qp-block-title">Vitals</h4>
      <div v-if="model" class="qp-vital">
        <span class="qp-vital-k">Model</span>
        <span class="qp-vital-v" :title="model">{{ model }}</span>
      </div>
      <div v-if="contextPct !== null" class="qp-vital is-bar">
        <span class="qp-vital-k">Context</span>
        <span class="qp-vital-v mono">{{ Math.round(contextPct) }}%</span>
        <span class="qp-bar"><span class="qp-bar-fill" :class="{ hot: contextPct >= 80 }" :style="{ width: `${contextPct}%` }" /></span>
      </div>
      <div v-if="vitals.costUsd !== null" class="qp-vital">
        <span class="qp-vital-k">Cost</span>
        <span class="qp-vital-v mono">{{ fmtCost(vitals.costUsd) }}</span>
      </div>
      <div v-if="linesLabel" class="qp-vital">
        <span class="qp-vital-k">Lines</span>
        <span class="qp-vital-v mono">{{ linesLabel }}</span>
      </div>
      <div v-if="tokensLabel" class="qp-vital">
        <span class="qp-vital-k">Tokens</span>
        <span class="qp-vital-v mono">{{ tokensLabel }}</span>
      </div>
    </section>

    <section v-if="showFiles" class="qp-block qp-files">
      <h4 class="qp-block-title">Files</h4>
      <div v-if="!live.files.length" class="qp-block-empty">
        {{ live.signals === 'activity' ? `${agentName} does not report its edits.` : 'nothing written yet' }}
      </div>
      <button
        v-for="f in live.files"
        :key="f"
        class="qp-file"
        :title="`${f} — open in QuantCode`"
        @click="store.openFile(f)"
      >
        <span class="qp-file-name">{{ basename(f) }}</span>
        <span class="qp-file-dir">{{ dirname(f) }}</span>
      </button>
    </section>

    <div v-if="brokerQueue.length" class="qp-block qp-broker">
      {{ brokerQueue.length }} suite tool call{{ brokerQueue.length === 1 ? '' : 's' }} waiting in the drawer's Agent tab.
    </div>
  </div>
</template>

<style scoped>
.qp-panel { display: flex; flex-direction: column; height: 100%; min-height: 0; overflow-y: auto; }
.qp-hero {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: 26px 16px 18px;
}
.qp-hero-face { width: 150px; }
.qp-hero-state {
  font-size: 13px;
  font-weight: 600;
  color: var(--qss-text-secondary, #9a9aa5);
  text-transform: lowercase;
  letter-spacing: 0.01em;
}
.qp-hero-state.is-waiting { color: var(--qss-warning, #d29a3f); }
.qp-hero-state.is-error { color: var(--qss-error, #f87171); }
.qp-hero-state.is-thinking,
.qp-hero-state.is-working { color: var(--qss-text, #d4d4d8); }
.qp-hero-doing {
  max-width: 100%;
  font-size: 11.5px;
  color: var(--qss-text-muted, #6e6e7a);
  text-align: center;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.qp-hero-doing.err { color: var(--qss-error, #f87171); white-space: normal; word-break: break-word; }

.qp-block {
  padding: 10px 14px;
  border-top: 1px solid var(--qss-border-subtle, #35353d);
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.qp-block-title {
  margin: 0 0 2px;
  font-size: 11px;
  font-weight: 600;
  color: var(--qss-text-muted, #6e6e7a);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}
.qp-block-title.is-warn { color: var(--qss-warning, #d29a3f); }
.qp-block-empty { font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }

.qp-need {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 6px 8px;
  border: 1px solid color-mix(in srgb, var(--qss-warning, #d29a3f) 40%, var(--qss-border, #47474f));
  border-radius: 6px;
  background: color-mix(in srgb, var(--qss-warning, #d29a3f) 6%, transparent);
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  text-align: left;
  cursor: pointer;
}
.qp-need:hover { background: color-mix(in srgb, var(--qss-warning, #d29a3f) 12%, transparent); }
.qp-need.active { border-color: var(--qss-warning, #d29a3f); }
.qp-need-title { font-size: 12.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-need-sub { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-need-detail { font-size: 11px; color: var(--qss-warning, #d29a3f); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

.qp-vital { display: flex; align-items: center; gap: 8px; font-size: 12px; flex-wrap: wrap; }
.qp-vital-k { width: 54px; flex-shrink: 0; color: var(--qss-text-muted, #6e6e7a); font-size: 11.5px; }
.qp-vital-v { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--qss-text, #d4d4d8); text-align: right; }
.qp-vital-v.mono { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11.5px; }
.qp-vital.is-bar .qp-vital-v { flex: 0 0 auto; }
.qp-bar { flex-basis: 100%; height: 4px; border-radius: 2px; background: var(--qss-bg-card, #292930); overflow: hidden; }
.qp-bar-fill { display: block; height: 100%; background: var(--qss-text-secondary, #9a9aa5); transition: width 0.4s ease; }
.qp-bar-fill.hot { background: var(--qss-warning, #d29a3f); }

.qp-files { flex-shrink: 0; }
.qp-file {
  display: flex;
  flex-direction: column;
  gap: 1px;
  padding: 4px 6px;
  margin: 0 -6px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  text-align: left;
  cursor: pointer;
}
.qp-file:hover { background: var(--qss-bg-hover, #313139); }
.qp-file-name { font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-file-dir { font-size: 10.5px; color: var(--qss-text-muted, #6e6e7a); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left; }
.qp-broker { font-size: 11.5px; color: var(--qss-warning, #d29a3f); }
</style>
