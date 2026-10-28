<script setup lang="ts">
import { useEngine } from '#systems/composables/useEngine'
import { useTauriEvent } from '@quantsuite/core'
import type { CacheStats } from '#systems/types'

type Scope = 'rankings' | 'ohlcv' | 'all'

const engine = useEngine()
const stats = ref<CacheStats | null>(null)
const loading = ref(false)
const clearing = ref(false)
const error = ref<string | null>(null)
// The scope waiting for confirmation: nothing is deleted before the dialog says yes.
const pending = ref<Scope | null>(null)
const cancelButton = ref<HTMLButtonElement | null>(null)

function fmtBytes(bytes: number): string {
  if (!bytes) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  return `${(bytes / Math.pow(1024, i)).toFixed(1)} ${units[i]}`
}

function rows(count: number | undefined): string {
  return count === undefined ? '' : ` (${count.toLocaleString('en-US')} rows)`
}

const confirmText = computed(() => {
  switch (pending.value) {
    case 'ohlcv': return {
      title: 'Clear price data?',
      what: `Deletes every saved candle${rows(stats.value?.ohlcv)} and the listing dates the exchanges reported.`,
      next: 'The next backtest or live evaluation downloads them again: Binance first, then Bybit, OKX, Coinbase and KuCoin. This can take several minutes.',
      action: 'Delete price data',
    }
    case 'rankings': return {
      title: 'Clear rankings?',
      what: `Deletes every saved CoinMarketCap ranking${rows(stats.value?.rankings)}. Prices stay.`,
      next: 'The rankings are downloaded again day by day, which takes a long time.',
      action: 'Delete rankings',
    }
    case 'all': return {
      title: 'Clear the whole cache?',
      what: 'Deletes prices, rankings, coin mappings and every saved download note.',
      next: 'Everything is downloaded again on the next run, which takes a long time.',
      action: 'Delete everything',
    }
    default: return null
  }
})

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

function ask(scope: Scope) {
  pending.value = scope
  void nextTick(() => cancelButton.value?.focus())
}

function cancel() {
  if (!clearing.value) pending.value = null
}

async function confirmClear() {
  const scope = pending.value
  if (!scope) return
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
    pending.value = null
  }
}

onMounted(refresh)
onDeactivated(cancel)
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
        <span class="qs-cache__num mono">{{ stats.rankings.toLocaleString('en-US') }}</span>
        <span class="qs-cache__lbl">ranking rows</span>
      </div>
      <div class="qs-cache__stat">
        <span class="qs-cache__num mono">{{ stats.ohlcv.toLocaleString('en-US') }}</span>
        <span class="qs-cache__lbl">ohlcv rows</span>
      </div>
      <div class="qs-cache__stat">
        <span class="qs-cache__num mono">{{ stats.coins.toLocaleString('en-US') }}</span>
        <span class="qs-cache__lbl">mapped coins</span>
      </div>
      <div class="qs-cache__stat">
        <span class="qs-cache__num mono">{{ fmtBytes(stats.sizeBytes) }}</span>
        <span class="qs-cache__lbl">db size</span>
      </div>
    </div>
    <p v-else class="qs-cache__empty">{{ loading ? 'Loading…' : 'Start the engine to read cache stats.' }}</p>

    <div class="qs-cache__actions">
      <button class="btn btn-ghost" :disabled="clearing" @click="ask('ohlcv')">Clear prices</button>
      <button class="btn btn-ghost" :disabled="clearing" @click="ask('rankings')">Clear rankings</button>
      <button class="btn btn-ghost" :disabled="clearing" @click="ask('all')">Clear all</button>
    </div>

    <p v-if="error" class="qs-cache__error">{{ error }}</p>

    <Teleport to="body">
      <!-- data-module: teleported out of the module root, the overlay carries
           the scope itself or it loses the theme bridge and the .btn styles. -->
      <div v-if="confirmText" class="qs-cache__overlay" data-module="systems" @mousedown.self="cancel">
        <div class="qs-cache__dialog" role="dialog" aria-modal="true" aria-labelledby="qs-cache-confirm-title"
          @keydown.esc.stop="cancel">
          <h3 id="qs-cache-confirm-title" class="qs-cache__dialog-title">{{ confirmText.title }}</h3>
          <p class="qs-cache__dialog-text">{{ confirmText.what }}</p>
          <p class="qs-cache__dialog-text">{{ confirmText.next }}</p>
          <p v-if="stats?.path" class="qs-cache__dialog-path mono" :title="stats.path">{{ stats.path }}</p>
          <p class="qs-cache__dialog-text qs-cache__dialog-warn">This cannot be undone.</p>
          <div class="qs-cache__dialog-actions">
            <button ref="cancelButton" class="btn" :disabled="clearing" @click="cancel">Cancel</button>
            <button class="btn qs-cache__danger" :disabled="clearing" @click="confirmClear">
              {{ clearing ? 'Deleting…' : confirmText.action }}
            </button>
          </div>
        </div>
      </div>
    </Teleport>
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

.qs-cache__overlay {
  position: fixed;
  inset: 0;
  z-index: 1100;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 16px;
  background: rgba(0, 0, 0, 0.55);
}

.qs-cache__dialog {
  display: flex;
  flex-direction: column;
  gap: 10px;
  width: 420px;
  max-width: 100%;
  padding: 20px;
  border: 1px solid var(--qs-border);
  border-radius: var(--qs-radius-lg);
  background: var(--qs-bg-card);
  color: var(--qs-text);
  box-shadow: 0 16px 48px rgba(0, 0, 0, 0.45);
}

.qs-cache__dialog-title {
  margin: 0 0 2px;
  font-size: 14px;
  font-weight: 600;
}

.qs-cache__dialog-text {
  margin: 0;
  font-size: 12.5px;
  line-height: 1.5;
  color: var(--qs-text-secondary);
}

.qs-cache__dialog-path {
  margin: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  padding: 6px 8px;
  border: 1px solid var(--qs-border-subtle);
  border-radius: var(--qs-radius);
  background: var(--qs-bg-input);
  font-size: 11px;
  color: var(--qs-text-muted);
}

.qs-cache__dialog-warn {
  color: var(--qs-text);
}

.qs-cache__dialog-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 6px;
}

.qs-cache__dialog-actions .btn {
  font-size: 12.5px;
  padding: 7px 14px;
}

/* Beats the module's [data-module] .btn rules, which carry the same weight. */
.qs-cache__dialog-actions .qs-cache__danger {
  border-color: var(--qs-error);
  background: transparent;
  color: var(--qs-error);
}

.qs-cache__dialog-actions .qs-cache__danger:hover:not(:disabled) {
  border-color: var(--qs-error);
  background: color-mix(in srgb, var(--qs-error) 14%, transparent);
}
</style>
