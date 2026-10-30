<script setup lang="ts">
/**
 * Usage (card 18; T3 Code's Usage page, PandaCode's agent usage): a page in
 * QuantPilot's main area, Ctrl+U or the palette. Cost — what the agents'
 * tokens would have cost through the API, an estimate and not the
 * subscription bill — and Tokens over 24 hours to 90 days, with a breakdown
 * per model (trend, cache hit rate, cost per million tokens); Limits — the
 * subscription windows as % used with their reset. C / T / L switch tabs,
 * Ctrl+Shift+1..4 the period, Escape goes back.
 */
import { computed, watch } from 'vue'
import { formatMoney } from '@quantsuite/core'
import { ArrowLeft, RefreshCw } from 'lucide-vue-next'
import { usePilotStore } from '#pilot/stores/pilot'
import { useUsageStore, type UsageTab } from '#pilot/stores/usage'
import { useKeysStore } from '#pilot/stores/keys'
import { byModel, compact, series, totals } from '#pilot/utils/usage'
import type { UsagePeriod } from '#pilot/types'
import type { Series } from './Chart.vue'

const store = usePilotStore()
const usage = useUsageStore()
const keys = useKeysStore()

const TABS: { id: UsageTab; label: string; command: string }[] = [
  { id: 'cost', label: 'Cost', command: 'usage.cost' },
  { id: 'tokens', label: 'Tokens', command: 'usage.tokens' },
  { id: 'limits', label: 'Limits', command: 'usage.limits' },
]
const PERIODS: { id: UsagePeriod; label: string; command: string }[] = [
  { id: '24h', label: '24 hours', command: 'usage.period.day' },
  { id: '7d', label: '7 days', command: 'usage.period.week' },
  { id: '30d', label: '30 days', command: 'usage.period.month' },
  { id: '90d', label: '90 days', command: 'usage.period.quarter' },
]

/** Providers and token kinds in fixed categorical slots (validated on the suite's dark surface). */
const PROVIDERS: Series[] = [
  { id: 'codex', label: 'Codex', color: 'var(--qp-viz-1)' },
  { id: 'claude', label: 'Claude Code', color: 'var(--qp-viz-2)' },
]
const KINDS: Series[] = [
  { id: 'input', label: 'Input', color: 'var(--qp-viz-1)' },
  { id: 'cacheRead', label: 'Cache read', color: 'var(--qp-viz-3)' },
  { id: 'cacheWrite', label: 'Cache write', color: 'var(--qp-viz-4)' },
  { id: 'output', label: 'Output', color: 'var(--qp-viz-5)' },
]

const report = computed(() => usage.report)
const sum = computed(() => totals(report.value?.rows ?? []))
const models = computed(() => byModel(report.value?.rows ?? []))
const providersShown = computed(() => PROVIDERS.filter((p) => report.value?.rows.some((r) => r.provider === p.id)))
const chart = computed(() => (report.value ? series(report.value, usage.tab === 'tokens' ? 'kind' : 'provider') : []))
const selected = computed(() => models.value.find((m) => m.model === usage.model) ?? null)
const trend = computed(() => (report.value && selected.value ? series(report.value, selected.value.cost === null ? 'kind' : 'provider', selected.value.model) : []))
const trendSeries = computed<Series[]>(() => {
  const m = selected.value
  if (!m) return []
  if (m.cost === null) return KINDS
  return PROVIDERS.filter((p) => p.id === m.provider)
})

const money = (v: number) => formatMoney(v, 'USD', { maximumFractionDigits: v !== 0 && Math.abs(v) < 1 ? 3 : 2, minimumFractionDigits: 2 })
const pct = (v: number) => (v > 0 && v < 0.005 ? '<1%' : `${Math.round(v * 100)}%`)
const unpricedShare = computed(() => (sum.value.tokens ? sum.value.unpricedTokens / sum.value.tokens : 0))
const providerLabel = (id: string) => store.adapterLabel(id)

watch(
  () => usage.tab,
  (t) => {
    if (t === 'limits') void usage.loadLimits()
    else void usage.load()
  },
  { immediate: true }
)
</script>

<template>
  <div class="qp-usage">
    <header class="qp-usage-head">
      <button class="qp-usage-back" :title="keys.hint('usage.close', 'Back')" @click="store.page = 'thread'"><ArrowLeft :size="15" /></button>
      <h2 class="qp-usage-title">Usage</h2>
      <div class="qp-seg" role="tablist" aria-label="View">
        <button
          v-for="t in TABS"
          :key="t.id"
          role="tab"
          class="qp-seg-btn"
          :class="{ on: usage.tab === t.id }"
          :aria-selected="usage.tab === t.id"
          :title="keys.hint(t.command, t.label)"
          @click="usage.setTab(t.id)"
        >
          {{ t.label }}
        </button>
      </div>
      <div v-if="usage.tab !== 'limits'" class="qp-seg" role="radiogroup" aria-label="Period">
        <button
          v-for="p in PERIODS"
          :key="p.id"
          role="radio"
          class="qp-seg-btn"
          :class="{ on: usage.period === p.id }"
          :aria-checked="usage.period === p.id"
          :title="keys.hint(p.command, `Past ${p.label}`)"
          @click="usage.setPeriod(p.id)"
        >
          {{ p.label }}
        </button>
      </div>
      <button class="qp-usage-refresh" :disabled="usage.loading || usage.limitsLoading" title="Read the logs again and update the prices" @click="usage.refresh()">
        <RefreshCw :size="13" :class="{ spin: usage.loading || usage.limitsLoading }" /> Refresh
      </button>
    </header>

    <div v-if="usage.error" class="qp-usage-error" role="alert">{{ usage.error }}</div>

    <!-- Limits -->
    <PilotUsageLimits v-if="usage.tab === 'limits'" />

    <!-- Cost and tokens -->
    <template v-else>
      <div v-if="!report" class="qp-usage-loading">
        <span class="qp-usage-skel" /><span class="qp-usage-skel" /><span class="qp-usage-skel" />
        <p>Reading the Claude Code and Codex logs…</p>
      </div>
      <div v-else class="qp-usage-body" :class="{ stale: usage.loading }">
        <section class="qp-usage-tiles">
          <template v-if="usage.tab === 'cost'">
            <div class="qp-tile hero">
              <span class="qp-tile-k">Estimated API cost</span>
              <span class="qp-tile-v">{{ money(sum.cost) }}</span>
              <span class="qp-tile-sub">What these tokens would cost through the API — not your subscription bill.</span>
            </div>
            <div class="qp-tile">
              <span class="qp-tile-k">Cache savings</span>
              <span class="qp-tile-v">{{ money(sum.savings) }}</span>
              <span class="qp-tile-sub">cache reads against uncached input</span>
            </div>
            <div class="qp-tile">
              <span class="qp-tile-k">Tokens</span>
              <span class="qp-tile-v">{{ compact(sum.tokens) }}</span>
              <span class="qp-tile-sub">{{ pct(sum.tokens ? sum.cacheRead / sum.tokens : 0) }} cache reads</span>
            </div>
            <div class="qp-tile">
              <span class="qp-tile-k">Unpriced</span>
              <span class="qp-tile-v">{{ unpricedShare ? pct(unpricedShare) : 'none' }}</span>
              <span class="qp-tile-sub">{{ unpricedShare ? 'of the tokens have no public price' : 'every model has a public price' }}</span>
            </div>
          </template>
          <template v-else>
            <div class="qp-tile hero">
              <span class="qp-tile-k">Tokens</span>
              <span class="qp-tile-v">{{ compact(sum.tokens) }}</span>
              <span class="qp-tile-sub">{{ sum.tokens.toLocaleString('en-US') }} in the period</span>
            </div>
            <div v-for="k in KINDS" :key="k.id" class="qp-tile">
              <span class="qp-tile-k"><i class="qp-tile-key" :style="{ background: k.color }" />{{ k.label }}</span>
              <span class="qp-tile-v">{{ compact(sum[k.id as 'input']) }}</span>
              <span class="qp-tile-sub">{{ pct(sum.tokens ? sum[k.id as 'input'] / sum.tokens : 0) }}</span>
            </div>
          </template>
        </section>

        <section class="qp-usage-card">
          <h3 class="qp-usage-h">{{ usage.tab === 'cost' ? 'Estimated cost' : 'Tokens' }} per {{ report.bucket }}</h3>
          <PilotUsageChart
            :buckets="chart"
            :series="usage.tab === 'cost' ? providersShown : KINDS"
            :bucket="report.bucket"
            :format="usage.tab === 'cost' ? money : compact"
            :label="usage.tab === 'cost' ? 'Estimated cost' : 'Tokens'"
          />
        </section>

        <section class="qp-usage-card qp-usage-models">
          <h3 class="qp-usage-h">Breakdown by model</h3>
          <div class="qp-models-scroll">
            <table class="qp-models">
              <thead>
                <tr>
                  <th>Model</th>
                  <th>Cost</th>
                  <th>Share</th>
                  <th>Tokens</th>
                  <th>Cache hit</th>
                  <th>Per 1M tokens</th>
                </tr>
              </thead>
              <tbody>
                <tr
                  v-for="m in models"
                  :key="`${m.provider}:${m.model}`"
                  :class="{ on: usage.model === m.model }"
                  tabindex="0"
                  :aria-pressed="usage.model === m.model"
                  @click="usage.model = usage.model === m.model ? null : m.model"
                  @keydown.enter.prevent="usage.model = usage.model === m.model ? null : m.model"
                >
                  <td>
                    <span class="qp-models-name">{{ m.model }}</span>
                    <span class="qp-models-prov">{{ providerLabel(m.provider) }}</span>
                  </td>
                  <td class="num">{{ m.cost === null ? 'unpriced' : money(m.cost) }}</td>
                  <td class="num">{{ m.cost === null ? '—' : pct(m.share) }}</td>
                  <td class="num">{{ compact(m.tokens) }}</td>
                  <td class="num">{{ pct(m.cacheHitRate) }}</td>
                  <td class="num">{{ m.costPerMillion === null ? '—' : money(m.costPerMillion) }}</td>
                </tr>
                <tr v-if="!models.length"><td colspan="6" class="qp-models-empty">No agent usage in this period.</td></tr>
              </tbody>
            </table>
          </div>
          <div v-if="selected" class="qp-model-detail">
            <div class="qp-model-detail-head">
              <strong>{{ selected.model }}</strong>
              <span>{{ selected.cost === null ? 'tokens per ' + report.bucket + ' (no public price)' : 'estimated cost per ' + report.bucket }}</span>
              <span class="qp-model-stat">cache hit <b>{{ pct(selected.cacheHitRate) }}</b></span>
              <span v-if="selected.costPerMillion !== null" class="qp-model-stat"><b>{{ money(selected.costPerMillion) }}</b> per 1M tokens</span>
            </div>
            <PilotUsageChart :buckets="trend" :series="trendSeries" :bucket="report.bucket" :format="selected.cost === null ? compact : money" :label="selected.model" />
          </div>
        </section>

        <p class="qp-usage-foot">
          {{ report.prices }} · {{ report.files }} log files read in {{ (report.scanMs / 1000).toFixed(1) }}s<template v-if="report.unreadable"> · {{ report.unreadable }} unreadable</template>
          <span class="qp-usage-sources" :title="report.sources.join('\n')"> · from {{ report.sources.length }} folders</span>
        </p>
      </div>
    </template>
  </div>
</template>

<style scoped>
.qp-usage {
  /* Categorical slots, validated on the suite's dark surface (#212121): blue, orange, aqua, yellow, violet. */
  --qp-viz-1: #3987e5;
  --qp-viz-2: #d95926;
  --qp-viz-3: #199e70;
  --qp-viz-4: #c98500;
  --qp-viz-5: #9085e9;
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  background: var(--qss-bg, #18181e);
  color: var(--qss-text, #d4d4d8);
}
.qp-usage-head { flex-shrink: 0; display: flex; align-items: center; gap: 10px; padding: 10px 18px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); flex-wrap: wrap; }
.qp-usage-back { display: inline-flex; padding: 4px; border: none; border-radius: 6px; background: transparent; color: var(--qss-text-secondary, #9a9aa5); cursor: pointer; }
.qp-usage-back:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-usage-title { margin: 0 6px 0 0; font-size: 15px; font-weight: 600; }
.qp-seg { display: inline-flex; border: 1px solid var(--qss-border, #47474f); border-radius: 7px; overflow: hidden; }
.qp-seg-btn { padding: 3px 10px; border: none; background: transparent; color: var(--qss-text-secondary, #9a9aa5); font: inherit; font-size: 12px; cursor: pointer; }
.qp-seg-btn + .qp-seg-btn { border-left: 1px solid var(--qss-border-subtle, #35353d); }
.qp-seg-btn:hover { color: var(--qss-text, #d4d4d8); }
.qp-seg-btn.on { background: var(--qss-bg-card, #292930); color: var(--qss-text, #d4d4d8); }
.qp-usage-refresh { margin-left: auto; display: inline-flex; align-items: center; gap: 6px; padding: 3px 10px; border: 1px solid var(--qss-border-subtle, #35353d); border-radius: 7px; background: transparent; color: var(--qss-text-secondary, #9a9aa5); font: inherit; font-size: 12px; cursor: pointer; }
.qp-usage-refresh:hover:not(:disabled) { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-usage-refresh:disabled { opacity: 0.6; cursor: default; }
.spin { animation: qp-spin 0.9s linear infinite; }
@keyframes qp-spin { to { transform: rotate(360deg); } }
.qp-usage-error { margin: 10px 18px 0; padding: 7px 10px; border-radius: 8px; border: 1px solid color-mix(in srgb, var(--qss-error, #f87171) 45%, var(--qss-border, #47474f)); font-size: 12px; color: var(--qss-text, #d4d4d8); }
.qp-usage-loading { padding: 24px 18px; display: flex; flex-direction: column; gap: 10px; }
.qp-usage-loading p { margin: 0; font-size: 12px; color: var(--qss-text-muted, #6e6e7a); }
.qp-usage-skel { display: block; height: 64px; border-radius: 10px; background: linear-gradient(90deg, var(--qss-bg-raised, #1f1f25), var(--qss-bg-card, #292930), var(--qss-bg-raised, #1f1f25)); background-size: 200% 100%; animation: qp-skel 1.4s ease-in-out infinite; }
.qp-usage-skel:nth-child(2) { height: 190px; }
@keyframes qp-skel { to { background-position: -200% 0; } }
@media (prefers-reduced-motion: reduce) { .qp-usage-skel, .spin { animation: none; } }
.qp-usage-body { flex: 1; min-height: 0; display: flex; flex-direction: column; gap: 12px; padding: 14px 18px 10px; overflow: hidden; transition: opacity 0.15s; }
.qp-usage-body.stale { opacity: 0.6; }
.qp-usage-tiles { flex-shrink: 0; display: grid; grid-template-columns: repeat(auto-fit, minmax(160px, 1fr)); gap: 10px; }
.qp-tile { display: flex; flex-direction: column; gap: 3px; padding: 10px 12px; border-radius: 10px; border: 1px solid var(--qss-border-subtle, #35353d); background: var(--qss-bg-raised, #1f1f25); min-width: 0; }
.qp-tile.hero { grid-column: span 2; }
.qp-tile-k { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-tile-key { width: 10px; height: 10px; border-radius: 2px; }
.qp-tile-v { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 20px; font-weight: 600; color: var(--qss-text, #d4d4d8); }
.qp-tile.hero .qp-tile-v { font-size: 26px; }
.qp-tile-sub { font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); overflow: hidden; text-overflow: ellipsis; }
.qp-usage-card { flex-shrink: 0; padding: 12px 14px; border-radius: 10px; border: 1px solid var(--qss-border-subtle, #35353d); background: var(--qss-bg-raised, #1f1f25); min-width: 0; }
.qp-usage-models { flex: 1; min-height: 120px; display: flex; flex-direction: column; }
.qp-usage-h { margin: 0 0 8px; font-size: 12.5px; font-weight: 600; color: var(--qss-text-secondary, #9a9aa5); }
.qp-models-scroll { flex: 1; min-height: 60px; overflow-y: auto; }
.qp-models { width: 100%; border-collapse: collapse; font-size: 12.5px; }
.qp-models th { position: sticky; top: 0; background: var(--qss-bg-raised, #1f1f25); padding: 3px 8px; font-weight: 500; font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); text-align: right; }
.qp-models th:first-child { text-align: left; }
.qp-models td { padding: 5px 8px; border-top: 1px solid var(--qss-border-subtle, #35353d); }
.qp-models td.num { text-align: right; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11.5px; }
.qp-models tbody tr { cursor: pointer; outline: none; }
.qp-models tbody tr:hover,
.qp-models tbody tr:focus-visible { background: var(--qss-bg-hover, #313139); }
.qp-models tbody tr.on { background: var(--qss-bg-card, #292930); }
.qp-models-name { margin-right: 8px; }
.qp-models-prov { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-models-empty { text-align: center; color: var(--qss-text-muted, #6e6e7a); cursor: default; }
.qp-model-detail { flex-shrink: 0; margin-top: 10px; padding-top: 10px; border-top: 1px solid var(--qss-border-subtle, #35353d); }
.qp-model-detail-head { display: flex; align-items: baseline; gap: 12px; flex-wrap: wrap; margin-bottom: 6px; font-size: 12px; color: var(--qss-text-muted, #6e6e7a); }
.qp-model-detail-head strong { color: var(--qss-text, #d4d4d8); font-size: 12.5px; }
.qp-model-stat b { font-family: var(--qss-font-mono, ui-monospace, monospace); color: var(--qss-text, #d4d4d8); font-weight: 600; }
.qp-usage-foot { flex-shrink: 0; margin: 0; font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-usage-sources { cursor: help; }
</style>
