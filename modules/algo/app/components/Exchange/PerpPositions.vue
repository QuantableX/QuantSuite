<script setup lang="ts">
/**
 * Perp positions on the Hyperliquid accounts with their liquidation price and
 * distance from the mark (PLAN-EXECUTION-VENUES §6, Venues 4/5). A position
 * closer to liquidation than the Settings minimum is marked.
 */
import { invoke } from '@tauri-apps/api/core'

interface PerpPosition {
  exchange_id: string; account: string; coin: string; side: string; size: number; entry_price: number
  mark_price: number | null; liquidation_price: number | null; liquidation_distance: number | null
  leverage: number | null; warn: boolean
}

const props = withDefaults(defineProps<{ exchangeId?: string | null; coin?: string | null }>(), { exchangeId: null, coin: null })
const rows = ref<PerpPosition[]>([])
const error = ref<string | null>(null)

const shown = computed(() => rows.value.filter((r) => !props.coin || r.coin.toUpperCase() === props.coin.toUpperCase()))

async function load() {
  error.value = null
  try {
    rows.value = await invoke<PerpPosition[]>('plugin:algo|venue_perp_positions', { exchange_id: props.exchangeId ?? null })
  } catch (e) {
    error.value = String(e)
  }
}

function price(n: number | null) {
  return n == null ? '—' : n.toLocaleString('en-US', { maximumFractionDigits: n < 10 ? 4 : 1 })
}

watch(() => `${props.exchangeId}:${props.coin}`, load)
onMounted(load)
onActivated(load)
defineExpose({ load })
</script>

<template>
  <div v-if="error || shown.length" class="perp">
    <p v-if="error" class="text-error perp__note">{{ error }}</p>
    <div v-for="r in shown" :key="r.exchange_id + r.coin + r.side" class="perp__row" :class="{ 'perp__row--warn': r.warn }">
      <span class="perp__what">{{ r.coin }} {{ r.side }} <span class="mono">{{ r.size }}</span><template v-if="r.leverage"> · {{ r.leverage }}x</template></span>
      <span class="perp__liq mono">
        liq. {{ price(r.liquidation_price) }}<template v-if="r.liquidation_distance != null"> · {{ (r.liquidation_distance * 100).toFixed(1) }} %</template>
      </span>
      <span class="perp__account">{{ r.account }}</span>
    </div>
  </div>
</template>

<style scoped>
.perp { display: flex; flex-direction: column; gap: 4px; font-size: 12px; }
.perp__note { font-size: 12px; }
.perp__row { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 2px 8px; }
.perp__row--warn .perp__liq { color: var(--qa-error); font-weight: 600; }
.perp__what { color: var(--qa-text); }
.perp__liq { color: var(--qa-text-secondary); text-align: right; }
.perp__account { grid-column: 1 / -1; color: var(--qa-text-muted); }
</style>
