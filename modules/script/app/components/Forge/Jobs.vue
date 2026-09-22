<script setup lang="ts">
/**
 * The forge at work: what to run (gauntlet, walk-forward, comparison, shelf
 * refresh; which indicators; full or fast Monte Carlo counts), the job in
 * flight with its contract check and five axes per indicator, the verdict,
 * the log — and the session's earlier jobs. The runner is QuantAlgo's
 * crate; this is its console. Moved from QuantAlgo with the forge.
 */
import { useForgeStore } from '#script/stores/forge'
import {
  AXES,
  AXIS_LABELS,
  KIND_LABELS,
  formatElapsed,
  formatP,
  jobError,
  logLinesFor,
  progressFor,
  seriesFor,
} from '#script/utils/forge'
import { formatParams, gradeClass } from '#script/utils/format'
import type { ForgeJob, ForgeKind, ForgeRequest, SmitheryTimeframe } from '#script/types'

const forge = useForgeStore()

// ── The form ──
const kind = ref<ForgeKind>('gauntlet')
const selected = ref<string[]>([])
const fast = ref(false)
const perm = ref(120)
const boot = ref(200)
const garch = ref(100)
const seed = ref(42)
const folds = ref(4)
const timeframe = ref<SmitheryTimeframe>('all')
const showLog = ref(false)
const indicatorSearch = ref('')
const selectedOnly = ref(false)
const visibleRoster = computed(() => roster.value.filter((ind) =>
  (!selectedOnly.value || selected.value.includes(ind.key)) &&
  (!indicatorSearch.value.trim() || `${ind.name} ${ind.key}`.toLowerCase().includes(indicatorSearch.value.trim().toLowerCase())),
))

/** The tracks the shelf can run, `all` first — the score is earned on every track. */
const timeframes = computed<SmitheryTimeframe[]>(() => {
  const known = (forge.info?.supported_timeframes ?? ['1d', '4h', '1h', '1m']).filter(
    (t): t is SmitheryTimeframe => t === '1d' || t === '4h' || t === '1h' || t === '1m',
  )
  const tracks: SmitheryTimeframe[] = known.length ? known : ['1d']
  return kind.value !== 'walkforward' && tracks.length > 1 ? ['all', ...tracks] : tracks
})
const trackHints: Record<SmitheryTimeframe, string> = {
  all: 'every track in turn — 1d, 4h, 1h and 1m; each requires its own data and evidence',
  '1d': 'the daily reference track only',
  '4h': 'the 4h track only — the top-5 on Binance, BTC on Bybit / OKX / KuCoin',
  '1h': 'the 1h track only — seven years of hourly bars in the Monte Carlo ordeals',
  '1m': 'real one-minute candles; full runs retain the calendar horizons and can take much longer',
}

// A walk-forward re-selects on one series: `all` is a gauntlet thing.
watch(kind, (k) => {
  if (k === 'compare') folds.value = 3
  if (k === 'walkforward' && timeframe.value === 'all') timeframe.value = '1d'
  if ((k === 'gauntlet' || k === 'compare') && timeframes.value.includes('all')) timeframe.value = 'all'
})

const kinds: { id: ForgeKind; label: string; hint: string }[] = [
  { id: 'gauntlet', label: 'Gauntlet', hint: 'contract validators, then asset · exchange · parameter · temporal · Monte Carlo' },
  { id: 'walkforward', label: 'Walk-forward', hint: 'optimize growth, drawdown and risk ratios; save a research subversion under the base script' },
  { id: 'compare', label: 'Comparison', hint: 'old vs new · costs · held-out dates · LCES ratio transfer; no automatic promotion' },
  { id: 'refresh', label: 'Refresh shelf', hint: 'fetch real candles for the selected timeframe from the exchanges' },
]

const roster = computed(() =>
  forge.indicators
    .map((ind) => ({
      ...ind,
      certification: timeframe.value === 'all' ? ind.certification : (ind.timeframes?.[timeframe.value] ?? null),
    }))
    .sort((a, b) => {
      const ca = a.certification?.certified ? 1 : 0
      const cb = b.certification?.certified ? 1 : 0
      if (ca !== cb) return cb - ca
      return (b.certification?.score ?? -1) - (a.certification?.score ?? -1)
    }),
)

const needsIndicators = computed(() => kind.value !== 'refresh')
const canRun = computed(
  () => !forge.isRunning && !forge.starting && (!needsIndicators.value || selected.value.length >= (kind.value === 'compare' ? 2 : 1)),
)

// A request handed over by the Roster or the Shelf: the form takes it on.
watch(
  () => forge.draft,
  (draft) => {
    if (!draft) return
    indicatorSearch.value = ''
    selectedOnly.value = draft.indicators.length > 0 && !draft.indicators.some((key) => key === 'all' || key === 'certified')
    kind.value = draft.kind
    selected.value = [...draft.indicators]
    fast.value = draft.fast
    timeframe.value = draft.timeframe ?? (draft.kind === 'walkforward' ? '1d' : 'all')
  },
  { immediate: true },
)

function toggleKey(key: string) {
  selected.value = selected.value.includes(key) ? selected.value.filter((k) => k !== key) : [...selected.value, key]
}

function pickCertified() {
  selected.value = forge.indicators
    .filter((i) => (timeframe.value === 'all' ? i.certification?.certified : i.timeframes?.[timeframe.value]?.certified))
    .map((i) => i.key)
}

function pickBaselines() {
  const keys = ['hilbert', 'rankbreak', 'extremes', 'bocpd', 'ensemble', 'ensemble_original',
    'council2', 'council2_original', 'scale', 'scale_original', 'consensus', 'robust']
  selected.value = keys.filter((key) => forge.indicators.some((i) => i.key === key))
}

function pickAll() {
  selected.value = forge.indicators.map((i) => i.key)
}

function pickNone() {
  selected.value = []
}

async function run() {
  const request: ForgeRequest = {
    kind: kind.value,
    indicators: needsIndicators.value ? [...selected.value] : [],
    fast: kind.value === 'gauntlet' && fast.value,
  }
  if (kind.value === 'gauntlet' && !fast.value) {
    request.perm = perm.value
    request.boot = boot.value
    request.garch = garch.value
    request.seed = seed.value
  }
  if (kind.value === 'walkforward' || kind.value === 'compare') request.folds = folds.value
  request.timeframe = timeframe.value
  await forge.run(request)
}

function jobTrack(j: ForgeJob): string | null {
  const tf = j.request.timeframe ?? (j.kind === 'walkforward' ? '1d' : 'all')
  return tf === '1d' ? null : tf === 'all' ? 'all tracks' : `${tf} track`
}

// ── The job on display ──
const job = computed<ForgeJob | null>(() => forge.detailJob)
const multiTrack = computed(() => job.value?.kind === 'gauntlet' && (job.value.request.timeframe ?? 'all') === 'all')
const rows = computed(() => (job.value && job.value.kind !== 'refresh' ? progressFor(job.value) : []))
const series = computed(() => (job.value?.kind === 'refresh' ? seriesFor(job.value) : []))
const logLines = computed(() => (job.value ? logLinesFor(job.value).slice(-300) : []))
const jobFailure = computed(() => (job.value ? jobError(job.value) : null))
const history = computed(() => [...forge.jobs].reverse())

const doneEvent = computed(() => job.value?.summary.find((e) => e.event === 'done') ?? null)
const comparisonRows = computed(() => job.value?.summary.filter((e) => e.event === 'comparison') ?? [])
const comparisonResult = computed(() => job.value?.summary.find((e) => e.event === 'comparison_done') ?? null)

function statusClass(status: string): string {
  switch (status) {
    case 'running':
      return 'is-running'
    case 'done':
      return 'is-ok'
    case 'failed':
      return 'is-error'
    default:
      return ''
  }
}

function axisText(status: string, score: number | null): string {
  if (status === 'done') return score == null ? '—' : score.toFixed(0)
  if (status === 'running') return '…'
  if (status === 'skipped') return 'skip'
  return ''
}

function jobIndicators(j: ForgeJob): string {
  const named = j.summary.find((e) => e.event === 'job')
  const list = Array.isArray(named?.indicators) ? (named.indicators as string[]) : j.indicators
  return list.length > 6 ? `${list.slice(0, 6).join(', ')} +${list.length - 6}` : list.join(', ')
}

function verdictFor(j: ForgeJob): string {
  const comparison = j.summary.find((e) => e.event === 'comparison_done')
  if (comparison) return `${comparison.winner}: ${comparison.passed ? 'comparison passed' : 'not promoted'}`
  const verdicts = j.summary.filter((e) => e.event === 'verdict')
  if (verdicts.length) {
    const certified = verdicts.filter((e) => e.certified === true).length
    return `${certified} certified of ${verdicts.length}`
  }
  const wf = j.summary.filter((e) => e.event === 'walkforward')
  if (wf.length) return `${wf.length} walk-forward result${wf.length === 1 ? '' : 's'}`
  const ser = j.summary.filter((e) => e.event === 'series')
  if (ser.length) return `${ser.filter((e) => e.ok === true).length} of ${ser.length} series refreshed`
  return ''
}
</script>

<template>
  <div class="qsf-jobs">
    <!-- What to run -->
    <div class="qsc-card qsf-form">
      <div class="qsf-kinds">
        <div class="qsc-tabbar" role="tablist" aria-label="Job">
          <button
            v-for="k in kinds"
            :key="k.id"
            class="qsc-tabbtn"
            :class="{ 'is-active': kind === k.id }"
            role="tab"
            :aria-selected="kind === k.id"
            :title="k.hint"
            @click="kind = k.id"
          >
            {{ k.label }}
          </button>
        </div>
        <span class="muted qsf-hint">{{ kinds.find((k) => k.id === kind)?.hint }}</span>
      </div>

      <div v-if="needsIndicators" class="qsf-pick">
        <div class="qsf-pick-head">
          <span class="qsc-label">Indicators</span>
          <button class="qsc-btn is-sm" @click="pickCertified">Certified</button>
          <button v-if="kind === 'compare'" class="qsc-btn is-sm" @click="pickBaselines">Originals + candidates</button>
          <button class="qsc-btn is-sm" @click="pickAll">All</button>
          <button class="qsc-btn is-sm" @click="pickNone">None</button>
          <span class="muted qsf-count">{{ selected.length }} picked</span>
        </div>
        <div class="qsf-picker-search"><input v-model="indicatorSearch" class="qsc-input" type="search" aria-label="Find indicators for test" placeholder="Find an indicator…" /><label class="qsc-check"><input v-model="selectedOnly" type="checkbox" /> Selected only</label></div>
        <div class="qsf-keys">
          <label
            v-for="ind in visibleRoster"
            :key="ind.key"
            class="qsf-key"
            :class="{ 'is-on': selected.includes(ind.key) }"
            :title="ind.certification?.source === 'historical' ? 'Historical reference; current full run required' : 'Evidence for the selected track'"
          >
            <input type="checkbox" :checked="selected.includes(ind.key)" @change="toggleKey(ind.key)" />
            <span class="qsf-key-name">{{ ind.name }}</span>
            <span class="mono qsf-key-score">{{ ind.certification ? `${+ind.certification.score.toFixed(1)}/${ind.certification.grade}` : '—' }}</span>
          </label>
          <p v-if="!visibleRoster.length" class="muted">No indicators match this selection.</p>
        </div>
      </div>

      <div class="qsf-track">
        <span class="qsc-label">Track</span>
        <div class="qsc-tabbar" role="tablist" aria-label="Certification track">
          <button
            v-for="tf in timeframes"
            :key="tf"
            class="qsc-tabbtn mono qsf-track-btn"
            :class="{ 'is-active': timeframe === tf }"
            role="tab"
            :aria-selected="timeframe === tf"
            :title="trackHints[tf]"
            @click="timeframe = tf"
          >
            {{ tf }}
          </button>
        </div>
        <span class="muted qsf-hint">{{ trackHints[timeframe] }}</span>
      </div>

      <div v-if="kind === 'gauntlet'" class="qsf-mc">
        <label class="qsc-check">
          <input v-model="fast" type="checkbox" />
          <span>Fast — perm 25 / boot 40 / garch 20: a smoke test, never a certification</span>
        </label>
        <details v-if="!fast" class="qsf-advanced"><summary>Advanced settings · Monte Carlo samples & seed</summary><div class="qsf-counts">
          <label><span class="qsc-label">Permutations</span><input v-model.number="perm" class="qsc-input qsf-num" type="number" min="1" max="5000" /></label>
          <label><span class="qsc-label">Bootstraps</span><input v-model.number="boot" class="qsc-input qsf-num" type="number" min="1" max="5000" /></label>
          <label><span class="qsc-label">GARCH charts</span><input v-model.number="garch" class="qsc-input qsf-num" type="number" min="1" max="5000" /></label>
          <label><span class="qsc-label">Seed</span><input v-model.number="seed" class="qsc-input qsf-num" type="number" min="0" /></label>
        </div></details>
      </div>
      <div v-else-if="kind === 'walkforward' || kind === 'compare'" class="qsf-counts">
        <label><span class="qsc-label">{{ kind === 'compare' ? 'Development eras' : 'Folds' }}</span><input v-model.number="folds" class="qsc-input qsf-num" type="number" min="2" max="12" /></label>
      </div>
      <p v-if="kind === 'compare'" class="muted qsf-note">
        At least two frozen candidates. Selection uses earlier dates; the winner is locked before the final 25% is tested.
        Long-only costs: 20 / 40 bps round trip. Includes separate short and LCES ratio diagnostics.
        These histories were used by earlier forge rounds; results still need forward paper validation.
      </p>
      <p v-if="kind === 'refresh'" class="muted qsf-note">
        Incremental from the last cached candle. Binance falls back to OKX, Bybit and KuCoin.
        The first 1m import starts in January 2023 and can take much longer than an hourly refresh.
      </p>

      <div class="qsf-run">
        <button class="qsc-btn is-primary" :disabled="!canRun" @click="run">
          {{ forge.starting ? 'Starting…' : forge.isRunning ? 'Forge busy' : `Run ${KIND_LABELS[kind]}` }}
        </button>
        <button v-if="forge.isRunning" class="qsc-btn is-danger" @click="forge.cancel()">Cancel</button>
        <span v-if="forge.info" class="muted qsf-workers">{{ forge.info.workers }} workers · vault {{ forge.info.root }}</span>
      </div>
      <p v-if="forge.runError" class="qsc-err qsf-error">{{ forge.runError }}</p>
    </div>

    <!-- The job -->
    <div v-if="job" class="qsc-card qsf-job">
      <div class="qsf-job-head">
        <div class="qsf-job-title">
          <span class="qsc-tag" :class="statusClass(job.status)">{{ job.status }}</span>
          <span class="qsf-job-kind">{{ KIND_LABELS[job.kind] ?? job.kind }}</span>
          <span v-if="jobTrack(job)" class="qsc-tag is-accent">{{ jobTrack(job) }}</span>
          <span v-if="job.request.fast" class="qsc-tag">fast — smoke test</span>
          <span class="muted qsf-job-meta">
            started {{ new Date(job.started_at).toLocaleTimeString() }} · {{ formatElapsed(job.started_at, job.finished_at) }}
            <template v-if="doneEvent && typeof doneEvent.elapsed_s === 'number'"> · forge {{ doneEvent.elapsed_s }}s</template>
          </span>
        </div>
        <button class="qsc-btn is-sm" @click="showLog = !showLog">{{ showLog ? 'Hide log' : `Log (${logLines.length})` }}</button>
      </div>
      <p v-if="jobFailure" class="qsc-err qsf-error">{{ jobFailure }}</p>

      <!-- gauntlet -->
      <div v-if="job.kind === 'gauntlet'" class="qsf-table-scroll">
        <table class="qsc-table">
          <thead>
            <tr>
              <th>Indicator</th>
              <th v-if="multiTrack">Track</th>
              <th>Contract</th>
              <th v-for="axis in AXES" :key="axis" class="num">{{ AXIS_LABELS[axis] }}</th>
              <th class="num">Score</th>
              <th>Grade</th>
              <th class="num">Perm. p</th>
              <th>Verdict</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="r in rows" :key="r.key">
              <td>
                <span class="qsf-ind">{{ r.name ?? r.indicator }}</span>
                <span class="mono muted qsf-ind-key">{{ r.indicator }}</span>
              </td>
              <td v-if="multiTrack" class="mono">{{ r.track ?? '' }}</td>
              <td>
                <span v-if="r.contract === 'passed'" class="qsc-ok">Laws 1–2 ✓</span>
                <span v-else-if="r.contract === 'failed'" class="qsc-err" :title="r.contractMessage ?? ''">violation</span>
                <span v-else-if="r.contract === 'running'" class="muted">checking…</span>
                <span v-else-if="r.error" class="qsc-err">failed</span>
                <span v-else class="muted">{{ r.started ? '…' : 'queued' }}</span>
              </td>
              <td v-for="axis in AXES" :key="axis" class="num mono" :class="{ 'qsf-cell-running': r.axes[axis].status === 'running' }">
                {{ axisText(r.axes[axis].status, r.axes[axis].score) }}
              </td>
              <td class="num mono strong">{{ r.verdict ? r.verdict.score.toFixed(0) : '' }}</td>
              <td>
                <span v-if="r.verdict" class="qsc-grade" :class="gradeClass(r.verdict.grade, r.verdict.certified)" :title="r.verdict.grade">{{ r.verdict.grade.charAt(0) }}</span>
              </td>
              <td class="num mono">{{ r.verdict ? formatP(r.verdict.perm_p) : '' }}</td>
              <td class="qsf-verdict">
                <template v-if="r.verdict">
                  <span v-if="job.request.fast" class="qsc-tag" title="Reduced Monte Carlo counts — run the full gauntlet for a verdict">smoke test</span>
                  <span v-else-if="r.verdict.certified" class="qsc-tag is-ok">certified</span>
                  <span v-else class="qsc-tag" :title="r.verdict.reasons.join('\n')">back to the forge</span>
                  <button class="qsc-link mono qsf-report-link" @click="forge.openReport(r.verdict!.report_name)">{{ r.verdict.report_name }}</button>
                </template>
                <span v-else-if="r.error" class="qsc-err" :title="r.error">{{ r.error }}</span>
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <!-- walk-forward -->
      <div v-else-if="job.kind === 'walkforward'" class="qsf-table-scroll">
        <table class="qsc-table">
          <thead>
            <tr>
              <th>Indicator</th>
              <th>Folds (chosen · IS selected Sharpe)</th>
              <th class="num">OOS Sharpe</th>
              <th class="num">IS mean</th>
              <th class="num">WFE</th>
              <th>Plateau-medoid choice</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="r in rows" :key="r.key">
              <td>
                <span class="qsf-ind">{{ r.name ?? r.key }}</span>
                <span class="mono muted qsf-ind-key">{{ r.key }}</span>
              </td>
              <td class="qsf-folds">
                <span v-if="!r.folds.length && !r.error" class="muted">{{ r.started ? '…' : 'queued' }}</span>
                <span v-for="f in r.folds" :key="f.fold" class="mono qsf-fold">
                  {{ f.fold }}: {{ formatParams(f.chosen) }} · {{ f.is_selected_sharpe.toFixed(2) }}
                </span>
                <span v-if="r.error" class="qsc-err">{{ r.error }}</span>
              </td>
              <td class="num mono">{{ r.walkforward ? r.walkforward.oos_sharpe.toFixed(2) : '' }}</td>
              <td class="num mono">{{ r.walkforward ? r.walkforward.is_sharpe_mean.toFixed(2) : '' }}</td>
              <td class="num mono" :class="r.walkforward && (r.walkforward.wfe ?? 0) >= 0.5 ? 'qsc-ok' : ''">
                {{ r.walkforward ? (r.walkforward.wfe == null ? '—' : r.walkforward.wfe.toFixed(2)) : '' }}
              </td>
              <td class="mono qsf-choice">
                <template v-if="r.walkforward">
                  {{ formatParams(r.walkforward.final_choice) }}
                  <span class="muted qsf-choice-note">
                    {{ (r.walkforward.wfe ?? 0) >= 0.5 ? 'WFE ≥ 0.5 — a candidate; re-run the full gauntlet before promoting' : 'WFE < 0.5 — the forged defaults stand' }}
                  </span>
                </template>
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <div v-else-if="job.kind === 'compare'">
        <table class="qsc-table">
          <thead><tr><th>Candidate</th><th class="num">Development objective</th><th>Checks</th></tr></thead>
          <tbody>
            <tr v-for="r in comparisonRows" :key="String(r.indicator)">
              <td>{{ r.name ?? r.indicator }}</td>
              <td class="num mono">{{ typeof r.selection_score === 'number' ? r.selection_score.toFixed(3) : '—' }}</td>
              <td>{{ Array.isArray(r.errors) && r.errors.length ? r.errors.join('; ') : `${r.cells} scenarios` }}</td>
            </tr>
          </tbody>
        </table>
        <p v-if="!comparisonResult" class="muted qsf-note">
          {{ job.status === 'running' ? 'Comparing candidates, then testing the locked winner…' : 'No completed comparison.' }}
        </p>
        <template v-else>
          <p class="qsf-note">Selected: <strong>{{ comparisonResult.winner }}</strong> ·
            {{ comparisonResult.passed ? 'comparison passed; full gauntlet and paper validation required' : 'final checks failed — no promotion' }}
          </p>
          <p v-if="Array.isArray(comparisonResult.reasons)" class="muted qsf-note">{{ comparisonResult.reasons.join(' · ') }}</p>
          <button class="qsc-btn is-sm" @click="forge.openReport(String(comparisonResult.report_name))">Open comparison report</button>
        </template>
      </div>

      <!-- refresh -->
      <table v-else class="qsc-table">
        <thead>
          <tr>
            <th>Series</th>
            <th>Result</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="s in series" :key="s.key">
            <td class="mono">{{ s.key }}</td>
            <td :class="s.ok ? 'qsc-ok' : 'qsc-err'">{{ s.status }}</td>
          </tr>
          <tr v-if="!series.length">
            <td colspan="2" class="muted">{{ job.status === 'running' ? 'Fetching…' : 'Nothing reported.' }}</td>
          </tr>
        </tbody>
      </table>

      <pre v-if="showLog" class="qsc-pre qsf-log">{{ logLines.join('\n') || '(no log lines)' }}</pre>
      <p class="muted qsf-cmd mono">{{ job.command }}</p>
    </div>

    <div v-else class="qsc-card muted qsf-empty">
      No forge job this session yet. Pick indicators above, or start from the Roster.
    </div>

    <!-- Earlier jobs -->
    <div v-if="history.length > 1" class="qsc-card is-flush qsf-history">
      <table class="qsc-table">
        <thead>
          <tr>
            <th>Started</th>
            <th>Job</th>
            <th>Indicators</th>
            <th>Status</th>
            <th>Result</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="j in history" :key="j.id" :class="{ 'is-current': job?.id === j.id }">
            <td class="mono">{{ new Date(j.started_at).toLocaleTimeString() }}</td>
            <td>{{ KIND_LABELS[j.kind] ?? j.kind }}<span v-if="j.request.fast" class="muted"> · fast</span></td>
            <td class="mono qsf-history-ind">{{ jobIndicators(j) }}</td>
            <td><span class="qsc-tag" :class="statusClass(j.status)">{{ j.status }}</span></td>
            <td>{{ verdictFor(j) }}</td>
            <td class="qsf-cell-right">
              <button class="qsc-btn is-sm" :disabled="job?.id === j.id" @click="forge.showJob(j.id)">Show</button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<style scoped>
.qsf-jobs {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.qsf-form {
  display: flex;
  flex-direction: column;
  gap: 14px;
}
.qsf-kinds {
  display: flex;
  align-items: center;
  gap: 14px;
  flex-wrap: wrap;
}
.qsf-hint {
  font-size: 12px;
}
.qsf-pick {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.qsf-pick-head {
  display: flex;
  align-items: center;
  gap: 6px;
}
.qsf-pick-head .qsc-label {
  margin-right: 8px;
}
.qsf-count {
  margin-left: 8px;
  font-size: 12px;
}
.qsf-keys {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
  gap: 2px 12px;
}
.qsf-key {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 6px;
  border-radius: 6px;
  font-size: 12px;
  color: var(--qss-text-secondary);
  cursor: pointer;
}
.qsf-key:hover {
  background: var(--qss-bg-hover);
}
.qsf-key.is-on {
  color: var(--qss-text);
}
.qsf-key input {
  accent-color: var(--qss-accent);
}
.qsf-key-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.qsf-key-score {
  font-size: 11px;
  color: var(--qss-text-muted);
}
.qsf-track {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}
.qsf-track-btn {
  min-width: 44px;
  padding: 5px 10px;
}
.qsf-mc {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.qsf-counts {
  display: flex;
  flex-wrap: wrap;
  gap: 16px;
}
.qsf-counts label {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.qsf-num {
  width: 120px;
}
.qsf-note {
  font-size: 12px;
  line-height: 1.5;
}
.qsf-run {
  display: flex;
  align-items: center;
  gap: 10px;
}
.qsf-workers {
  font-size: 11px;
}
.qsf-error {
  font-size: 12px;
  white-space: pre-wrap;
}
.qsf-job {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.qsf-job-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}
.qsf-job-title {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}
.qsf-job-kind {
  font-size: 13px;
  font-weight: 600;
  color: var(--qss-text);
}
.qsf-job-meta {
  font-size: 12px;
}
.qsf-table-scroll {
  overflow-x: auto;
}
.qsf-ind {
  display: block;
  font-weight: 600;
  color: var(--qss-text);
}
.qsf-ind-key {
  font-size: 11px;
}
.qsf-cell-running {
  color: var(--qss-text);
}
.qsf-verdict {
  white-space: nowrap;
}
.qsf-report-link {
  margin-left: 8px;
  font-size: 11px;
}
.qsf-folds {
  display: flex;
  flex-direction: column;
  gap: 2px;
  font-size: 11px;
}
.qsf-fold {
  color: var(--qss-text-secondary);
}
.qsf-choice {
  font-size: 12px;
}
.qsf-choice-note {
  display: block;
  margin-top: 2px;
  font-size: 11px;
  font-family: var(--qss-font-sans);
}
.qsf-log {
  max-height: 320px;
}
.qsf-cmd {
  font-size: 11px;
  word-break: break-all;
}
.qsf-empty {
  font-size: 13px;
}
.qsf-history-ind {
  font-size: 11px;
}
.qsf-cell-right {
  text-align: right;
}
</style>
