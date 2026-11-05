<script setup lang="ts">
/**
 * QuantFlow's own settings — grid shape, week start, calendars and a backup.
 * Suite-level settings live in the shell's QSettingsModal (PLAN-V3 §3).
 */
definePageMeta({ layout: 'flow' })

import { invoke } from '@tauri-apps/api/core'
import { useAppStore } from '#plan/stores/app'
import { useCalendarsStore } from '#plan/stores/calendars'
import type { CalendarColor } from '#plan/types'

const app = useAppStore()
const calendars = useCalendarsStore()
const runtimeConfig = useRuntimeConfig()
const settings = computed(() => app.settings)

const backupPath = ref('')
const error = ref('')
const confirmingDelete = ref<string | null>(null)

async function save() {
  await app.saveSettings()
}

async function backup() {
  error.value = ''
  try {
    backupPath.value = await invoke<string>('plugin:plan|backup_database')
  } catch (e) {
    error.value = String(e)
  }
}

async function removeCalendar(id: string) {
  confirmingDelete.value = null
  error.value = ''
  try {
    await calendars.remove(id)
  } catch (e) {
    error.value = String(e)
  }
}

const COLORS: CalendarColor[] = [
  // Two rows of seven, warm across the top and cool below — the grid's column
  // count and this length are tied together; change one and change the other.
  'red', 'orange', 'amber', 'lime', 'green', 'mint', 'teal',
  'cyan', 'blue', 'indigo', 'purple', 'pink', 'brown', 'slate',
]
</script>

<template>
  <div class="qp-set">
    <header class="qp-set__head">
      <h1>Settings</h1>
      <span class="qp-set__sub">QuantFlow v{{ runtimeConfig.public.appVersion }}</span>
    </header>

    <div class="qp-set__panels">
      <section class="qp-set__section">
        <h2>Grid</h2>
        <div class="qp-set__grid">
          <label>
            <span class="qp-set__label">Opens on</span>
            <select v-model="settings.defaultView" class="qp-select" @change="save">
              <option value="day">Day</option>
              <option value="week">Week</option>
              <option value="month">Month</option>
              <option value="agenda">Agenda</option>
            </select>
          </label>
          <label>
            <span class="qp-set__label">Week starts on</span>
            <select v-model.number="settings.weekStartsOn" class="qp-select" @change="save">
              <option :value="1">Monday</option>
              <option :value="0">Sunday</option>
            </select>
          </label>
          <label>
            <span class="qp-set__label">Snap to</span>
            <select v-model.number="settings.slotMinutes" class="qp-select" @change="save">
              <option :value="5">5 minutes</option>
              <option :value="10">10 minutes</option>
              <option :value="15">15 minutes</option>
              <option :value="30">30 minutes</option>
            </select>
          </label>
          <label>
            <span class="qp-set__label">Time format</span>
            <select v-model="settings.timeFormat" class="qp-select" @change="save">
              <option value="24h">24-hour</option>
              <option value="12h">12-hour</option>
            </select>
          </label>
        </div>
        <label class="qp-set__check">
          <input v-model="settings.showWeekends" type="checkbox" @change="save" />
          <span>Show weekends in the week view</span>
        </label>
      </section>

      <section class="qp-set__section qp-set__section--calendars">
        <h2>Calendars</h2>
        <table>
          <tbody>
            <tr v-for="cal in calendars.list" :key="cal.id">
              <td>
                <input
                  :value="cal.name"
                  class="qp-input"
                  @change="calendars.update(cal.id, { name: ($event.target as HTMLInputElement).value })"
                />
              </td>
              <td class="qp-set__colors">
                <div class="qp-set__swatches">
                  <button
                    v-for="c in COLORS"
                    :key="c"
                    class="qp-dot qp-set__swatch"
                    :data-color="c"
                    :class="{ 'is-on': cal.color === c }"
                    :aria-label="c"
                    @click="calendars.update(cal.id, { color: c })"
                  />
                </div>
              </td>
              <td class="qp-set__right">
                <template v-if="confirmingDelete === cal.id">
                  <span class="qp-set__warn">Deletes its events too.</span>
                  <button class="qp-btn qp-set__danger" @click="removeCalendar(cal.id)">Delete</button>
                  <button class="qp-btn" @click="confirmingDelete = null">Cancel</button>
                </template>
                <button v-else class="qp-btn" @click="confirmingDelete = cal.id">Delete</button>
              </td>
            </tr>
          </tbody>
        </table>
      </section>

      <section class="qp-set__section qp-set__section--data">
        <h2>Calendar data</h2>
        <p class="qp-set__hint">
          Back up your calendars, events and reminders.
        </p>
        <button class="qp-btn" @click="backup">Back up calendar</button>
        <p v-if="backupPath" class="qp-set__ok">Saved to {{ backupPath }}</p>
        <p v-if="error" class="qp-set__err">{{ error }}</p>
      </section>
    </div>
  </div>
</template>

<style scoped>
.qp-set {
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap);
  height: 100%;
  min-height: 0;
  padding: var(--qss-page-edge);
  overflow: auto;
  background: var(--qp-bg);
  container-type: inline-size;
}

.qp-set__head {
  flex-shrink: 0;
  display: flex;
  align-items: baseline;
  gap: var(--qss-gap);
}

.qp-set__head h1 {
  margin: 0;
  font-size: var(--qss-text-title);
  font-weight: 600;
  letter-spacing: -.01em;
}

.qp-set__sub {
  color: var(--qp-text-secondary);
  font-size: var(--qss-text-label);
  font-variant-numeric: tabular-nums;
}

/* Three panels side by side: the calendar list gets the widest, its colours stay on one line. */
.qp-set__panels {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1.5fr) minmax(0, 1fr);
  gap: var(--qss-gap);
}

.qp-set__section {
  min-width: 0;
  padding: var(--qss-panel-pad);
  border: 1px solid var(--qp-border-subtle);
  border-radius: var(--qss-panel-radius);
  background: var(--qp-bg-raised);
}

.qp-set__section h2 {
  margin: 0 0 var(--qss-gap-tight);
  color: var(--qp-text);
  font-size: var(--qss-text-section);
  font-weight: 600;
}

.qp-set__grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(130px, 1fr));
  gap: var(--qss-gap-tight) var(--qss-gap);
}

.qp-set__grid label {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.qp-set__label {
  color: var(--qp-text-secondary);
  font-size: var(--qss-text-label);
}

.qp-set .qp-input,
.qp-set .qp-select {
  height: var(--qss-ctl-h);
  padding: 0 8px;
  font-size: var(--qss-text-label);
}

.qp-set__check {
  display: flex;
  align-items: center;
  gap: var(--qss-gap-tight);
  margin-top: var(--qss-gap-tight);
  color: var(--qp-text-secondary);
  font-size: var(--qss-text-label);
}

table {
  width: 100%;
  border-collapse: collapse;
}

/* A calendar's row wraps its colours under the name when the panel is narrow. */
tr {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px var(--qss-gap);
  padding: 6px 0;
  border-top: 1px solid var(--qp-border-subtle);
}

tr:first-child {
  padding-top: 0;
  border-top: none;
}

tr:last-child {
  padding-bottom: 0;
}

td {
  padding: 0;
}

td:first-child {
  flex: 1 1 120px;
  min-width: 0;
}

.qp-set__colors {
  flex: 0 0 auto;
}

.qp-set__swatches {
  display: flex;
  gap: 4px;
}

.qp-set__swatch {
  width: 15px;
  height: 15px;
  border: none;
  border-radius: 5px;
  cursor: pointer;
}

.qp-set__swatch.is-on {
  outline: 2px solid var(--qp-accent);
  outline-offset: 1px;
}

.qp-set__right {
  flex: 0 0 auto;
  margin-left: auto;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 6px;
}

.qp-set .qp-btn {
  height: 26px;
  padding: 0 10px;
  font-size: var(--qss-text-label);
  white-space: nowrap;
}

.qp-set__warn {
  color: var(--qp-text-secondary);
  font-size: var(--qss-text-meta);
}

.qp-set__danger:hover {
  border-color: var(--qss-error, #ff4757);
  color: var(--qss-error, #ff4757);
}

.qp-set__hint {
  margin: 0 0 var(--qss-gap-tight);
  color: var(--qp-text-secondary);
  font-size: var(--qss-text-label);
  line-height: 1.5;
}

.qp-set__ok,
.qp-set__err {
  margin: var(--qss-gap-tight) 0 0;
  font-size: var(--qss-text-label);
  overflow-wrap: anywhere;
}

.qp-set__ok {
  color: var(--qss-success, #2ed573);
}

.qp-set__err {
  color: var(--qss-error, #ff4757);
}

code {
  padding: 1px 5px;
  border-radius: 4px;
  background: var(--qp-bg-input);
  font-family: var(--qss-font-mono, monospace);
  font-size: var(--qss-text-meta);
}

/* Two columns, with the data group running under both. */
@container (max-width: 1100px) {
  .qp-set__panels {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  .qp-set__section--data {
    grid-column: 1 / -1;
  }
}

@container (max-width: 640px) {
  .qp-set__panels {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
