<script setup lang="ts">
import { useAppStore } from '#systems/stores/app'
import { useSystemsStore } from '#systems/stores/systems'
import { useConfigStore } from '#systems/stores/config'
import { useLiveStore } from '#systems/stores/live'
import { useBacktestStore } from '#systems/stores/backtest'
import { useActiveView } from '#systems/composables/useActiveView'
import { aggregateName } from '#systems/composables/useIndicatorOptions'

const app = useAppStore()
const systems = useSystemsStore()
const config = useConfigStore()
const live = useLiveStore()
const backtest = useBacktestStore()
const { systemId, view } = useActiveView()

const system = computed(() => systems.byId(systemId.value))
const cfg = computed(() => config.get(systemId.value))
const marketLabel = computed(() => {
  if (!cfg.value.marketFilter) return 'off'
  const ind = cfg.value.marketIndicator
  if (!ind) return 'Same as ranking'
  if (ind.trend === 'total_breakout') return 'Breakout'
  if (ind.trend === 'ema_cross') return `EMA ${ind.emaCross.fastLength}/${ind.emaCross.slowLength}`
  if (ind.trend === 'aggregate') return aggregateName(ind.aggregate.length)
  return ind.trend
})
const engineRunning = computed(() => app.engineStatus.status === 'running')

const liveState = computed(() => live.stateFor(systemId.value))
const btState = computed(() => backtest.stateFor(systemId.value))
</script>

<template>
  <div class="qs-context">
    <section class="qs-context__section">
      <span class="label">System</span>
      <div class="qs-context__system">
        <span class="qs-context__short">{{ system?.short }}</span>
        <span class="qs-context__name" :title="system?.name">{{ system?.name }}</span>
      </div>
      <p class="qs-context__desc">{{ system?.description }}</p>
    </section>

    <section class="qs-context__section">
      <span class="label">Engine</span>
      <div class="qs-context__row">
        <span>Status</span>
        <span :class="engineRunning ? 'qs-ok' : 'qs-muted'">{{ engineRunning ? 'running' : 'stopped' }}</span>
      </div>
      <div class="qs-context__row">
        <span>Ranking</span>
        <span class="mono">{{ cfg.rankingSource }}</span>
      </div>
      <div class="qs-context__row">
        <span>Trend</span>
        <span class="mono" :title="cfg.indicator.trend === 'aggregate' ? (cfg.indicator.aggregate ?? []).join(', ') : undefined">{{
          (cfg.indicator.trend ?? 'ema_cross') === 'ema_cross'
            ? `EMA ${cfg.indicator.emaCross.fastLength}/${cfg.indicator.emaCross.slowLength}`
            : cfg.indicator.trend === 'aggregate'
              ? aggregateName((cfg.indicator.aggregate ?? []).length)
              : cfg.indicator.trend }}</span>
      </div>
      <div v-if="cfg.compareTrends?.length" class="qs-context__row">
        <span>Compare</span>
        <span class="mono qs-context__compare" :title="cfg.compareTrends.join(', ')">{{ cfg.compareTrends.join(', ') }}</span>
      </div>
      <div class="qs-context__row">
        <span>TOTAL filter</span>
        <span class="mono qs-context__compare" :class="cfg.marketFilter ? '' : 'qs-muted'"
          :title="marketLabel">{{ marketLabel }}</span>
      </div>
    </section>

    <!-- Live context -->
    <section v-if="view === 'live'" class="qs-context__section">
      <span class="label">Best Asset</span>
      <div v-if="liveState.result?.best" class="qs-context__best">{{ liveState.result.best }}</div>
      <p v-else class="qs-context__desc">Run a live evaluation to rank today's top coins.</p>
      <div v-if="liveState.result?.marketFilter?.enabled" class="qs-context__row">
        <span>TOTAL</span>
        <span :class="liveState.result.marketFilter.bullish ? 'qs-ok' : 'qs-bad'">
          {{ liveState.result.marketFilter.bullish ? 'bullish' : 'bearish · USD only' }}
        </span>
      </div>
      <div class="qs-legend">
        <div class="qs-legend__item"><span class="qs-legend__box qs-legend__box--win" />A beats B</div>
        <div class="qs-legend__item"><span class="qs-legend__box qs-legend__box--lose" />A loses to B</div>
      </div>
    </section>

    <!-- Backtest context -->
    <section v-if="view === 'backtest'" class="qs-context__section">
      <span class="label">Run Window</span>
      <div class="qs-context__row"><span>Start</span><span class="mono">{{ cfg.startDate }}</span></div>
      <div class="qs-context__row"><span>End</span><span class="mono">{{ cfg.endDate }}</span></div>
      <div class="qs-context__row"><span>Cadence</span><span class="mono">{{ cfg.cadence }}</span></div>
      <div class="qs-context__row"><span>Fee</span><span class="mono">{{ (cfg.feeRate * 100).toFixed(2) }}%</span></div>
      <template v-if="btState.notes.length">
        <span class="label" style="margin-top: 14px">Notes</span>
        <ul class="qs-notes">
          <li v-for="(n, i) in btState.notes" :key="i">{{ n }}</li>
        </ul>
      </template>
    </section>
  </div>
</template>

<style scoped>
.qs-context {
  display: flex;
  flex-direction: column;
  padding: 18px 16px;
  gap: 18px;
}

.qs-context__section {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.qs-context__system {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.qs-context__short {
  font-weight: 700;
  font-size: 12px;
  color: var(--qs-accent);
}

.qs-context__name {
  font-size: 13px;
  color: var(--qs-text);
}

.qs-context__desc {
  margin: 0;
  font-size: 12px;
  color: var(--qs-text-muted);
  line-height: 1.5;
}

.qs-context__row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 12px;
  color: var(--qs-text-secondary);
}

.qs-context__compare {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  text-align: right;
}

.qs-ok {
  color: var(--qs-success);
}

.qs-muted {
  color: var(--qs-text-muted);
}

.qs-bad {
  color: var(--qs-error);
}

.qs-context__best {
  font-size: 28px;
  font-weight: 700;
  letter-spacing: 0.02em;
  color: var(--qs-text);
}

.qs-legend {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 8px;
}

.qs-legend__item {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: var(--qs-text-secondary);
}

.qs-legend__box {
  width: 12px;
  height: 12px;
  border-radius: 3px;
}

.qs-legend__box--win {
  background: var(--qs-success);
}

.qs-legend__box--lose {
  background: color-mix(in srgb, var(--qs-error) 70%, transparent);
}

.qs-notes {
  margin: 0;
  padding-left: 16px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  font-size: 12px;
  color: var(--qs-text-muted);
}
</style>
