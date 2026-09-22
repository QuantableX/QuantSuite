<script setup lang="ts">
import { reactive } from 'vue'
import { useStrategiesStore } from '#algo/stores/strategies'
import { useExchangeStore } from '#algo/stores/exchange'
import { useAppStore } from '#algo/stores/app'
import type { Strategy, BacktestConfig } from '#algo/types'

const strategiesStore = useStrategiesStore()
const exchangeStore = useExchangeStore()
const appStore = useAppStore()

const props = withDefaults(
  defineProps<{
    strategies?: Strategy[]
    defaultStrategyId?: string
    preSelectedStrategyId?: string | null
  }>(),
  {
    strategies: undefined,
    defaultStrategyId: '',
    preSelectedStrategyId: null,
  },
)

const emit = defineEmits<{
  run: [config: BacktestConfig]
}>()

// Resolve strategies: use prop if provided, fall back to store
const resolvedStrategies = computed(() =>
  props.strategies ?? strategiesStore.strategies,
)

// Resolve initial strategy ID from either prop
const initialStrategyId = computed(() =>
  props.preSelectedStrategyId ?? props.defaultStrategyId ?? '',
)

function toDateString(date: Date): string {
  const y = date.getFullYear()
  const m = String(date.getMonth() + 1).padStart(2, '0')
  const d = String(date.getDate()).padStart(2, '0')
  return `${y}-${m}-${d}`
}

const now = new Date()
const thirtyDaysAgo = new Date(now.getTime() - 30 * 24 * 60 * 60 * 1000)

const direction = ref<'both' | 'long' | 'short'>('both')

// The exchange is one of the connected exchanges and the pair one of that
// exchange's listed pairs (PLAN-QUANTALGO §5) — the provider the candles come
// from is derived from the exchange, never typed.
const form = reactive({
  strategy_id: initialStrategyId.value,
  exchange_id: '',
  pair: '',
  timeframe: appStore.settings.default_timeframe || '1h',
  start_date: toDateString(thirtyDaysAgo),
  end_date: toDateString(now),
  // The same numbers a new bot starts with (Settings → Bot defaults).
  initial_capital: appStore.settings.default_budget || 10000,
  commission: appStore.settings.paper_fee_pct,
})

const selectedExchange = computed(() => exchangeStore.byId(form.exchange_id))

// Start on the default exchange from Settings when it is still connected,
// else on the first connected one; follow the list when the choice vanishes.
watch(
  () => exchangeStore.exchanges,
  (list) => {
    if (form.exchange_id && list.some((e) => e.id === form.exchange_id)) return
    const preferred = appStore.settings.default_exchange_id
    form.exchange_id = (preferred && list.some((e) => e.id === preferred))
      ? preferred
      : (list[0]?.id ?? '')
  },
  { immediate: true },
)

// Sync strategy ID when props change
watch(
  initialStrategyId,
  (val: string) => {
    if (val) form.strategy_id = val
  },
)

const timeframes = [
  { value: '1m', label: '1 Minute' },
  { value: '5m', label: '5 Minutes' },
  { value: '15m', label: '15 Minutes' },
  { value: '1h', label: '1 Hour' },
  { value: '4h', label: '4 Hours' },
  { value: '1d', label: '1 Day' },
]

const directions = [
  { value: 'both', label: 'Both' },
  { value: 'long', label: 'Long Only' },
  { value: 'short', label: 'Short Only' },
]

const canRun = computed(() =>
  !!form.strategy_id && !!form.exchange_id && !!form.pair && !!selectedExchange.value,
)

// The form lives on in the module's warm cache, so its first values would
// outlast a change of the bot defaults; follow Settings until the user
// edits a field here (user, 2026-09-07).
let touched = false
function markTouched() {
  touched = true
}

function seedFromSettings() {
  if (touched) return
  // The same getter New Bot reads — fallbacks included, so an empty stored
  // pair cannot leave this form on the first pair of the list.
  const d = appStore.botDefaults
  form.timeframe = d.timeframe
  form.initial_capital = d.budget
  form.commission = d.fee
  const listed = exchangeStore.pairsByExchange[form.exchange_id]
  if (!listed || listed.includes(d.pair)) form.pair = d.pair
}

watch(() => appStore.botDefaults, seedFromSettings, { immediate: true })
onActivated(seedFromSettings)

function handleSubmit() {
  const exchange = selectedExchange.value
  if (!exchange) return
  emit('run', {
    strategy_id: form.strategy_id,
    exchange: exchange.provider,
    exchange_id: exchange.id,
    pair: form.pair,
    timeframe: form.timeframe,
    start_date: form.start_date,
    end_date: form.end_date,
    initial_capital: form.initial_capital,
    commission: form.commission,
    strategy_params: { direction: direction.value },
  })
}
</script>

<template>
  <form class="card config-form" @submit.prevent="handleSubmit">
    <div class="form-grid">
      <!-- Row 1 -->
      <div class="field">
        <label class="label" for="bt-strategy">Strategy</label>
        <select
          id="bt-strategy"
          v-model="form.strategy_id"
          class="input"
        >
          <option value="" disabled>Select a strategy</option>
          <option
            v-for="strat in resolvedStrategies"
            :key="strat.id"
            :value="strat.id"
          >
            {{ strat.name }}
          </option>
        </select>
      </div>
      <div class="field">
        <label class="label" for="bt-exchange">Exchange</label>
        <AlgoExchangeSelect id="bt-exchange" v-model="form.exchange_id" />
      </div>

      <!-- Row 2 -->
      <div class="field">
        <label class="label" for="bt-pair">Trading Pair</label>
        <AlgoExchangePairSelect
          id="bt-pair"
          v-model="form.pair"
          :exchange-id="form.exchange_id"
          :preferred="appStore.botDefaults.pair"
        />
      </div>
      <div class="field">
        <label class="label" for="bt-timeframe">Timeframe</label>
        <select
          id="bt-timeframe"
          v-model="form.timeframe"
          class="input"
          @change="markTouched"
        >
          <option
            v-for="tf in timeframes"
            :key="tf.value"
            :value="tf.value"
          >
            {{ tf.label }}
          </option>
        </select>
      </div>

      <!-- Row 3 -->
      <div class="field">
        <label class="label" for="bt-start">Start Date</label>
        <input
          id="bt-start"
          v-model="form.start_date"
          class="input"
          type="date"
        />
      </div>
      <div class="field">
        <label class="label" for="bt-end">End Date</label>
        <input
          id="bt-end"
          v-model="form.end_date"
          class="input"
          type="date"
        />
      </div>

      <!-- Row 4 -->
      <div class="field">
        <label class="label" for="bt-capital">Initial Capital ($)</label>
        <input
          id="bt-capital"
          v-model.number="form.initial_capital"
          class="input"
          type="number"
          min="0"
          step="100"
          @input="markTouched"
        />
      </div>
      <div class="field">
        <label class="label" for="bt-commission">Commission (%)</label>
        <input
          id="bt-commission"
          v-model.number="form.commission"
          class="input"
          type="number"
          min="0"
          max="100"
          step="0.01"
          @input="markTouched"
        />
      </div>

      <!-- Row 5 -->
      <div class="field">
        <label class="label" for="bt-direction">Direction</label>
        <select
          id="bt-direction"
          v-model="direction"
          class="input"
        >
          <option
            v-for="d in directions"
            :key="d.value"
            :value="d.value"
          >
            {{ d.label }}
          </option>
        </select>
      </div>
    </div>

    <div class="form-footer">
      <p class="data-note text-muted">
        <template v-if="selectedExchange">Candles from {{ selectedExchange.name }} ({{ selectedExchange.provider }}) public market data · </template>
        capital and commission follow the bot defaults in Settings.
      </p>
      <button
        type="submit"
        class="btn btn-primary run-btn"
        :disabled="!canRun"
      >
        Run Backtest
      </button>
    </div>
  </form>
</template>

<style scoped>
.config-form {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.form-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 14px 20px;
}

@media (max-width: 900px) {
  .form-grid {
    grid-template-columns: 1fr 1fr;
  }
}

.form-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding-top: 4px;
  border-top: 1px solid var(--qa-border-subtle);
}

.field {
  display: flex;
  flex-direction: column;
}

.data-note {
  font-size: 12px;
}

.run-btn {
  min-width: 200px;
  flex-shrink: 0;
}
</style>
