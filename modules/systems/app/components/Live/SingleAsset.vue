<script setup lang="ts">
import type { LiveResult } from '#systems/types'
defineProps<{ result: LiveResult; compact?: boolean }>()
</script>

<template>
  <section v-if="result.singleAsset && result.singleAssetSignal" class="card qs-single-live" :class="{ 'qs-single-live--compact': compact }">
    <header v-if="!compact">
      <span class="label">Single Asset</span>
      <h2>{{ result.singleAsset.pair }}</h2>
      <p>{{ result.singleAsset.exchange }} · {{ result.singleAsset.timeframe }} · {{ result.singleAsset.direction === 'long_short' ? 'Long/Short' : 'Long/Cash' }}</p>
    </header>
    <div class="qs-single-live__metrics">
      <div><span class="label">Confirmed signal</span><strong class="qs-single-live__signal">{{ result.singleAssetSignal.signal }}</strong></div>
      <div><span class="label">Next position</span><strong>{{ result.best }}</strong></div>
      <div><span class="label">Last confirmed close</span><strong class="mono">{{ result.singleAssetSignal.close.toLocaleString(undefined, { maximumFractionDigits: 8 }) }} {{ result.singleAsset.pair.split('/')[1] }}</strong></div>
      <div><span class="label">Candle closed at (UTC)</span><strong class="mono">{{ result.singleAssetSignal.closedAt.replace('T', ' ').replace('Z', '') }}</strong></div>
    </div>
    <p class="qs-single-live__note">The confirmed signal applies from the next candle open.</p>
  </section>
</template>

<style scoped>
.qs-single-live { padding: 24px; min-width: 0; overflow: auto; }
.qs-single-live header { display: flex; flex-direction: column; gap: 8px; }
.qs-single-live h2 { margin: 0; font-size: 24px; font-weight: 600; }
.qs-single-live p { margin: 0; font-size: 12px; color: var(--qs-text-secondary); }
.qs-single-live__metrics { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 24px; margin: 28px 0; }
.qs-single-live__metrics > div { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
.qs-single-live strong { font-size: 17px; font-weight: 600; overflow-wrap: anywhere; }
.qs-single-live__signal { text-transform: capitalize; }
.qs-single-live--compact .qs-single-live__metrics { margin-top: 0; gap: 14px; }
.qs-single-live--compact strong { font-size: 13px; }
.qs-single-live .qs-single-live__note { font-size: 11px; color: var(--qs-text-muted); }
@media (max-width: 800px) { .qs-single-live__metrics { grid-template-columns: 1fr; } }
</style>
