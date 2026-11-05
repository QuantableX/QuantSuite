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
    <header class="qn-settings__head">
      <h1>Settings</h1>
      <span class="qn-settings__sub">QuantNotes v{{ runtimeConfig.public.appVersion }}</span>
    </header>

    <div class="qn-settings__panels">
      <section class="qn-section qn-section--general">
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

      <section class="qn-section qn-section--editor">
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

      <section class="qn-section qn-section--properties">
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

      <section class="qn-section qn-section--data">
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
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap);
  height: 100%;
  min-height: 0;
  padding: var(--qss-page-edge);
  overflow: auto;
  background: var(--qn-bg);
  container-type: inline-size;
}

.qn-settings__head {
  flex-shrink: 0;
  display: flex;
  align-items: baseline;
  gap: var(--qss-gap);
}

.qn-settings__head h1 {
  margin: 0;
  font-size: var(--qss-text-title);
  font-weight: 600;
  letter-spacing: -0.01em;
}

.qn-settings__sub {
  color: var(--qn-text-secondary);
  font-size: var(--qss-text-label);
  font-variant-numeric: tabular-nums;
}

/* Three columns: the short groups beside the property list. */
.qn-settings__panels {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: var(--qss-gap);
}

.qn-section--properties {
  grid-column: 3;
  grid-row: 1 / span 2;
}

.qn-section--data {
  grid-column: 1 / span 2;
}

.qn-section {
  padding: var(--qss-panel-pad);
  border: 1px solid var(--qn-border-subtle);
  border-radius: var(--qss-panel-radius);
  background: var(--qn-bg-sidebar);
}

.qn-section h2 {
  margin: 0 0 var(--qss-gap-tight);
  color: var(--qn-text);
  font-size: var(--qss-text-section);
  font-weight: 600;
}

.qn-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
  gap: var(--qss-gap-tight) var(--qss-gap);
  align-items: end;
}

.qn-grid label {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.qn-label {
  color: var(--qn-text-secondary);
  font-size: var(--qss-text-label);
}

.qn-input,
.qn-select {
  width: 100%;
  height: var(--qss-ctl-h);
  padding: 0 8px;
  border: 1px solid var(--qn-border);
  border-radius: var(--qss-ctl-radius);
  background: var(--qn-bg-input);
  color: var(--qn-text);
  font-size: var(--qss-text-label);
  font-family: inherit;
  outline: none;
}

.qn-grid .qn-check {
  flex-direction: row;
  align-items: center;
  gap: var(--qss-gap-tight);
  min-height: var(--qss-ctl-h);
  color: var(--qn-text-secondary);
  font-size: var(--qss-text-label);
}

.qn-hint {
  margin: 0 0 var(--qss-gap-tight);
  color: var(--qn-text-secondary);
  font-size: var(--qss-text-label);
  line-height: 1.5;
}

table {
  width: 100%;
  border-collapse: collapse;
}

td {
  height: var(--qss-row-h);
  padding: 2px 8px 2px 0;
  border-top: 1px solid var(--qn-border-subtle);
  color: var(--qn-text);
  font-size: 13px;
}

td:last-child {
  padding-right: 0;
}

.qn-kind {
  color: var(--qn-text-secondary);
  font-size: var(--qss-text-label);
  white-space: nowrap;
}

.qn-right {
  text-align: right;
}

.qn-right > * + * {
  margin-left: 6px;
}

.qn-warn {
  color: var(--qn-text-secondary);
  font-size: var(--qss-text-meta);
}

.qn-btn {
  height: 26px;
  padding: 0 10px;
  border: 1px solid var(--qn-border);
  border-radius: var(--qss-ctl-radius);
  background: var(--qn-bg-card);
  color: var(--qn-text-secondary);
  font-size: var(--qss-text-label);
  white-space: nowrap;
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
  margin: var(--qss-gap-tight) 0 0;
  font-size: var(--qss-text-label);
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
  font-size: var(--qss-text-meta);
}

/* Two columns: properties beside the data group, under the two short ones. */
@container (max-width: 1100px) {
  .qn-settings__panels {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  .qn-section--properties,
  .qn-section--data {
    grid-column: auto;
    grid-row: auto;
  }
}

@container (max-width: 640px) {
  .qn-settings__panels {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
