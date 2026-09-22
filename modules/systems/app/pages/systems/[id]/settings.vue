<script setup lang="ts">
definePageMeta({ layout: 'systems', path: '/algo/manual/:id/settings' })

import { useAppStore } from '#systems/stores/app'
import { useSystemsStore } from '#systems/stores/systems'
import { useConfigStore } from '#systems/stores/config'
import { useEngine } from '#systems/composables/useEngine'

const route = useRoute()
const app = useAppStore()
const systems = useSystemsStore()
const config = useConfigStore()
const engine = useEngine()

const systemId = computed(() => route.params.id as string)
const system = computed(() => systems.byId(systemId.value))
const planned = computed(() => system.value?.status !== 'ready')

const saving = ref(false)
const savedAt = ref<string | null>(null)
const engineRunning = computed(() => app.engineStatus.status === 'running')
const panel = ref<'configuration' | 'signals' | 'data'>('configuration')

let savedTimer: ReturnType<typeof setTimeout> | null = null

onMounted(() => config.load(systemId.value))
onUnmounted(() => { if (savedTimer) clearTimeout(savedTimer) })

async function save() {
  saving.value = true
  await config.save(systemId.value)
  saving.value = false
  // Inline confirmation next to the button, gone again after a few seconds.
  savedAt.value = new Date().toLocaleTimeString()
  if (savedTimer) clearTimeout(savedTimer)
  savedTimer = setTimeout(() => { savedAt.value = null }, 4000)
}

const engineBusy = ref(false)
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
        <button class="btn btn-primary" :disabled="saving" @click="save">
          {{ saving ? 'Saving…' : 'Save Config' }}
        </button>
      </div>
    </header>

    <div v-if="engineError" class="qs-settings__error">{{ engineError }}</div>

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

.qs-settings__head {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 10px;
}

.qs-settings__title {
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
  [data-panel='data'] .qs-settings__side { grid-template-columns: minmax(0, 1fr); }
  [data-panel='configuration'] .qs-settings__form { grid-template-columns: minmax(0, 1fr); grid-template-areas: 'universe' 'window' 'costs'; }
  [data-panel='configuration'] .qs-settings__form :deep(.qs-form__section--universe .qs-form__grid) { grid-template-columns: repeat(2, minmax(0, 1fr)); }
}
</style>
