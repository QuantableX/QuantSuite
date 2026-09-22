<script setup lang="ts">
definePageMeta({ layout: 'algo' })

import { useAppStore } from '#algo/stores/app'
import { useExchangeStore } from '#algo/stores/exchange'
import { invoke } from '@tauri-apps/api/core'
import type { AppSettings } from '#algo/types'

const appStore = useAppStore()
const exchangeStore = useExchangeStore()
const config = useRuntimeConfig()

// Local reactive copy of settings
const localSettings = reactive<AppSettings>({ ...appStore.settings })

// Python detection state
const isDetectingPython = ref(false)
const pythonDetected = ref(false)
const pythonDetectError = ref<string | null>(null)

// Save state
const isSaving = ref(false)
const saveMessage = ref<string | null>(null)

const timeframeOptions = ['1m', '5m', '15m', '1h', '4h', '1d']

// The default exchange falls back to the first connected one until a choice
// is made, so the pair dropdown is never stuck on "select an exchange first".
const effectiveExchangeId = computed(
  () => localSettings.default_exchange_id ?? exchangeStore.exchanges[0]?.id ?? null,
)
const defaultExchangeId = computed({
  get: () => effectiveExchangeId.value ?? '',
  set: (value: string) => {
    localSettings.default_exchange_id = value || null
  },
})

// Sync local settings when store updates
watch(
  () => appStore.settings,
  (newSettings) => {
    Object.assign(localSettings, newSettings)
  },
  { deep: true },
)

// Auto-save with debounce
const stopWatch = watchDebounced(
  localSettings,
  async () => {
    await saveAllSettings()
  },
  { debounce: 300, deep: true },
)

async function saveAllSettings() {
  isSaving.value = true
  saveMessage.value = null
  try {
    await appStore.saveSettings({ ...localSettings })
    saveMessage.value = 'Settings saved'
    setTimeout(() => {
      saveMessage.value = null
    }, 2000)
  } catch (err) {
    console.error('Failed to save settings:', err)
    saveMessage.value = 'Failed to save'
  } finally {
    isSaving.value = false
  }
}

async function detectPython() {
  isDetectingPython.value = true
  pythonDetectError.value = null
  pythonDetected.value = false
  try {
    const path = await appStore.detectPython()
    localSettings.python_path = path
    pythonDetected.value = true
  } catch (err) {
    pythonDetectError.value = String(err)
  } finally {
    isDetectingPython.value = false
  }
}

function toggleTheme() {
  localSettings.theme = localSettings.theme === 'dark' ? 'light' : 'dark'
  appStore.applyTheme(localSettings.theme)
}

async function handleExportData() {
  try {
    await invoke('plugin:algo|export_all_data')
  } catch (err) {
    console.error('Failed to export data:', err)
  }
}

async function handleImportData() {
  try {
    await invoke('plugin:algo|import_data')
  } catch (err) {
    console.error('Failed to import data:', err)
  }
}

onUnmounted(() => {
  stopWatch()
})
</script>

<template>
  <div class="settings">
    <!-- Save indicator -->
    <Transition name="fade">
      <div v-if="saveMessage" class="settings__toast" :class="{ 'settings__toast--error': saveMessage === 'Failed to save' }">
        {{ saveMessage }}
      </div>
    </Transition>

    <div class="settings__columns">
      <div class="settings__col">
        <!-- Bot defaults: what a new bot and a backtest start from -->
        <section class="card settings__section">
          <h3 class="settings__section-title">Bot defaults</h3>
          <p class="settings__note text-muted">
            A new bot and a backtest start from these. Creating a bot writes what it used back here.
          </p>

          <div class="field-grid">
            <div class="settings__field">
              <label class="label" for="default-exchange">Exchange</label>
              <AlgoExchangeSelect id="default-exchange" v-model="defaultExchangeId" />
            </div>
            <div class="settings__field">
              <label class="label" for="default-pair">Trading pair</label>
              <AlgoExchangePairSelect
                id="default-pair"
                v-model="localSettings.default_pair"
                :exchange-id="effectiveExchangeId"
                :preferred="localSettings.default_pair || appStore.botDefaults.pair"
              />
            </div>
            <div class="settings__field">
              <label class="label" for="default-timeframe">Timeframe</label>
              <select id="default-timeframe" v-model="localSettings.default_timeframe" class="input">
                <option v-for="tf in timeframeOptions" :key="tf" :value="tf">{{ tf }}</option>
              </select>
            </div>
          </div>

          <div class="field-grid field-grid--3">
            <div class="settings__field">
              <label class="label" for="default-budget">Budget</label>
              <input id="default-budget" v-model.number="localSettings.default_budget" type="number" class="input mono" min="100" step="100" />
            </div>
            <div class="settings__field">
              <label class="label" for="risk-per-trade">Risk per trade (%)</label>
              <input id="risk-per-trade" v-model.number="localSettings.risk_per_trade" type="number" class="input mono" min="0.1" max="10" step="0.1" />
            </div>
            <div class="settings__field">
              <label class="label" for="max-positions">Max positions</label>
              <input id="max-positions" v-model.number="localSettings.max_concurrent_positions" type="number" class="input mono" min="1" max="20" />
            </div>
            <div class="settings__field">
              <label class="label" for="slippage">Slippage (%, paper)</label>
              <input id="slippage" v-model.number="localSettings.slippage_tolerance" type="number" class="input mono" min="0" max="5" step="0.01" />
            </div>
            <div class="settings__field">
              <label class="label" for="paper-fee">Fee (%, paper)</label>
              <input id="paper-fee" v-model.number="localSettings.paper_fee_pct" type="number" class="input mono" min="0" max="5" step="0.01" />
            </div>
            <div class="settings__field">
              <label class="label" for="warmup">Warm-up candles</label>
              <input id="warmup" v-model.number="localSettings.default_warmup_candles" type="number" class="input mono" min="2" max="3000" step="50" />
            </div>
          </div>
        </section>

        <!-- General -->
        <section class="card settings__section">
          <h3 class="settings__section-title">General</h3>
          <div class="field-grid">
            <div class="settings__field">
              <label class="label">Theme</label>
              <div class="settings__theme-row">
                <span class="settings__theme-value">{{ localSettings.theme === 'dark' ? 'Dark' : 'Light' }}</span>
                <button class="btn btn-sm" @click="toggleTheme">
                  {{ localSettings.theme === 'dark' ? 'Switch to Light' : 'Switch to Dark' }}
                </button>
              </div>
            </div>
            <div class="settings__field">
              <label class="label" for="font-size">Font size</label>
              <input id="font-size" v-model.number="localSettings.font_size" type="number" class="input mono" min="10" max="20" />
            </div>
          </div>
        </section>

        <!-- Data -->
        <section class="card settings__section">
          <h3 class="settings__section-title">Data</h3>
          <div class="settings__field">
            <label class="label" for="strategy-dir">Strategy directory</label>
            <input id="strategy-dir" v-model="localSettings.strategy_dir" type="text" class="input" placeholder="./strategies" />
          </div>
          <div class="settings__field">
            <label class="label" for="backtest-dir">Backtest data directory</label>
            <input id="backtest-dir" v-model="localSettings.backtest_dir" type="text" class="input" placeholder="./backtest_data" />
          </div>
          <div class="settings__data-actions">
            <button class="btn btn-sm" @click="handleExportData">Export all data</button>
            <button class="btn btn-sm" @click="handleImportData">Import data</button>
          </div>
        </section>
      </div>

      <div class="settings__col">
        <!-- Python -->
        <section class="card settings__section">
          <h3 class="settings__section-title">Python</h3>
          <div class="settings__field">
            <label class="label" for="python-path">Interpreter</label>
            <div class="settings__path-row">
              <input id="python-path" v-model="localSettings.python_path" type="text" class="input" placeholder="/usr/bin/python3" />
              <button class="btn btn-sm" :disabled="isDetectingPython" @click="detectPython">
                {{ isDetectingPython ? 'Detecting...' : 'Auto-detect' }}
              </button>
            </div>
            <div v-if="pythonDetected" class="settings__detect-result settings__detect-result--success">
              <span class="settings__checkmark">&#10003;</span>
              Python found at {{ localSettings.python_path }}
            </div>
            <div v-if="pythonDetectError" class="settings__detect-result settings__detect-result--error">
              {{ pythonDetectError }}
            </div>
            <p class="settings__note text-muted">
              Runs trading strategies and backtests (numpy, pandas, scipy).
            </p>
          </div>
        </section>

        <!-- Notifications -->
        <section class="card settings__section">
          <h3 class="settings__section-title">Notifications</h3>
          <div class="settings__toggle-field">
            <label class="settings__toggle-label" for="notify-trade">Notify on trade fill</label>
            <label class="toggle">
              <input id="notify-trade" v-model="localSettings.notify_on_trade" type="checkbox" class="toggle__input" />
              <span class="toggle__slider" />
            </label>
          </div>
          <div class="settings__toggle-field">
            <label class="settings__toggle-label" for="notify-error">Notify on error</label>
            <label class="toggle">
              <input id="notify-error" v-model="localSettings.notify_on_error" type="checkbox" class="toggle__input" />
              <span class="toggle__slider" />
            </label>
          </div>
          <div class="settings__toggle-field">
            <label class="settings__toggle-label" for="notify-daily">Notify on daily summary</label>
            <label class="toggle">
              <input id="notify-daily" v-model="localSettings.notify_on_daily_summary" type="checkbox" class="toggle__input" />
              <span class="toggle__slider" />
            </label>
          </div>
        </section>

        <!-- About -->
        <section class="card settings__section">
          <h3 class="settings__section-title">About</h3>
          <div class="settings__about">
            <p class="settings__about-name">QuantAlgo</p>
            <p class="settings__about-desc text-muted">Multi-bot trading terminal on real data</p>
            <p class="settings__about-version text-muted">Version {{ config.public.appVersion }}</p>
          </div>
        </section>

        <div class="settings__footer">
          <button class="btn btn-primary" :disabled="isSaving" @click="saveAllSettings">
            {{ isSaving ? 'Saving...' : 'Save settings' }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.settings {
  height: 100%;
  overflow-y: auto;
  padding: 20px;
  position: relative;
}

.settings__columns {
  display: grid;
  grid-template-columns: minmax(0, 3fr) minmax(0, 2fr);
  gap: 16px;
  align-items: start;
}

.settings__col {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-width: 0;
}

@media (max-width: 1100px) {
  .settings__columns {
    grid-template-columns: 1fr;
  }
}

/* Toast */
.settings__toast {
  position: fixed;
  top: 16px;
  right: 16px;
  padding: 8px 16px;
  background: var(--qa-accent);
  color: var(--qa-bg);
  font-size: 13px;
  font-weight: 500;
  border-radius: var(--qa-radius);
  z-index: 100;
}

.settings__toast--error {
  background: var(--qa-error);
  color: #fff;
}

/* Section */
.settings__section-title {
  font-size: 12px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--qa-text-secondary);
  margin: 0 0 12px;
}

.settings__note {
  font-size: 12px;
  line-height: 1.45;
  margin: -6px 0 12px;
}

/* Fields */
.field-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 12px 16px;
  margin-bottom: 12px;
}

.field-grid:last-child {
  margin-bottom: 0;
}

.field-grid--3 {
  grid-template-columns: repeat(3, minmax(0, 1fr));
}

.settings__field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-bottom: 12px;
  min-width: 0;
}

.field-grid .settings__field,
.settings__field:last-child {
  margin-bottom: 0;
}

.settings__field .input {
  width: 100%;
}

/* Theme row */
.settings__theme-row {
  display: flex;
  align-items: center;
  gap: 12px;
  min-height: 34px;
}

.settings__theme-value {
  font-size: 13px;
  font-weight: 500;
  color: var(--qa-text);
  min-width: 40px;
}

/* Path row */
.settings__path-row {
  display: flex;
  gap: 8px;
  align-items: center;
}

.settings__path-row .input {
  flex: 1;
}

/* Detect result */
.settings__detect-result {
  margin-top: 6px;
  font-size: 12px;
  display: flex;
  align-items: center;
  gap: 6px;
}

.settings__detect-result--success {
  color: var(--qa-accent);
}

.settings__detect-result--error {
  color: var(--qa-error);
}

.settings__checkmark {
  font-weight: 700;
}

.settings__field .settings__note {
  margin: 8px 0 0;
}

/* Toggle fields */
.settings__toggle-field {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 0;
  border-bottom: 1px solid var(--qa-border-subtle);
}

.settings__toggle-field:last-child {
  border-bottom: none;
  padding-bottom: 0;
}

.settings__toggle-label {
  font-size: 13px;
  color: var(--qa-text);
  cursor: pointer;
}

/* Toggle switch */
.toggle {
  position: relative;
  display: inline-block;
  width: 40px;
  height: 22px;
  flex-shrink: 0;
}

.toggle__input {
  opacity: 0;
  width: 0;
  height: 0;
}

.toggle__slider {
  position: absolute;
  cursor: pointer;
  inset: 0;
  background: var(--qa-bg-hover);
  border: 1px solid var(--qa-border);
  border-radius: 11px;
  transition: all var(--qa-transition);
}

.toggle__slider::before {
  content: '';
  position: absolute;
  height: 16px;
  width: 16px;
  left: 2px;
  bottom: 2px;
  background: var(--qa-text-muted);
  border-radius: 50%;
  transition: all var(--qa-transition);
}

.toggle__input:checked + .toggle__slider {
  background: var(--qa-accent);
  border-color: var(--qa-accent);
}

.toggle__input:checked + .toggle__slider::before {
  transform: translateX(18px);
  background: var(--qa-bg);
}

/* Data actions */
.settings__data-actions {
  display: flex;
  gap: 8px;
  margin-top: 4px;
}

/* About */
.settings__about {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.settings__about-name {
  font-size: 15px;
  font-weight: 600;
  color: var(--qa-text);
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Fira Code', monospace;
}

.settings__about-desc {
  font-size: 13px;
}

.settings__about-version {
  font-size: 12px;
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Fira Code', monospace;
}

/* Footer */
.settings__footer {
  display: flex;
  justify-content: flex-end;
}

/* Fade transition */
.fade-enter-active,
.fade-leave-active {
  transition: opacity 200ms ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
