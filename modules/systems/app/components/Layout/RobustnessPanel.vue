<script setup lang="ts">
/**
 * The active system's robustness verdict (docs/ROBUSTNESS.md): the newest System
 * Gauntlet run for it, its failed gates, and buttons that start one. Runs are
 * research runs of kind 'robustness' (QuantScript's research service); the check
 * itself runs as a forge job, so only one at a time.
 *
 * A verdict belongs to a config, not to a name: a run counts when it checked the
 * system's current settings, whatever system it was started from (Gauntlets run
 * during research carry the research name). Settings that cannot change a result
 * (end date, compared indicators, live start, download pacing) are ignored. A
 * full check outranks a newer quick check.
 *
 * While the forge is busy a check waits in the algo module's robustness queue
 * and starts by itself; the panel shows its place and can take it off again.
 */
import { invoke } from '@tauri-apps/api/core'
import { Info } from 'lucide-vue-next'

const props = defineProps<{ systemId: string; config?: object | null }>()

interface Verdict { status: string; grade: string; score: number | null; robust: boolean; failures: string[]; complete: boolean }
interface Run {
  id: string; kind: string; status: string; title?: string; created_at: string; started_at?: string; finished_at?: string
  system?: { id?: string | null; name?: string }; config?: Record<string, unknown>; verdict?: Verdict; error?: string; fast?: boolean
}
interface Job { id: string; kind: string; status: string; request?: { research_id?: string | null } }
interface RunEvent { event: string; at: string; message?: string }
/** A check waiting for the forge (the algo module's robustness queue). */
interface Queued { id: string; system: string | null; fast: boolean; queued_at: string; error: string | null }

const HINT = 'Parameters, time, exchange, assets, Monte Carlo and execution. A result is only trusted when it is robust.'
/** Fields a check's result does not depend on. */
const IGNORED = new Set(['endDate', 'compareTrends', 'liveStartDate', 'minRequestInterval'])

const runs = ref<Run[]>([])
const job = ref<Job | null>(null)
const queue = ref<Queued[]>([])
const progress = ref('')
const busy = ref(false)
const error = ref('')
const showGates = ref(false)
const report = ref('')
const now = ref(Date.now())

function isEmpty(value: unknown): boolean {
  if (value == null || value === false) return true
  if (Array.isArray(value)) return value.length === 0
  return typeof value === 'object' && Object.keys(value as object).length === 0
}

function same(a: unknown, b: unknown): boolean {
  if (a === b) return true
  if (Array.isArray(a) || Array.isArray(b)) {
    return Array.isArray(a) && Array.isArray(b) && a.length === b.length && a.every((v, i) => same(v, b[i]))
  }
  if (a && b && typeof a === 'object' && typeof b === 'object') return sameFields(a as Record<string, unknown>, b as Record<string, unknown>)
  return false
}

function sameFields(a: Record<string, unknown>, b: Record<string, unknown>, ignored?: Set<string>): boolean {
  for (const key of new Set([...Object.keys(a), ...Object.keys(b)])) {
    if (ignored?.has(key)) continue
    // A field one side lacks counts as unset: older saves omit empty fields.
    if (!(key in a) || !(key in b)) {
      if (!isEmpty(a[key]) || !isEmpty(b[key])) return false
      continue
    }
    if (!same(a[key], b[key])) return false
  }
  return true
}

/** The run checked the system's current settings. */
function checksCurrent(run: Run): boolean {
  return !!run.config && !!props.config && sameFields(run.config, props.config as Record<string, unknown>, IGNORED)
}

const checks = computed(() => runs.value.filter(r => r.kind === 'robustness'))
const own = computed(() => checks.value.filter(r => r.system?.id === props.systemId))
const running = computed(() => own.value.find(r => r.status === 'running' || r.status === 'draft'))
const latest = computed(() => {
  const done = checks.value.filter(r => r.status === 'done' && r.verdict && checksCurrent(r))
  return done.find(r => !r.fast) ?? done[0]
})
/** A verdict exists only for settings the system no longer has. */
const outdated = computed(() => latest.value ? undefined : own.value.find(r => r.status === 'done' && r.verdict))
/** The system's newest check failed or stopped after its verdict. */
const failed = computed(() => {
  const last = own.value.find(r => r.status !== 'running' && r.status !== 'draft')
  if (!last || (last.status !== 'failed' && last.status !== 'interrupted')) return undefined
  return !latest.value || last.created_at > latest.value.created_at ? last : undefined
})
/** Another forge job holds the forge: what it is, for the hint. */
const elsewhere = computed(() => {
  const j = job.value
  if (!j || j.status !== 'running' || (running.value && j.request?.research_id === running.value.id)) return ''
  const run = runs.value.find(r => r.id === j.request?.research_id)
  if (!run) return 'another forge job'
  const name = (run.system?.name ?? 'another system').replace(/ /g, ' ') // a name never breaks across lines
  // A run reads done shortly before its forge job closes.
  const ending = run.status !== 'running' && run.status !== 'draft'
  if (run.kind === 'robustness') return ending ? `finishing the check of ${name}` : `checking ${name}`
  return ending ? `finishing ${run.title ?? 'a research run'}` : (run.title ?? 'a research run')
})
const waiting = computed(() => queue.value.filter(c => !c.error))
/** This system's place in the queue, and a queued start that failed. */
const queued = computed(() => waiting.value.find(c => c.system === props.systemId))
const place = computed(() => queued.value ? waiting.value.indexOf(queued.value) + 1 : 0)
const unstarted = computed(() => queue.value.find(c => c.system === props.systemId && c.error))

const label = computed(() => {
  const v = latest.value?.verdict
  if (!v) return 'Not checked'
  const grade = `${v.grade}${v.score != null ? ' · ' + Math.round(v.score) : ''}`
  return v.status === 'robust' ? `Robust · ${grade}` : v.status === 'incomplete' ? `Incomplete · ${grade}` : `Not robust · ${grade}`
})
const tone = computed(() => latest.value?.verdict?.status === 'robust' ? 'qs-ok' : latest.value?.verdict ? 'qs-bad' : 'qs-muted')
const elapsed = computed(() => {
  const start = running.value?.started_at ?? running.value?.created_at
  if (!start) return ''
  const minutes = Math.max(0, Math.floor((now.value - Date.parse(start)) / 60000))
  return minutes < 60 ? `${minutes} min` : `${Math.floor(minutes / 60)} h ${minutes % 60} min`
})
const verdictText = computed(() => {
  if (running.value) return `Checking · ${elapsed.value}`
  if (queued.value) return `Queued · #${place.value}${queued.value.fast ? ' · quick' : ''}`
  return label.value
})
const status = computed(() => {
  if (running.value) return progress.value || 'Starting…'
  if (queued.value) {
    const ahead = place.value - 1
    const before = ahead ? ` and ${ahead} queued check${ahead > 1 ? 's' : ''}` : ''
    return elsewhere.value ? `Starts after ${elsewhere.value}${before}.` : 'Starting…'
  }
  if (unstarted.value) return `Queued check could not start: ${unstarted.value.error}`
  if (elsewhere.value) return `Forge busy: ${elsewhere.value}. A new check waits in the queue.`
  if (failed.value) return `Last check ${failed.value.status}${failed.value.error ? ': ' + failed.value.error : ''}`
  if (outdated.value) return `Settings changed since the ${day(outdated.value).replace(/-/g, '‑')} check.`
  return ''
})

function day(run: Run): string {
  return (run.finished_at || run.created_at).slice(0, 10)
}

async function refresh() {
  try {
    const [list, jobs, waitingChecks] = await Promise.all([
      invoke<Run[]>('plugin:script|research_list'),
      invoke<Job[]>('plugin:algo|smithery_jobs').catch(() => [] as Job[]),
      invoke<Queued[]>('plugin:algo|robustness_queue').catch(() => [] as Queued[]),
    ])
    const before = running.value?.id
    runs.value = list
    job.value = jobs.find(j => j.status === 'running') ?? null
    queue.value = waitingChecks
    now.value = Date.now()
    if (running.value?.id === before) await loadProgress() // a new running check: the watcher below loads it
  } catch (e) { error.value = String(e) }
}

/** The running check's newest progress message. */
async function loadProgress() {
  const run = running.value
  if (!run) { progress.value = ''; return }
  try {
    const data = await invoke<{ events: RunEvent[] }>('plugin:script|research_read', { run_id: run.id, section: 'events' })
    if (running.value?.id === run.id) progress.value = [...data.events].reverse().find(e => e.event === 'progress' && e.message)?.message ?? ''
  } catch (e) { error.value = String(e) }
}

async function check(fast: boolean) {
  busy.value = true
  error.value = ''
  try {
    // A busy forge queues the check; it starts by itself when the forge is free.
    await invoke('plugin:algo|research_robustness', { system: props.systemId, fast, title: null, queue: true })
    await refresh()
  } catch (e) { error.value = String(e) } finally { busy.value = false }
}

async function unqueue(id: string) {
  error.value = ''
  try { queue.value = await invoke<Queued[]>('plugin:algo|robustness_unqueue', { id }) } catch (e) { error.value = String(e) }
}

async function openReport() {
  if (!latest.value) return
  try {
    const data = await invoke<{ html: string }>('plugin:script|research_read', { run_id: latest.value.id, section: 'html' })
    report.value = data.html
  } catch (e) { error.value = String(e) }
}

// Poll while this system's check or any other forge job runs, or checks wait.
let timer: ReturnType<typeof setInterval> | null = null
watch(() => !!running.value || !!elsewhere.value || queue.value.length > 0, active => {
  if (active && !timer) timer = setInterval(refresh, 5000)
  if (!active && timer) { clearInterval(timer); timer = null }
})
watch(() => props.systemId, () => { showGates.value = false; report.value = ''; error.value = '' })
// Another system's running check: its progress at once, not on the next poll.
watch(() => running.value?.id, (id, before) => { if (id !== before) { progress.value = ''; now.value = Date.now(); loadProgress() } })
onMounted(refresh)
onActivated(refresh)
onBeforeUnmount(() => { if (timer) clearInterval(timer) })
</script>

<template>
  <section class="qs-robust">
    <!-- A CSS tooltip: the app's webview shows no native title tooltips. -->
    <div class="qs-robust__head">
      <span class="label">Robustness</span>
      <button type="button" class="qs-robust__info" aria-label="What the check tests" aria-describedby="qs-robust-tip"><Info :size="12" aria-hidden="true" /></button>
    </div>
    <span id="qs-robust-tip" role="tooltip" class="qs-robust__tip">{{ HINT }}</span>
    <div class="qs-robust__row">
      <span>Verdict</span>
      <span :class="running || queued ? 'qs-muted' : tone">{{ verdictText }}</span>
    </div>
    <div v-if="latest && !running && !queued" class="qs-robust__row">
      <span>Checked</span>
      <span class="qs-robust__checked">
        <span class="mono">{{ day(latest) }}{{ latest.fast ? ' · quick' : '' }}</span>
        <button type="button" class="qs-robust__link" @click="openReport">Report</button>
      </span>
    </div>
    <p v-if="status" class="qs-robust__status" :title="status">{{ status }}</p>
    <button v-if="queued || unstarted" type="button" class="qs-robust__link" @click="unqueue((queued ?? unstarted)!.id)">Remove from queue</button>
    <button v-if="latest?.verdict?.failures?.length && !running && !queued" type="button" class="qs-robust__link" @click="showGates = !showGates">
      {{ showGates ? 'Hide' : 'Show' }} {{ latest.verdict.failures.length }} failed gate{{ latest.verdict.failures.length > 1 ? 's' : '' }}
    </button>
    <ul v-if="showGates && latest?.verdict && !running && !queued" class="qs-robust__gates">
      <li v-for="(f, i) in latest.verdict.failures" :key="i">{{ f }}</li>
    </ul>
    <div class="qs-robust__actions">
      <button type="button" class="btn btn-ghost" :disabled="busy || !!running || !!queued" @click="check(true)">Quick check</button>
      <button type="button" class="btn btn-ghost" :disabled="busy || !!running || !!queued" @click="check(false)">Full check</button>
    </div>
    <p v-if="error" class="qs-robust__error">{{ error }}</p>
    <div v-if="report" class="qs-robust__sheet" role="dialog" aria-label="Robustness report">
      <div class="qs-robust__sheet-bar">
        <span>Robustness report</span>
        <button type="button" class="btn btn-ghost" @click="report = ''">Close</button>
      </div>
      <iframe :srcdoc="report" sandbox="allow-scripts" title="Robustness report" />
    </div>
  </section>
</template>

<style scoped>
.qs-robust { display: flex; flex-direction: column; gap: 8px; }
.qs-robust { position: relative; }
.qs-robust__head { display: inline-flex; align-items: center; gap: 4px; align-self: flex-start; margin-bottom: 6px; cursor: help; }
.qs-robust__head .label { margin: 0; }
.qs-robust__info { display: grid; place-items: center; padding: 0; border: 0; background: none; color: var(--qs-text-muted); cursor: help; }
.qs-robust__head:hover .qs-robust__info { color: var(--qs-text); }
/* Shown at once while the label or the icon is hovered, or the icon has focus. */
.qs-robust__tip { position: absolute; top: 22px; left: 0; right: 0; z-index: 10; padding: 8px 10px; border: 1px solid var(--qs-border); border-radius: 8px; background: var(--qs-bg-card); box-shadow: 0 8px 24px #0006; font-size: 11px; line-height: 1.5; color: var(--qs-text-secondary); opacity: 0; visibility: hidden; pointer-events: none; transition: opacity 120ms ease, visibility 120ms; }
.qs-robust__head:hover + .qs-robust__tip, .qs-robust__head:focus-within + .qs-robust__tip { opacity: 1; visibility: visible; }
.qs-robust__row { display: flex; justify-content: space-between; gap: 10px; font-size: 12px; color: var(--qs-text-secondary); }
.qs-robust__row > span:last-child { text-align: right; }
.qs-robust__checked { display: inline-flex; align-items: baseline; gap: 8px; }
.qs-ok { color: var(--qs-success); }
.qs-bad { color: var(--qs-error); }
.qs-muted { color: var(--qs-text-muted); }
.qs-robust__link { all: unset; cursor: pointer; font-size: 12px; color: var(--qs-accent); }
.qs-robust__link:hover { text-decoration: underline; }
.qs-robust__link:focus-visible { outline: 1px solid var(--qs-accent); outline-offset: 2px; border-radius: 2px; }
/* Up to three lines (a failed check's error can be long); the tooltip has all of it. */
.qs-robust__status { display: -webkit-box; -webkit-line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; margin: 0; font-size: 11px; line-height: 1.4; color: var(--qs-text-muted); overflow-wrap: break-word; }
.qs-robust__gates { margin: 0; padding-left: 16px; font-size: 12px; color: var(--qs-text-muted); }
.qs-robust__gates li { margin-bottom: 4px; overflow-wrap: anywhere; }
.qs-robust__actions { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; }
.qs-robust__actions .btn { min-width: 0; height: 28px; padding: 0 6px; font-size: 12px; white-space: nowrap; }
.qs-robust__error { margin: 0; font-size: 12px; color: var(--qs-error); overflow-wrap: anywhere; }
.qs-robust__sheet { position: fixed; inset: 40px; z-index: 50; display: flex; flex-direction: column; background: var(--qs-bg-card); border: 1px solid var(--qs-border); border-radius: 10px; overflow: hidden; }
.qs-robust__sheet-bar { display: flex; justify-content: space-between; align-items: center; padding: 8px 12px; font-size: 13px; border-bottom: 1px solid var(--qs-border); }
.qs-robust__sheet iframe { flex: 1; border: 0; width: 100%; background: #fff; }
</style>
