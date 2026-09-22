<script setup lang="ts">
/**
 * The registry as a table: one score per indicator — the worst of its
 * certification tracks, certified only when every track certified it (the
 * user's rule: robust for every timeframe or not robust). The row carries
 * what decides; everything else — the hypothesis, the tracks, the defaults,
 * the parameter space, the smoke test, the walk-forward, the script itself —
 * opens on click. Certified first, then by score. Moved from QuantAlgo with
 * the forge (docs/PLAN-QUANTSCRIPT.md §2).
 */
import { useForgeStore } from '#script/stores/forge'
import { useWorkbenchStore } from '#script/stores/workbench'
import { formatP } from '#script/utils/forge'
import { formatParams, gradeClass } from '#script/utils/format'
import type { IndicatorInfo, SmitheryTimeframe } from '#script/types'

const router = useRouter()
const forge = useForgeStore()
const wb = useWorkbenchStore()

const TRACKS = ['1d', '4h', '1h', '1m'] as const

const expanded = ref<string | null>(null)
const scoreTrack = ref<SmitheryTimeframe>('all')
const onlyCertified = ref(false)
const filter = ref('')

const rows = computed(() => {
  const q = filter.value.trim().toLowerCase()
  return forge.indicators
    .map((ind) => ({
      ...ind,
      certification: scoreTrack.value === 'all' ? ind.certification : (ind.timeframes?.[scoreTrack.value] ?? null),
    }))
    .filter((ind) => !ind.variant)
    .filter((ind) => !onlyCertified.value || ind.certification?.certified || children(ind.key).some(v => v.certification?.certified))
    .filter((ind) => !q || [ind, ...children(ind.key)].some(v => v.key.includes(q) || v.name.toLowerCase().includes(q)))
    .sort((a, b) => {
      const ca = a.certification?.certified ? 1 : 0
      const cb = b.certification?.certified ? 1 : 0
      if (ca !== cb) return cb - ca
      const sa = a.certification?.score ?? -1
      const sb = b.certification?.score ?? -1
      if (sa !== sb) return sb - sa
      return a.name.localeCompare(b.name)
    })
})

function children(key: string): IndicatorInfo[] {
  return forge.indicators.filter(ind => ind.variant && ind.base_key === key)
}

/** The tracks behind the one score, for the score's tooltip. */
function tracksText(ind: IndicatorInfo): string {
  const parts = TRACKS.map((tf) => {
    const v = ind.timeframes?.[tf]
    return `${tf} ${v ? `${v.score}/${v.grade}${v.certified ? '' : ' ✗'}` : 'not run'}`
  })
  const c = ind.certification
  const worst = c?.worst_track ? ` — the score is the worst track (${c.worst_track})` : ''
  return `${parts.join(' · ')}${worst}`
}

function statusText(ind: IndicatorInfo): string {
  const c = ind.certification
  if (!c) return 'untested'
  if (c.certified) return c.source === 'historical' ? 'historical pass' : 'certified'
  if (scoreTrack.value === 'all' && (c.tracks_run ?? 0) < (c.tracks ?? TRACKS.length)) return `${c.tracks_run ?? 0} of ${c.tracks ?? TRACKS.length} tracks`
  return 'forge'
}

function toggle(key: string) {
  expanded.value = expanded.value === key ? null : key
}

function gauntlet(ind: IndicatorInfo, fast: boolean) {
  forge.openForge({ kind: 'gauntlet', indicators: [ind.key], fast, timeframe: scoreTrack.value })
}

function walkforward(ind: IndicatorInfo) {
  forge.openForge({ kind: 'walkforward', indicators: [ind.key], fast: false, timeframe: scoreTrack.value === 'all' ? '1d' : scoreTrack.value })
}

/** The indicator's script, in the editor — this module's whole point. */
async function openScript(ind: IndicatorInfo) {
  if (!wb.listing) await wb.loadListing()
  const at = wb.locationOf(ind.base_key ?? ind.key)
  if (!at) {
    wb.setNotice(`No script of the listing defines "${ind.key}" — reload the scripts.`, 'warn')
    return
  }
  await wb.openScript(at.file, at.line)
  void router.push('/script')
}

function scriptOf(ind: IndicatorInfo): string | null {
  return wb.locationOf(ind.base_key ?? ind.key)?.file ?? null
}
</script>

<template>
  <div class="qsf-roster">
    <div v-if="forge.registryLoading && !forge.registry" class="qsf-state muted qsc-pulse">Reading the forge's registry…</div>

    <template v-else-if="forge.registry">
      <div class="qsf-filters">
        <label class="qsf-filter">
          <span class="qsc-label">Score for</span>
          <select v-model="scoreTrack" class="qsc-select" aria-label="Score timeframe">
            <option value="all">All tracks — weakest score</option>
            <option value="1d">1d — daily / LCES reference</option>
            <option value="4h">4h</option>
            <option value="1h">1h</option>
            <option value="1m">1m — minute</option>
          </select>
        </label>
        <label class="qsc-check"><input v-model="onlyCertified" type="checkbox" /> Certified only</label>
        <input v-model="filter" class="qsc-input qsf-filter-input" type="search" placeholder="Filter…" spellcheck="false" aria-label="Filter indicators" />
        <span class="muted qsf-count">{{ rows.length }} indicators · individual scores never imply all-track certification</span>
      </div>
      <div v-if="forge.createError" class="qsc-note is-error">{{ forge.createError }}</div>
      <div v-if="forge.registry.unavailable_variants?.length" class="qsc-note is-warn">
        <p>Some saved subversions need attention. Their files are retained in your script folder.</p>
        <p v-for="child in forge.registry.unavailable_variants" :key="child.key" :title="child.path">{{ child.base_key }} › {{ child.label }}: {{ child.error }}</p>
      </div>

      <div class="qsc-card is-flush qsf-table-wrap">
        <table class="qsc-table qsf-table">
          <!-- Fixed layout: the numeric columns and the buttons keep their
               width, the name column takes what is left and truncates, so
               the buttons are always on screen. -->
          <colgroup>
            <col />
            <col class="col-score" />
            <col class="col-grade" />
            <col class="col-p" />
            <col class="col-status" />
            <col class="col-actions" />
          </colgroup>
          <thead>
            <tr>
              <th>Indicator</th>
              <th class="num" :title="scoreTrack === 'all' ? 'The worst measured track; missing tracks prevent overall certification' : `Gauntlet on ${scoreTrack} bars`">Score</th>
              <th>Grade</th>
              <th class="num" title="The worst permutation p across the tracks">Perm. p</th>
              <th :title="scoreTrack === 'all' ? 'Certified on every track' : `Certification on ${scoreTrack} only`">Status</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            <template v-for="ind in rows" :key="ind.key">
              <tr class="is-clickable qsf-row" :class="{ 'is-open': expanded === ind.key }" @click="toggle(ind.key)">
                <td class="qsf-cell-name">
                  <div class="qsf-name">{{ ind.name }} <span v-if="children(ind.key).length" class="muted">· {{ children(ind.key).length }} subversions</span></div>
                  <div class="qsf-key mono">{{ ind.key }}<span v-if="scriptOf(ind)" class="muted"> · {{ scriptOf(ind) }}</span></div>
                </td>
                <td class="num mono strong" :title="tracksText(ind)">{{ ind.certification ? +ind.certification.score.toFixed(1) : '—' }}</td>
                <td>
                  <span class="qsc-grade" :class="gradeClass(ind.certification?.grade, ind.certification?.certified)" :title="ind.certification?.capped ? 'Capped at B: a track failed the luck gate' : ''">
                    {{ ind.certification?.grade ?? '—' }}
                  </span>
                </td>
                <td class="num mono">{{ formatP(ind.certification?.perm_p) }}</td>
                <td>
                  <span class="qsc-tag" :class="{ 'is-ok': ind.certification?.certified }">{{ statusText(ind) }}</span>
                </td>
                <td class="qsf-cell-action" @click.stop>
                  <div class="qsf-actions">
                    <button class="qsc-btn is-sm" :disabled="forge.isRunning" :title="`Full gauntlet: ${scoreTrack}`" @click="gauntlet(ind, false)">Gauntlet</button>
                    <button class="qsc-btn is-sm" :disabled="forge.creating !== null" title="A RegimeTrend strategy on this indicator, in QuantAlgo" @click="forge.createStrategy(ind.key)">
                      {{ forge.creating === ind.key ? 'Creating…' : 'New strategy' }}
                    </button>
                  </div>
                </td>
              </tr>
              <tr v-if="expanded === ind.key" class="qsf-detail-row">
                <td colspan="6">
                  <div class="qsf-detail">
                    <p class="qsf-hypothesis"><span class="qsc-label qsf-inline-label">Hypothesis</span> {{ ind.hypothesis }}</p>
                    <p v-if="ind.certification?.source === 'historical'" class="muted">
                      Historical reference score. Run the current full gauntlet and Comparison before treating it as current evidence.
                    </p>
                    <p v-if="ind.certification?.reasons?.length" class="muted">{{ ind.certification.reasons.join(' · ') }}</p>

                    <div class="qsf-tracks">
                      <span class="qsc-label">Tracks</span>
                      <span v-for="tf in TRACKS" :key="tf" class="qsf-track mono">
                        <span class="qsf-track-tf">{{ tf }}</span>
                        <template v-if="ind.timeframes?.[tf]">
                          <span :class="ind.timeframes[tf]!.certified ? 'qsc-ok' : ''">{{ +ind.timeframes[tf]!.score.toFixed(1) }}/{{ ind.timeframes[tf]!.grade }}</span>
                          · p {{ formatP(ind.timeframes[tf]!.perm_p) }}
                          <button v-if="ind.timeframes[tf]!.report" class="qsc-link mono" @click="forge.openReport(ind.timeframes[tf]!.report!)">report</button>
                        </template>
                        <span v-else class="muted">not run</span>
                      </span>
                      <span class="muted qsf-rule">overall certification needs all four tracks, including 1m</span>
                    </div>

                    <div class="qsf-grid">
                      <div>
                        <span class="qsc-label qsf-inline-label">Warm-up</span>
                        <span class="mono qsf-value">{{ ind.warmup_bars }} bars</span>
                      </div>
                      <div>
                        <span class="qsc-label qsf-inline-label">Defaults</span>
                        <span class="mono qsf-value">{{ formatParams(ind.params) || 'parameter-free' }}</span>
                      </div>
                      <div>
                        <span class="qsc-label qsf-inline-label">Parameter space</span>
                        <span class="mono qsf-value">
                          <template v-if="Object.keys(ind.param_space).length">
                            <span v-for="(range, name) in ind.param_space" :key="name" class="qsf-range">{{ name }} ∈ [{{ range[0] }}, {{ range[1] }}]</span>
                          </template>
                          <template v-else>none — the gauntlet has nothing to perturb</template>
                        </span>
                      </div>
                    </div>

                    <div v-if="children(ind.key).length" class="qsc-card">
                      <span class="qsc-label">Subversions of {{ ind.name }}</span>
                      <div v-for="child in children(ind.key)" :key="child.key" class="qsf-detail-actions">
                        <span>{{ child.variant?.label }}</span>
                        <span class="mono">{{ formatParams(child.params) }}</span>
                        <span class="muted">{{ child.variant?.status }} · {{ statusText(child) }}</span>
                        <button class="qsc-btn is-sm" :disabled="forge.isRunning" @click="gauntlet(child, false)">Validate subversion</button>
                        <button class="qsc-btn is-sm" :disabled="forge.creating !== null" @click="forge.createStrategy(child.key)">Use subversion</button>
                      </div>
                    </div>
                    <div class="qsf-detail-actions">
                      <button class="qsc-btn is-sm" :disabled="forge.isRunning" title="Reduced Monte Carlo counts on every track — a smoke test, never a certification" @click="gauntlet(ind, true)">Fast run</button>
                      <button class="qsc-btn is-sm" :disabled="forge.isRunning" :title="`Walk-forward on ${scoreTrack === 'all' ? '1d' : scoreTrack}`" @click="walkforward(ind)">Walk-forward</button>
                      <button class="qsc-btn is-sm is-primary" title="The script that defines this indicator, in the editor" @click="openScript(ind)">Open script</button>
                    </div>
                  </div>
                </td>
              </tr>
            </template>
          </tbody>
        </table>
      </div>

      <p class="qsf-footer muted">
        {{ scoreTrack === 'all' ? 'Overall shows the weakest measured track; all four must pass for certification.' : `Showing the ${scoreTrack} track.` }}
        Certification requires score ≥ 70, permutation p ≤ 0.10 and complete full-run evidence.
        Historical scores remain labeled references. Registry read {{ new Date(forge.registry.generated_at).toLocaleString() }}.
      </p>
    </template>
  </div>
</template>

<style scoped>
.qsf-roster {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.qsf-state {
  padding: 40px 0;
  text-align: center;
  font-size: 13px;
}
.qsf-filters {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px;
  font-size: 12px;
}
.qsf-filter {
  display: flex;
  align-items: center;
  gap: 8px;
}
.qsf-filter-input {
  width: 180px;
}
.qsf-count {
  font-size: 11.5px;
}
.qsf-table-wrap {
  overflow-x: auto;
}
.qsf-table {
  min-width: 680px;
  table-layout: fixed;
}
.col-score { width: 64px; }
.col-grade { width: 64px; }
.col-p { width: 78px; }
.col-status { width: 118px; }
.col-actions { width: 196px; }
.qsf-table th {
  overflow: hidden;
  text-overflow: ellipsis;
}
.qsf-row.is-open td {
  border-bottom-color: transparent;
}
.qsf-detail-row td {
  padding-top: 0;
  background: var(--qss-bg-hover);
}
.qsf-cell-name {
  overflow: hidden;
  white-space: nowrap;
}
.qsf-name,
.qsf-key {
  overflow: hidden;
  text-overflow: ellipsis;
}
.qsf-name {
  font-weight: 600;
  line-height: 1.25;
  color: var(--qss-text);
}
.qsf-key {
  margin-top: 1px;
  font-size: 11px;
  line-height: 1.2;
  color: var(--qss-text-muted);
}
.qsf-cell-action {
  padding-left: 4px;
  padding-right: 8px;
}
.qsf-actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 4px;
}
.qsf-detail {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 4px 4px 10px;
  font-size: 12.5px;
}
.qsf-hypothesis {
  line-height: 1.55;
  max-width: 900px;
  color: var(--qss-text);
}
.qsf-inline-label {
  display: inline-block;
  margin-right: 8px;
}
.qsf-tracks {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 16px;
  font-size: 12px;
}
.qsf-track {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.qsf-track-tf {
  padding: 0 5px;
  border: 1px solid var(--qss-border);
  border-radius: 4px;
  font-size: 11px;
  font-weight: 600;
  color: var(--qss-text);
}
.qsf-rule {
  font-size: 11px;
}
.qsf-grid {
  display: flex;
  flex-wrap: wrap;
  gap: 24px;
}
.qsf-value {
  font-size: 12px;
  color: var(--qss-text-secondary);
}
.qsf-range + .qsf-range {
  margin-left: 12px;
}
.qsf-detail-actions {
  display: flex;
  gap: 6px;
}
.qsf-footer {
  font-size: 11px;
}
</style>
