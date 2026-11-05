<script setup lang="ts">
definePageMeta({ layout: 'algo' })

import { invoke } from '@tauri-apps/api/core'
import { useStrategiesStore } from '#algo/stores/strategies'
import { inActiveKeepAliveTree } from '@quantsuite/core'
import type { PreflightResult } from '#algo/types'

const router = useRouter()
const strategiesStore = useStrategiesStore()

// New strategy form state
const showNewForm = ref(false)
const newName = ref('')
const newDesc = ref('')
const isCreating = ref(false)
const isSaving = ref(false)
const saveError = ref<string | null>(null)
const paramsError = ref<string | null>(null)
const isValidating = ref(false)
const validationResult = ref<PreflightResult | null>(null)
const validationError = ref<string | null>(null)

// Deploy modal state
const showDeployModal = ref(false)

const hasActiveStrategy = computed(() => !!strategiesStore.activeStrategyId)
const activeStrategy = computed(() => strategiesStore.activeStrategy)

// Toolbar event handlers
function validateParams(): boolean {
  paramsError.value = null
  const raw = strategiesStore.paramsContent.trim()
  if (!raw) return true
  try {
    JSON.parse(raw)
    return true
  } catch (err) {
    paramsError.value = `Invalid params JSON: ${String(err)}`
    return false
  }
}

async function handleSave(): Promise<boolean> {
  if (!strategiesStore.activeStrategyId) return false
  // A select() still in flight leaves the editor on the previous strategy's
  // code — saving now would write it under the newly selected id.
  if (strategiesStore.savedStrategyId !== strategiesStore.activeStrategyId) {
    saveError.value = 'Strategy is still loading — try again.'
    return false
  }
  if (!validateParams()) {
    saveError.value = paramsError.value
    return false
  }
  isSaving.value = true
  saveError.value = null
  try {
    await strategiesStore.save(
      strategiesStore.activeStrategyId,
      strategiesStore.editorContent,
      strategiesStore.paramsContent,
    )
    return true
  } catch (err) {
    saveError.value = String(err)
    console.error('Failed to save strategy:', err)
    return false
  } finally {
    isSaving.value = false
  }
}

async function handleValidate() {
  if (!strategiesStore.activeStrategyId) return
  validationResult.value = null
  validationError.value = null

  const saved = await handleSave()
  if (!saved) {
    validationError.value = saveError.value ?? paramsError.value ?? 'Save the strategy before validation.'
    return
  }

  isValidating.value = true
  try {
    validationResult.value = await invoke<PreflightResult>('plugin:algo|validate_strategy', {
      strategyId: strategiesStore.activeStrategyId,
    })
  } catch (err) {
    validationError.value = String(err)
  } finally {
    isValidating.value = false
  }
}

async function handleRename(name: string) {
  if (!strategiesStore.activeStrategyId) return
  const trimmed = name.trim()
  if (!trimmed || trimmed === activeStrategy.value?.name) return

  try {
    await strategiesStore.updateMeta(strategiesStore.activeStrategyId, { name: trimmed })
  } catch (err) {
    console.error('Failed to rename strategy:', err)
  }
}

async function handleDelete() {
  if (!strategiesStore.activeStrategyId || !activeStrategy.value) return
  const confirmed = window.confirm(`Delete "${activeStrategy.value.name}"? This cannot be undone.`)
  if (!confirmed) return

  try {
    await strategiesStore.delete(strategiesStore.activeStrategyId)
  } catch (err) {
    console.error('Failed to delete strategy:', err)
  }
}

function handleRunBacktest() {
  if (!strategiesStore.activeStrategyId) return
  router.push(`/algo/backtest?strategy=${strategiesStore.activeStrategyId}`)
}

// "New Bot" from the toolbar: the strategy is saved first (a bot runs the
// file on disk, not the editor buffer), then the create modal opens with it
// pre-selected.
async function handleDeploy() {
  if (!strategiesStore.activeStrategyId) return
  saveError.value = null
  if (strategiesStore.isDirty) {
    const saved = await handleSave()
    if (!saved) return
  }
  showDeployModal.value = true
}

function handleBotCreated(_bot: unknown, started: boolean) {
  showDeployModal.value = false
  if (started) router.push('/algo/bots')
}

async function handleCreate() {
  if (!newName.value.trim()) return
  isCreating.value = true
  try {
    const strategy = await strategiesStore.create(newName.value.trim(), newDesc.value.trim())
    await strategiesStore.select(strategy.id)
    showNewForm.value = false
    newName.value = ''
    newDesc.value = ''
  } catch (err) {
    console.error('Failed to create strategy:', err)
  } finally {
    isCreating.value = false
  }
}

function cancelCreate() {
  showNewForm.value = false
  newName.value = ''
  newDesc.value = ''
}

// Keyboard shortcut: Ctrl+S to save
function handleKeydown(e: KeyboardEvent) {
  if ((e.ctrlKey || e.metaKey) && e.key === 's') {
    e.preventDefault()
    if (hasActiveStrategy.value) {
      handleSave()
    }
  }
}

// V3 warm cache: the page stays mounted while another module is active, so the
// shortcut is armed only while it is visible, otherwise a hidden editor answers
// Ctrl+S and saves stale content. Binding in both hooks is safe: the same
// handler is never added twice — but a page that mounts late into a stage the
// user has already left gets onMounted without onActivated and after the
// stage's onDeactivated flush, so it would answer Ctrl+S from inside another
// module forever; hence the guard below. The strategy select is a one-time
// load and stays unconditional.
function bindSaveShortcut() {
  window.addEventListener('keydown', handleKeydown)
}

function unbindSaveShortcut() {
  window.removeEventListener('keydown', handleKeydown)
}

onMounted(async () => {
  if (inActiveKeepAliveTree()) bindSaveShortcut()
  // Strategies are loaded in the layout bootstrap, but we still need to
  // select the active strategy to populate the editor
  if (strategiesStore.activeStrategyId) {
    await strategiesStore.select(strategiesStore.activeStrategyId)
  }
})

// QuantScript's forge creates a strategy from an indicator and lands here
// with `?select=<id>` (docs/PLAN-QUANTSCRIPT.md): the list is re-read — the
// layout only loads it on mount and this stage may be warm — and the new
// strategy opens in the editor. The query is consumed so a reload or a
// back-navigation does not select it again.
const route = useRoute()
async function selectFromQuery() {
  const id = route.query.select
  if (typeof id !== 'string' || !id) return
  await strategiesStore.load()
  await strategiesStore.select(id)
  void router.replace({ path: route.path })
}
watch(() => route.query.select, () => void selectFromQuery(), { immediate: true })

onActivated(bindSaveShortcut)
onDeactivated(unbindSaveShortcut)

onBeforeUnmount(unbindSaveShortcut)
</script>

<template>
  <div class="strategies-page">
    <!-- Toolbar -->
    <AlgoStrategyToolbar
      :strategy="activeStrategy"
      :is-saving="isSaving"
      @create="showNewForm = true"
      @remove="handleDelete"
      @save="handleSave"
      @validate="handleValidate"
      @run-backtest="handleRunBacktest"
      @deploy="handleDeploy"
      @rename="handleRename"
    />

    <div v-if="showNewForm" class="new-form card">
      <h3 class="new-form__title">Create New Strategy</h3>
      <div class="new-form__field">
        <label class="label" for="strategy-name">Name</label>
        <input
          id="strategy-name"
          v-model="newName"
          class="input"
          type="text"
          placeholder="My Strategy"
          @keydown.enter="handleCreate"
        />
      </div>
      <div class="new-form__field new-form__field--wide">
        <label class="label" for="strategy-desc">Description</label>
        <input
          id="strategy-desc"
          v-model="newDesc"
          class="input"
          type="text"
          placeholder="Brief description..."
          @keydown.enter="handleCreate"
        />
      </div>
      <div class="new-form__actions">
        <button class="btn" @click="cancelCreate">Cancel</button>
        <button
          class="btn btn-primary"
          :disabled="!newName.trim() || isCreating"
          @click="handleCreate"
        >
          {{ isCreating ? 'Creating...' : 'Create' }}
        </button>
      </div>
    </div>

    <!-- Editor area: the code takes the room, its parameters and checks sit beside it -->
    <div v-if="hasActiveStrategy" class="editor-area">
      <AlgoStrategyEditor
        v-model:content="strategiesStore.editorContent"
        :strategy-id="strategiesStore.activeStrategyId!"
      />
      <div class="params-panel card">
        <div class="params-panel__header">
          <div>
            <h3 class="params-panel__title">Strategy Params</h3>
            <p class="params-panel__hint">Saved JSON is applied before on_start().</p>
          </div>
          <span v-if="strategiesStore.paramsContent !== strategiesStore.savedParamsContent" class="params-panel__dirty">
            Unsaved
          </span>
        </div>
        <textarea
          v-model="strategiesStore.paramsContent"
          class="params-panel__textarea"
          spellcheck="false"
          @blur="validateParams"
        />
        <div v-if="paramsError" class="params-panel__error">{{ paramsError }}</div>
        <div v-if="saveError" class="params-panel__error">{{ saveError }}</div>
        <div v-if="isValidating" class="params-panel__status">Validating strategy...</div>
        <div v-else-if="validationError" class="params-panel__error">{{ validationError }}</div>
        <div v-else-if="validationResult" class="params-panel__checks">
          <div
            v-for="check in validationResult.checks"
            :key="check.id + check.message"
            class="params-panel__check"
            :class="'params-panel__check--' + check.status"
          >
            <span class="params-panel__check-label">{{ check.label }}</span>
            <span class="params-panel__check-message">{{ check.message }}</span>
          </div>
        </div>
      </div>
    </div>

    <!-- Empty state -->
    <QEmptyState v-else class="empty-state" compact title="Select a strategy from the sidebar or create a new one">
      <button class="btn btn-primary" @click="showNewForm = true">
        New Strategy
      </button>
    </QEmptyState>

    <!-- Create-bot modal (outside v-if/v-else chain, uses Teleport) -->
    <AlgoBotCreateModal
      :visible="showDeployModal"
      :strategy-id="strategiesStore.activeStrategyId"
      @close="showDeployModal = false"
      @created="handleBotCreated"
    />
  </div>
</template>

<style scoped>
.strategies-page {
  container-type: inline-size;
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap);
  height: 100%;
  overflow-y: auto;
  padding: var(--qss-page-edge);
}

.editor-area {
  flex: 1 1 0;
  min-height: 420px;
  display: grid;
  grid-template-columns: minmax(0, 1fr) 320px;
  gap: var(--qss-gap);
}

/* The editor fills its panel, so its own 400px minimum would only push the page. */
.editor-area :deep(.strategy-editor) {
  min-width: 0;
  min-height: 0;
  border: 1px solid var(--qa-border);
  border-radius: var(--qss-panel-radius);
  overflow: hidden;
}

.params-panel {
  min-width: 0;
  min-height: 0;
  overflow-y: auto;
}

.params-panel__header {
  display: flex;
  justify-content: space-between;
  gap: var(--qss-gap);
  margin-bottom: var(--qss-gap-tight);
}

.params-panel__title {
  font-size: var(--qss-text-section);
  font-weight: 600;
  color: var(--qa-text);
  margin: 0;
}

.params-panel__hint {
  font-size: var(--qss-text-meta);
  color: var(--qa-text-muted);
  margin: 2px 0 0;
}

.params-panel__dirty {
  align-self: flex-start;
  color: var(--qa-warning);
  font-size: var(--qss-text-meta);
  font-weight: 600;
}

.params-panel__textarea {
  display: block;
  width: 100%;
  height: 220px;
  min-height: 92px;
  resize: vertical;
  box-sizing: border-box;
  border: 1px solid var(--qa-border);
  border-radius: 8px;
  background: var(--qa-bg-input);
  color: var(--qa-text);
  padding: var(--qss-gap-tight);
  font-size: var(--qss-text-label);
  line-height: 1.45;
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', Menlo, Consolas, monospace;
}

.params-panel__textarea:focus {
  outline: none;
  border-color: var(--qa-accent);
}

.params-panel__error,
.params-panel__status {
  margin-top: 6px;
  font-size: var(--qss-text-label);
}

.params-panel__error {
  color: var(--qa-error);
}

.params-panel__status {
  color: var(--qa-text-muted);
}

/* Validation results: a list on the panel's ground, the status in the label's colour. */
.params-panel__checks {
  display: flex;
  flex-direction: column;
  margin-top: var(--qss-gap-tight);
}

.params-panel__check {
  padding: 6px 0;
  border-top: 1px solid var(--qa-border-subtle);
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.params-panel__check--ok .params-panel__check-label {
  color: var(--qa-success);
}

.params-panel__check--warn .params-panel__check-label {
  color: var(--qa-warning);
}

.params-panel__check--error .params-panel__check-label {
  color: var(--qa-error);
}

.params-panel__check-label {
  font-size: var(--qss-text-label);
  color: var(--qa-text);
  font-weight: 600;
}

.params-panel__check-message {
  font-size: var(--qss-text-meta);
  color: var(--qa-text-muted);
}

/* New strategy form: one row between the toolbar and the editor */
.new-form {
  flex-shrink: 0;
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  gap: var(--qss-gap-tight) var(--qss-gap);
}

.new-form__title {
  flex: 1 1 100%;
  font-size: var(--qss-text-section);
  font-weight: 600;
  color: var(--qa-text);
}

.new-form__field {
  flex: 1 1 200px;
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.new-form__field--wide {
  flex-grow: 2;
}

.new-form__actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--qss-gap-tight);
}

@container (max-width: 860px) {
  .editor-area {
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: minmax(240px, 1fr) auto;
  }

  .params-panel__textarea {
    height: 92px;
  }
}
</style>
