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

// Backtests run on the bot timeframes only (1h, 4h, 1d — stores/app.ts).
const timeframes = [
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
      <!-- What runs -->
      <div class="field field--wide">
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
      <div class="field field--wide">
        <label class="label" for="bt-exchange">Exchange</label>
        <AlgoExchangeSelect id="bt-exchange" v-model="form.exchange_id" />
      </div>
      <div class="field field--wide">
        <label class="label" for="bt-pair">Trading Pair</label>
        <AlgoExchangePairSelect
          id="bt-pair"
          v-model="form.pair"
          :exchange-id="form.exchange_id"
          :preferred="appStore.botDefaults.pair"
        />
      </div>

      <!-- How it runs -->
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
  container-type: inline-size;
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap);
}

/* Six columns: what runs takes two each, how it runs one each. */
.form-grid {
  display: grid;
  grid-template-columns: repeat(6, minmax(0, 1fr));
  gap: var(--qss-gap);
}

.field--wide {
  grid-column: span 2;
}

.field {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.form-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--qss-gap);
  padding-top: var(--qss-gap-tight);
  border-top: 1px solid var(--qa-border-subtle);
}

.data-note {
  min-width: 0;
  font-size: var(--qss-text-label);
}

.run-btn {
  min-width: 128px;
  flex-shrink: 0;
}

@container (max-width: 820px) {
  .form-grid {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .field--wide {
    grid-column: auto;
  }

  .field--wide:nth-child(3) {
    grid-column: 1 / -1;
  }
}

@container (max-width: 480px) {
  .form-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .form-footer {
    flex-wrap: wrap;
  }
}
</style>
