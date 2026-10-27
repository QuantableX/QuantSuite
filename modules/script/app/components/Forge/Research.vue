<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { useForgeStore } from '#script/stores/forge'

interface Run { id: string; title: string; kind: string; status: string; created_at: string; indicators: string[]; error?: string; config?: Record<string, unknown>; path?: string }
interface Verdict { score?: number; grade?: string; certified?: boolean; report?: string }
interface Row { id?: string; key?: string; name: string; role?: string; size?: number; error?: string; members?: string[]; params?: unknown; parameters?: unknown; metrics?: Record<string, number | null>; certification?: Verdict; timeframes?: Record<string, Verdict | null>; equity?: [string, number][]; selected?: Record<string, Verdict | null>; overall?: { selected: Verdict } }
// coverage: a list per size (first-generation profit runs) or the report's coverage object.
interface Results { run: Run; rows: Row[]; total: number; coverage?: unknown; validation?: string }

const forge = useForgeStore()
const runs = ref<Run[]>([])
const selected = ref('')
const result = ref<Results | null>(null)
const html = ref('')
const showingReport = ref(false)
const busy = ref(false)
const error = ref('')
const notice = ref('')
const search = ref('')
const offset = ref(0)
const expanded = ref('')
const track = ref('1d')
const showForm = ref(false)
const title = ref('Trend indicator research')
const kind = ref('gauntlet')
const keys = ref('')
const sizes = ref('')
const budget = ref(50000)
const top = ref(10)
const reference = ref('')
watch(kind, value => { budget.value = value === 'aggregates' ? 5000000 : 50000 })
const systems = ref<{ id: string; name: string }[]>([])
const systemId = ref('')
const startDate = ref('')
const endDate = ref('')
let timer: ReturnType<typeof setInterval> | undefined
let generation = 0
const active = computed(() => runs.value.find(r => r.id === selected.value))
const filteredRuns = computed(() => runs.value.filter(r => r.title.toLowerCase().includes(search.value.toLowerCase())))
const rows = computed(() => result.value?.rows ?? [])
const selectedCurve = computed(() => rows.value.find(r => (r.id || r.key) === expanded.value) ?? rows.value.find(r => r.equity?.length))
const chart = computed(() => {
  const values = selectedCurve.value?.equity?.map(p => p[1]).filter(Number.isFinite) ?? []
  if (values.length < 2) return ''
  const logged = values.map(v => Math.log(Math.max(v, 1e-10)))
  const low = Math.min(...logged), high = Math.max(...logged)
  return logged.map((v, i) => `${20 + i / (values.length - 1) * 920},${175 - (v - low) / (high - low || 1) * 155}`).join(' ')
})
const KIND_LABEL: Record<string, string> = { import: 'Historical report', profit: 'System comparison', aggregates: 'Aggregate comparison', gauntlet: 'Gauntlet', gauntlet_overview: 'Gauntlet overview' }
// Gauntlet runs list registry rows; the overview lists report rows (selected version per track).
const verdict = (row: Row, tf: string) => row.timeframes?.[tf] ?? row.selected?.[tf] ?? null
const overallOf = (row: Row) => row.certification ?? row.overall?.selected ?? null
const coverageRows = computed(() => {
  const coverage = result.value?.coverage
  if (Array.isArray(coverage)) return coverage.map((c: { size: number; tested: number; exhaustive: boolean }) => `${c.size} members: ${c.tested} tested · ${c.exhaustive ? 'exhaustive' : 'sampled'}`)
  const bySize = (coverage as { by_size?: Record<string, number> } | undefined)?.by_size
  return bySize ? Object.entries(bySize).map(([size, n]) => `${size === '1' ? 'Singles' : `${size} members`}: ${n.toLocaleString('en-US')} tested`) : []
})
const format = (value: unknown, digits = 2) => typeof value === 'number' && Number.isFinite(value) ? value.toLocaleString('en-US', { maximumFractionDigits: digits }) : '—'
const parseKeys = (text: string) => [...new Set(text.split(/[\s,]+/).filter(Boolean))]

async function guard(work: () => Promise<void>) {
  busy.value = true; error.value = ''; notice.value = ''
  try { await work() } catch (e) { error.value = String(e) } finally { busy.value = false }
}
async function refresh() { runs.value = await invoke<Run[]>('plugin:script|research_list') }
async function load() {
  if (!selected.value) return
  const id = selected.value, token = ++generation
  const data = await invoke<Results>('plugin:script|research_read', { run_id: id, offset: offset.value, limit: 50, compact: false })
  if (token !== generation) return
  result.value = data
  html.value = ''
  if (data.run.kind === 'import' || (showingReport.value && data.run.status === 'done')) {
    const report = await invoke<{ html: string }>('plugin:script|research_read', { run_id: id, section: 'html' })
    if (token !== generation) return
    // Opaque sandbox origin, no network or desktop bridge. Preserve the original
    // offline interaction code while denying remote resources and forms.
    // Generated reports follow the app's dark theme; imported ones ignore it.
    // (<\/script> keeps the SFC parser from ending this block.)
    html.value = `<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; font-src data:; connect-src 'none'; form-action 'none'; base-uri 'none'"><script>document.documentElement.dataset.theme='dark'<\/script>${report.html}`
  }
}
// A finished run opens on its English report; the results table is one click away.
async function select(id: string) { selected.value = id; showingReport.value = true; offset.value = 0; expanded.value = ''; await guard(load) }
async function importReport() {
  const source = await open({ multiple: false, filters: [{ name: 'Research reports', extensions: ['html', 'htm'] }] })
  if (typeof source !== 'string') return
  await guard(async () => {
    const run = await invoke<Run>('plugin:script|research_import', { source })
    await refresh(); selected.value = run.id; await load()
  })
}
async function loadSystem() {
  if (!systemId.value) return
  await guard(async () => {
    const config = await invoke<Record<string, unknown>>('plugin:systems|get_system_config', { systemId: systemId.value })
    startDate.value = String(config.startDate ?? '')
    // Only closed history is compared: a saved end of today becomes yesterday.
    const yesterday = new Date(Date.now() - 86400000).toISOString().slice(0, 10)
    endDate.value = String(config.endDate ?? '') >= yesterday ? yesterday : String(config.endDate ?? '')
  })
}
// The report tools create the run and start it in one call, like their MCP tools.
async function startReport(command: string, args: Record<string, unknown>) {
  const started = await invoke<{ run: Run }>(command, { title: title.value, indicators: parseKeys(keys.value), ...args })
  await refresh(); selected.value = started.run.id; showForm.value = false; showingReport.value = true; offset.value = 0; await load(); await forge.resume()
}
async function create() {
  await guard(async () => {
    if (kind.value === 'gauntlet_overview') return startReport('plugin:algo|research_gauntlet_overview', {})
    if (kind.value === 'profit' || kind.value === 'aggregates') {
      if (!systemId.value) throw new Error('Choose the saved system to compare.')
      const common = { system: systemId.value, start_date: startDate.value, end_date: endDate.value, top: top.value, budget: budget.value,
        sizes: sizes.value.trim() ? parseKeys(sizes.value).map(Number) : null }
      return kind.value === 'profit'
        ? startReport('plugin:algo|research_profit_overview', common)
        : startReport('plugin:algo|research_aggregate_comparison', { ...common, reference: reference.value.trim() ? parseKeys(reference.value) : null })
    }
    const request = { title: title.value, kind: kind.value, indicators: parseKeys(keys.value) }
    const run = await invoke<Run>('plugin:script|research_create', { request })
    await refresh(); selected.value = run.id; showForm.value = false; offset.value = 0; await load()
  })
}
async function start() {
  await guard(async () => {
    await invoke('plugin:algo|research_run', { run_id: selected.value })
    await refresh(); await load(); await forge.resume()
  })
}
async function install(row: Row) {
  await guard(async () => {
    await invoke('plugin:script|research_install', { run_id: selected.value, indicator: row.key, timeframe: track.value })
    await invoke('plugin:algo|list_indicators', { refresh: true })
    const strategy = await invoke<{ name: string }>('plugin:algo|create_strategy_from_indicator', { indicator: row.key })
    notice.value = `Created ${strategy.name} in QuantAlgo.`
    await forge.loadRegistry(true)
  })
}
async function copyPrompt() {
  await guard(async () => {
    await navigator.clipboard.writeText('Create more distinct trend indicators for my private QuantScript library. Start with quantsuite.script.research_context, stage candidates in a new research run, run the full native Gauntlet on 1h/4h/1d, and show the results in QuantScript → Forge → Research. Keep failures and do not change my live bots. Compare profitability with my saved system when requested, including variable-size aggregates of 2+ distinct indicators.')
    notice.value = 'AI prompt copied. Use it with an AI connected to QuantMCP.'
  })
}
async function page(delta: number) { offset.value = Math.max(0, offset.value + delta); await guard(load) }
onMounted(async () => {
  await guard(async () => {
    await refresh()
    systems.value = await invoke('plugin:systems|list_systems')
    if (runs.value[0]) { selected.value = runs.value[0].id; showingReport.value = true; await load() }
  })
  timer = setInterval(async () => {
    if (busy.value || !runs.value.some(r => r.status === 'running')) return
    try { await refresh(); if (active.value?.kind !== 'import') await load() } catch (e) { error.value = String(e) }
  }, 4000)
})
onUnmounted(() => { clearInterval(timer); generation++ })
</script>

<template>
  <section class="research">
    <div class="research-heading">
      <div><h2>From an idea to tested evidence</h2><p>Private indicators, full Gauntlet results, and system comparisons in one place.</p></div>
      <div class="research-actions">
        <button class="qsc-btn" :disabled="busy" @click="copyPrompt">Copy AI prompt</button>
        <button class="qsc-btn" :disabled="busy" @click="importReport">Import report</button>
        <button class="qsc-btn is-primary" :disabled="busy" @click="showForm = !showForm">New research</button>
      </div>
    </div>
    <p v-if="error" class="qsc-note is-error" role="alert">{{ error }}</p>
    <p v-if="notice" class="qsc-note" role="status">{{ notice }}</p>
    <form v-if="showForm" class="research-form" @submit.prevent="create">
      <label>Research title<input v-model="title" required maxlength="160"></label>
      <label>Evaluation<select v-model="kind"><option value="gauntlet">Full Gauntlet + optimization (new candidates)</option><option value="gauntlet_overview">Gauntlet overview of the library (no re-test)</option><option value="profit">System profitability + aggregates</option><option value="aggregates">Aggregate comparison (any size vs. the current aggregate)</option></select></label>
      <label class="wide">Indicator keys<textarea v-model="keys" rows="2" :placeholder="kind === 'gauntlet_overview' ? 'Comma-separated base keys; leave empty for the whole library' : kind === 'profit' || kind === 'aggregates' ? 'Comma-separated keys; leave empty for every indicator in the version for the system\'s timeframe' : 'Comma-separated keys; leave empty for AI-authored Gauntlet candidates'" /></label>
      <template v-if="kind === 'profit' || kind === 'aggregates'">
        <label>Saved system<select v-model="systemId" required @change="loadSystem"><option value="" disabled>Select baseline</option><option v-for="system in systems" :key="system.id" :value="system.id">{{ system.name }}</option></select></label>
        <label>Aggregate sizes<input v-model="sizes" placeholder="All sizes from 2 upward"></label>
        <label>Start<input v-model="startDate" type="date" required></label>
        <label>End (before today)<input v-model="endDate" type="date" required></label>
        <label>Aggregates to test<input v-model.number="budget" type="number" min="0" max="5000000" step="1000"></label>
        <label>{{ kind === 'aggregates' ? 'Aggregates shown' : 'Shown per group' }}<input v-model.number="top" type="number" min="1" max="50"></label>
        <label v-if="kind === 'aggregates'" class="wide">Aggregate to beat<input v-model="reference" placeholder="Comma-separated keys; leave empty for the system's own aggregate"></label>
        <p v-if="kind === 'aggregates'" class="muted">Searches aggregates of any size (general and 1d versions only) against the current one: all pairs, growth by size from the best 200 of each size, your saved systems and earlier leaders as starting points, then local search until no single add, remove or swap improves the top 25. The top 10 and the best small sizes are re-run natively, and the leader is restarted from 2023 and 2025.</p>
        <p v-else class="muted">Every candidate alone, then aggregates of 2+ different indicators: small sizes in full where they fit, larger ones sampled, then local search around the leaders. The system's universe, market filter and costs stay unchanged; every candidate shown is re-run with the native engine. Large libraries take a while.</p>
      </template>
      <p v-else-if="kind === 'gauntlet_overview'" class="muted">Compiles the existing native Gauntlet evidence of the library: Standard and selected versions on 1h, 4h and 1d, the five axes, walk-forward and parameters. Nothing is re-tested; it runs as a forge job of about a minute.</p>
      <p v-else class="muted">All three tracks · 120 permutations · 200 bootstraps · 100 GARCH paths · four walk-forward folds. Failed candidates remain visible.</p>
      <button class="qsc-btn is-primary" :disabled="busy || (kind !== 'gauntlet' && forge.isRunning)">{{ kind === 'gauntlet' ? 'Create private draft' : 'Create report' }}</button>
    </form>
    <div class="research-workspace">
      <aside class="research-runs">
        <div class="research-actions"><input v-model="search" aria-label="Find research" placeholder="Find research…"><button class="qsc-btn is-sm" :disabled="busy" @click="guard(refresh)">Refresh</button></div>
        <button v-for="run in filteredRuns" :key="run.id" class="research-run" :class="{ selected: selected === run.id }" :disabled="busy" @click="select(run.id)">
          <strong>{{ run.title }}</strong><span>{{ KIND_LABEL[run.kind] ?? run.kind }} · {{ run.status }}</span><small>{{ new Date(run.created_at).toLocaleDateString('en-US') }}</small>
        </button>
        <p v-if="!runs.length" class="muted">Import an existing report or create a research run. The AI can add new candidates through MCP.</p>
      </aside>
      <div class="research-detail">
        <template v-if="active">
          <div class="research-heading"><div><h3>{{ active.title }}</h3><p>{{ KIND_LABEL[active.kind] ?? active.kind }} · {{ active.status }} · {{ active.kind === 'gauntlet_overview' && !active.indicators.length ? 'whole library' : `${active.indicators.length} selected indicators` }}</p></div>
            <button v-if="active.kind !== 'import' && active.status === 'done'" class="qsc-btn" :disabled="busy" @click="guard(async () => { showingReport = !showingReport; await load() })">{{ showingReport ? 'Results table' : 'English HTML report' }}</button>
            <button v-if="active.status === 'draft'" class="qsc-btn is-primary" :disabled="busy || forge.isRunning || !active.indicators.length" @click="start">Run evaluation</button>
            <button v-if="active.status === 'running'" class="qsc-btn" :disabled="busy" @click="guard(async () => { await invoke('plugin:algo|smithery_cancel'); await refresh(); await load() })">Cancel evaluation</button>
          </div>
          <p v-if="active.error" class="qsc-note is-error">{{ active.error }}</p>
          <p v-if="active.status === 'draft'" class="muted">A private library copy is ready. AI candidates can be added to this draft before evaluation.</p>
          <p v-if="active.status === 'running'" class="qsc-note">Evaluation is running. Progress is available in Run tests. Results are saved here as they become available.</p>
          <iframe v-if="html" class="research-report" :srcdoc="html" sandbox="allow-scripts allow-downloads" title="Research report" />
          <template v-else-if="rows.length">
            <p v-if="result?.validation" class="muted">{{ result.validation }}</p>
            <div v-if="chart" class="research-chart"><strong>{{ selectedCurve?.name }}</strong><span>Capital · logarithmic scale</span><svg viewBox="0 0 960 200" role="img" aria-label="Historical equity curve"><polyline :points="chart" fill="none" stroke="currentColor" stroke-width="2" /></svg></div>
            <div class="research-actions" v-if="active.kind === 'gauntlet'"><label>Target track <select v-model="track"><option>1h</option><option>4h</option><option>1d</option></select></label><span class="muted">Overall pass requires every track. Strategy creation uses the selected track.</span></div>
            <div class="research-table"><table><thead><tr><th>Indicator / candidate</th><template v-if="active.kind === 'profit'"><th>Size</th><th>Capital ×</th><th>Max DD %</th><th>Sharpe</th></template><template v-else><th>Version</th><th>1H</th><th>4H</th><th>1D</th><th>Overall</th></template><th /></tr></thead>
              <tbody><template v-for="row in rows" :key="row.id || row.key"><tr>
                <td><button class="research-name" @click="expanded = expanded === (row.id || row.key) ? '' : (row.id || row.key || '')">{{ row.name }}</button><small v-if="row.error" class="research-error">{{ row.error }}</small></td>
                <template v-if="active.kind === 'profit'"><td>{{ row.size || 'Baseline' }}</td><td>{{ format(row.metrics?.net_return_multiplier) }}</td><td>{{ format(row.metrics?.max_drawdown_pct) }}</td><td>{{ format(row.metrics?.sharpe) }}</td></template>
                <template v-else><td>{{ row.role ?? 'Selected per track' }}</td><td v-for="tf in ['1h','4h','1d']" :key="tf">{{ format(verdict(row, tf)?.score, 1) }} <span :class="{ pass: verdict(row, tf)?.certified }">{{ verdict(row, tf)?.certified ? 'Pass' : 'Uncertified' }}</span></td><td>{{ format(overallOf(row)?.score, 1) }} <span :class="{ pass: overallOf(row)?.certified }">{{ overallOf(row)?.certified ? 'Pass' : 'Uncertified' }}</span></td></template>
                <td><button v-if="active.kind === 'gauntlet'" class="qsc-btn is-sm" :disabled="busy || active.status !== 'done' || !row.timeframes?.[track]?.certified" @click="install(row)">Create strategy</button></td>
              </tr><tr v-if="expanded === (row.id || row.key)"><td :colspan="active.kind === 'profit' ? 6 : 7"><pre>{{ JSON.stringify(row.parameters || row.params || row, null, 2) }}</pre></td></tr></template></tbody></table></div>
            <div class="research-actions"><button class="qsc-btn is-sm" :disabled="busy || offset === 0" @click="page(-50)">Previous</button><span>{{ offset + 1 }}–{{ Math.min(offset + 50, result?.total || 0) }} / {{ result?.total }}</span><button class="qsc-btn is-sm" :disabled="busy || offset + 50 >= (result?.total || 0)" @click="page(50)">Next</button></div>
            <details v-if="coverageRows.length"><summary>Aggregate search coverage</summary><div class="research-coverage"><span v-for="line in coverageRows" :key="line">{{ line }}</span></div></details>
          </template>
        </template>
        <div v-else class="research-empty"><h3>Your research, kept together</h3><p>Select a run to inspect its evidence and comparisons.</p></div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.research { display: flex; flex-direction: column; gap: 18px; min-height: 560px; }
.research-heading, .research-actions { display: flex; align-items: center; justify-content: space-between; gap: 10px; flex-wrap: wrap; }
.research h2 { font-size: 19px; margin: 0 0 8px; font-weight: 550; }
.research h3 { margin: 0 0 6px; font-size: 16px; }
.research p { margin: 4px 0; color: var(--qss-text-muted); font-size: 12px; line-height: 1.6; }
.research input, .research select, .research textarea { background: var(--qss-bg); color: var(--qss-text); border: 1px solid var(--qss-border); border-radius: 5px; padding: 9px; min-width: 0; }
.research-form { padding: 18px; display: grid; grid-template-columns: 1fr 1fr; gap: 14px; background: var(--qss-bg-raised); border: 1px solid var(--qss-border); border-radius: 8px; }
.research-form label { display: flex; flex-direction: column; gap: 6px; font-size: 12px; }
.research-form .wide { grid-column: 1 / -1; }
.research-workspace { display: grid; grid-template-columns: 235px minmax(0, 1fr); border: 1px solid var(--qss-border); border-radius: 8px; min-height: 480px; overflow: hidden; }
.research-runs { padding: 12px; border-right: 1px solid var(--qss-border); display: flex; flex-direction: column; gap: 8px; max-height: 75vh; overflow-y: auto; }
.research-runs input { width: 100%; }
.research-run { text-align: left; background: transparent; color: var(--qss-text); border: 1px solid transparent; border-radius: 6px; padding: 12px; cursor: pointer; display: flex; flex-direction: column; gap: 7px; overflow-wrap: anywhere; }
.research-run.selected { border-color: var(--qss-border); background: var(--qss-bg-raised); }
.research-run strong { font-size: 12px; font-weight: 550; }
.research-run span, .research-run small { color: var(--qss-text-muted); font-size: 10px; }
.research-detail { min-width: 0; padding: 18px; display: flex; flex-direction: column; gap: 16px; }
.research-report { width: 100%; height: 75vh; min-height: 540px; border: 0; border-radius: 6px; background: #111110; }
.research-table { overflow: auto; }
.research table { border-collapse: collapse; width: 100%; font-size: 11px; text-align: left; }
.research th, .research td { padding: 12px 9px; border-bottom: 1px solid var(--qss-border); }
.research th { color: var(--qss-text-muted); font-weight: 500; white-space: nowrap; }
.research td span { display: block; color: var(--qss-text-muted); font-size: 9px; }
.research td .pass { color: #65cda5; }
.research-name { background: none; border: none; color: inherit; padding: 0; text-align: left; cursor: pointer; }
.research pre { max-height: 300px; overflow: auto; font-size: 11px; white-space: pre-wrap; overflow-wrap: anywhere; }
.research-error { display: block; color: #e4a080; }
.research-chart { border: 1px solid var(--qss-border); border-radius: 6px; padding: 14px; color: #65cda5; }
.research-chart strong, .research-chart span { display: block; font-size: 11px; margin-bottom: 5px; }
.research-chart span { color: var(--qss-text-muted); }
.research-chart svg { width: 100%; max-height: 220px; }
.research-coverage { display: flex; gap: 10px; flex-wrap: wrap; font-size: 11px; padding: 12px 0; }
.research-empty { margin: auto; text-align: center; }
@media (max-width: 1050px) { .research-workspace { grid-template-columns: 1fr; } .research-runs { border-right: 0; border-bottom: 1px solid var(--qss-border); max-height: 200px; overflow: auto; } .research-form { grid-template-columns: 1fr; } .research-detail { padding: 12px; } }
</style>
