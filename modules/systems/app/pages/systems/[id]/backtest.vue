<script setup lang="ts">
definePageMeta({ layout: 'systems', path: '/algo/manual/:id/backtest' })

import { useSystemsStore } from '#systems/stores/systems'
import { useConfigStore } from '#systems/stores/config'
import { useBacktestStore } from '#systems/stores/backtest'
import { useIndicatorOptions } from '#systems/composables/useIndicatorOptions'
import SystemsSettingsMarketFilterSelect from '#systems/components/Settings/MarketFilterSelect.vue'
import type { MarketIndicatorConfig, TrendKind } from '#systems/types'

const route = useRoute()
const systems = useSystemsStore()
const config = useConfigStore()
const backtest = useBacktestStore()

const systemId = computed(() => route.params.id as string)
const system = computed(() => systems.byId(systemId.value))
const planned = computed(() => system.value?.status !== 'ready')
const state = computed(() => backtest.stateFor(systemId.value))
const cfg = computed(() => config.get(systemId.value))
const { options: memberOptions, trendOptions, compareOptions } = useIndicatorOptions(() => cfg.value)
const trend = computed(() => cfg.value.indicator.trend ?? 'ema_cross')
const optionsOpen = ref(false)
const optionsEl = ref<HTMLElement | null>(null)

onClickOutside(optionsEl, () => { optionsOpen.value = false })
function closeOptions() {
  if (!optionsOpen.value) return
  optionsOpen.value = false
  optionsEl.value?.querySelector<HTMLButtonElement>('.qs-backtest__options-toggle')?.focus()
}
watch(() => state.value.isRunning, () => { optionsOpen.value = false })

onMounted(() => config.load(systemId.value))

// The pickers here edit the same run configuration as Settings and persist
// it at once — there is no Save button on this screen.
function setTrend(trend: TrendKind) {
  const current = config.get(systemId.value)
  config.update(systemId.value, {
    indicator: { ...current.indicator, trend },
    compareTrends: current.compareTrends.filter(k => k !== trend),
  })
  void config.save(systemId.value)
}

function setCompare(compareTrends: TrendKind[]) {
  config.update(systemId.value, { compareTrends })
  void config.save(systemId.value)
}

function setAggregate(aggregate: TrendKind[]) {
  const current = config.get(systemId.value)
  config.update(systemId.value, { indicator: { ...current.indicator, aggregate } })
  void config.save(systemId.value)
}

function setMarketFilter(marketFilter: boolean) {
  config.update(systemId.value, { marketFilter })
  void config.save(systemId.value)
}

function setMarketIndicator(marketIndicator: MarketIndicatorConfig | null) {
  config.update(systemId.value, { marketIndicator })
  void config.save(systemId.value)
}

function run() {
  void backtest.run(systemId.value)
}
</script>

<template>
  <SystemsSystemPlanned v-if="planned" :system="system" />
  <div v-else class="qs-backtest">
    <header class="qs-backtest__head">
      <h1 class="qs-backtest__title">Backtest</h1>

      <div class="qs-backtest__signals">
        <SystemsSettingsIndicatorSelect
          class="qs-backtest__indicator"
          compact
          :disabled="state.isRunning"
          :model-value="trend"
          :options="trendOptions"
          @update:model-value="setTrend($event as TrendKind)"
        />
        <div ref="optionsEl" class="qs-backtest__options" :class="{ 'is-open': optionsOpen }"
          @keydown.esc="closeOptions">
          <button class="btn qs-backtest__options-toggle" type="button" :aria-expanded="optionsOpen"
            :aria-controls="`backtest-parameters-${systemId}`" :disabled="state.isRunning" @click="optionsOpen = !optionsOpen">
            Parameters
          </button>
          <div :id="`backtest-parameters-${systemId}`" class="qs-backtest__parameters">
            <SystemsBacktestCompareSelect
              class="qs-backtest__members"
              label="Members"
              :title="trend === 'aggregate' ? 'The indicators whose signals are averaged: bullish above 0, bearish below 0' : 'Members are available for Aggregate indicators'"
              empty-hint="Pick indicators to average"
              unit="member"
              unit-plural="members"
              :disabled="state.isRunning || trend !== 'aggregate'"
              :model-value="trend === 'aggregate' ? cfg.indicator.aggregate ?? [] : []"
              :options="memberOptions"
              @update:model-value="setAggregate"
            />
            <SystemsBacktestCompareSelect
              class="qs-backtest__compare"
              :disabled="state.isRunning"
              :model-value="cfg.compareTrends"
              :options="compareOptions"
              :exclude="trend"
              @update:model-value="setCompare"
            />
            <label
              class="qs-check qs-backtest__filter"
              title="Positions only while the selected TOTAL market signal is bullish, otherwise USD."
            >
              <input
                type="checkbox"
                :checked="cfg.marketFilter"
                :disabled="state.isRunning"
                @change="setMarketFilter(($event.target as HTMLInputElement).checked)"
              />
              <span>TOTAL filter</span>
            </label>
            <SystemsSettingsMarketFilterSelect class="qs-backtest__market" compact
              :config="cfg" :disabled="state.isRunning || !cfg.marketFilter" @update:model-value="setMarketIndicator" />
          </div>
        </div>
      </div>

      <button class="btn btn-primary qs-backtest__run" :disabled="state.isRunning" @click="run">
        {{ state.isRunning ? (backtest.runningSystemId === systemId ? 'Running…' : 'Queued…') : 'Run Backtest' }}
      </button>
    </header>

    <!-- Reserve this track even when idle: progress never moves the controls
         or the chart. Run dates and costs already live in the right sidebar. -->
    <div class="qs-backtest__status">
      <div v-if="state.isRunning" class="qs-backtest__progress">
        <div class="qs-bar" role="progressbar" aria-label="Backtest progress"
          :aria-valuenow="Math.round(state.progressValue * 100)" :aria-valuemin="0" :aria-valuemax="100">
          <div class="qs-bar__fill" :style="{ width: `${Math.round(state.progressValue * 100)}%` }" />
        </div>
        <span class="qs-backtest__progress-lbl" :title="state.progressLabel || 'Working…'" role="status">{{ state.progressLabel || 'Working…' }}</span>
      </div>
    </div>

    <div v-if="state.error" class="qs-backtest__error">{{ state.error }}</div>

    <template v-if="state.result">
      <div v-if="!state.isRunning && state.result.skippedStrategies?.length" class="qs-backtest__notice" role="status">
        <strong>{{ state.result.skippedStrategies.length }} variant(s) skipped — unavailable indicator data</strong>
        <ul>
          <li v-for="skipped in state.result.skippedStrategies" :key="skipped.key">
            <strong>{{ skipped.label }}</strong>: {{ skipped.reason }}
          </li>
        </ul>
      </div>
      <p v-if="state.result.strategies?.length === 0" class="qs-backtest__notice">
        No strategy could be evaluated. Choose an indicator compatible with the available data.
      </p>
      <SystemsBacktestEquityChart v-else :result="state.result" class="qs-backtest__chart" />
      <div v-if="state.result.strategies?.length !== 0" class="qs-backtest__grid">
        <SystemsBacktestMetricsTable :result="state.result" />
        <SystemsBacktestForcedRotations :result="state.result" />
      </div>
    </template>

    <div v-else-if="!state.isRunning" class="qs-empty">
      <span class="qs-empty__mark">◈</span>
      <p>No backtest results. Choose your parameters, then run a backtest.</p>
    </div>
  </div>
</template>

<style scoped>
/* The page claims the full main area so the equity chart absorbs the spare
   height instead of leaving a gap under a fixed-height canvas. */
.qs-backtest {
  container: manual-backtest / inline-size;
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-height: 100%;
}

.qs-backtest__head {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  flex-wrap: nowrap;
  min-width: 0;
  gap: 16px;
}

.qs-backtest__title {
  flex-shrink: 0;
  margin: 0;
  font-size: 17px;
  font-weight: 600;
}

.qs-backtest__status {
  flex: 0 0 26px;
  min-width: 0;
  height: 26px;
}

/* All controls share one row; progress has its own fixed-height track. */
.qs-backtest__signals {
  margin-left: auto;
  flex: 0 1 833px;
  min-width: 0;
  display: flex;
  flex-wrap: nowrap;
  align-items: center;
  gap: 6px;
}

.qs-backtest__options { display: contents; }
.qs-backtest__options-toggle { display: none; }
.qs-backtest__parameters {
  display: flex;
  align-items: center;
  gap: 6px;
  flex: 1;
  min-width: 0;
}

.qs-backtest__indicator {
  flex: 1 1 200px;
  min-width: 110px;
  max-width: 200px;
  width: 200px;
}

.qs-backtest__market {
  flex: 1 1 265px;
  min-width: 165px;
  max-width: 265px;
}

.qs-backtest__members,
.qs-backtest__compare {
  flex: 0 0 120px;
  width: 120px;
}

/* Same height as the compact pickers beside it. */
.qs-backtest__filter {
  box-sizing: border-box;
  flex: 0 0 104px;
  width: 104px;
  height: 32px;
  padding: 5px 10px;
  border-radius: var(--qs-radius);
  white-space: nowrap;
}

.qs-backtest__run {
  margin-left: 0;
  box-sizing: border-box;
  flex: 0 0 128px;
  width: 128px;
  height: 32px;
  white-space: nowrap;
  padding: 6px 14px;
  font-size: 13px;
}

.qs-backtest__chart {
  flex: 1 1 auto;
  min-height: 260px;
}

.qs-backtest__progress {
  min-width: 0;
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12px;
  line-height: 18px;
  color: var(--qs-text-secondary);
}

.qs-backtest__progress-lbl {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.qs-bar {
  width: 100%;
  flex: 0 0 4px;
  height: 4px;
  border-radius: 999px;
  background: var(--qs-bg-card);
  overflow: hidden;
}

.qs-bar__fill {
  height: 100%;
  background: var(--qs-accent);
  transition: width 200ms ease;
}

.qs-backtest__error {
  flex-shrink: 0;
  padding: 8px 12px;
  border: 1px solid color-mix(in srgb, var(--qs-error) 50%, var(--qs-border));
  border-radius: var(--qs-radius);
  background: color-mix(in srgb, var(--qs-error) 12%, transparent);
  color: var(--qs-error);
  font-size: 13px;
}

.qs-backtest__notice {
  flex-shrink: 0;
  padding: 10px 12px;
  border: 1px solid var(--qs-border);
  border-radius: var(--qs-radius);
  background: var(--qs-bg-card);
  color: var(--qs-text-secondary);
  font-size: 12px;
}

.qs-backtest__notice ul { margin: 6px 0 0; padding-left: 20px; }

.qs-backtest__grid {
  flex-shrink: 0;
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
  align-items: stretch;
}

/* Allow the grid children to shrink below their content width so a wide
   metrics table scrolls inside its tile instead of stretching it. */
.qs-backtest__grid > * {
  min-width: 0;
}

.qs-empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  color: var(--qs-text-muted);
}

.qs-empty__mark {
  font-size: 32px;
  color: var(--qs-accent);
}

@media (max-width: 1000px) {
  .qs-backtest__grid {
    grid-template-columns: 1fr;
  }
}

/* A small window keeps the primary signal and Run on one row. Secondary
   parameters stay available in a popover instead of forcing a second row. */
@container manual-backtest (max-width: 920px) {
  .qs-backtest__head { gap: 12px; }
  .qs-backtest__signals { justify-content: flex-end; }
  .qs-backtest__options { display: block; position: relative; flex-shrink: 0; }
  .qs-backtest__options-toggle { display: inline-flex; height: 32px; padding: 5px 10px; font-size: 12px; }
  .qs-backtest__parameters {
    display: none;
    position: absolute;
    top: calc(100% + 8px);
    right: 0;
    z-index: 20;
    width: min(350px, calc(100cqw - 140px));
    padding: 14px;
    border: 1px solid var(--qs-border);
    border-radius: var(--qs-radius-lg);
    background: var(--qs-bg-card);
    box-shadow: 0 12px 30px #0004;
  }
  .qs-backtest__options.is-open .qs-backtest__parameters { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .qs-backtest__members, .qs-backtest__compare { width: 100%; }
  .qs-backtest__filter { width: fit-content; grid-column: 1 / -1; }
  .qs-backtest__market { grid-column: 1 / -1; max-width: none; }
}

@container manual-backtest (max-width: 470px) {
  .qs-backtest__head { gap: 8px; }
  .qs-backtest__run { flex-basis: 120px; width: 120px; }
  .qs-backtest__members :deep(.qs-cmp__panel) { left: 0; right: auto; }
}
</style>
