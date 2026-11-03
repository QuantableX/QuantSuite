<script setup lang="ts">
/**
 * A review guide beside a diff (round 3, card 04; T3 Code's "Write guide"):
 * the summary, the files in reading order grouped by concern — each with
 * what changed, its risk and what to check, one click from its file and
 * line — and questions for the author. Written by one read-only agent run
 * (Claude Code or Codex, picked here), kept per diff and marked stale when
 * the diff changes. Nothing is posted anywhere.
 */
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { ChevronDown, RefreshCw, Sparkles } from 'lucide-vue-next'
import { pilot } from '#pilot/utils/invoke'
import { usePilotStore } from '#pilot/stores/pilot'
import { useThreadStore } from '#pilot/stores/thread'
import { loadGuideAgent, riskLine, saveGuideAgent, type GuideAgent } from '#pilot/utils/guide'
import type { AgentSetup } from '#pilot/utils/agentMenu'
import type { Guide, GuideState, GuideTarget } from '#pilot/types'

const props = defineProps<{ target: GuideTarget; contextId?: string | null; refresh?: number }>()
const emit = defineEmits<{ open: [path: string, line: number | null] }>()

const store = usePilotStore()
const threads = useThreadStore()

const state = ref<GuideState | null>(null)
const error = ref('')
const loading = ref(false)
const writing = ref(false)
const elapsed = ref(0)
let tick: ReturnType<typeof setInterval> | null = null
onBeforeUnmount(() => tick && clearInterval(tick))

const targetKey = computed(() => JSON.stringify(props.target))
async function load() {
  loading.value = true
  error.value = ''
  try {
    state.value = await pilot<GuideState>('plugin:pilot|pilot_guide_get', { target: props.target })
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    loading.value = false
  }
}
watch([targetKey, () => props.refresh], () => void load(), { immediate: true })

// ── The agent ──
const fallbackAgent = computed<GuideAgent>(() => {
  const a = store.newThreadAgent(props.contextId ?? 'general')
  const provider = a === 'codex' ? 'codex' : 'claude'
  return { provider, model: store.newThreadSetup(props.contextId ?? 'general', provider).model, instance: '' }
})
const agent = ref<GuideAgent>(loadGuideAgent(fallbackAgent.value))
const modelName = (provider: string, model: string) => threads.catalogs[provider]?.models.find((m) => m.id === model)?.displayName ?? model.replace(/^claude-/, '')
const agentText = computed(() => `${store.accountLabel(agent.value.provider, agent.value.instance)}${agent.value.model ? ` · ${modelName(agent.value.provider, agent.value.model)}` : ''}`)
const menuBtn = ref<HTMLElement | null>(null)
const menuOpen = ref(false)
function pickAgent(s: AgentSetup) {
  menuOpen.value = false
  if (s.provider !== 'claude' && s.provider !== 'codex') {
    error.value = 'Guides are written by Claude Code or Codex.'
    return
  }
  agent.value = { provider: s.provider, model: s.model, instance: s.instance ?? '' }
  saveGuideAgent(agent.value)
}

async function write() {
  writing.value = true
  error.value = ''
  elapsed.value = 0
  tick = setInterval(() => elapsed.value++, 1000)
  try {
    const guide = await pilot<Guide>('plugin:pilot|pilot_guide_write', { target: props.target, provider: agent.value.provider, model: agent.value.model || null, instance: agent.value.instance || null })
    if (guide) state.value = { guide, stale: false, files: state.value?.files ?? 0 }
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    writing.value = false
    if (tick) clearInterval(tick)
    tick = null
  }
}

const guide = computed(() => state.value?.guide ?? null)
const split = (p: string) => {
  const i = p.lastIndexOf('/')
  return { dir: i >= 0 ? p.slice(0, i + 1) : '', name: i >= 0 ? p.slice(i + 1) : p }
}
const written = computed(() => {
  const g = guide.value
  if (!g) return ''
  const d = new Date(g.createdAt)
  const time = d.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit', hour12: false })
  const when = d.toDateString() === new Date().toDateString() ? time : `${d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' })}, ${time}`
  return `${store.adapterLabel(g.provider)}${g.model ? ` · ${modelName(g.provider, g.model)}` : ''} · ${when}`
})
</script>

<template>
  <section class="qp-guide" aria-label="Review guide">
    <header class="qp-guide-head">
      <span class="qp-guide-title"><Sparkles :size="13" /> Review guide</span>
      <span v-if="state?.stale" class="qp-guide-stale" title="The diff changed since this guide was written">Stale</span>
      <span class="qp-guide-spacer" />
      <span class="qp-guide-split">
        <button class="qp-guide-write" :disabled="writing || loading || !state?.files" :title="`Write it with ${agentText}`" @click="write">
          <RefreshCw v-if="guide" :size="12" :class="{ spin: writing }" />
          <Sparkles v-else :size="12" />
          {{ writing ? `Writing… ${elapsed}s` : guide ? 'Write again' : 'Write guide' }}
        </button>
        <button ref="menuBtn" class="qp-guide-agent" :disabled="writing" aria-haspopup="menu" :aria-expanded="menuOpen" :title="`Written by ${agentText} — change`" @click.stop="menuOpen = !menuOpen">
          <PilotAgentIcon :agent="agent.provider" :size="12" decorative />
          <ChevronDown :size="12" />
        </button>
      </span>
      <PilotAgentMenu
        v-if="menuOpen"
        :anchor="menuBtn"
        title="Write guides with"
        :project="contextId ?? null"
        :provider="agent.provider"
        :instance="agent.instance"
        :model="agent.model"
        @pick="pickAgent"
        @close="menuOpen = false"
      />
    </header>

    <div class="qp-guide-body">
      <p v-if="error" class="qp-guide-err" role="alert">{{ error }}</p>
      <p v-if="loading && !state" class="qp-guide-note">Reading the diff…</p>
      <template v-else-if="!guide">
        <p v-if="state && !state.files" class="qp-guide-note">There are no changes to guide through.</p>
        <div v-else-if="state" class="qp-guide-empty">
          <p>
            A walkthrough of these {{ state.files }} file{{ state.files === 1 ? '' : 's' }}: what the change does, the order to read it in, the risky
            parts and what to check. {{ agentText }} reads the diff once, without tools; nothing is posted.
          </p>
          <p v-if="writing" class="qp-guide-note">Big changes take a minute or two.</p>
        </div>
      </template>
      <template v-else>
        <p class="qp-guide-meta">{{ riskLine(guide) }} · {{ written }}</p>
        <p v-if="state?.stale" class="qp-guide-stale-note">The diff changed since this guide was written — write it again for the current one.</p>
        <p class="qp-guide-summary">{{ guide.summary }}</p>
        <ol class="qp-guide-groups">
          <li v-for="(g, gi) in guide.groups" :key="gi" class="qp-guide-group" :class="{ uncovered: g.title === 'Not covered by the guide' }">
            <div class="qp-guide-gtitle">{{ g.title }}</div>
            <div v-for="f in g.files" :key="f.path" class="qp-guide-file" :data-guide-file="f.path">
              <button class="qp-guide-path" :title="f.line ? `Open ${f.path} at line ${f.line}` : `Open ${f.path}`" @click="emit('open', f.path, f.line)">
                <span class="qp-guide-risk" :data-risk="f.risk || 'none'" :title="f.risk ? `${f.risk} risk` : 'not covered'" />
                <span class="qp-guide-dir">{{ split(f.path).dir }}</span><span class="qp-guide-name">{{ split(f.path).name }}</span>
                <span v-if="f.line" class="qp-guide-line">:{{ f.line }}</span>
                <span v-if="guide.cut.includes(f.path)" class="qp-guide-cut" title="Its diff was cut for the prompt">cut</span>
              </button>
              <p v-if="f.what" class="qp-guide-what">{{ f.what }}</p>
              <ul v-if="f.check.length" class="qp-guide-check">
                <li v-for="(c, ci) in f.check" :key="ci">{{ c }}</li>
              </ul>
            </div>
          </li>
        </ol>
        <template v-if="guide.questions.length">
          <div class="qp-guide-gtitle">Questions for the author</div>
          <ul class="qp-guide-questions">
            <li v-for="(q, qi) in guide.questions" :key="qi">{{ q }}</li>
          </ul>
        </template>
        <p v-if="guide.cut.length" class="qp-guide-note">{{ guide.cut.length }} file{{ guide.cut.length === 1 ? '' : 's' }} had more diff than the prompt takes; the guide saw the start (marked "cut").</p>
      </template>
    </div>
  </section>
</template>

<style scoped>
.qp-guide { flex: 1; min-height: 0; display: flex; flex-direction: column; font-size: 12.5px; color: var(--qss-text, #d4d4d8); }
.qp-guide-head { display: flex; align-items: center; gap: 8px; padding: 8px 10px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); }
.qp-guide-title { display: inline-flex; align-items: center; gap: 6px; font-weight: 600; }
.qp-guide-stale { padding: 1px 7px; border-radius: 999px; border: 1px solid color-mix(in srgb, var(--qss-warning, #d29a3f) 55%, transparent); color: var(--qss-warning, #d29a3f); font-size: 10.5px; font-weight: 600; }
.qp-guide-spacer { flex: 1; }
.qp-guide-split { display: inline-flex; }
.qp-guide-write,
.qp-guide-agent { display: inline-flex; align-items: center; gap: 5px; height: 24px; border: 1px solid var(--qss-border, #47474f); background: transparent; color: var(--qss-text, #d4d4d8); font: inherit; font-size: 11.5px; cursor: pointer; }
.qp-guide-write { padding: 0 9px; border-radius: 6px 0 0 6px; }
.qp-guide-agent { padding: 0 6px; border-left: none; border-radius: 0 6px 6px 0; }
.qp-guide-write:hover:not(:disabled),
.qp-guide-agent:hover:not(:disabled) { background: var(--qss-bg-hover, #313139); }
.qp-guide-write:disabled,
.qp-guide-agent:disabled { opacity: 0.55; cursor: default; }
.spin { animation: qp-guide-spin 1s linear infinite; }
@keyframes qp-guide-spin { to { transform: rotate(360deg); } }
.qp-guide-body { flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 10px; padding: 10px 12px 14px; }
.qp-guide-err { margin: 0; color: var(--qss-error, #f87171); font-size: 12px; }
.qp-guide-note { margin: 0; color: var(--qss-text-muted, #6e6e7a); font-size: 12px; line-height: 1.5; }
.qp-guide-empty p { margin: 0; color: var(--qss-text-secondary, #9a9aa5); line-height: 1.55; }
.qp-guide-meta { margin: 0; font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-guide-stale-note { margin: 0; padding: 6px 9px; border-radius: 6px; background: color-mix(in srgb, var(--qss-warning, #d29a3f) 10%, transparent); color: var(--qss-warning, #d29a3f); font-size: 12px; }
.qp-guide-summary { margin: 0; line-height: 1.55; }
.qp-guide-groups { margin: 0; padding-left: 18px; display: flex; flex-direction: column; gap: 12px; }
.qp-guide-group::marker { color: var(--qss-text-muted, #6e6e7a); font-size: 11.5px; }
.qp-guide-group.uncovered .qp-guide-gtitle { color: var(--qss-warning, #d29a3f); }
.qp-guide-gtitle { margin-bottom: 4px; font-weight: 600; font-size: 12.5px; }
.qp-guide-file { display: flex; flex-direction: column; gap: 2px; padding: 4px 0 6px; }
.qp-guide-path { display: flex; align-items: center; gap: 0; min-width: 0; padding: 2px 4px; margin-left: -4px; border: none; border-radius: 5px; background: transparent; color: inherit; font: inherit; text-align: left; cursor: pointer; }
.qp-guide-path:hover { background: var(--qss-bg-hover, #313139); }
.qp-guide-risk { width: 7px; height: 7px; margin-right: 7px; border-radius: 50%; flex-shrink: 0; background: var(--qss-border, #47474f); }
.qp-guide-risk[data-risk='high'] { background: var(--qss-error, #f87171); }
.qp-guide-risk[data-risk='medium'] { background: var(--qss-warning, #d29a3f); }
.qp-guide-risk[data-risk='low'] { background: var(--qss-success, #34d399); }
.qp-guide-dir { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--qss-text-muted, #6e6e7a); }
.qp-guide-name { flex-shrink: 0; white-space: nowrap; }
.qp-guide-line { flex-shrink: 0; color: var(--qss-text-muted, #6e6e7a); font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11.5px; }
.qp-guide-cut { margin-left: 6px; padding: 0 5px; border-radius: 4px; border: 1px solid var(--qss-border, #47474f); color: var(--qss-text-muted, #6e6e7a); font-size: 10px; }
.qp-guide-what { margin: 0 0 0 14px; line-height: 1.5; color: var(--qss-text-secondary, #9a9aa5); }
.qp-guide-check { margin: 2px 0 0 14px; padding-left: 16px; color: var(--qss-text-secondary, #9a9aa5); line-height: 1.5; }
.qp-guide-questions { margin: 0; padding-left: 18px; line-height: 1.55; color: var(--qss-text-secondary, #9a9aa5); }
</style>
