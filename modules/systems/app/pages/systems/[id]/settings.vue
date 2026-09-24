<script setup lang="ts">
definePageMeta({ layout: 'systems', path: '/algo/manual/:id/settings' })

import { useAppStore } from '#systems/stores/app'
import { useSystemsStore } from '#systems/stores/systems'
import { useConfigStore } from '#systems/stores/config'
import { useEngine } from '#systems/composables/useEngine'
import { useBacktestStore } from '#systems/stores/backtest'
import { useLiveStore } from '#systems/stores/live'
import { strategyIcon } from '#systems/utils/strategyIcons'

const route = useRoute()
const app = useAppStore()
const systems = useSystemsStore()
const config = useConfigStore()
const engine = useEngine()
const router = useRouter()
const backtest = useBacktestStore()
const live = useLiveStore()

const systemId = computed(() => route.params.id as string)
const system = computed(() => systems.byId(systemId.value))
const planned = computed(() => system.value?.status !== 'ready')

const saving = ref(false)
const savedAt = ref<string | null>(null)
const engineRunning = computed(() => app.engineStatus.status === 'running')
const panel = ref<'configuration' | 'signals' | 'data'>('configuration')
const name = ref('')
const short = ref('')
const description = ref('')
const icon = ref('grid')
const confirmDelete = ref(false)
const deleting = ref(false)
const deleteError = ref<string | null>(null)
const evaluating = computed(() => backtest.stateFor(systemId.value).isRunning || live.stateFor(systemId.value).loading)
watch(system, s => {
  name.value = s?.name ?? ''
  short.value = s?.short ?? ''
  description.value = s?.description ?? ''
  icon.value = strategyIcon(s).id
  confirmDelete.value = false
}, { immediate: true })

let savedTimer: ReturnType<typeof setTimeout> | null = null

onMounted(() => config.load(systemId.value))
onUnmounted(() => { if (savedTimer) clearTimeout(savedTimer) })

async function save() {
  if (!system.value || !name.value.trim()) return
  saving.value = true
  const metadata = { ...system.value, name: name.value.trim(), short: short.value.trim(), description: description.value.trim(), icon: icon.value }
  const ok = await config.save(systemId.value, metadata)
  saving.value = false
  if (!ok) return
  systems.replace(metadata)
  // Inline confirmation next to the button, gone again after a few seconds.
  savedAt.value = new Date().toLocaleTimeString()
  if (savedTimer) clearTimeout(savedTimer)
  savedTimer = setTimeout(() => { savedAt.value = null }, 4000)
}

const engineBusy = ref(false)
async function remove() {
  if (evaluating.value || deleting.value) return
  const id = systemId.value
  deleting.value = true
  deleteError.value = null
  try {
    await systems.remove(id)
    config.forget(id)
    app.setActiveSystem(systems.systems[0]?.id ?? '')
    await router.replace('/algo/manual')
  } catch (e) {
    deleteError.value = String(e)
  } finally {
    deleting.value = false
  }
}

const engineError = ref<string | null>(null)
async function toggleEngine() {
  engineBusy.value = true
  engineError.value = null
  try {
    if (app.engineStatus.status === 'running') await engine.stopEngine()
    else await engine.startEngine()
    await app.refreshEngineStatus()
  } catch (e) {
    // A missing Python or a failed spawn would otherwise only reset the button.
    engineError.value = String(e)
  } finally {
    engineBusy.value = false
  }
}
</script>

<template>
  <SystemsSystemPlanned v-if="planned" :system="system" />
  <div v-else class="qs-settings" :data-panel="panel">
    <header class="qs-settings__head">
      <h1 class="qs-settings__title">{{ system?.name }}</h1>
      <span class="qs-settings__engine" :class="{ 'qs-settings__engine--on': engineRunning }">
        <span class="qs-settings__dot" />
        {{ engineRunning ? 'engine running' : 'engine stopped' }}
      </span>
      <div class="qs-settings__actions">
        <Transition name="fade">
          <span v-if="savedAt" class="qs-settings__saved">Saved {{ savedAt }}</span>
        </Transition>
        <button class="btn" :disabled="engineBusy" @click="toggleEngine">
          {{ engineRunning ? 'Stop Engine' : 'Start Engine' }}
        </button>
        <button class="btn btn-primary" :disabled="saving || deleting || !name.trim() || !config.configBySystem[systemId]" @click="save">
          {{ saving ? 'Saving…' : 'Save strategy' }}
        </button>
      </div>
    </header>

    <div v-if="engineError" class="qs-settings__error">{{ engineError }}</div>

    <section class="card qs-settings__identity" aria-label="Strategy details">
      <label class="qs-settings__name"><span class="label">Name</span><input v-model="name" class="input" :disabled="saving || deleting" required /></label>
      <label><span class="label">Short label (optional)</span><input v-model="short" class="input" :disabled="saving || deleting" /></label>
      <SystemsSettingsSymbolSelect v-model="icon" :disabled="saving || deleting" />
      <label class="qs-settings__description"><span class="label">Description</span><input v-model="description" class="input" :disabled="saving || deleting" /></label>
      <div class="qs-settings__manage">
        <button class="btn" :disabled="saving || deleting" @click="router.push({ path: '/algo/manual/new', query: { copy: systemId } })">Duplicate</button>
        <button class="btn" :disabled="saving || deleting || evaluating" @click="confirmDelete = !confirmDelete">Delete</button>
      </div>
      <div v-if="confirmDelete" class="qs-settings__confirm" role="alert">
        <span>Delete “{{ system?.name }}” and its saved settings?</span>
        <button class="btn" :disabled="deleting" @click="confirmDelete = false">Cancel</button>
        <button class="btn" :disabled="deleting || evaluating" @click="remove">{{ deleting ? 'Deleting…' : 'Delete strategy' }}</button>
      </div>
      <p v-if="deleteError" class="qs-settings__error" role="alert">{{ deleteError }}</p>
    </section>

    <nav class="qs-settings__tabs" aria-label="Settings sections">
      <button type="button" :aria-pressed="panel === 'configuration'" @click="panel = 'configuration'">Universe &amp; costs</button>
      <button type="button" :aria-pressed="panel === 'signals'" @click="panel = 'signals'">Signals</button>
      <button type="button" :aria-pressed="panel === 'data'" @click="panel = 'data'">Data &amp; cache</button>
    </nav>

    <div class="qs-settings__grid">
      <SystemsSettingsRunConfigForm class="qs-settings__form" :system-id="systemId" />
      <div class="qs-settings__side">
        <SystemsSettingsProviderStatus class="qs-settings__side-card" />
        <SystemsSettingsCacheStatsPanel class="qs-settings__side-card" />
      </div>
    </div>
  </div>
</template>

<style scoped>
.qs-settings {
  container: manual-settings / inline-size;
  display: flex;
  flex-direction: column;
  gap: 14px;
  width: 100%;
}

.qs-settings__tabs { display: none; }

.qs-settings__identity { display: grid; grid-template-columns: minmax(0, 2fr) minmax(0, 1fr) 160px; gap: 12px; padding: 16px; }
.qs-settings__identity label { min-width: 0; }
.qs-settings__description { grid-column: 1 / 3; }
.qs-settings__manage { display: flex; gap: 8px; align-items: flex-end; justify-content: flex-end; }
.qs-settings__confirm { grid-column: 1 / -1; display: flex; flex-wrap: wrap; align-items: center; gap: 8px; color: var(--qs-error); font-size: 13px; }
.qs-settings__confirm span { flex: 1; }

.qs-settings__head {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 10px;
}

.qs-settings__title {
  overflow-wrap: anywhere;
  margin: 0;
  font-size: 17px;
  font-weight: 600;
}

.qs-settings__engine {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 3px 10px;
  border: 1px solid var(--qs-border);
  border-radius: 999px;
  font-size: 11px;
  color: var(--qs-text-muted);
}

.qs-settings__dot {
  width: 6px;
  height: 6px;
  border-radius: 999px;
  background: var(--qs-text-muted);
}

.qs-settings__engine--on {
  color: var(--qs-text-secondary);
  border-color: color-mix(in srgb, var(--qs-success) 40%, var(--qs-border));
}

.qs-settings__engine--on .qs-settings__dot {
  background: var(--qs-success);
}

.qs-settings__actions {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-shrink: 0;
  margin-left: auto;
}

.qs-settings__actions .btn {
  padding: 6px 14px;
  font-size: 13px;
}

.qs-settings__saved {
  font-size: 12px;
  color: var(--qs-success);
  white-space: nowrap;
}

.qs-settings__error {
  flex-shrink: 0;
  padding: 8px 12px;
  border: 1px solid color-mix(in srgb, var(--qs-error) 50%, var(--qs-border));
  border-radius: var(--qs-radius);
  background: color-mix(in srgb, var(--qs-error) 12%, transparent);
  color: var(--qs-error);
  font-size: 13px;
}

.qs-settings__grid {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 280px;
  gap: 14px;
  align-items: stretch;
}

.qs-settings__side {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.qs-settings__side-card {
  flex: 1;
}

@container manual-settings (max-width: 950px) {
  .qs-settings__tabs { display: flex; gap: 4px; border-bottom: 1px solid var(--qs-border-subtle); }
  .qs-settings__tabs button {
    padding: 6px 10px;
    border: 0;
    border-bottom: 2px solid transparent;
    background: transparent;
    color: var(--qs-text-muted);
    font-size: 13px;
    cursor: pointer;
  }
  .qs-settings__tabs button[aria-pressed='true'] { color: var(--qs-text); border-bottom-color: var(--qs-accent); }
  .qs-settings__grid { grid-template-columns: minmax(0, 1fr); }
  [data-panel='configuration'] .qs-settings__form { grid-template-areas: 'universe universe' 'window costs'; }
  [data-panel='configuration'] .qs-settings__form :deep(.qs-form__section--universe .qs-form__grid) { grid-template-columns: repeat(4, minmax(0, 1fr)); }
  [data-panel='configuration'] .qs-settings__form :deep(.qs-form__section--signals) { display: none; }
  [data-panel='signals'] .qs-settings__form { grid-template-columns: minmax(0, 1fr); grid-template-areas: 'signals'; }
  [data-panel='signals'] .qs-settings__form :deep(.qs-form__section:not(.qs-form__section--signals)) { display: none; }
  .qs-settings__side { display: none; }
  [data-panel='data'] .qs-settings__form { display: none; }
  [data-panel='data'] .qs-settings__side { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); align-items: start; }
  .qs-settings__form :deep(.qs-form__heading) { display: none; }
}

@container manual-settings (max-width: 540px) {
  .qs-settings__identity { grid-template-columns: minmax(0, 1fr) 140px; }
  .qs-settings__name, .qs-settings__description, .qs-settings__manage { grid-column: 1 / -1; }
  [data-panel='data'] .qs-settings__side { grid-template-columns: minmax(0, 1fr); }
  [data-panel='configuration'] .qs-settings__form { grid-template-columns: minmax(0, 1fr); grid-template-areas: 'universe' 'window' 'costs'; }
  [data-panel='configuration'] .qs-settings__form :deep(.qs-form__section--universe .qs-form__grid) { grid-template-columns: repeat(2, minmax(0, 1fr)); }
}
</style>
