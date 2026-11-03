<script setup lang="ts">
/**
 * Schedules (round 3, card 03): prompts that run on their own — the list
 * with each one's next run and last result, and on the right the selected
 * schedule (run now, edit, pause, delete, its history with a link to each
 * run's thread) or the form. "Schedule this…" in a thread's menu opens the
 * form prefilled from the thread.
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { CalendarClock, Pause, Play, Plus } from 'lucide-vue-next'
import { useTauriEvent } from '@quantsuite/core'
import { usePilotStore } from '#pilot/stores/pilot'
import { useAppStore } from '#pilot/stores/app'
import { useSchedulesStore } from '#pilot/stores/schedules'
import { useThreadStore } from '#pilot/stores/thread'
import { describeRecurrence, inLabel, runLabel, whenLabel } from '#pilot/utils/schedules'
import { MODES } from '#pilot/utils/models'
import type { Schedule } from '#pilot/types'

const store = usePilotStore()
const app = useAppStore()
const schedules = useSchedulesStore()
const threads = useThreadStore()

onMounted(() => {
  void schedules.load()
  if (!store.contexts.length) void store.refreshContexts()
})
useTauriEvent('pilot-schedules-changed', () => void schedules.load())

// The "in 5m" labels move on while the view is open.
const now = ref(Date.now())
const timer = setInterval(() => (now.value = Date.now()), 30_000)
onBeforeUnmount(() => clearInterval(timer))

const current = computed(() => schedules.current)
watch(
  () => schedules.list,
  (list) => {
    if (!schedules.selected && !schedules.editing && list.length) schedules.select(list[0]!.id)
  },
)

const contextName = (id: string) => (id === 'general' ? 'General' : store.contexts.find((c) => c.id === id)?.name ?? 'A removed project')
const modelName = (provider: string, model: string) => threads.catalogs[provider]?.models.find((m) => m.id === model)?.displayName ?? model.replace(/^claude-/, '')
function agentOf(s: Schedule): string {
  return s.provider || store.newThreadAgent(s.contextId) || ''
}
function agentText(s: Schedule): string {
  const a = agentOf(s)
  if (!a) return 'Default agent'
  const label = store.accountLabel(a, s.instance)
  return s.provider ? `${label}${s.model ? ` · ${modelName(a, s.model)}` : ''}` : `${label} (default)`
}
const modeText = (m: string) => MODES.find((x) => x.id === m)?.label ?? 'Default'
function stateOf(s: Schedule): { label: string; tone: string } {
  if (!s.enabled) return { label: 'Paused', tone: 'paused' }
  if (!s.nextDue) return { label: 'Done', tone: 'done' }
  return { label: 'On', tone: 'on' }
}
function nextText(s: Schedule): string {
  if (!s.enabled) return 'Paused'
  if (!s.nextDue) return 'Nothing left to run'
  return `${whenLabel(s.nextDue, now.value)} · ${inLabel(s.nextDue, now.value)}`
}
const runTone = (status: string) => (status === 'started' ? 'ok' : status === 'failed' ? 'bad' : status === 'starting' ? '' : 'warn')

function openThread(id: string | null) {
  if (!id) return
  void store.openSession(id)
  app.setView('conversation')
}

/** A new schedule starts in the project of the thread on screen. */
function startNew() {
  const t = store.sessions.find((x) => x.id === store.activeId)
  schedules.startNew({ contextId: t?.contextId || 'general' })
}

const confirmDelete = ref(false)
watch(() => schedules.selected, () => (confirmDelete.value = false))
const running = ref(false)
async function runNow(id: string) {
  running.value = true
  try {
    await schedules.runNow(id)
  } finally {
    running.value = false
  }
}
</script>

<template>
  <div class="qp-sched">
    <header class="qp-sched-head">
      <div class="qp-sched-head-text">
        <h1>Schedules</h1>
        <p>Prompts that run on their own at the times you set — while QuantSuite is open.</p>
      </div>
      <button class="qp-btn qp-btn-primary" @click="startNew()"><Plus :size="13" /> New schedule</button>
    </header>

    <div v-if="schedules.loaded && !schedules.list.length && !schedules.editing" class="qp-sched-empty">
      <CalendarClock :size="30" :stroke-width="1.5" />
      <h2>No schedules yet</h2>
      <p>For recurring work: "every weekday at 08:00 review the open pull requests", "every 6 hours check the build". Each run is a thread you can open.</p>
      <button class="qp-btn qp-btn-primary" @click="startNew()"><Plus :size="13" /> New schedule</button>
    </div>

    <div v-else class="qp-sched-split">
      <nav class="qp-sched-list" aria-label="Schedules">
        <button
          v-for="s in schedules.list"
          :key="s.id"
          class="qp-sched-item"
          :class="{ on: schedules.editing ? s.id === schedules.editing.id : s.id === schedules.selected, off: !s.enabled }"
          :data-schedule="s.id"
          @click="schedules.select(s.id)"
        >
          <span class="qp-sched-item-top">
            <PilotAgentIcon v-if="agentOf(s)" :agent="agentOf(s)" :size="13" decorative />
            <span class="qp-sched-item-name">{{ s.name }}</span>
            <span class="qp-sched-pill" :class="`is-${stateOf(s).tone}`">{{ stateOf(s).label }}</span>
          </span>
          <span class="qp-sched-item-line">{{ describeRecurrence(s.recurrence) }} · {{ contextName(s.contextId) }}</span>
          <span class="qp-sched-item-line">
            <span>{{ nextText(s) }}</span>
            <span v-if="s.lastRun" class="qp-sched-last" :class="`is-${runTone(s.lastRun.status)}`">· {{ runLabel(s.lastRun.status, s.lastRun.manual) }} {{ whenLabel(s.lastRun.at, now) }}</span>
          </span>
        </button>
      </nav>

      <section class="qp-sched-pane">
        <PilotSchedulesEditor v-if="schedules.editing" :key="schedules.editing.id || 'new'" />
        <div v-else-if="current" class="qp-sched-detail">
          <header class="qp-sched-dhead">
            <div class="qp-sched-dtitle">
              <h2>{{ current.name }}</h2>
              <span class="qp-sched-pill" :class="`is-${stateOf(current).tone}`">{{ stateOf(current).label }}</span>
            </div>
            <div class="qp-sched-actions">
              <button class="qp-btn qp-btn-sm" :disabled="running" title="Run it now; the next scheduled run still happens" @click="runNow(current.id)"><Play :size="12" /> {{ running ? 'Starting…' : 'Run now' }}</button>
              <button class="qp-btn qp-btn-sm" @click="schedules.edit(current.id)">Edit</button>
              <button class="qp-btn qp-btn-sm" @click="schedules.setEnabled(current.id, !current.enabled)">
                <template v-if="current.enabled"><Pause :size="12" /> Pause</template>
                <template v-else><Play :size="12" /> Resume</template>
              </button>
              <button v-if="!confirmDelete" class="qp-btn qp-btn-sm qp-btn-danger" @click="confirmDelete = true">Delete</button>
              <span v-else class="qp-sched-confirm" role="alert">
                Delete it? Its threads stay.
                <button class="qp-btn qp-btn-sm qp-btn-danger" @click="schedules.remove(current.id)">Delete</button>
                <button class="qp-btn qp-btn-sm" @click="confirmDelete = false">Keep</button>
              </span>
            </div>
          </header>
          <p v-if="schedules.error" class="qp-sched-err" role="alert">{{ schedules.error }}</p>

          <dl class="qp-sched-facts">
            <div><dt>When</dt><dd>{{ describeRecurrence(current.recurrence) }}</dd></div>
            <div><dt>Next run</dt><dd>{{ nextText(current) }}</dd></div>
            <div><dt>Project</dt><dd>{{ contextName(current.contextId) }}</dd></div>
            <div>
              <dt>Agent</dt>
              <dd class="qp-sched-agentdd"><PilotAgentIcon v-if="agentOf(current)" :agent="agentOf(current)" :size="12" decorative />{{ agentText(current) }}</dd>
            </div>
            <div><dt>Mode</dt><dd>{{ modeText(current.mode) }}</dd></div>
            <div>
              <dt>Each run</dt>
              <dd>
                <template v-if="current.target === 'same'">
                  Continues
                  <button v-if="current.threadId" class="qp-link" @click="openThread(current.threadId)">its thread</button>
                  <template v-else>one thread (made by the first run)</template>
                </template>
                <template v-else>Starts a new thread</template>
              </dd>
            </div>
            <div><dt>If missed</dt><dd>{{ current.missed === 'once' ? 'Runs once when QuantSuite starts' : 'Skipped' }}</dd></div>
          </dl>

          <div class="qp-sched-promptview">{{ current.prompt }}</div>

          <h3 class="qp-sched-h3">History</h3>
          <div class="qp-sched-runs">
            <div v-for="r in schedules.runs[current.id] ?? []" :key="r.id" class="qp-sched-run" :data-run="r.status">
              <span class="qp-sched-dot" :class="`is-${runTone(r.status)}`" />
              <span class="qp-sched-run-what">{{ runLabel(r.status, r.manual) }}</span>
              <span class="qp-sched-run-when">{{ whenLabel(r.at, now) }}</span>
              <span class="qp-sched-run-note" :title="r.note || undefined">{{ r.note }}</span>
              <button v-if="r.threadId" class="qp-link" @click="openThread(r.threadId)">Open thread</button>
            </div>
            <p v-if="!(schedules.runs[current.id] ?? []).length" class="qp-sched-none">No runs yet. {{ current.enabled && current.nextDue ? `The first is ${whenLabel(current.nextDue, now).toLowerCase()}.` : '' }}</p>
          </div>
        </div>
        <div v-else class="qp-sched-none qp-sched-pick">Pick a schedule.</div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.qp-sched { flex: 1; min-height: 0; display: flex; flex-direction: column; font-size: 12.5px; color: var(--qss-text, #d4d4d8); }
.qp-sched-head { display: flex; align-items: center; gap: 16px; padding: 16px 20px 12px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); }
.qp-sched-head-text { flex: 1; min-width: 0; }
.qp-sched-head h1 { margin: 0; font-size: 16px; font-weight: 600; }
.qp-sched-head p { margin: 2px 0 0; font-size: 12px; color: var(--qss-text-muted, #6e6e7a); }
.qp-sched-head .qp-btn,
.qp-sched-empty .qp-btn { display: inline-flex; align-items: center; gap: 5px; }
.qp-sched-empty { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; padding: 24px; text-align: center; color: var(--qss-text-muted, #6e6e7a); }
.qp-sched-empty h2 { margin: 4px 0 0; font-size: 15px; font-weight: 600; color: var(--qss-text, #d4d4d8); }
.qp-sched-empty p { max-width: 440px; margin: 0 0 6px; line-height: 1.55; font-size: 12.5px; }
.qp-sched-split { flex: 1; min-height: 0; display: flex; }
.qp-sched-list { width: 330px; flex-shrink: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 2px; padding: 10px; border-right: 1px solid var(--qss-border-subtle, #35353d); }
.qp-sched-item { display: flex; flex-direction: column; gap: 3px; padding: 9px 10px; border: 1px solid transparent; border-radius: 8px; background: transparent; color: inherit; font: inherit; text-align: left; cursor: pointer; }
.qp-sched-item:hover { background: var(--qss-bg-hover, #313139); }
.qp-sched-item.on { background: var(--qss-bg-card, #292930); border-color: var(--qss-border-subtle, #35353d); }
.qp-sched-item.off .qp-sched-item-name { color: var(--qss-text-secondary, #9a9aa5); }
.qp-sched-item-top { display: flex; align-items: center; gap: 7px; min-width: 0; }
.qp-sched-item-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 600; }
.qp-sched-item-line { display: flex; gap: 4px; min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; font-size: 11.5px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-sched-last { overflow: hidden; text-overflow: ellipsis; color: var(--qss-text-muted, #6e6e7a); }
.qp-sched-last.is-bad { color: var(--qss-error, #f87171); }
.qp-sched-last.is-warn { color: var(--qss-warning, #d29a3f); }
.qp-sched-pill { flex-shrink: 0; padding: 1px 7px; border-radius: 999px; font-size: 10.5px; font-weight: 600; border: 1px solid var(--qss-border, #47474f); color: var(--qss-text-secondary, #9a9aa5); }
.qp-sched-pill.is-on { border-color: color-mix(in srgb, var(--qss-success, #34d399) 50%, transparent); color: var(--qss-success, #34d399); }
.qp-sched-pill.is-paused { border-color: color-mix(in srgb, var(--qss-warning, #d29a3f) 50%, transparent); color: var(--qss-warning, #d29a3f); }
.qp-sched-pane { flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column; }
.qp-sched-detail { flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 14px; padding: 16px 20px; }
.qp-sched-dhead { display: flex; flex-wrap: wrap; align-items: center; gap: 10px 16px; }
.qp-sched-dtitle { flex: 1; min-width: 200px; display: flex; align-items: center; gap: 10px; }
.qp-sched-dtitle h2 { margin: 0; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 15px; font-weight: 600; }
.qp-sched-actions { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
.qp-sched-actions .qp-btn { display: inline-flex; align-items: center; gap: 5px; }
.qp-sched-confirm { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-sched-err { margin: 0; color: var(--qss-error, #f87171); font-size: 12px; }
.qp-sched-facts { display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: 10px 18px; margin: 0; }
.qp-sched-facts div { min-width: 0; }
.qp-sched-facts dt { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-sched-facts dd { margin: 2px 0 0; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-sched-agentdd { display: flex; align-items: center; gap: 6px; }
.qp-sched-promptview { max-height: 180px; overflow-y: auto; padding: 10px 12px; border: 1px solid var(--qss-border-subtle, #35353d); border-radius: 8px; background: var(--qss-bg-raised, #1f1f25); white-space: pre-wrap; line-height: 1.55; overflow-wrap: anywhere; }
.qp-sched-h3 { margin: 4px 0 0; font-size: 12.5px; font-weight: 600; }
.qp-sched-runs { display: flex; flex-direction: column; gap: 1px; }
.qp-sched-run { display: flex; align-items: center; gap: 10px; min-width: 0; padding: 6px 8px; border-radius: 6px; }
.qp-sched-run:hover { background: var(--qss-bg-hover, #313139); }
.qp-sched-dot { width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0; background: var(--qss-border, #47474f); }
.qp-sched-dot.is-ok { background: var(--qss-success, #34d399); }
.qp-sched-dot.is-warn { background: var(--qss-warning, #d29a3f); }
.qp-sched-dot.is-bad { background: var(--qss-error, #f87171); }
.qp-sched-run-what { width: 110px; flex-shrink: 0; }
.qp-sched-run-when { width: 120px; flex-shrink: 0; color: var(--qss-text-secondary, #9a9aa5); font-variant-numeric: tabular-nums; }
.qp-sched-run-note { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--qss-text-muted, #6e6e7a); font-size: 12px; }
.qp-link { padding: 0 2px; border: none; background: transparent; color: var(--qss-accent, #7aa2f7); font: inherit; cursor: pointer; }
.qp-link:hover { text-decoration: underline; }
.qp-sched-none { margin: 0; padding: 8px; color: var(--qss-text-muted, #6e6e7a); font-size: 12px; }
.qp-sched-pick { margin: auto; }
</style>
