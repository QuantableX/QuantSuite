<script setup lang="ts">
import { useEngine } from '#systems/composables/useEngine'
import type { SingleAssetConfig } from '#systems/types'

const props = defineProps<{ modelValue: SingleAssetConfig; disabled?: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [value: SingleAssetConfig] }>()
const engine = useEngine()
const exchanges = ['coinbase', 'kraken', 'binance', 'bybit', 'okx', 'bitget', 'kucoin', 'gate', 'mexc', 'bitfinex', 'htx']
const pairs = ref<string[]>([])
const loading = ref(false)
const error = ref<string | null>(null)
const filter = ref('')
let generation = 0
const filtered = computed(() => pairs.value.filter(pair => pair === props.modelValue.pair || pair.includes(filter.value.trim().toUpperCase())))
const unavailable = computed(() => !loading.value && !error.value && !!props.modelValue.pair && !pairs.value.includes(props.modelValue.pair))

function set<K extends keyof SingleAssetConfig>(key: K, value: SingleAssetConfig[K]) {
  emit('update:modelValue', { ...props.modelValue, [key]: value, ...(key === 'exchange' ? { pair: '' } : {}) })
}

async function load() {
  const token = ++generation
  const exchange = props.modelValue.exchange
  loading.value = true
  error.value = null
  pairs.value = []
  try {
    const result = await engine.pairMarkets(exchange)
    if (token !== generation) return
    pairs.value = result.pairs
    if (!props.modelValue.pair) {
      const preferred = ['BTC/USD', 'BTC/USDT'].find(pair => result.pairs.includes(pair))
      if (preferred) set('pair', preferred)
    }
  } catch (e) {
    if (token === generation) error.value = String(e)
  } finally {
    if (token === generation) loading.value = false
  }
}
watch(() => props.modelValue.exchange, load, { immediate: true })
onBeforeUnmount(() => { generation++ })
</script>

<template>
  <div class="qs-single-form">
    <div class="qs-single-form__grid">
      <label><span class="label">Exchange</span>
        <select class="select" aria-label="Exchange" :value="modelValue.exchange" :disabled="disabled" @change="set('exchange', ($event.target as HTMLSelectElement).value)">
          <option v-for="exchange in exchanges" :key="exchange" :value="exchange">{{ exchange }}</option>
        </select>
      </label>
      <label><span class="label">Timeframe</span>
        <select class="select" aria-label="Timeframe" :value="modelValue.timeframe" :disabled="disabled" @change="set('timeframe', ($event.target as HTMLSelectElement).value as SingleAssetConfig['timeframe'])">
          <option value="1h">1 hour</option><option value="4h">4 hours</option><option value="1d">Daily</option>
        </select>
      </label>
      <label class="qs-single-form__wide"><span class="label">Pair</span>
        <input v-if="pairs.length > 12" v-model="filter" class="input" placeholder="Filter pairs…" aria-label="Filter pairs" :disabled="disabled" />
        <select class="select" aria-label="Pair" :value="modelValue.pair" :disabled="disabled || loading || !!error" @change="set('pair', ($event.target as HTMLSelectElement).value)">
          <option value="">{{ loading ? 'Loading pairs…' : 'Choose a pair' }}</option>
          <option v-if="modelValue.pair && !pairs.includes(modelValue.pair)" :value="modelValue.pair">{{ modelValue.pair }}</option>
          <option v-for="pair in filtered" :key="pair" :value="pair">{{ pair }}</option>
        </select>
      </label>
      <label class="qs-single-form__wide"><span class="label">Position mode</span>
        <select class="select" aria-label="Position mode" :value="modelValue.direction" :disabled="disabled" @change="set('direction', ($event.target as HTMLSelectElement).value as SingleAssetConfig['direction'])">
          <option value="long_cash">Long/Cash</option><option value="long_short">Long/Short</option>
        </select>
      </label>
    </div>
    <p class="qs-single-form__hint">{{ modelValue.direction === 'long_cash' ? 'Bullish: long the asset. Bearish: hold the quote currency.' : 'Bullish: long. Bearish: short. Neutral signals stay in cash.' }}</p>
    <p class="qs-single-form__hint">Public market data; no exchange account required.</p>
    <p v-if="error" class="qs-single-form__error" role="alert">{{ error }}</p>
    <p v-if="unavailable" class="qs-single-form__error" role="alert">This pair is unavailable on {{ modelValue.exchange }}. Choose a listed pair.</p>
    <button class="btn btn--sm" type="button" :disabled="disabled || loading" @click="load">{{ loading ? 'Loading markets…' : 'Refresh pairs' }}</button>
  </div>
</template>

<style scoped>
.qs-single-form { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
.qs-single-form__grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }
.qs-single-form label { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
.qs-single-form__wide { grid-column: 1 / -1; }
.qs-single-form__hint { margin: 0; font-size: 11px; line-height: 1.5; color: var(--qs-text-muted); }
.qs-single-form__error { margin: 0; font-size: 12px; color: var(--qs-error); overflow-wrap: anywhere; }
.qs-single-form > .btn { align-self: flex-start; }
</style>
