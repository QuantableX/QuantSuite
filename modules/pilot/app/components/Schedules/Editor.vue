<script setup lang="ts">
/**
 * A schedule's form (round 3, card 03): what runs (prompt, project, agent,
 * mode), when (once, every day, weekdays, chosen days, every N hours — local
 * time), where each run goes (a new thread or always the same one) and what
 * a slot missed while QuantSuite was closed does.
 */
import { computed, onMounted, ref, watch } from 'vue'
import { ChevronDown } from 'lucide-vue-next'
import { usePilotStore } from '#pilot/stores/pilot'
import { useSchedulesStore } from '#pilot/stores/schedules'
import { useThreadStore } from '#pilot/stores/thread'
import { DAYS, GITHUB_EVENTS, describeWhen, switchKind } from '#pilot/utils/schedules'
import { MODES } from '#pilot/utils/models'
import type { AgentSetup } from '#pilot/utils/agentMenu'
import type { Recurrence, Trigger } from '#pilot/types'

const store = usePilotStore()
const schedules = useSchedulesStore()
const threads = useThreadStore()
const s = computed(() => schedules.editing!)

const KINDS: { id: Recurrence['kind']; label: string }[] = [
  { id: 'once', label: 'Once' },
  { id: 'daily', label: 'Every day' },
  { id: 'weekdays', label: 'Weekdays' },
  { id: 'days', label: 'Chosen days' },
  { id: 'hours', label: 'Every N hours' },
]

const contexts = computed(() => [{ id: 'general', name: 'General' }, ...store.contexts.filter((c) => c.kind === 'workspace').map((c) => ({ id: c.id, name: c.name }))])
onMounted(() => {
  if (!store.contexts.length) void store.refreshContexts()
})
watch(
  () => s.value?.contextId,
  (c) => void store.loadSetup(c ?? 'general'),
  { immediate: true },
)

// ── The agent: the schedule's, else what a new thread in the project gets ──
const agentBtn = ref<HTMLElement | null>(null)
const agentOpen = ref(false)
const agent = computed(() => s.value.provider || store.newThreadAgent(s.value.contextId) || '')
const modelName = (provider: string, model: string) => threads.catalogs[provider]?.models.find((m) => m.id === model)?.displayName ?? model.replace(/^claude-/, '')
const agentText = computed(() => {
  if (!agent.value) return 'Default agent'
  const label = store.accountLabel(agent.value, s.value.instance)
  const model = s.value.provider ? s.value.model : store.newThreadSetup(s.value.contextId, agent.value).model
  return `${label}${model ? ` · ${modelName(agent.value, model)}` : ''}${s.value.provider ? '' : ' (default)'}`
})
function pickAgent(setup: AgentSetup) {
  agentOpen.value = false
  s.value.provider = setup.provider
  s.value.model = setup.model
  s.value.effort = setup.effort
  s.value.instance = setup.instance ?? ''
}

// ── When ──
function setKind(kind: Recurrence['kind']) {
  s.value.recurrence = switchKind(s.value.recurrence, kind)
}
function toggleDay(n: number) {
  const r = s.value.recurrence
  if (r.kind !== 'days') return
  r.days = r.days.includes(n) ? r.days.filter((d) => d !== n) : [...r.days, n].sort()
}
const summary = computed(() => describeWhen(s.value))
const TRIGGERS: { id: Trigger['kind']; label: string }[] = [
  { id: 'time', label: 'At a time' },
  { id: 'github', label: 'GitHub event' },
  { id: 'webhook', label: 'Webhook' },
]
const triggered = computed(() => s.value.trigger.kind !== 'time')
function setTrigger(kind: Trigger['kind']) {
  if (kind === s.value.trigger.kind) return
  s.value.trigger = kind === 'github' ? { kind, repo: '', events: ['issue_opened'], branch: '' } : { kind }
  // Event text comes from outside: the agent only reads and proposes, unless you choose otherwise.
  if (kind !== 'time') s.value.mode = 'plan'
}
function toggleEvent(id: string) {
  const t = s.value.trigger
  if (t.kind !== 'github') return
  t.events = t.events.includes(id) ? t.events.filter((e) => e !== id) : [...t.events, id]
}
/** Anyone who can open an issue can write what the agent reads. */
const exposed = computed(() => triggered.value && s.value.mode !== 'plan')
/** A one-off's or an every-N-hours start: date and 24-hour time apart. */
function setStamp(key: 'at' | 'from', date: string | null, time: string | null) {
  const r = s.value.recurrence as Record<string, unknown>
  const cur = String(r[key] ?? '')
  if (date === '') return
  r[key] = `${date ?? cur.slice(0, 10)}T${time ?? cur.slice(11, 16)}`
}
const unattended = computed(() => s.value.mode === 'ask')

const promptEl = ref<HTMLTextAreaElement | null>(null)
onMounted(() => promptEl.value?.focus())
async function save() {
  await schedules.save()
}
</script>

<template>
  <form class="qp-sched-form" @submit.prevent="save" @keydown.esc="schedules.editing = null">
    <header class="qp-sched-form-head">
      <h2>{{ s.id ? 'Edit schedule' : 'New schedule' }}</h2>
      <span class="qp-sched-summary">{{ summary }}</span>
    </header>

    <div class="qp-sched-body">
      <label class="qp-sched-field">
        <span>Name <em>(empty = the prompt's first line)</em></span>
        <input v-model="s.name" class="qp-input" placeholder="e.g. Morning review" spellcheck="false" />
      </label>
      <label class="qp-sched-field qp-sched-grow">
        <span>Prompt</span>
        <textarea ref="promptEl" v-model="s.prompt" class="qp-input qp-sched-prompt" placeholder="What the agent does each time, e.g. Review the open pull requests and list what needs me." />
      </label>

      <div class="qp-sched-grid">
        <label class="qp-sched-field">
          <span>Project</span>
          <select v-model="s.contextId" class="qp-input">
            <option v-for="c in contexts" :key="c.id" :value="c.id">{{ c.name }}</option>
          </select>
        </label>
        <div class="qp-sched-field">
          <span>Agent</span>
          <button ref="agentBtn" type="button" class="qp-input qp-sched-agent" aria-haspopup="menu" :aria-expanded="agentOpen" @click.stop="agentOpen = !agentOpen">
            <PilotAgentIcon v-if="agent" :agent="agent" :size="13" decorative />
            <span class="qp-sched-agent-text">{{ agentText }}</span>
            <ChevronDown :size="13" />
          </button>
          <PilotAgentMenu
            v-if="agentOpen"
            :anchor="agentBtn"
            title="Each run with"
            :project="s.contextId"
            :provider="s.provider || null"
            :instance="s.instance"
            :model="s.provider ? s.model : undefined"
            :effort="s.provider ? s.effort : undefined"
            @pick="pickAgent"
            @close="agentOpen = false"
          />
        </div>
        <label class="qp-sched-field">
          <span>Mode</span>
          <select v-model="s.mode" class="qp-input">
            <option value="">Default</option>
            <option v-for="m in MODES" :key="m.id" :value="m.id">{{ m.label }}</option>
          </select>
        </label>
      </div>
      <p v-if="unattended" class="qp-sched-hint">Supervised runs stop at every command until you answer — no one may be watching.</p>

      <div class="qp-sched-field">
        <span>What starts it</span>
        <div class="qp-seg qp-sched-kinds" role="radiogroup" aria-label="What starts it">
          <button v-for="k in TRIGGERS" :key="k.id" type="button" class="qp-seg-btn" role="radio" :class="{ on: s.trigger.kind === k.id }" :aria-checked="s.trigger.kind === k.id" @click="setTrigger(k.id)">{{ k.label }}</button>
        </div>
      </div>

      <div v-if="s.trigger.kind === 'github'" class="qp-sched-field">
        <span>Repository <em>(owner/name; read through the gh command, once a minute)</em></span>
        <input v-model="s.trigger.repo" class="qp-input" placeholder="owner/name" spellcheck="false" />
        <span>Events</span>
        <div class="qp-sched-events">
          <label v-for="e in GITHUB_EVENTS" :key="e.id" class="qp-check"><input type="checkbox" :checked="s.trigger.events.includes(e.id)" @change="toggleEvent(e.id)" /> {{ e.label }}</label>
        </div>
        <template v-if="s.trigger.events.includes('push')">
          <span>Branch <em>(empty = the default branch)</em></span>
          <input v-model="s.trigger.branch" class="qp-input" placeholder="main" spellcheck="false" />
        </template>
        <p class="qp-sched-hint">What exists when it starts does not count. It runs while QuantSuite is open, and at most 10 times an hour.</p>
      </div>
      <div v-else-if="s.trigger.kind === 'webhook'" class="qp-sched-field">
        <span>Webhook</span>
        <PilotSchedulesListener :schedule-id="s.id" />
        <p class="qp-sched-hint">It runs at most 10 times an hour, and a repeated delivery runs once.</p>
      </div>
      <p v-if="triggered && !exposed" class="qp-sched-hint">In Plan mode the agent can only read and propose, which is the safe choice here: the event's text comes from outside and it goes to the agent marked as data.</p>
      <p v-if="exposed" class="qp-sched-warn" role="alert">The event's text comes from outside, and the agent reads it. Outside Plan mode, anyone who can write that text could try to make your agent run commands. Your own permission rules also apply.</p>

      <div v-if="!triggered" class="qp-sched-field">
        <span>When <em>(this computer's local time)</em></span>
        <div class="qp-seg qp-sched-kinds" role="radiogroup" aria-label="Repeats">
          <button v-for="k in KINDS" :key="k.id" type="button" class="qp-seg-btn" role="radio" :class="{ on: s.recurrence.kind === k.id }" :aria-checked="s.recurrence.kind === k.id" @click="setKind(k.id)">{{ k.label }}</button>
        </div>
        <div class="qp-sched-when">
          <template v-if="s.recurrence.kind === 'once'">
            <input :value="s.recurrence.at.slice(0, 10)" type="date" class="qp-input" aria-label="Date" @change="setStamp('at', ($event.target as HTMLInputElement).value, null)" />
            <span>at</span>
            <PilotSchedulesTimeField :model-value="s.recurrence.at.slice(11, 16)" label="Time" @update:model-value="setStamp('at', null, $event)" />
          </template>
          <template v-else-if="s.recurrence.kind === 'hours'">
            <span>Every</span>
            <input v-model.number="s.recurrence.every" type="number" min="1" max="168" class="qp-input qp-sched-num" aria-label="Hours" />
            <span>hours, starting</span>
            <input :value="s.recurrence.from.slice(0, 10)" type="date" class="qp-input" aria-label="Starting on" @change="setStamp('from', ($event.target as HTMLInputElement).value, null)" />
            <span>at</span>
            <PilotSchedulesTimeField :model-value="s.recurrence.from.slice(11, 16)" label="Starting at" @update:model-value="setStamp('from', null, $event)" />
          </template>
          <template v-else>
            <div v-if="s.recurrence.kind === 'days'" class="qp-sched-days" role="group" aria-label="Days">
              <button
                v-for="d in DAYS"
                :key="d.n"
                type="button"
                class="qp-sched-day"
                :class="{ on: s.recurrence.days.includes(d.n) }"
                :aria-pressed="s.recurrence.days.includes(d.n)"
                @click="toggleDay(d.n)"
              >
                {{ d.short }}
              </button>
            </div>
            <span>at</span>
            <PilotSchedulesTimeField v-model="s.recurrence.time" label="Time" />
          </template>
        </div>
      </div>

      <div class="qp-sched-grid qp-sched-grid-2">
        <fieldset class="qp-sched-field">
          <legend>Each run</legend>
          <label class="qp-radio"><input v-model="s.target" type="radio" value="new" /> Starts a new thread</label>
          <label class="qp-radio"><input v-model="s.target" type="radio" value="same" /> Continues the same thread</label>
        </fieldset>
        <fieldset v-if="!triggered" class="qp-sched-field">
          <legend>If QuantSuite was closed at that time</legend>
          <label class="qp-radio"><input v-model="s.missed" type="radio" value="skip" /> Skip it</label>
          <label class="qp-radio"><input v-model="s.missed" type="radio" value="once" /> Run it once when QuantSuite starts</label>
        </fieldset>
      </div>
      <p class="qp-sched-hint">A run is skipped while the previous one is still working. Schedules run only while QuantSuite is open.</p>
    </div>

    <footer class="qp-sched-foot">
      <label class="qp-check"><input v-model="s.enabled" type="checkbox" /> On</label>
      <span v-if="schedules.error" class="qp-sched-err" role="alert">{{ schedules.error }}</span>
      <span class="qp-sched-spacer" />
      <button type="button" class="qp-btn" @click="schedules.editing = null">Cancel</button>
      <button type="submit" class="qp-btn qp-btn-primary" :disabled="schedules.busy || !s.prompt.trim()">{{ schedules.busy ? 'Saving…' : 'Save' }}</button>
    </footer>
  </form>
</template>

<style scoped>
.qp-sched-form { flex: 1; min-height: 0; display: flex; flex-direction: column; }
.qp-sched-form-head { display: flex; align-items: baseline; gap: 12px; padding: 16px 20px 10px; }
.qp-sched-form-head h2 { margin: 0; font-size: 15px; font-weight: 600; }
.qp-sched-summary { color: var(--qss-text-secondary, #9a9aa5); font-size: 12px; }
.qp-sched-body { flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 14px; padding: 4px 20px 16px; }
.qp-sched-field { display: flex; flex-direction: column; gap: 5px; min-width: 0; margin: 0; padding: 0; border: none; }
.qp-sched-field > span,
.qp-sched-field > legend { padding: 0; font-size: 11.5px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-sched-field em { font-style: normal; color: var(--qss-text-muted, #6e6e7a); }
.qp-sched-grow { flex: 1 0 auto; }
.qp-sched-prompt { min-height: 150px; height: 26vh; resize: vertical; line-height: 1.5; }
.qp-sched-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 12px; }
.qp-sched-grid-2 { grid-template-columns: repeat(2, minmax(0, 1fr)); }
.qp-input {
  min-width: 0;
  background: var(--qss-bg, #18181e);
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 12.5px;
  padding: 6px 9px;
  outline: none;
}
.qp-input:focus { border-color: var(--qss-accent, #a0a0a8); }
.qp-input::placeholder { color: var(--qss-text-muted, #6e6e7a); }
.qp-input[type='date'] { color-scheme: dark; }
.qp-sched-agent { display: flex; align-items: center; gap: 7px; text-align: left; cursor: pointer; }
.qp-sched-agent-text { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-seg { display: inline-flex; border: 1px solid var(--qss-border, #47474f); border-radius: 7px; overflow: hidden; }
.qp-seg-btn { padding: 5px 12px; border: none; background: transparent; color: var(--qss-text-secondary, #9a9aa5); font: inherit; font-size: 12px; cursor: pointer; }
.qp-seg-btn + .qp-seg-btn { border-left: 1px solid var(--qss-border-subtle, #35353d); }
.qp-seg-btn.on { background: var(--qss-bg-card, #292930); color: var(--qss-text, #d4d4d8); }
.qp-sched-kinds { align-self: flex-start; }
.qp-sched-when { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; font-size: 12.5px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-sched-num { width: 64px; }
.qp-sched-days { display: inline-flex; gap: 4px; }
.qp-sched-day { min-width: 38px; height: 28px; padding: 0 6px; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; background: transparent; color: var(--qss-text-secondary, #9a9aa5); font: inherit; font-size: 12px; cursor: pointer; }
.qp-sched-day.on { border-color: var(--qss-text, #d4d4d8); background: var(--qss-text, #d4d4d8); color: var(--qss-bg, #18181e); font-weight: 600; }
.qp-radio { display: flex; align-items: center; gap: 7px; font-size: 12.5px; cursor: pointer; }
.qp-check { display: flex; align-items: center; gap: 7px; font-size: 12.5px; cursor: pointer; }
.qp-sched-events { display: flex; flex-wrap: wrap; gap: 6px 14px; }
.qp-sched-warn { margin: -4px 0 0; font-size: 12px; line-height: 1.45; color: var(--qss-warning, #fbbf24); }
.qp-sched-hint { margin: -4px 0 0; font-size: 11.5px; line-height: 1.45; color: var(--qss-text-muted, #6e6e7a); }
.qp-sched-foot { display: flex; align-items: center; gap: 10px; padding: 12px 20px; border-top: 1px solid var(--qss-border-subtle, #35353d); }
.qp-sched-spacer { flex: 1; }
.qp-sched-err { min-width: 0; color: var(--qss-error, #f87171); font-size: 12px; }
</style>
