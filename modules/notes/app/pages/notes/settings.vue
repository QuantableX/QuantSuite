<script setup lang="ts">
/**
 * QuantNotes' own settings page — editor preferences, the property schema,
 * and a manual database backup.
 *
 * Suite-level settings live in the shell's QSettingsModal (PLAN-V3 §3); this
 * page is what the module's General section there routes to.
 */
definePageMeta({ layout: 'notes' })

import { invoke } from '@tauri-apps/api/core'
import { useAppStore } from '#notes/stores/app'
import { useSchemaStore } from '#notes/stores/schema'

const app = useAppStore()
const schema = useSchemaStore()
const runtimeConfig = useRuntimeConfig()
const settings = computed(() => app.settings)

const backupPath = ref('')
const backupError = ref('')

async function save() {
  await app.saveSettings()
}

async function backup() {
  backupError.value = ''
  try {
    backupPath.value = await invoke<string>('plugin:notes|backup_database')
  } catch (error) {
    backupError.value = String(error)
  }
}

const KIND_LABELS: Record<string, string> = {
  text: 'Text',
  number: 'Number',
  select: 'Select',
  multi_select: 'Multi-select',
  date: 'Date',
  checkbox: 'Checkbox',
  url: 'URL',
}

/** Deleting a property drops its values from every note, so it asks first. */
const confirmingDelete = ref<string | null>(null)

async function deleteProperty(id: string) {
  confirmingDelete.value = null
  await schema.deleteProperty(id)
}
</script>

<template>
  <div class="qn-settings">
    <div class="qn-settings__card">
      <header class="qn-settings__head">
        <h1>Settings</h1>
        <span class="qn-settings__sub">QuantNotes v{{ runtimeConfig.public.appVersion }}</span>
      </header>

      <section class="qn-section">
        <h2>General</h2>
        <div class="qn-grid">
          <label>
            <span class="qn-label">Theme</span>
            <select v-model="settings.theme" class="qn-select" @change="save">
              <option value="dark">Dark</option>
              <option value="light">Light</option>
              <option value="system">System</option>
            </select>
          </label>
          <label>
            <span class="qn-label">Font size</span>
            <input v-model.number="settings.fontSize" class="qn-input" type="number" min="11" max="22" @change="save" />
          </label>
          <label>
            <span class="qn-label">Note font</span>
            <select v-model="settings.defaultNoteFont" class="qn-select" @change="save">
              <option value="sans">Sans</option>
              <option value="serif">Serif</option>
              <option value="mono">Mono</option>
            </select>
          </label>
          <label>
            <span class="qn-label">Opens on</span>
            <select v-model="settings.defaultViewId" class="qn-select" @change="save">
              <option value="">First view</option>
              <option v-for="view in schema.views" :key="view.id" :value="view.id">{{ view.name }}</option>
            </select>
          </label>
        </div>
      </section>

      <section class="qn-section">
        <h2>Editor</h2>
        <div class="qn-grid">
          <label class="qn-check">
            <input v-model="settings.smartQuotes" type="checkbox" @change="save" />
            <span>Smart quotes</span>
          </label>
          <label class="qn-check">
            <input v-model="settings.autoformat" type="checkbox" @change="save" />
            <span>Markdown autoformat</span>
          </label>
          <label>
            <span class="qn-label">Spellcheck language</span>
            <input v-model="settings.spellcheckLanguage" class="qn-input" @change="save" />
          </label>
        </div>
      </section>

      <section class="qn-section">
        <h2>Properties</h2>
        <p class="qn-hint">
          The collection's schema. Every note carries these; each view chooses
          which of them it shows.
        </p>
        <table>
          <tbody>
            <tr v-for="property in schema.properties" :key="property.id">
              <td>{{ property.name }}</td>
              <td class="qn-kind">{{ KIND_LABELS[property.kind] ?? property.kind }}</td>
              <td class="qn-right">
                <template v-if="confirmingDelete === property.id">
                  <span class="qn-warn">Removes its values from every note.</span>
                  <button class="qn-btn qn-btn--danger" @click="deleteProperty(property.id)">Delete</button>
                  <button class="qn-btn" @click="confirmingDelete = null">Cancel</button>
                </template>
                <button v-else class="qn-btn" @click="confirmingDelete = property.id">Delete</button>
              </td>
            </tr>
            <tr v-if="!schema.properties.length">
              <td colspan="3" class="qn-hint">No properties yet.</td>
            </tr>
          </tbody>
        </table>
      </section>

      <section class="qn-section">
        <h2>Data</h2>
        <p class="qn-hint">
          Everything lives in one database under
          <code>~/.quantsuite/modules/notes/</code>.
        </p>
        <button class="qn-btn" @click="backup">Back up now</button>
        <p v-if="backupPath" class="qn-ok">Saved to {{ backupPath }}</p>
        <p v-if="backupError" class="qn-err">{{ backupError }}</p>
      </section>
    </div>
  </div>
</template>

<style scoped>
.qn-settings {
  height: 100%;
  padding: 28px;
  overflow: auto;
  background: var(--qn-bg);
}

.qn-settings__card {
  max-width: 820px;
  margin: 0 auto;
  padding: 24px;
  border: 1px solid var(--qn-border);
  border-radius: var(--qn-radius-lg);
  background: var(--qn-bg-sidebar);
}

.qn-settings__head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  margin-bottom: 20px;
}

.qn-settings__head h1 {
  margin: 0;
  font-size: 21px;
  font-weight: 700;
}

.qn-settings__sub {
  color: var(--qn-text-muted);
  font-size: 11px;
}

.qn-section {
  padding: 18px 0;
  border-top: 1px solid var(--qn-border-subtle);
}

.qn-section h2 {
  margin: 0 0 10px;
  color: var(--qn-text-secondary);
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.qn-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: 12px;
}

.qn-grid label {
  display: flex;
  flex-direction: column;
  gap: 5px;
}

.qn-label {
  color: var(--qn-text-muted);
  font-size: 11px;
}

.qn-input,
.qn-select {
  padding: 6px 9px;
  border: 1px solid var(--qn-border);
  border-radius: var(--qn-radius);
  background: var(--qn-bg-input);
  color: var(--qn-text);
  font-size: 13px;
  font-family: inherit;
  outline: none;
}

.qn-check {
  flex-direction: row !important;
  align-items: center;
  gap: 8px !important;
  color: var(--qn-text-secondary);
  font-size: 13px;
}

.qn-hint {
  margin: 0 0 10px;
  color: var(--qn-text-muted);
  font-size: 12px;
  line-height: 1.5;
}

table {
  width: 100%;
  border-collapse: collapse;
}

td {
  padding: 8px 0;
  border-bottom: 1px solid var(--qn-border-subtle);
  color: var(--qn-text);
  font-size: 13px;
}

.qn-kind {
  color: var(--qn-text-muted);
  font-size: 12px;
}

.qn-right {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
}

.qn-warn {
  color: var(--qn-text-muted);
  font-size: 11px;
}

.qn-btn {
  padding: 5px 11px;
  border: 1px solid var(--qn-border);
  border-radius: var(--qn-radius);
  background: var(--qn-bg-card);
  color: var(--qn-text-secondary);
  font-size: 12px;
  cursor: pointer;
}

.qn-btn:hover {
  border-color: var(--qn-accent);
  color: var(--qn-text);
}

.qn-btn--danger:hover {
  border-color: var(--qn-error);
  color: var(--qn-error);
}

.qn-ok,
.qn-err {
  margin: 8px 0 0;
  font-size: 12px;
}

.qn-ok {
  color: var(--qn-success);
}

.qn-err {
  color: var(--qn-error);
}

code {
  padding: 1px 5px;
  border-radius: 4px;
  background: var(--qn-bg-input);
  font-family: var(--qss-font-mono, monospace);
  font-size: 11px;
}
</style>
