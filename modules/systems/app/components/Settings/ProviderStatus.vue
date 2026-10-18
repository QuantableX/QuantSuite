<script setup lang="ts">
import { useTauriEvent } from '@quantsuite/core'
import { useEngine } from '#systems/composables/useEngine'
import { useAppStore } from '#systems/stores/app'
import type { RankingArchiveStatus } from '#systems/types'

const app = useAppStore()
const engine = useEngine()

const providers = [
  { id: 'cmc', label: 'CoinMarketCap snapshots', note: 'Historical top-N rankings, daily or weekly' },
  { id: 'local', label: 'Local reconstruction', note: 'CoinGecko market-cap fallback' },
  { id: 'ccxt', label: 'CCXT / Binance', note: 'Daily OHLCV candles' },
  { id: 'coingecko', label: 'CoinGecko market chart', note: 'OHLCV fallback' },
]

const engineRunning = computed(() => app.engineStatus.status === 'running')

// The daily ranking archive: one CMC snapshot per day, downloaded once and
// kept. The download runs inside the engine and reports every finished day.
const archive = ref<RankingArchiveStatus | null>(null)
const archiveError = ref<string | null>(null)
const busy = ref(false)

async function archiveCall(action: 'status' | 'build' | 'cancel') {
  busy.value = true
  try {
    archive.value = await engine.rankingArchive(action)
    archiveError.value = null
  } catch (e) {
    archiveError.value = String(e)
  } finally {
    busy.value = false
  }
}

const progress = computed(() => {
  const a = archive.value
  return a && a.total ? a.done / a.total : 0
})

// One request per day at the engine's 1.2 s pace.
const minutes = computed(() => {
  const a = archive.value
  if (!a) return 0
  const left = a.running ? a.total - a.done : a.coverage.missingDays
  return Math.max(1, Math.round(left * 1.2 / 60))
})

const n = (value: number) => value.toLocaleString('en-US')

onMounted(() => archiveCall('status'))
useTauriEvent<RankingArchiveStatus>('engine:ranking_archive', payload => { archive.value = payload })
</script>

<template>
  <div class="card qs-prov">
    <div class="qs-prov__head">
      <h3 class="qs-prov__title">Data Providers</h3>
      <span class="pill">{{ engineRunning ? 'engine up' : 'engine down' }}</span>
    </div>
    <ul class="qs-prov__list">
      <li v-for="p in providers" :key="p.id" class="qs-prov__item">
        <span class="qs-prov__dot" :class="engineRunning ? 'qs-prov__dot--ok' : 'qs-prov__dot--idle'" />
        <div class="qs-prov__meta">
          <span class="qs-prov__label">{{ p.label }}</span>
          <span class="qs-prov__note">{{ p.note }}</span>
        </div>
      </li>
    </ul>

    <section class="qs-prov__archive" aria-label="Daily ranking archive">
      <div class="qs-prov__archive-head">
        <span class="qs-prov__label">Daily ranking archive</span>
        <span v-if="archive" class="qs-prov__note">
          <span class="mono">{{ n(archive.coverage.days) }}</span> of
          <span class="mono">{{ n(archive.coverage.totalDays) }}</span> days
        </span>
      </div>
      <span v-if="archive" class="qs-prov__note">
        <span class="mono">{{ archive.coverage.start }}</span> to <span class="mono">{{ archive.coverage.end }}</span>
      </span>

      <template v-if="archive?.running">
        <div class="qs-bar" role="progressbar" aria-label="Daily ranking archive download"
          :aria-valuenow="Math.round(progress * 100)" :aria-valuemin="0" :aria-valuemax="100">
          <div class="qs-bar__fill" :style="{ width: `${Math.round(progress * 100)}%` }" />
        </div>
        <div class="qs-prov__archive-row">
          <span class="qs-prov__note" role="status">
            Day <span class="mono">{{ n(archive.done) }}</span> of <span class="mono">{{ n(archive.total) }}</span>
            <template v-if="archive.current"> · <span class="mono">{{ archive.current }}</span></template>
            · about <span class="mono">{{ minutes }}</span> min left
          </span>
          <button class="btn btn-ghost" :disabled="busy" @click="archiveCall('cancel')">Stop</button>
        </div>
      </template>
      <template v-else-if="archive && archive.coverage.missingDays > 0">
        <button class="btn" :disabled="busy" @click="archiveCall('build')">Download {{ n(archive.coverage.missingDays) }} days</button>
        <span class="qs-prov__note">
          <template v-if="archive.cancelled">Stopped. It continues where it left off. </template>
          About <span class="mono">{{ minutes }}</span> min in the background; backtests keep working.
        </span>
      </template>
      <span v-else-if="archive" class="qs-prov__note">Complete. New days are added when a run needs them.</span>
      <span v-else class="qs-prov__note">{{ busy ? 'Loading…' : 'Start the engine to read the archive.' }}</span>

      <p v-if="archive?.error || archiveError" class="qs-prov__error" role="alert">{{ archive?.error || archiveError }}</p>
    </section>
  </div>
</template>

<style scoped>
.qs-prov {
  padding: 12px 14px;
}

.qs-prov__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  margin-bottom: 10px;
}

.qs-prov__title {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
}

.qs-prov__list {
  margin: 0;
  padding: 0;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.qs-prov__item {
  display: flex;
  align-items: flex-start;
  gap: 10px;
}

.qs-prov__dot {
  margin-top: 5px;
  width: 8px;
  height: 8px;
  border-radius: 999px;
  flex-shrink: 0;
}

.qs-prov__dot--ok {
  background: var(--qs-success);
}

.qs-prov__dot--idle {
  background: var(--qs-text-muted);
}

.qs-prov__meta {
  display: flex;
  flex-direction: column;
}

.qs-prov__label {
  font-size: 12px;
  color: var(--qs-text);
}

.qs-prov__note {
  font-size: 11px;
  color: var(--qs-text-muted);
}

.qs-prov__archive {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 12px;
  padding-top: 12px;
  border-top: 1px solid var(--qs-border);
}

.qs-prov__archive-head,
.qs-prov__archive-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.qs-prov__archive .btn {
  align-self: flex-start;
}

/* Same bar as the backtest page's progress track, on the card's surface. */
.qs-bar {
  width: 100%;
  height: 4px;
  border-radius: 999px;
  background: color-mix(in srgb, var(--qs-text-muted) 30%, transparent);
  overflow: hidden;
}

.qs-bar__fill {
  height: 100%;
  background: var(--qs-accent);
  transition: width 200ms ease;
}

.qs-prov__error {
  margin: 0;
  font-size: 11px;
  color: var(--qs-error);
}
</style>
