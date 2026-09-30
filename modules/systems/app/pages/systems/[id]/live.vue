<script setup lang="ts">
definePageMeta({ layout: 'systems', path: '/algo/manual/:id/live' })

import { useSystemsStore } from '#systems/stores/systems'
import { useConfigStore } from '#systems/stores/config'
import { useLiveStore } from '#systems/stores/live'
import { matchesMarket } from '#systems/utils/systemMode'
import SystemsLiveOverview from '#systems/components/Live/Overview.vue'
import SystemsBacktestEquityChart from '#systems/components/Backtest/EquityChart.vue'
import SystemsBacktestForcedRotations from '#systems/components/Backtest/ForcedRotations.vue'

const route = useRoute()
const systems = useSystemsStore()
const config = useConfigStore()
const live = useLiveStore()

const systemId = computed(() => route.params.id as string)
const system = computed(() => systems.byId(systemId.value))
const planned = computed(() => system.value?.status !== 'ready')
const state = computed(() => live.stateFor(systemId.value))
const cfg = computed(() => config.get(systemId.value))
const result = computed(() => state.value.result && matchesMarket(state.value.result, cfg.value) ? state.value.result : null)
const tracking = computed(() => result.value?.tracking)
const startChanged = computed(() => tracking.value && tracking.value.startDate !== cfg.value.liveStartDate)

onMounted(() => config.load(systemId.value))

function run() {
  void live.refresh(systemId.value)
}
</script>

<template>
  <SystemsSystemPlanned v-if="planned" :system="system" />
  <div v-else class="qs-live">
    <header class="qs-live__head">
      <h1 class="qs-live__title">Live Evaluation</h1>
      <span class="qs-live__range mono">{{ cfg.liveStartDate }} → Today</span>

      <button class="btn btn-primary qs-live__run" :disabled="state.loading" @click="run">
        {{ state.loading ? (live.runningSystemId === systemId ? 'Evaluating…' : 'Queued…') : 'Run Live Eval' }}
      </button>
    </header>

    <!-- Same track as the backtest: reserved even when idle, so progress never
         moves the header or the panels. -->
    <div class="qs-live__status">
      <div v-if="state.loading" class="qs-live__progress">
        <div class="qs-bar" role="progressbar" aria-label="Live evaluation progress"
          :aria-valuenow="Math.round(state.progressValue * 100)" :aria-valuemin="0" :aria-valuemax="100">
          <div class="qs-bar__fill" :style="{ width: `${Math.round(state.progressValue * 100)}%` }" />
        </div>
        <span class="qs-live__progress-lbl" :title="state.progressLabel || 'Working…'" role="status">{{ state.progressLabel || 'Working…' }}</span>
      </div>
      <div v-else-if="result" class="qs-live__meta mono">
        {{ tracking?.startDate }} → {{ result.asOf }} · {{ result.provider }} · {{ result.mode === 'single_asset' ? result.singleAsset?.pair : `${result.universe.length} assets` }} · Confirmed candles
      </div>
    </div>

    <div v-if="state.error" class="qs-live__error">{{ state.error }}</div>

    <p v-if="startChanged" class="qs-live__notice" role="status">The live start day changed. Run Live Eval to update the graph and history.</p>

    <template v-if="result && tracking">
      <div v-if="tracking.skippedStrategies?.length" class="qs-live__notice" role="status">
        <p v-for="skipped in tracking.skippedStrategies" :key="skipped.key">{{ skipped.label }}: {{ skipped.reason }}</p>
      </div>
      <SystemsBacktestEquityChart v-if="tracking.equityStrategy.length" :result="tracking" class="qs-live__chart" />
      <div v-else class="card qs-live__chart qs-live__no-history">{{ tracking.skippedStrategies?.length ? 'The selected strategy could not be evaluated. Check the indicator data shown above.' : 'No confirmed candles to track in this window. Choose an earlier live start day in Settings, or wait for a candle to close.' }}</div>
      <div class="qs-live__grid">
        <SystemsLiveOverview :result="result" />
        <SystemsBacktestForcedRotations :result="tracking" />
      </div>
    </template>

    <div v-else-if="!state.loading" class="qs-empty">
      <span class="qs-empty__mark">◈</span>
      <p>Set your live start day in Settings, then run an evaluation to track performance, standings and holdings through today.</p>
    </div>
  </div>
</template>

<style scoped>
/* The page owns the full main area so both panels scroll internally instead of
   pushing the layout into one long vertical scroll. */
.qs-live {
  container: manual-live / inline-size;
  display: flex;
  flex-direction: column;
  gap: 12px;
  height: 100%;
  min-height: 0;
}

.qs-live__head {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 10px;
  flex-shrink: 0;
}

.qs-live__title {
  margin: 0;
  font-size: 17px;
  font-weight: 600;
}

.qs-live__range { font-size: 12px; color: var(--qs-text-muted); }
.qs-live__notice { flex-shrink: 0; margin: 0; font-size: 12px; color: var(--qs-warning); }
.qs-live__notice p { margin: 0; }
.qs-live__chart { flex: 1 1 45%; min-height: 230px; }
.qs-live__no-history { display: flex; align-items: center; justify-content: center; padding: 20px; color: var(--qs-text-muted); font-size: 13px; }

.qs-live__meta {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
  color: var(--qs-text-muted);
}

.qs-live__status {
  flex: 0 0 26px;
  min-width: 0;
  height: 26px;
  display: flex;
  flex-direction: column;
  justify-content: center;
}

.qs-live__run {
  margin-left: auto;
  box-sizing: border-box;
  flex: 0 0 128px;
  width: 128px;
  height: 32px;
  white-space: nowrap;
  padding: 6px 14px;
  font-size: 13px;
}

.qs-live__progress {
  min-width: 0;
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12px;
  line-height: 18px;
  color: var(--qs-text-secondary);
}

.qs-live__progress-lbl {
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

.qs-live__error {
  flex-shrink: 0;
  padding: 8px 12px;
  border: 1px solid color-mix(in srgb, var(--qs-error) 50%, var(--qs-border));
  border-radius: var(--qs-radius);
  background: color-mix(in srgb, var(--qs-error) 12%, transparent);
  color: var(--qs-error);
  font-size: 12px;
}

.qs-live__grid {
  flex: 1 1 55%;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  gap: 12px;
}

.qs-live__grid > * {
  min-width: 0;
  min-height: 0;
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

@container manual-live (max-width: 620px) {
  .qs-live__grid {
    grid-template-columns: 1fr;
    grid-template-rows: repeat(2, minmax(0, 1fr));
    flex: 0 0 660px;
  }
}
</style>
