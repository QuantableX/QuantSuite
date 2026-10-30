<script setup lang="ts">
/**
 * The controls row under the message box (docs/PLAN-QUANTPILOT.md §6,
 * card 5): agent and model (Ctrl+Shift+M), effort (Ctrl+Shift+E),
 * permission mode (Ctrl+Shift+A), the context meter (click: Compact
 * context) and the usage-limit chip.
 *
 * Model and mode switch the live process when the CLI can (Claude Code
 * can, even mid-turn); effort applies from the next turn. A new model or
 * effort is also remembered for new threads; the mode is not — new
 * threads start with the default mode from Settings (T3 Code's rule).
 * The catalog is asked for only when a picker opens.
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { ChevronDown, ClipboardList, FilePen, Gauge, ShieldCheck, Sparkles, Zap } from 'lucide-vue-next'
import { useThreadStore } from '#pilot/stores/thread'
import { usePilotStore } from '#pilot/stores/pilot'
import { useKeysStore } from '#pilot/stores/keys'
import { useCommands } from '#pilot/utils/commands'
import { compactTokens, effortsFor, MODES, type MenuOption, type ProviderOption } from '#pilot/utils/models'
import { formatDuration } from '#pilot/utils/timeline'
import type { Mode, ProviderCatalog } from '#pilot/types'

const props = defineProps<{ threadId: string }>()
const emit = defineEmits<{ done: [] }>()
const threads = useThreadStore()
const store = usePilotStore()

/** Agents that can run a chat thread today (more arrive with card 20). */
const CHAT_PROVIDERS = ['claude', 'codex']

const view = computed(() => threads.view(props.threadId))
const thread = computed(() => store.sessions.find((s) => s.id === props.threadId) ?? null)
const provider = computed(() => thread.value?.provider || 'claude')
const running = computed(() => Boolean(view.value?.turnId))

type Open = 'model' | 'effort' | 'mode' | 'context' | null
const open = ref<Open>(null)
// Another thread in the same composer: its pickers start closed.
watch(
  () => props.threadId,
  () => {
    open.value = null
  }
)
const root = ref<HTMLElement | null>(null)

// ── Catalog, on demand ──
const catalog = ref<ProviderCatalog | null>(null)
const catalogLoading = ref(false)
const catalogError = ref<string | null>(null)
async function loadCatalog(id = provider.value, refresh = false) {
  catalogLoading.value = true
  catalogError.value = null
  const c = await threads.catalog(id, refresh)
  catalogLoading.value = false
  if (c) catalog.value = c
  else catalogError.value = 'The CLI did not report its models. You can still type a model id.'
}

/** Another agent takes an idle conversation along (card 15); a running one keeps its agent. */
const started = computed(() => Boolean(thread.value?.providerSessionId) || running.value || Boolean(view.value?.items.some((i) => i.kind === 'user')))
const providers = computed<ProviderOption[]>(() =>
  store.adapters
    .filter((a) => CHAT_PROVIDERS.includes(a.id))
    .map((a) => {
      if (a.found === false) return { id: a.id, label: a.label, available: false, reason: `${a.label} is not installed` }
      if (running.value && a.id !== provider.value) {
        return { id: a.id, label: a.label, available: false, reason: `${store.adapterLabel(provider.value)} is working — switch when it is done` }
      }
      return { id: a.id, label: a.label, available: true }
    })
)

/** Several models for a new thread's first message (card 10). */
function setFanout(list: { provider: string; model: string }[]) {
  if (list.length > 1) threads.fanout[props.threadId] = list
  else delete threads.fanout[props.threadId]
}

const modelLabel = computed(() => {
  const fan = threads.fanout[props.threadId]
  if (fan && fan.length > 1) return `${fan.length} models`
  const m = thread.value?.model ?? ''
  if (!m) return view.value?.model ? `Default · ${view.value.model.replace(/^claude-/, '')}` : 'Default'
  const known = catalog.value?.models.find((x) => x.id === m)
  return known?.displayName ?? m
})

const efforts = computed(() => (catalog.value ? effortsFor(catalog.value.models, thread.value?.model ?? '') : null))
const effortLabel = computed(() => {
  const e = thread.value?.effort || ''
  return e ? e.charAt(0).toUpperCase() + e.slice(1) : 'Default'
})
const effortOptions = computed<MenuOption[]>(() => [
  { id: '', label: 'Default', description: "The CLI's own setting" },
  ...(efforts.value ?? []).map((e) => ({ id: e, label: e.charAt(0).toUpperCase() + e.slice(1) })),
])

const modeInfo = computed(() => MODES.find((m) => m.id === thread.value?.mode) ?? MODES[4])
const MODE_ICONS: Record<Mode, unknown> = { ask: ShieldCheck, edits: FilePen, auto: Sparkles, plan: ClipboardList, full: Zap }
const modeOptions = computed<MenuOption[]>(() => MODES.map((m) => ({ id: m.id, label: m.label, description: m.description })))

// ── Context and limits ──
const usage = computed(() => view.value?.usage)
const contextPct = computed(() => {
  const u = usage.value
  if (!u?.contextTokens || !u.contextWindow) return null
  return Math.max(0, Math.min(100, (u.contextTokens / u.contextWindow) * 100))
})
const contextTitle = computed(() => {
  const u = usage.value
  if (!u?.contextTokens || !u.contextWindow) return ''
  return `${u.contextTokens.toLocaleString('en-US')} of ${u.contextWindow.toLocaleString('en-US')} tokens in context`
})
const RING = 2 * Math.PI * 6
const limit = computed(() => {
  const v = view.value
  if (!v) return null
  const rejected = v.rateLimits.find((w) => w.status === 'rejected')
  if (v.state !== 'limited' && !rejected) return null
  const reset = (rejected ?? v.rateLimits.find((w) => w.resetsAt))?.resetsAt
  return reset ? `Usage limit · resets in ${formatDuration(Math.max(0, reset - Date.now()))}` : 'Usage limit reached'
})

// ── Actions ──
async function toggle(which: Exclude<Open, null>) {
  if (open.value === which) {
    close()
    return
  }
  open.value = which
  if ((which === 'model' || which === 'effort') && !catalog.value) await loadCatalog()
}

function close() {
  open.value = null
  // Browsing another agent's models leaves its catalog loaded: back to the thread's own.
  if (catalog.value && catalog.value.provider !== provider.value) void loadCatalog()
  emit('done')
}

async function pickModel(picked: string, model: string) {
  close()
  if (!thread.value) return
  const switching = picked !== provider.value
  if (!switching && model === thread.value.model) return
  if (await store.update(props.threadId, switching ? { provider: picked, model } : { model })) {
    const remembered = { ...(store.settings?.model ?? {}) }
    if (model) remembered[picked] = model
    else delete remembered[picked]
    void store.saveSettings({ model: remembered }, { quiet: true })
    if (switching) await loadCatalog(picked)
  }
}

async function pickEffort(effort: string) {
  close()
  if (!thread.value || effort === thread.value.effort) return
  if (await store.update(props.threadId, { effort })) void store.saveSettings({ effort }, { quiet: true })
}

async function pickMode(mode: string) {
  close()
  if (!thread.value || mode === thread.value.mode) return
  await store.update(props.threadId, { mode: mode as Mode })
}

const canCompact = computed(() => !running.value && Boolean(thread.value?.providerSessionId))
async function compact() {
  close()
  if (canCompact.value) await threads.send(props.threadId, '/compact')
}

function onDocDown(e: MouseEvent) {
  if (!open.value) return
  if (root.value && !root.value.contains(e.target as Node)) open.value = null
}
/** `/model` and `/permissions` in the composer open these pickers. */
function onOpenControl(e: Event) {
  const d = (e as CustomEvent<{ threadId: string; which: Exclude<Open, null> }>).detail
  if (d?.threadId === props.threadId && open.value !== d.which) void toggle(d.which)
}
onMounted(() => {
  document.addEventListener('mousedown', onDocDown)
  window.addEventListener('qp-open-control', onOpenControl)
})
onBeforeUnmount(() => {
  document.removeEventListener('mousedown', onDocDown)
  window.removeEventListener('qp-open-control', onOpenControl)
})

// The pickers' keys (card 17: rebindable); only the open thread's composer answers.
const keys = useKeysStore()
const mine = (fn: () => unknown) => () => (store.activeId === props.threadId ? fn() : false)
useCommands({
  'modelPicker.toggle': mine(() => toggle('model')),
  'composer.effort': mine(() => toggle('effort')),
  'composer.mode': mine(() => toggle('mode')),
})
</script>

<template>
  <div ref="root" class="qp-ctl">
    <span class="qp-ctl-anchor">
      <button class="qp-ctl-btn" :class="{ on: open === 'model' }" :title="keys.hint('modelPicker.toggle', 'Model')" @click.stop="toggle('model')">
        <span class="qp-ctl-dot" :data-provider="provider" />
        <span class="qp-ctl-text">{{ store.adapterLabel(provider) }}</span>
        <span class="qp-ctl-sep">·</span>
        <span class="qp-ctl-text">{{ modelLabel }}</span>
        <ChevronDown :size="12" />
      </button>
      <div v-if="open === 'model'" class="qp-ctl-pop">
        <PilotThreadModelPicker
          :providers="providers"
          :provider="provider"
          :model="thread?.model ?? ''"
          :catalog="catalog"
          :loading="catalogLoading"
          :error="catalogError"
          @pick="pickModel"
          @close="close"
          @provider="(id: string) => loadCatalog(id)"
          :multi="threads.fanout[threadId] ?? []"
          :allow-multi="!started"
          @multi="setFanout"
          @refresh="loadCatalog(provider, true)"
        />
      </div>
    </span>

    <span v-if="efforts === null || efforts.length" class="qp-ctl-anchor">
      <button class="qp-ctl-btn" :class="{ on: open === 'effort' }" :title="keys.hint('composer.effort', 'Effort')" @click.stop="toggle('effort')">
        <Gauge :size="12" />
        <span class="qp-ctl-text">{{ effortLabel }}</span>
      </button>
      <div v-if="open === 'effort'" class="qp-ctl-pop">
        <PilotThreadMenuList
          :options="effortOptions"
          :selected="thread?.effort ?? ''"
          title="Effort — from the next turn"
          @pick="pickEffort"
          @close="close"
        />
      </div>
    </span>

    <span class="qp-ctl-anchor">
      <button class="qp-ctl-btn" :class="[{ on: open === 'mode' }, `mode-${modeInfo.id}`]" :title="keys.hint('composer.mode', 'Permissions')" @click.stop="toggle('mode')">
        <component :is="MODE_ICONS[modeInfo.id]" :size="12" />
        <span class="qp-ctl-text">{{ modeInfo.short }}</span>
      </button>
      <div v-if="open === 'mode'" class="qp-ctl-pop">
        <PilotThreadMenuList :options="modeOptions" :selected="thread?.mode ?? 'full'" title="Permissions" @pick="pickMode" @close="close" />
      </div>
    </span>

    <span class="qp-ctl-spacer" />

    <span v-if="limit" class="qp-ctl-limit">{{ limit }}</span>

    <span v-if="contextPct !== null" class="qp-ctl-anchor">
      <button class="qp-ctl-meter" :class="{ hot: contextPct >= 80 }" :title="contextTitle" @click.stop="toggle('context')">
        <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
          <circle cx="8" cy="8" r="6" class="qp-ring-bg" />
          <circle cx="8" cy="8" r="6" class="qp-ring" :stroke-dasharray="`${(contextPct / 100) * RING} ${RING}`" transform="rotate(-90 8 8)" />
        </svg>
        <span class="qp-ctl-num">{{ Math.round(contextPct) }}%</span>
        <span class="qp-ctl-of">of {{ compactTokens(usage?.contextWindow ?? 0) }}</span>
      </button>
      <div v-if="open === 'context'" class="qp-ctl-pop is-right">
        <PilotThreadMenuList
          :options="[{ id: 'compact', label: 'Compact context', description: 'Summarize the conversation so far to free the context window.', disabled: !canCompact, reason: running ? 'Wait for the turn to finish.' : 'Nothing to compact yet.' }]"
          selected=""
          :title="contextTitle"
          @pick="compact"
          @close="close"
        />
      </div>
    </span>
  </div>
</template>

<style scoped>
.qp-ctl { flex: 1; min-width: 0; display: flex; align-items: center; gap: 2px; }
.qp-ctl-anchor { position: relative; display: inline-flex; min-width: 0; }
.qp-ctl-btn,
.qp-ctl-meter {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  min-width: 0;
  height: 24px;
  padding: 0 7px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text-muted, #6e6e7a);
  font-size: 11.5px;
  cursor: pointer;
  white-space: nowrap;
}
.qp-ctl-btn:hover,
.qp-ctl-btn.on,
.qp-ctl-meter:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-ctl-btn:focus-visible,
.qp-ctl-meter:focus-visible { outline: 1px solid var(--qss-accent, #7aa2f7); }
.qp-ctl-btn.mode-full { color: var(--qss-text-secondary, #9a9aa5); }
.qp-ctl-btn.mode-plan { color: #a78bfa; }
.qp-ctl-text { overflow: hidden; text-overflow: ellipsis; max-width: 180px; }
.qp-ctl-sep { opacity: 0.6; }
.qp-ctl-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--qss-text-muted, #6e6e7a); flex-shrink: 0; }
.qp-ctl-dot[data-provider='claude'] { background: #d97757; }
.qp-ctl-dot[data-provider='codex'] { background: #e5e7eb; }
.qp-ctl-spacer { flex: 1; }
.qp-ctl-limit {
  font-size: 11px;
  color: var(--qss-warning, #d29a3f);
  padding: 2px 8px;
  border-radius: 10px;
  border: 1px solid color-mix(in srgb, var(--qss-warning, #d29a3f) 45%, var(--qss-border, #47474f));
  white-space: nowrap;
}
.qp-ring-bg { fill: none; stroke: var(--qss-bg-card, #292930); stroke-width: 2.5; }
.qp-ring { fill: none; stroke: var(--qss-text-secondary, #9a9aa5); stroke-width: 2.5; stroke-linecap: round; }
.qp-ctl-meter.hot .qp-ring { stroke: var(--qss-warning, #d29a3f); }
.qp-ctl-num { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; }
.qp-ctl-of { font-size: 11px; }
.qp-ctl-pop { position: absolute; bottom: calc(100% + 10px); left: -8px; z-index: 30; }
.qp-ctl-pop.is-right { left: auto; right: -8px; }
</style>
