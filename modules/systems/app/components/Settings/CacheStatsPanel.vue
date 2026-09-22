<script setup lang="ts">
import { useEngine } from '#systems/composables/useEngine'
import { useTauriEvent } from '@quantsuite/core'
import type { CacheStats } from '#systems/types'

const engine = useEngine()
const stats = ref<CacheStats | null>(null)
const loading = ref(false)
const clearing = ref(false)
const error = ref<string | null>(null)

function fmtBytes(bytes: number): string {
  if (!bytes) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  return `${(bytes / Math.pow(1024, i)).toFixed(1)} ${units[i]}`
}

async function refresh() {
  loading.value = true
  try {
    stats.value = await engine.cacheStats()
    error.value = null
  } catch (e) {
    // Without this the empty state passes a failed invoke off as "no data".
    stats.value = null
    error.value = String(e)
  } finally {
    loading.value = false
  }
}

async function clear(scope: 'rankings' | 'ohlcv' | 'all') {
  clearing.value = true
  error.value = null
  try {
    await engine.clearCache(scope)
    await refresh()
  } catch (e) {
    // Like refresh() above: a failed invoke has to say so, not just re-enable
    // the buttons as if the cache had been wiped.
    error.value = String(e)
  } finally {
    clearing.value = false
  }
}

onMounted(refresh)
useTauriEvent('cache:updated', () => refresh())
</script>

<template>
  <div class="card qs-cache">
    <div class="qs-cache__head">
      <h3 class="qs-cache__title">Local Cache</h3>
      <button class="btn btn-ghost qs-cache__refresh" :disabled="loading" @click="refresh">↻</button>
    </div>

    <div v-if="stats" class="qs-cache__stats">
      <div class="qs-cache__stat">
        <span class="qs-cache__num mono">{{ stats.rankings.toLocaleString() }}</span>
        <span class="qs-cache__lbl">ranking rows</span>
      </div>
      <div class="qs-cache__stat">
        <span class="qs-cache__num mono">{{ stats.ohlcv.toLocaleString() }}</span>
        <span class="qs-cache__lbl">ohlcv rows</span>
      </div>
      <div class="qs-cache__stat">
        <span class="qs-cache__num mono">{{ stats.coins.toLocaleString() }}</span>
        <span class="qs-cache__lbl">mapped coins</span>
      </div>
      <div class="qs-cache__stat">
        <span class="qs-cache__num mono">{{ fmtBytes(stats.sizeBytes) }}</span>
        <span class="qs-cache__lbl">db size</span>
      </div>
    </div>
    <p v-else class="qs-cache__empty">{{ loading ? 'Loading…' : 'Start the engine to read cache stats.' }}</p>

    <div class="qs-cache__actions">
      <button class="btn btn-ghost" :disabled="clearing" @click="clear('rankings')">Clear rankings</button>
      <button class="btn btn-ghost" :disabled="clearing" @click="clear('ohlcv')">Clear OHLCV</button>
      <button class="btn btn-ghost" :disabled="clearing" @click="clear('all')">Clear all</button>
    </div>

    <p v-if="error" class="qs-cache__error">{{ error }}</p>
  </div>
</template>

<style scoped>
.qs-cache {
  padding: 12px 14px;
}

.qs-cache__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 10px;
}

.qs-cache__title {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
}

.qs-cache__refresh {
  width: 26px;
  height: 26px;
  padding: 0;
}

.qs-cache__stats {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
  margin-bottom: 12px;
}

.qs-cache__stat {
  display: flex;
  flex-direction: column;
  gap: 1px;
  padding: 8px 10px;
  border: 1px solid var(--qs-border-subtle);
  border-radius: var(--qs-radius);
  background: var(--qs-bg-input);
}

.qs-cache__num {
  font-size: 16px;
  font-weight: 600;
  color: var(--qs-text);
}

.qs-cache__lbl {
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--qs-text-muted);
}

.qs-cache__empty {
  margin: 0 0 12px;
  font-size: 12px;
  color: var(--qs-text-muted);
}

.qs-cache__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.qs-cache__actions .btn {
  font-size: 12px;
  padding: 6px 10px;
}

.qs-cache__error {
  margin: 10px 0 0;
  font-size: 12px;
  color: var(--qs-error);
}
</style>
