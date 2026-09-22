<script setup lang="ts">
/**
 * Create a bot (PLAN-QUANTALGO §3.4): a strategy on a connected exchange,
 * one of that exchange's pairs, a timeframe, a mode and a budget. Every
 * choice is a dropdown fed by the module's own data; the preflight runs on
 * the draft before anything is saved.
 */
import { useStrategiesStore } from '#algo/stores/strategies'
import { useExchangeStore } from '#algo/stores/exchange'
import { useAppStore } from '#algo/stores/app'
import { useBotsStore } from '#algo/stores/bots'
import type { TradingMode, PreflightResult, PreflightCheck, BotDraft, Bot, BotDefaults } from '#algo/types'

const props = withDefaults(defineProps<{
  visible: boolean
  /** Pre-select this strategy (the Strategies page's Deploy). */
  strategyId?: string | null
}>(), {
  strategyId: null,
})

const emit = defineEmits<{
  close: []
  created: [bot: Bot, started: boolean]
}>()

const strategiesStore = useStrategiesStore()
const exchangeStore = useExchangeStore()
const appStore = useAppStore()
const botsStore = useBotsStore()

// ── Draft ──

const name = ref('')
const strategyId = ref('')
const exchangeId = ref('')
const pair = ref('')
const timeframe = ref('1h')
const tradingMode = ref<TradingMode>('paper')
const budget = ref(10000)
/** The live acknowledgement — ticked again for every new bot. */
const liveConfirmed = ref(false)
const showAdvanced = ref(false)
const riskPerTrade = ref(1)
const maxPositions = ref(3)
const slippage = ref(0.1)
const fee = ref(0.1)
const warmupCandles = ref(200)

const preflightResult = ref<PreflightResult | null>(null)
const isRunningPreflight = ref(false)
const preflightError = ref<string | null>(null)
let preflightRequestId = 0

const isSubmitting = ref(false)
const submitError = ref<string | null>(null)

const timeframeOptions = ['1m', '5m', '15m', '1h', '4h', '1d']


const settings = computed(() => appStore.settings)
const strategies = computed(() => strategiesStore.strategies)
const selectedExchange = computed(() => exchangeStore.byId(exchangeId.value))
const quoteAsset = computed(() => pair.value.split('/')[1] ?? '')

// A new bot starts from the bot defaults in Settings — the store's one
// getter with the fallbacks, the same the backtest form reads — and
// creating one writes what it used back there (user, 2026-09-07).
const LEGACY_DEFAULTS_KEY = 'algo.bot-defaults'

function readDefaults(): BotDefaults {
  return { ...appStore.botDefaults }
}

async function writeDefaults(values: BotDefaults, usedExchangeId: string) {
  try {
    await appStore.saveSettings({
      default_exchange_id: usedExchangeId || null,
      default_timeframe: values.timeframe,
      default_pair: values.pair,
      default_budget: values.budget,
      risk_per_trade: values.risk_per_trade,
      max_concurrent_positions: values.max_positions,
      slippage_tolerance: values.slippage,
      paper_fee_pct: values.fee,
      default_warmup_candles: values.warmup_candles,
    })
  } catch (err) {
    console.error('[create bot] Could not store the bot defaults:', err)
  }
}

// The previous build kept the defaults in this browser only; carry them into
// Settings once, then forget the key.
async function migrateLegacyDefaults(): Promise<boolean> {
  try {
    const raw = localStorage.getItem(LEGACY_DEFAULTS_KEY)
    if (!raw) return false
    localStorage.removeItem(LEGACY_DEFAULTS_KEY)
    const parsed = JSON.parse(raw) as Partial<BotDefaults>
    await writeDefaults({ ...readDefaults(), ...parsed }, settings.value.default_exchange_id ?? '')
    return true
  } catch {
    return false
  }
}

const defaults = ref<BotDefaults>(readDefaults())

// Live order routing exists for Binance and Bybit spot (PLAN-QUANTALGO §4).
const LIVE_PROVIDERS = new Set(['binance', 'bybit'])
const liveSupported = computed(() => !!selectedExchange.value && LIVE_PROVIDERS.has(selectedExchange.value.provider))
const liveSandbox = computed(() => selectedExchange.value?.sandbox === true)
const liveVenueLabel = computed(() => {
  const p = selectedExchange.value?.provider
  if (p === 'binance') return 'Binance spot'
  if (p === 'bybit') return 'Bybit spot'
  return p ? p.charAt(0).toUpperCase() + p.slice(1) : 'the exchange'
})

const draft = computed<BotDraft | null>(() => {
  if (!strategyId.value || !exchangeId.value || !pair.value) return null
  return {
    name: name.value.trim() || null,
    strategy_id: strategyId.value,
    exchange_id: exchangeId.value,
    pair: pair.value,
    timeframe: timeframe.value,
    trading_mode: tradingMode.value,
    budget: budget.value,
    config: {
      risk_per_trade: riskPerTrade.value,
      max_positions: maxPositions.value,
      slippage: slippage.value,
      fee: fee.value,
      warmup_candles: warmupCandles.value,
    },
  }
})

const draftKey = computed(() => (draft.value ? JSON.stringify(draft.value) : ''))

const canCreate = computed(() => draft.value !== null && !isSubmitting.value && budget.value >= 100)
const canStart = computed(() =>
  canCreate.value
  && preflightResult.value !== null
  && preflightResult.value.can_start
  && (tradingMode.value !== 'live' || liveConfirmed.value),
)
const preflightHasFatal = computed(() =>
  preflightResult.value !== null && !preflightResult.value.can_start,
)

// ── Reset ──

function resetForm() {
  name.value = ''
  strategyId.value = props.strategyId ?? strategiesStore.activeStrategyId ?? strategies.value[0]?.id ?? ''
  const preferred = settings.value.default_exchange_id
  exchangeId.value = (preferred && exchangeStore.exchanges.some((e) => e.id === preferred))
    ? preferred
    : (exchangeStore.exchanges[0]?.id ?? '')
  pair.value = ''
  defaults.value = readDefaults()
  timeframe.value = defaults.value.timeframe
  tradingMode.value = 'paper'
  liveConfirmed.value = false
  budget.value = defaults.value.budget
  riskPerTrade.value = defaults.value.risk_per_trade
  maxPositions.value = defaults.value.max_positions
  slippage.value = defaults.value.slippage
  fee.value = defaults.value.fee
  warmupCandles.value = defaults.value.warmup_candles
  showAdvanced.value = false
  preflightResult.value = null
  preflightError.value = null
  preflightRequestId += 1
  isRunningPreflight.value = false
  isSubmitting.value = false
  submitError.value = null
}

watch(() => props.visible, (val) => {
  if (!val) return
  resetForm()
  void migrateLegacyDefaults().then((migrated) => {
    if (migrated && props.visible) resetForm()
  })
})

// A live bot needs a venue with order routing; back to paper otherwise.
watch(liveSupported, (supported) => {
  if (!supported && tradingMode.value === 'live') tradingMode.value = 'paper'
})

// The acknowledgement is for one exact bot: another exchange, pair, budget
// or mode is a new decision.
watch([exchangeId, pair, budget, tradingMode], () => {
  liveConfirmed.value = false
})

// The preflight follows the draft (debounced): it is the module's own
// judgement of the exact bot that would be created.
watchDebounced(draftKey, () => {
  if (draft.value) {
    void runPreflight()
  } else {
    preflightResult.value = null
    preflightError.value = null
    preflightRequestId += 1
  }
}, { debounce: 500 })

async function runPreflight() {
  const current = draft.value
  if (!current) return
  const requestId = ++preflightRequestId
  isRunningPreflight.value = true
  preflightError.value = null
  preflightResult.value = null
  try {
    const result = await botsStore.preflight(current)
    if (requestId === preflightRequestId) preflightResult.value = result
  } catch (err) {
    if (requestId === preflightRequestId) preflightError.value = String(err)
  } finally {
    if (requestId === preflightRequestId) isRunningPreflight.value = false
  }
}

async function submit(start: boolean) {
  const current = draft.value
  if (!current || isSubmitting.value) return
  isSubmitting.value = true
  submitError.value = null
  try {
    const bot = await botsStore.create(current)
    void writeDefaults({
      exchange_id: exchangeId.value || null,
      timeframe: timeframe.value,
      pair: pair.value,
      budget: budget.value,
      risk_per_trade: riskPerTrade.value,
      max_positions: maxPositions.value,
      slippage: slippage.value,
      fee: fee.value,
      warmup_candles: warmupCandles.value,
    }, exchangeId.value)
    if (start) {
      await botsStore.start(bot.id)
    }
    emit('created', bot, start)
    emit('close')
  } catch (err) {
    submitError.value = String(err)
  } finally {
    isSubmitting.value = false
  }
}

function close() {
  if (!isSubmitting.value) emit('close')
}

function onOverlayClick(e: MouseEvent) {
  if (e.target === e.currentTarget) close()
}

function statusIcon(status: PreflightCheck['status']): string {
  switch (status) {
    case 'ok': return '✓'
    case 'warn': return '⚠'
    case 'error': return '✗'
  }
}

onMounted(() => {
  const handler = (e: KeyboardEvent) => {
    if (e.key === 'Escape' && props.visible) close()
  }
  window.addEventListener('keydown', handler)
  onUnmounted(() => window.removeEventListener('keydown', handler))
})
</script>

<template>
  <Teleport to="body">
    <Transition name="create-modal">
      <div
        v-if="visible"
        class="create-overlay"
        @click="onOverlayClick"
        @contextmenu.prevent
      >
        <div class="create-panel" @click.stop>
          <div class="create-header">
            <h2 class="create-title">New Bot</h2>
            <button class="close-btn" aria-label="Close" @click="close">&#10005;</button>
          </div>

          <div class="create-body">
            <div class="section">
              <div class="section-label">Name <span class="optional">(optional — made from the parts otherwise)</span></div>
              <input v-model="name" class="create-input" type="text" placeholder="e.g. Hilbert BTC daily" spellcheck="false" />
            </div>

            <div class="section">
              <div class="section-label">Strategy</div>
              <select v-model="strategyId" class="create-select">
                <option value="" disabled>Select strategy</option>
                <option v-for="s in strategies" :key="s.id" :value="s.id">{{ s.name }}</option>
              </select>
            </div>

            <div class="section">
              <div class="section-label">Exchange</div>
              <AlgoExchangeSelect v-model="exchangeId" />
            </div>

            <div class="section">
              <div class="section-label">Trading Pair</div>
              <AlgoExchangePairSelect
                v-model="pair"
                :exchange-id="exchangeId"
                :preferred="defaults.pair"
              />
            </div>

            <div class="section section--row">
              <div class="section-col">
                <div class="section-label">Timeframe</div>
                <select v-model="timeframe" class="create-select">
                  <option v-for="tf in timeframeOptions" :key="tf" :value="tf">{{ tf }}</option>
                </select>
              </div>
              <div class="section-col">
                <div class="section-label">Budget <span v-if="quoteAsset" class="optional">({{ quoteAsset }})</span></div>
                <input v-model.number="budget" class="create-input mono" type="number" min="100" step="100" />
              </div>
            </div>

            <div class="section">
              <div class="section-label">Trading Mode</div>
              <div class="mode-buttons">
                <button class="mode-btn" :class="{ active: tradingMode === 'paper' }" @click="tradingMode = 'paper'">
                  Paper Trading
                </button>
                <button
                  class="mode-btn mode-btn--live"
                  :class="{ active: tradingMode === 'live' }"
                  :disabled="!liveSupported"
                  :title="liveSupported ? 'Real market orders on ' + liveVenueLabel : 'Live order routing is implemented for Binance and Bybit spot'"
                  @click="tradingMode = 'live'"
                >
                  Live Trading
                </button>
              </div>
              <p v-if="tradingMode === 'paper'" class="mode-hint">
                Paper bots trade on the exchange's live candles with simulated fills. Live order routing is
                available on Binance and Bybit spot.
              </p>
              <div v-else class="live-notice">
                <p class="live-notice__text">
                  <strong>Real orders.</strong> This bot places market orders on {{ liveVenueLabel }} for
                  {{ pair || 'the chosen pair' }} with up to {{ budget }} {{ quoteAsset }} of the account
                  balance<template v-if="liveSandbox"> — sandbox environment, no real funds</template>.
                  Spot is long-only: a sell signal closes the position, a short is never opened. The paper
                  fee and slippage settings do not apply; the venue's fills and fees are booked as reported.
                </p>
                <label class="live-confirm">
                  <input v-model="liveConfirmed" type="checkbox" />
                  <span>I understand this bot trades {{ liveSandbox ? 'on the sandbox account' : 'real money' }}</span>
                </label>
              </div>
            </div>

            <div class="section">
              <button type="button" class="advanced-toggle" @click="showAdvanced = !showAdvanced">
                {{ showAdvanced ? '▾' : '▸' }} Risk settings
                <span class="advanced-summary mono">
                  {{ riskPerTrade }}% · max {{ maxPositions }} · slip {{ slippage }}% · fee {{ fee }}% · warm-up {{ warmupCandles }}
                </span>
              </button>
              <div v-if="showAdvanced" class="advanced-grid">
                <label class="adv-field">
                  <span>Risk per trade (%)</span>
                  <input v-model.number="riskPerTrade" class="create-input mono" type="number" min="0.1" max="10" step="0.1" />
                </label>
                <label class="adv-field">
                  <span>Max positions</span>
                  <input v-model.number="maxPositions" class="create-input mono" type="number" min="1" max="20" />
                </label>
                <label class="adv-field">
                  <span>Slippage (%, paper)</span>
                  <input v-model.number="slippage" class="create-input mono" type="number" min="0" max="5" step="0.01" />
                </label>
                <label class="adv-field">
                  <span>Fee (%, paper)</span>
                  <input v-model.number="fee" class="create-input mono" type="number" min="0" max="5" step="0.01" />
                </label>
                <label class="adv-field">
                  <span>Warm-up candles</span>
                  <input v-model.number="warmupCandles" class="create-input mono" type="number" min="2" max="3000" step="50" />
                </label>
              </div>
            </div>

            <div class="section">
              <div class="section-label-row">
                <span class="section-label" style="margin-bottom: 0;">Preflight Checks</span>
                <button class="btn btn-sm" :disabled="!draft || isRunningPreflight" @click="runPreflight">
                  {{ isRunningPreflight ? 'Checking...' : 'Run Preflight' }}
                </button>
              </div>
              <div v-if="isRunningPreflight" class="loading-msg">Running preflight checks...</div>
              <div v-else-if="preflightError" class="error-msg">{{ preflightError }}</div>
              <div v-else-if="preflightResult" class="checklist">
                <div
                  v-for="check in preflightResult.checks"
                  :key="check.id + check.message"
                  class="check-item"
                  :class="'check-' + check.status"
                >
                  <span class="check-icon">{{ statusIcon(check.status) }}</span>
                  <div class="check-body">
                    <span class="check-label">{{ check.label }}</span>
                    <span class="check-message">{{ check.message }}</span>
                  </div>
                </div>
                <div v-if="preflightHasFatal" class="preflight-blocked">
                  Preflight failed — the bot can be created but not started
                </div>
              </div>
              <div v-else class="empty-msg">Preflight runs when strategy, exchange and pair are chosen</div>
            </div>

            <div v-if="submitError" class="error-msg">{{ submitError }}</div>
          </div>

          <div class="create-footer">
            <button class="btn" :disabled="isSubmitting" @click="close">Cancel</button>
            <button class="btn" :disabled="!canCreate" @click="submit(false)">
              {{ isSubmitting ? 'Working...' : 'Create' }}
            </button>
            <button class="btn btn-success start-btn" :disabled="!canStart" @click="submit(true)">
              <span class="start-icon">&#9654;</span>
              Create &amp; Start
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style>
/* Unscoped: the panel is teleported to body. Everything hangs off .create-overlay. */
.create-overlay {
  position: fixed;
  inset: 0;
  z-index: 1000;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.55);
  backdrop-filter: blur(2px);
}

.create-panel {
  width: 600px;
  max-width: 95vw;
  max-height: 88vh;
  display: flex;
  flex-direction: column;
  background: var(--qa-bg-sidebar);
  border: 1px solid var(--qa-border);
  border-radius: var(--qa-radius-lg);
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.4);
}

.create-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px 20px;
  border-bottom: 1px solid var(--qa-border);
  flex-shrink: 0;
}

.create-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--qa-text);
  margin: 0;
}

.create-overlay .close-btn {
  width: 24px;
  height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--qa-text-muted);
  font-size: 12px;
  cursor: pointer;
}

.create-overlay .close-btn:hover {
  color: var(--qa-text);
}

.create-body {
  padding: 20px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.create-footer {
  display: flex;
  justify-content: flex-end;
  align-items: center;
  gap: 8px;
  padding: 12px 20px;
  border-top: 1px solid var(--qa-border);
  flex-shrink: 0;
}

.create-overlay .section {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding-bottom: 16px;
  border-bottom: 1px solid var(--qa-border-subtle);
}

.create-overlay .section:last-child {
  border-bottom: none;
  padding-bottom: 0;
}

.create-overlay .section--row {
  flex-direction: row;
  gap: 12px;
}

.create-overlay .section-col {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}

.create-overlay .section-label {
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  font-weight: 500;
  color: var(--qa-text-muted);
  margin-bottom: 2px;
}

.create-overlay .optional {
  text-transform: none;
  letter-spacing: 0;
  font-weight: 400;
}

.create-overlay .section-label-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 4px;
}

.create-select,
.create-input {
  width: 100%;
  padding: 8px 12px;
  border-radius: 8px;
  font-size: 12px;
  outline: none;
  border: 1px solid var(--qa-border);
  background: var(--qa-bg-input, var(--qa-bg-hover));
  color: var(--qa-text);
  transition: border-color var(--qa-transition);
}

.create-select {
  cursor: pointer;
}

.create-select:focus,
.create-input:focus {
  border-color: var(--qa-accent);
}

.create-select option {
  background: var(--qa-bg-sidebar);
  color: var(--qa-text);
}

.create-overlay .mono {
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', Menlo, Consolas, monospace;
}

.create-overlay .mode-buttons {
  display: flex;
  gap: 6px;
}

.create-overlay .mode-btn {
  flex: 1;
  padding: 8px 12px;
  border-radius: 8px;
  font-size: 12px;
  font-weight: 500;
  border: 1px solid var(--qa-border);
  background: var(--qa-bg-hover);
  color: var(--qa-text);
  cursor: pointer;
  transition: all var(--qa-transition);
}

.create-overlay .mode-btn.active {
  background: var(--qa-accent);
  color: var(--qa-bg);
  border-color: var(--qa-accent);
}

.create-overlay .mode-btn--live.active {
  background: var(--qa-error);
  border-color: var(--qa-error);
  color: #fff;
}

.create-overlay .mode-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.create-overlay .live-notice {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px 12px;
  font-size: 11px;
  line-height: 1.5;
  color: var(--qa-text);
  background: color-mix(in srgb, var(--qa-error) 10%, transparent);
  border: 1px solid color-mix(in srgb, var(--qa-error) 30%, transparent);
  border-radius: 8px;
}

.create-overlay .live-notice__text {
  margin: 0;
}

.create-overlay .live-confirm {
  display: flex;
  align-items: center;
  gap: 8px;
  font-weight: 500;
  cursor: pointer;
}

.create-overlay .live-confirm input {
  accent-color: var(--qa-error);
}

.create-overlay .mode-hint {
  font-size: 11px;
  color: var(--qa-text-muted);
  line-height: 1.4;
}

.create-overlay .advanced-toggle {
  display: flex;
  align-items: center;
  gap: 8px;
  background: none;
  border: none;
  padding: 0;
  color: var(--qa-text-secondary);
  font-size: 12px;
  cursor: pointer;
  text-align: left;
}

.create-overlay .advanced-toggle:hover {
  color: var(--qa-text);
}

.create-overlay .advanced-summary {
  font-size: 11px;
  color: var(--qa-text-muted);
}

.create-overlay .advanced-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px 12px;
}

.create-overlay .adv-field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 11px;
  color: var(--qa-text-secondary);
}

.create-overlay .checklist {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.create-overlay .check-item {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 8px 12px;
  background: var(--qa-bg-hover);
  border: 1px solid var(--qa-border);
  border-radius: 8px;
}

.create-overlay .check-icon {
  flex-shrink: 0;
  width: 16px;
  font-size: 12px;
  line-height: 1;
  margin-top: 1px;
  text-align: center;
}

.create-overlay .check-ok .check-icon { color: var(--qa-success); }
.create-overlay .check-warn .check-icon { color: var(--qa-warning); }
.create-overlay .check-error .check-icon { color: var(--qa-error); }

.create-overlay .check-body {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.create-overlay .check-label {
  font-size: 12px;
  font-weight: 500;
  color: var(--qa-text);
}

.create-overlay .check-message {
  font-size: 11px;
  color: var(--qa-text-muted);
  line-height: 1.4;
}

.create-overlay .preflight-blocked {
  font-size: 11px;
  color: var(--qa-warning);
  padding: 6px 12px;
  background: color-mix(in srgb, var(--qa-warning) 10%, transparent);
  border: 1px solid color-mix(in srgb, var(--qa-warning) 25%, transparent);
  border-radius: 6px;
  margin-top: 4px;
}

.create-overlay .empty-msg {
  font-size: 12px;
  color: var(--qa-text-muted);
  padding: 8px 12px;
  background: var(--qa-bg-hover);
  border: 1px solid var(--qa-border);
  border-radius: 8px;
}

.create-overlay .loading-msg {
  font-size: 12px;
  color: var(--qa-text-secondary);
  padding: 8px 12px;
}

.create-overlay .error-msg {
  font-size: 12px;
  color: var(--qa-error);
  padding: 8px 12px;
  background: color-mix(in srgb, var(--qa-error) 10%, transparent);
  border: 1px solid color-mix(in srgb, var(--qa-error) 25%, transparent);
  border-radius: 8px;
}

.create-overlay .start-btn {
  display: flex;
  align-items: center;
  gap: 6px;
}

.create-overlay .start-icon {
  font-size: 10px;
  line-height: 1;
}

.create-modal-enter-active,
.create-modal-leave-active {
  transition: opacity 150ms ease;
}

.create-modal-enter-from,
.create-modal-leave-to {
  opacity: 0;
}
</style>
