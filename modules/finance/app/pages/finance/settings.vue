<script setup lang="ts">
/**
 * QuantFinance's own settings — currency, locale and a manual backup.
 *
 * Accounts and categories went with the ledger, and a plan line carries its
 * own colour, so there is nothing else left to configure.
 */
definePageMeta({ layout: 'finance' })

import { invoke } from '@tauri-apps/api/core'
import { useAppStore } from '#finance/stores/app'
import { usePlanStore } from '#finance/stores/plan'

const app = useAppStore()
const plan = usePlanStore()
const runtimeConfig = useRuntimeConfig()
const settings = computed(() => app.settings)

const backupPath = ref('')
const error = ref('')

async function save() {
  await app.saveSettings()
}

async function backup() {
  error.value = ''
  try {
    backupPath.value = await invoke<string>('plugin:finance|backup_database')
  } catch (e) {
    error.value = String(e)
  }
}
</script>

<template>
  <div class="qf-set">
    <div class="qf-set__card">
      <header class="qf-set__head">
        <h1>Settings</h1>
        <span class="qf-set__sub">QuantFinance v{{ runtimeConfig.public.appVersion }}</span>
      </header>

      <section class="qf-set__section">
        <h2>General</h2>
        <div class="qf-set__grid">
          <label>
            <span class="qf-set__label">Currency</span>
            <input v-model="settings.currency" class="qf-input" maxlength="3" @change="save" />
          </label>
          <label>
            <span class="qf-set__label">Locale</span>
            <input v-model="settings.locale" class="qf-input" @change="save" />
          </label>
        </div>
        <p class="qf-set__hint">
          One currency for the whole plan. Multi-currency with exchange rates is
          a separate project, not a setting.
        </p>
      </section>

      <section class="qf-set__section">
        <h2>Data</h2>
        <p class="qf-set__hint">
          {{ plan.items.length }} line(s) in one database under
          <code>~/.quantsuite/modules/finance/</code>.
        </p>
        <button class="qf-btn" @click="backup">Back up now</button>
        <p v-if="backupPath" class="qf-set__ok">Saved to {{ backupPath }}</p>
        <p v-if="error" class="qf-set__err">{{ error }}</p>
      </section>
    </div>
  </div>
</template>

<style scoped>
.qf-set {
  height: 100%;
  padding: 24px;
  overflow: auto;
  background: var(--qf-bg);
}

.qf-set__card {
  max-width: 720px;
  margin: 0 auto;
  padding: 24px;
  border: 1px solid var(--qf-border);
  border-radius: var(--qf-radius-lg);
  background: var(--qf-bg-raised);
}

.qf-set__head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  margin-bottom: 18px;
}

.qf-set__head h1 {
  margin: 0;
  font-size: 20px;
  font-weight: 700;
}

.qf-set__sub {
  color: var(--qf-text-muted);
  font-size: 11px;
}

.qf-set__section {
  padding: 16px 0;
  border-top: 1px solid var(--qf-border-subtle);
}

.qf-set__section h2 {
  margin: 0 0 10px;
  color: var(--qf-text-secondary);
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.qf-set__grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
  gap: 12px;
}

.qf-set__grid label {
  display: flex;
  flex-direction: column;
  gap: 5px;
  min-width: 0;
}

.qf-set__label {
  color: var(--qf-text-muted);
  font-size: 11px;
}

.qf-set__hint {
  margin: 8px 0 10px;
  color: var(--qf-text-muted);
  font-size: 12px;
  line-height: 1.5;
}

.qf-set__ok {
  margin: 8px 0 0;
  color: var(--qf-positive);
  font-size: 12px;
}

.qf-set__err {
  margin: 8px 0 0;
  color: var(--qf-negative);
  font-size: 12px;
}

code {
  padding: 1px 5px;
  border-radius: 4px;
  background: var(--qf-bg-input);
  font-family: var(--qss-font-mono, monospace);
  font-size: 11px;
}
</style>
