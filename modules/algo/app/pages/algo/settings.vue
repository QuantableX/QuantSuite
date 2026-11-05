<script setup lang="ts">
definePageMeta({ layout: 'algo' })

import { BOT_TIMEFRAMES, useAppStore } from '#algo/stores/app'
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

const timeframeOptions = BOT_TIMEFRAMES

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
    <div class="settings__columns">
      <div class="settings__col settings__col--bots">
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
              <label class="label" for="default-timeframe">Timeframe</label>
              <select id="default-timeframe" v-model="localSettings.default_timeframe" class="input">
                <option v-for="tf in timeframeOptions" :key="tf" :value="tf">{{ tf }}</option>
              </select>
            </div>
            <div class="settings__field settings__field--full">
              <label class="label" for="default-pair">Trading pair</label>
              <AlgoExchangePairSelect
                id="default-pair"
                v-model="localSettings.default_pair"
                :exchange-id="effectiveExchangeId"
                :preferred="localSettings.default_pair || appStore.botDefaults.pair"
              />
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

        <!-- Perp (Venues 4/5) -->
        <section class="card settings__section">
          <h3 class="settings__section-title">Perp</h3>
          <p class="settings__note text-muted">
            Perp positions use isolated margin, 1x by default. A rotation system has no reason for leverage; the cap
            keeps systems and bots below it. A position that would open closer to its liquidation price than the
            minimum distance is refused.
          </p>
          <div class="field-grid field-grid--2">
            <div class="settings__field">
              <label class="label" for="perp-cap">Leverage cap (x)</label>
              <input id="perp-cap" v-model.number="localSettings.perp_max_leverage" type="number" class="input mono" min="1" max="20" step="1" />
            </div>
            <div class="settings__field">
              <label class="label" for="perp-liq">Min. liquidation distance (%)</label>
              <input id="perp-liq" v-model.number="localSettings.perp_min_liquidation_distance_pct" type="number" class="input mono" min="0" max="95" step="1" />
            </div>
          </div>
        </section>
      </div>

      <div class="settings__col settings__col--app">
        <!-- General -->
        <section class="card settings__section">
          <h3 class="settings__section-title">General</h3>
          <div class="field-grid field-grid--2">
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
      </div>

      <div class="settings__col settings__col--about">
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
      </div>
    </div>

    <!-- Right under the columns -->
    <div class="settings__footer">
      <Transition name="fade">
        <span v-if="saveMessage" class="settings__toast" :class="{ 'settings__toast--error': saveMessage === 'Failed to save' }" role="status">
          {{ saveMessage }}
        </span>
      </Transition>
      <button class="btn btn-primary" :disabled="isSaving" @click="saveAllSettings">
        {{ isSaving ? 'Saving...' : 'Save settings' }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.settings {
  container-type: inline-size;
  height: 100%;
  overflow-y: auto;
  padding: var(--qss-page-edge);
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap);
}

/* Three columns of panels, each as tall as its content. */
.settings__columns {
  flex: none;
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: var(--qss-gap);
  align-items: start;
  align-content: start;
}

.settings__col {
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap);
  min-width: 0;
}

/* Two columns, the last panels side by side underneath: it all still fits one screen. */
@container (max-width: 1160px) {
  .settings__columns {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .settings__col--about {
    grid-column: 1 / -1;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

@container (max-width: 640px) {
  .settings__columns,
  .settings__col--about {
    grid-template-columns: minmax(0, 1fr);
  }
}

/* Save result, next to the button */
.settings__toast {
  font-size: var(--qss-text-label);
  font-weight: 500;
  color: var(--qa-text-secondary);
}

.settings__toast--error {
  color: var(--qa-error);
}

/* Section */
.settings__section-title {
  font-size: var(--qss-text-section);
  font-weight: 600;
  color: var(--qa-text);
  margin: 0 0 var(--qss-gap-tight);
}

.settings__note {
  font-size: var(--qss-text-label);
  line-height: 1.45;
  margin: -4px 0 var(--qss-gap-tight);
}

/* Fields */
.field-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--qss-gap-tight) var(--qss-gap);
  margin-bottom: var(--qss-gap-tight);
}

.field-grid:last-child {
  margin-bottom: 0;
}

.field-grid--3 {
  grid-template-columns: repeat(3, minmax(0, 1fr));
}

.field-grid--2 {
  grid-template-columns: repeat(2, minmax(0, 1fr));
}

.settings__field {
  display: flex;
  flex-direction: column;
  gap: 0;
  margin-bottom: var(--qss-gap-tight);
  min-width: 0;
}

.settings__field--full {
  grid-column: 1 / -1;
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
  gap: var(--qss-gap-tight);
  min-height: var(--qss-ctl-h);
}

.settings__theme-value {
  font-size: var(--qss-text-section);
  font-weight: 500;
  color: var(--qa-text);
}

/* Path row */
.settings__path-row {
  display: flex;
  gap: var(--qss-gap-tight);
  align-items: center;
}

.settings__path-row .input {
  flex: 1;
  min-width: 0;
}

/* Detect result */
.settings__detect-result {
  margin-top: 6px;
  font-size: var(--qss-text-label);
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
  margin: var(--qss-gap-tight) 0 0;
}

/* Toggle fields */
.settings__toggle-field {
  display: flex;
  align-items: center;
  justify-content: space-between;
  min-height: var(--qss-row-h);
  border-bottom: 1px solid var(--qa-border-subtle);
}

.settings__toggle-field:last-child {
  border-bottom: none;
}

.settings__toggle-label {
  font-size: var(--qss-text-section);
  color: var(--qa-text);
  cursor: pointer;
}

/* Toggle switch */
.toggle {
  position: relative;
  display: inline-block;
  width: 34px;
  height: 20px;
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
  border-radius: 10px;
  transition: all var(--qa-transition);
}

.toggle__slider::before {
  content: '';
  position: absolute;
  height: 14px;
  width: 14px;
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
  transform: translateX(14px);
  background: var(--qa-bg);
}

/* Data actions */
.settings__data-actions {
  display: flex;
  gap: var(--qss-gap-tight);
  margin-top: var(--qss-gap-tight);
}

/* About */
.settings__about {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.settings__about-name {
  font-size: var(--qss-text-section);
  font-weight: 600;
  color: var(--qa-text);
}

.settings__about-desc {
  font-size: var(--qss-text-label);
}

.settings__about-version {
  font-size: var(--qss-text-label);
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Fira Code', monospace;
}

/* Footer */
.settings__footer {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: var(--qss-gap);
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
