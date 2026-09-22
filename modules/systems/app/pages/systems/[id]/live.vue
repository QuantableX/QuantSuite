<script setup lang="ts">
definePageMeta({ layout: 'systems', path: '/algo/manual/:id/live' })

import { useSystemsStore } from '#systems/stores/systems'
import { useConfigStore } from '#systems/stores/config'
import { useLiveStore } from '#systems/stores/live'

const route = useRoute()
const systems = useSystemsStore()
const config = useConfigStore()
const live = useLiveStore()

const systemId = computed(() => route.params.id as string)
const system = computed(() => systems.byId(systemId.value))
const planned = computed(() => system.value?.status !== 'ready')
const state = computed(() => live.stateFor(systemId.value))

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

      <div class="qs-live__status">
        <div v-if="state.loading" class="qs-live__progress">
          <div class="qs-bar"><div class="qs-bar__fill" :style="{ width: `${Math.round(state.progressValue * 100)}%` }" /></div>
          <span class="qs-live__progress-lbl" :title="state.progressLabel || 'Working…'" role="status">{{ state.progressLabel || 'Working…' }}</span>
        </div>
        <div v-else-if="state.result" class="qs-live__meta mono"
          :title="`${state.result.asOf} · ${state.result.provider} · ${state.result.universe.length} assets`">
          {{ state.result.asOf }} · {{ state.result.provider }} · {{ state.result.universe.length }} assets
        </div>
      </div>

      <button class="btn btn-primary qs-live__run" :disabled="state.loading" @click="run">
        {{ state.loading ? (live.runningSystemId === systemId ? 'Evaluating…' : 'Queued…') : 'Run Live Eval' }}
      </button>
    </header>

    <div v-if="state.error" class="qs-live__error">{{ state.error }}</div>

    <div v-if="state.result" class="qs-live__grid">
      <SystemsLiveRanking :result="state.result" />
      <SystemsLiveScoreMatrix :result="state.result" />
    </div>

    <div v-else-if="!state.loading" class="qs-empty">
      <span class="qs-empty__mark">◈</span>
      <p>No evaluation yet. Run an evaluation to score today's coins.</p>
    </div>
  </div>
</template>

<style scoped>
/* The page owns the full main area so both panels scroll internally instead of
   pushing the layout into one long vertical scroll. */
.qs-live {
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

.qs-live__meta {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
  color: var(--qs-text-muted);
}

.qs-live__status {
  flex: 1 1 140px;
  min-width: 0;
  max-width: 400px;
  height: 32px;
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
  font-size: 11px;
  line-height: 1.3;
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
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: clamp(240px, 32%, 340px) 1fr;
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

@media (max-width: 1120px) {
  .qs-live {
    height: auto;
  }

  .qs-live__grid {
    grid-template-columns: 1fr;
  }
}
</style>
