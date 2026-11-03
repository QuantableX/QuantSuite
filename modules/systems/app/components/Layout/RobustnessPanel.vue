<script setup lang="ts">
/**
 * The active system's robustness verdict (docs/ROBUSTNESS.md): the newest System
 * Gauntlet run for it, its failed gates, and a button that starts one. Runs are
 * research runs of kind 'robustness' (QuantScript's research service); the check
 * itself runs as a forge job, so only one at a time.
 */
import { invoke } from '@tauri-apps/api/core'

const props = defineProps<{ systemId: string }>()

interface Verdict { status: string; grade: string; score: number | null; robust: boolean; failures: string[]; complete: boolean }
interface Run { id: string; kind: string; status: string; created_at: string; finished_at?: string; system?: { id?: string | null; name?: string }; verdict?: Verdict; error?: string; fast?: boolean }

const runs = ref<Run[]>([])
const busy = ref(false)
const error = ref('')
const showGates = ref(false)
const report = ref('')

const mine = computed(() => runs.value.filter(r => r.kind === 'robustness' && r.system?.id === props.systemId))
const running = computed(() => mine.value.find(r => r.status === 'running' || r.status === 'draft'))
const latest = computed(() => mine.value.find(r => r.status === 'done' && r.verdict))
const label = computed(() => {
  const v = latest.value?.verdict
  if (!v) return 'Not checked'
  const grade = `${v.grade}${v.score != null ? ' · ' + Math.round(v.score) : ''}`
  return v.status === 'robust' ? `Robust · ${grade}` : v.status === 'incomplete' ? `Incomplete · ${grade}` : `Not robust · ${grade}`
})
const tone = computed(() => latest.value?.verdict?.status === 'robust' ? 'qs-ok' : latest.value?.verdict ? 'qs-bad' : 'qs-muted')

async function refresh() {
  try { runs.value = await invoke<Run[]>('plugin:script|research_list') } catch (e) { error.value = String(e) }
}

async function check(fast: boolean) {
  busy.value = true
  error.value = ''
  try {
    await invoke('plugin:algo|research_robustness', { system: props.systemId, fast, title: null })
    await refresh()
  } catch (e) { error.value = String(e) } finally { busy.value = false }
}

async function openReport() {
  if (!latest.value) return
  try {
    const data = await invoke<{ html: string }>('plugin:script|research_read', { run_id: latest.value.id, section: 'html' })
    report.value = data.html
  } catch (e) { error.value = String(e) }
}

let timer: ReturnType<typeof setInterval> | null = null
watch(running, value => {
  if (value && !timer) timer = setInterval(refresh, 5000)
  if (!value && timer) { clearInterval(timer); timer = null }
})
watch(() => props.systemId, () => { showGates.value = false; report.value = '' })
onMounted(refresh)
onActivated(refresh)
onBeforeUnmount(() => { if (timer) clearInterval(timer) })
</script>

<template>
  <section class="qs-robust">
    <span class="label">Robustness</span>
    <div class="qs-robust__row">
      <span>Verdict</span>
      <span :class="tone">{{ running ? 'Checking…' : label }}</span>
    </div>
    <div v-if="latest && !running" class="qs-robust__row">
      <span>Checked</span>
      <span class="mono">{{ (latest.finished_at || latest.created_at).slice(0, 10) }}{{ latest.fast ? ' · fast' : '' }}</span>
    </div>
    <button v-if="latest?.verdict?.failures?.length" type="button" class="qs-robust__link" @click="showGates = !showGates">
      {{ showGates ? 'Hide' : 'Show' }} {{ latest.verdict.failures.length }} failed gate{{ latest.verdict.failures.length > 1 ? 's' : '' }}
    </button>
    <ul v-if="showGates && latest?.verdict" class="qs-robust__gates">
      <li v-for="(f, i) in latest.verdict.failures" :key="i">{{ f }}</li>
    </ul>
    <div class="qs-robust__actions">
      <button type="button" class="btn btn-ghost" :disabled="busy || !!running" @click="check(true)">Quick check</button>
      <button type="button" class="btn btn-ghost" :disabled="busy || !!running" @click="check(false)">Full check</button>
      <button v-if="latest" type="button" class="btn btn-ghost" @click="openReport">Report</button>
    </div>
    <p class="qs-robust__hint">Parameters, time, exchange, assets, Monte Carlo and execution. A result is only trusted when it is robust.</p>
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
.qs-robust__row { display: flex; justify-content: space-between; gap: 10px; font-size: 12px; color: var(--qs-text-muted); }
.qs-robust__row > span:last-child { text-align: right; }
.qs-ok { color: var(--qs-success); }
.qs-bad { color: var(--qs-error); }
.qs-muted { color: var(--qs-text-muted); }
.qs-robust__link { all: unset; cursor: pointer; font-size: 12px; color: var(--qs-accent); }
.qs-robust__gates { margin: 0; padding-left: 16px; font-size: 12px; color: var(--qs-text-muted); }
.qs-robust__gates li { margin-bottom: 4px; overflow-wrap: anywhere; }
.qs-robust__actions { display: flex; flex-wrap: wrap; gap: 6px; }
.qs-robust__hint { margin: 0; font-size: 12px; line-height: 1.5; color: var(--qs-text-muted); }
.qs-robust__error { margin: 0; font-size: 12px; color: var(--qs-error); overflow-wrap: anywhere; }
.qs-robust__sheet { position: fixed; inset: 40px; z-index: 50; display: flex; flex-direction: column; background: var(--qs-bg-card); border: 1px solid var(--qs-border); border-radius: 10px; overflow: hidden; }
.qs-robust__sheet-bar { display: flex; justify-content: space-between; align-items: center; padding: 8px 12px; font-size: 13px; border-bottom: 1px solid var(--qs-border); }
.qs-robust__sheet iframe { flex: 1; border: 0; width: 100%; background: #fff; }
</style>
