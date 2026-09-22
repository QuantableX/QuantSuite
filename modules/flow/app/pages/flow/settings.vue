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
    <div class="qp-set__card">
      <header class="qp-set__head">
        <h1>Settings</h1>
        <span class="qp-set__sub">QuantFlow v{{ runtimeConfig.public.appVersion }}</span>
      </header>

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

      <section class="qp-set__section">
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
              <td class="qp-set__swatches">
                <button
                  v-for="c in COLORS"
                  :key="c"
                  class="qp-dot qp-set__swatch"
                  :data-color="c"
                  :class="{ 'is-on': cal.color === c }"
                  :aria-label="c"
                  @click="calendars.update(cal.id, { color: c })"
                />
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

      <section class="qp-set__section">
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
  height: 100%;
  padding: 28px;
  overflow: auto;
  background: var(--qp-bg);
}

.qp-set__card {
  max-width: 820px;
  margin: 0 auto;
  padding: 24px;
  border: 1px solid var(--qp-border);
  border-radius: var(--qp-radius-lg);
  background: var(--qp-bg-raised);
}

.qp-set__head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  margin-bottom: 20px;
}

.qp-set__head h1 {
  margin: 0;
  font-size: 21px;
  font-weight: 700;
}

.qp-set__sub {
  color: var(--qp-text-muted);
  font-size: 11px;
}

.qp-set__section {
  padding: 18px 0;
  border-top: 1px solid var(--qp-border-subtle);
}

.qp-set__section h2 {
  margin: 0 0 10px;
  color: var(--qp-text-secondary);
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.qp-set__grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(190px, 1fr));
  gap: 12px;
}

.qp-set__grid label {
  display: flex;
  flex-direction: column;
  gap: 5px;
}

.qp-set__label {
  color: var(--qp-text-muted);
  font-size: 11px;
}

.qp-set__check {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 12px;
  color: var(--qp-text-secondary);
  font-size: 12px;
}

table {
  width: 100%;
  border-collapse: collapse;
}

td {
  padding: 7px 6px 7px 0;
  border-bottom: 1px solid var(--qp-border-subtle);
  vertical-align: middle;
}

.qp-set__swatches {
  display: flex;
  gap: 4px;
  width: 190px;
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
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
}

.qp-set__warn {
  color: var(--qp-text-muted);
  font-size: 11px;
}

.qp-set__danger:hover {
  border-color: var(--qss-error, #ff4757);
  color: var(--qss-error, #ff4757);
}

.qp-set__hint {
  margin: 0 0 10px;
  color: var(--qp-text-muted);
  font-size: 12px;
  line-height: 1.5;
}

.qp-set__ok,
.qp-set__err {
  margin: 8px 0 0;
  font-size: 12px;
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
  font-size: 11px;
}
</style>
