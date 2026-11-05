<script setup lang="ts">
definePageMeta({ layout: 'algo' })

import { useBacktestStore } from '#algo/stores/backtest'
import { useStrategiesStore } from '#algo/stores/strategies'
import type { BacktestConfig, BacktestResult } from '#algo/types'

const route = useRoute()
const backtestStore = useBacktestStore()
const strategiesStore = useStrategiesStore()

// UI state
const showConfig = ref(true)
const saveName = ref('')
const showSavePrompt = ref(false)
const isSaving = ref(false)
// A failed run used to go to the console only, which read as a dead
// button (user, 2026-09-08); it stays above the form until the next run.
const runError = ref<string | null>(null)

// Pre-selected strategy from query param
const preSelectedStrategyId = computed(() => {
  return (route.query.strategy as string) ?? null
})

// Comparison mode
const comparisonResults = computed(() => backtestStore.comparisonResults)
const isComparing = computed(() => comparisonResults.value.length > 1)

async function handleRun(config: BacktestConfig) {
  showConfig.value = false
  runError.value = null
  try {
    await backtestStore.run(config)
  } catch (err) {
    console.error('Backtest failed:', err)
    runError.value = err instanceof Error ? err.message : String(err)
    showConfig.value = true
  }
}

function handleReconfigure() {
  showConfig.value = true
}

function handleSavePrompt() {
  if (!backtestStore.currentResult) return
  saveName.value = ''
  showSavePrompt.value = true
}

async function handleSave() {
  if (!backtestStore.currentResult || !saveName.value.trim()) return
  isSaving.value = true
  try {
    await backtestStore.save(backtestStore.currentResult, saveName.value.trim())
    showSavePrompt.value = false
    saveName.value = ''
  } catch (err) {
    console.error('Failed to save backtest:', err)
  } finally {
    isSaving.value = false
  }
}

function cancelSave() {
  showSavePrompt.value = false
  saveName.value = ''
}

async function handleLoadSaved(id: string) {
  try {
    await backtestStore.get(id)
    showConfig.value = false
  } catch (err) {
    console.error('Failed to load backtest:', err)
  }
}

async function handleDeleteSaved(id: string) {
  try {
    await backtestStore.delete(id)
  } catch (err) {
    console.error('Failed to delete backtest:', err)
  }
}

// Watch for result completion to switch view
watch(() => backtestStore.currentResult, (result) => {
  if (result && !backtestStore.isRunning) {
    showConfig.value = false
  }
})

onMounted(async () => {
  await Promise.all([
    backtestStore.load(),
    backtestStore.init(),
    strategiesStore.load(),
  ])
})
</script>

<template>
  <div class="backtest-page">
    <div class="backtest-main">
      <!-- Configuration Section -->
      <div v-if="showConfig && !backtestStore.isRunning" class="config-section">
        <div v-if="runError" class="run-error" role="alert">
          <strong>Backtest failed.</strong> {{ runError }}
        </div>
        <AlgoBacktestConfig
          :pre-selected-strategy-id="preSelectedStrategyId"
          @run="handleRun"
        />
      </div>

      <!-- Progress Bar -->
      <div v-if="backtestStore.isRunning" class="progress-section">
        <div class="progress-card card">
          <h3 class="progress-title">Running Backtest</h3>
          <div class="progress-info">
            <span class="progress-pct">{{ Math.round(backtestStore.progress.pct) }}%</span>
            <span class="progress-message text-muted">{{ backtestStore.progress.message }}</span>
          </div>
          <div class="progress-bar-track">
            <div
              class="progress-bar-fill"
              :style="{ width: `${backtestStore.progress.pct}%` }"
            />
          </div>
        </div>
      </div>

      <!-- Results Section -->
      <div
        v-if="!showConfig && !backtestStore.isRunning && backtestStore.currentResult"
        class="results-section"
        :class="{ 'results-section--comparing': isComparing }"
      >
        <div class="results-toolbar">
          <button class="btn btn-sm" @click="handleReconfigure">
            New Backtest
          </button>
          <button class="btn btn-sm btn-primary" @click="handleSavePrompt">
            Save Result
          </button>
        </div>

        <!-- Save prompt, one row above the results -->
        <div v-if="showSavePrompt" class="save-prompt card">
          <h4 class="save-prompt__title">Save Backtest Result</h4>
          <div class="save-prompt__field">
            <label class="label" for="save-name">Name</label>
            <input
              id="save-name"
              v-model="saveName"
              class="input"
              type="text"
              placeholder="e.g., BTC RSI Strategy - April 2026"
              @keydown.enter="handleSave"
            />
          </div>
          <div class="save-prompt__actions">
            <button class="btn btn-sm" @click="cancelSave">Cancel</button>
            <button
              class="btn btn-sm btn-primary"
              :disabled="!saveName.trim() || isSaving"
              @click="handleSave"
            >
              {{ isSaving ? 'Saving...' : 'Save' }}
            </button>
          </div>
        </div>

        <AlgoBacktestResults class="results-body" :result="backtestStore.currentResult" />

        <!-- Comparison view -->
        <div v-if="isComparing" class="comparison-section">
          <AlgoBacktestComparison :results="comparisonResults" />
        </div>
      </div>

      <!-- Empty state when no config and no result -->
      <QEmptyState
        v-if="!showConfig && !backtestStore.isRunning && !backtestStore.currentResult"
        compact
        title="No backtest result loaded."
      >
        <button class="btn" @click="showConfig = true">Configure Backtest</button>
      </QEmptyState>
    </div>

    <!-- Saved backtests sidebar -->
    <aside v-if="backtestStore.backtests.length" class="backtest-sidebar card">
      <h3 class="sidebar-title">Saved Backtests</h3>
      <div class="saved-list">
        <div
          v-for="bt in backtestStore.backtests"
          :key="bt.id"
          class="saved-item"
          :class="{ 'saved-item--active': bt.id === backtestStore.activeBacktestId }"
          @click="handleLoadSaved(bt.id)"
        >
          <div class="saved-item__name">{{ bt.name }}</div>
          <div class="saved-item__meta text-muted">
            {{ bt.created_at ? new Date(bt.created_at).toLocaleDateString('en-US', { dateStyle: 'medium' }) : '' }}
          </div>
          <button
            class="saved-item__delete btn btn-sm"
            @click.stop="handleDeleteSaved(bt.id)"
          >
            Delete
          </button>
        </div>
      </div>
    </aside>
  </div>
</template>

<style scoped>
/* The saved list sits next to the page and wraps under it when it would squeeze the form. */
.backtest-page {
  display: flex;
  flex-wrap: wrap;
  gap: var(--qss-gap);
  height: 100%;
  overflow-y: auto;
  padding: var(--qss-page-edge);
}

.backtest-main {
  flex: 999 1 540px;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap);
}

/* Config section: the form spans the page */
.config-section {
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap);
}

.run-error {
  padding: var(--qss-gap-tight) var(--qss-panel-pad);
  border: 1px solid color-mix(in srgb, var(--qa-error) 50%, var(--qa-border));
  border-radius: var(--qa-radius);
  background: color-mix(in srgb, var(--qa-error) 12%, transparent);
  color: var(--qa-error);
  font-size: var(--qss-text-section);
  line-height: 1.5;
  overflow-wrap: anywhere;
}

/* Progress: a slim track at the top of the page, where the form was */
.progress-section {
  flex-shrink: 0;
}

.progress-card {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  align-items: baseline;
  gap: var(--qss-gap-tight) var(--qss-gap);
}

.progress-title {
  font-size: var(--qss-text-section);
  font-weight: 600;
  color: var(--qa-text);
}

.progress-bar-track {
  grid-column: 1 / -1;
  width: 100%;
  height: 4px;
  background: var(--qa-bg-hover);
  border-radius: 2px;
  overflow: hidden;
}

.progress-bar-fill {
  height: 100%;
  background: var(--qa-accent);
  border-radius: 2px;
  transition: width 300ms ease;
}

.progress-info {
  min-width: 0;
  display: flex;
  align-items: baseline;
  gap: var(--qss-gap-tight);
}

.progress-pct {
  font-size: var(--qss-text-section);
  font-weight: 600;
  color: var(--qa-text);
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', monospace;
  font-variant-numeric: tabular-nums;
}

.progress-message {
  min-width: 0;
  font-size: var(--qss-text-label);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* Results take the page's height; the chart and the trades split it. */
.results-section {
  flex: 1 1 0;
  min-height: 560px;
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap);
}

.results-body {
  flex: 1 1 0;
  min-height: 0;
}

/* A comparison adds a whole chart and table: this one state may scroll the page. */
.results-section--comparing {
  flex: none;
}

.results-section--comparing .results-body {
  flex: none;
  height: 720px;
}

.results-toolbar {
  flex-shrink: 0;
  display: flex;
  gap: var(--qss-gap-tight);
}

/* Save prompt */
.save-prompt {
  flex-shrink: 0;
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  gap: var(--qss-gap-tight) var(--qss-gap);
}

.save-prompt__title {
  align-self: center;
  font-size: var(--qss-text-section);
  font-weight: 600;
  color: var(--qa-text);
}

.save-prompt__field {
  flex: 1 1 280px;
  min-width: 0;
}

.save-prompt__actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--qss-gap-tight);
}

/* Comparison */
.comparison-section {
  padding-top: var(--qss-gap);
  border-top: 1px solid var(--qa-border);
}

/* Saved results, a list next to the page */
.backtest-sidebar {
  flex: 1 0 260px;
  min-height: 0;
  overflow-y: auto;
}

.sidebar-title {
  font-size: var(--qss-text-section);
  font-weight: 600;
  color: var(--qa-text);
  margin-bottom: var(--qss-gap-tight);
}

.saved-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.saved-item {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 2px 6px;
  padding: 6px var(--qss-gap-tight);
  border: 1px solid transparent;
  border-radius: var(--qa-radius);
  cursor: pointer;
  transition: background var(--qa-transition);
}

.saved-item:hover {
  background: var(--qa-bg-hover);
}

.saved-item--active {
  background: var(--qa-bg-hover);
  border-color: var(--qa-border);
}

.saved-item__name {
  flex: 1 1 100%;
  font-size: var(--qss-text-section);
  font-weight: 500;
  color: var(--qa-text);
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.saved-item__meta {
  font-size: var(--qss-text-meta);
  flex: 1 1 auto;
}

.saved-item__delete {
  margin-left: auto;
  opacity: 0;
  transition: opacity var(--qa-transition);
}

.saved-item:hover .saved-item__delete {
  opacity: 1;
}
</style>
