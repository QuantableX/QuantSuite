<script setup lang="ts">
/**
 * Suite → Server (PLAN-SERVER-CLIENT §5, card Server 2/7): run this
 * installation as the server at home — tray only, started at boot, restarted
 * by a watchdog, and the one machine that trades — plus its health. The
 * gateway, devices and connections (card Server 3/7) are in
 * SettingsSuiteServerGateway.
 */
import { invoke } from '@tauri-apps/api/core'
import { useServerLink } from '@quantsuite/core'
import SettingsSuiteServerGateway from './SettingsSuiteServerGateway.vue'

// While this PC uses a server, the health below is the server's (server_status
// goes there) and this PC's own server switches do not apply (card 5).
const link = useServerLink()

type Role = 'local' | 'server' | 'client'
interface Status {
  role: Role
  stored_role: Role | null
  version: string
  started_at: string
  uptime_secs: number
  data_root: string
  disk_free_bytes: number | null
  disk_total_bytes: number | null
  gateway: string | null
  watchdog: boolean | null
  autostart: boolean | null
}
interface BotLine { id: string; name: string; mode: string; status: string; last_error: string | null }
interface NightlyLine { system_id: string; close_date: string | null; status: string; finished_at: string | null }
interface Trading { executors_allowed: boolean; kill_switch: boolean; bots_running: number; bots: BotLine[]; nightly: NightlyLine[] }

const status = ref<Status | null>(null)
const trading = ref<Trading | null>(null)
const busy = ref(false)
const error = ref<string | null>(null)
const restartHint = ref(false)

function inTauri(): boolean {
  return import.meta.client && '__TAURI__' in window
}

function uptime(s: number): string {
  const d = Math.floor(s / 86400)
  const h = Math.floor((s % 86400) / 3600)
  const m = Math.floor((s % 3600) / 60)
  return d ? `${d} d ${h} h` : h ? `${h} h ${m} min` : `${m} min`
}

function gb(bytes: number | null): string {
  return bytes == null ? '—' : `${(bytes / 1e9).toFixed(1)} GB`
}

function when(iso: string | null): string {
  if (!iso) return '—'
  const d = new Date(iso)
  return `${d.toISOString().slice(0, 10)} ${d.toISOString().slice(11, 16)} UTC`
}

const isServer = computed(() => status.value?.stored_role === 'server')
const diskLow = computed(() => {
  const s = status.value
  return !!s && s.disk_free_bytes != null && s.disk_total_bytes != null && s.disk_free_bytes / s.disk_total_bytes < 0.1
})

async function load() {
  if (!inTauri()) return
  error.value = null
  try {
    status.value = await invoke<Status>('plugin:qs|server_status')
    trading.value = await invoke<Trading>('plugin:algo|server_trading_health').catch(() => null)
  } catch (e) {
    error.value = String(e)
  }
}

async function setServer(on: boolean) {
  busy.value = true
  error.value = null
  try {
    await invoke('plugin:qs|server_set_role', { role: on ? 'server' : 'local' })
    restartHint.value = true
    await load()
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}

async function setWatchdog(on: boolean) {
  busy.value = true
  error.value = null
  try {
    await invoke('plugin:qs|server_set_watchdog', { enabled: on })
    await load()
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}

let timer: ReturnType<typeof setInterval> | null = null
onMounted(() => {
  load()
  timer = setInterval(load, 30_000)
})
onUnmounted(() => {
  if (timer) clearInterval(timer)
})
</script>

<template>
  <div>
    <div v-if="link.isClient.value" class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">This PC uses the server {{ link.status.value?.connection?.name }}</p>
        <p class="qsu-hint">
          Trading, research and your personal data come from the server; QuantPilot, QuantCode, QuantCanvas, the
          console, QuantMCP, memory and the HUD work on this PC. The health below is the server's. To use this PC's
          own data again, choose "This PC" under Connect to a server.
        </p>
      </div>
    </div>

    <div v-if="!link.isClient.value" class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">Run as server</p>
        <p class="qsu-hint">
          This machine runs QuantSuite day and night: it starts in the tray without the window (open it from the tray
          icon), runs the bots and the nightly Manual trades, and keeps all the data. Your other PCs connect to it.
        </p>
        <p v-if="restartHint" class="qsu-hint qsv-warn">Takes effect at the next start of QuantSuite.</p>
      </div>
      <button
        class="qsu-toggle"
        :class="{ 'is-on': isServer }"
        role="switch"
        :aria-checked="isServer"
        :disabled="!status || busy || status.role === 'client'"
        @click="setServer(!isServer)"
      />
    </div>

    <div v-if="!link.isClient.value" class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">Restart QuantSuite when it exits</p>
        <p class="qsu-hint">
          A Windows Task Scheduler entry checks every 5 minutes while you are signed in and starts QuantSuite again
          after a crash or a kill — in the tray, as a server. While it runs, the check does nothing.
        </p>
      </div>
      <button
        class="qsu-toggle"
        :class="{ 'is-on': status?.watchdog === true }"
        role="switch"
        :aria-checked="status?.watchdog === true"
        :disabled="!status || status.watchdog === null || busy"
        :title="status?.watchdog === null ? 'Windows only' : ''"
        @click="setWatchdog(status?.watchdog !== true)"
      />
    </div>

    <SettingsSuiteServerGateway />

    <div class="qsu-row qsv-section">
      <div class="qsu-row-text">
        <p class="qsu-label">Setting up the server machine</p>
        <ul class="qsv-list">
          <li>A dedicated Windows user that signs in automatically, with "Start QuantSuite with Windows" on (General).</li>
          <li>BitLocker with TPM: the disk stays encrypted and unlocks without a prompt after a power loss.</li>
          <li>Sleep and hibernation off; the screen may turn off.</li>
          <li>Windows Update restarts outside 23:00–02:00 UTC (Active hours), so the nightly runs are never interrupted.</li>
          <li>Wired network, a UPS (protects the databases and open orders), and "power on after power loss" in the BIOS.</li>
          <li>Hardware: 4+ cores, 16–32 GB RAM, an SSD.</li>
        </ul>
      </div>
    </div>

    <div v-if="status" class="qsu-row qsv-section">
      <div class="qsu-row-text">
        <p class="qsu-label">Health</p>
        <table class="qsv-table">
          <tbody>
            <tr><th>Role</th><td>{{ status.role }}{{ status.stored_role && status.stored_role !== status.role ? ` (stored: ${status.stored_role})` : '' }}</td></tr>
            <tr><th>Version</th><td class="qsv-mono">{{ status.version }}</td></tr>
            <tr><th>Up since</th><td>{{ when(status.started_at) }} · {{ uptime(status.uptime_secs) }}</td></tr>
            <tr><th>Disk</th><td :class="{ 'qsv-warn': diskLow }">{{ gb(status.disk_free_bytes) }} free of {{ gb(status.disk_total_bytes) }} · <span class="qsv-mono">{{ status.data_root }}</span></td></tr>
            <tr><th>Started at boot</th><td>{{ status.autostart === null ? '—' : status.autostart ? 'yes' : 'no' }}</td></tr>
            <tr><th>Watchdog</th><td>{{ status.watchdog === null ? 'not on this OS' : status.watchdog ? 'on' : 'off' }}</td></tr>
            <tr><th>Gateway</th><td>{{ status.gateway ? `listening on ${status.gateway}` : 'not listening' }}</td></tr>
            <template v-if="trading">
              <tr><th>Trading</th><td :class="{ 'qsv-warn': trading.kill_switch || !trading.executors_allowed }">
                {{ !trading.executors_allowed ? 'off here — this PC is a client' : trading.kill_switch ? 'stopped by the kill switch' : 'allowed' }}
              </td></tr>
              <tr><th>Bots</th><td>{{ trading.bots_running }} running of {{ trading.bots.length }}</td></tr>
            </template>
          </tbody>
        </table>
        <template v-if="trading && trading.bots.length">
          <p class="qsu-hint qsv-sub">Bots</p>
          <ul class="qsv-list">
            <li v-for="b in trading.bots" :key="b.id" :class="{ 'qsv-warn': b.status === 'error' || b.status === 'paused' }">
              {{ b.name }} · {{ b.mode }} · {{ b.status }}<template v-if="b.last_error"> — {{ b.last_error }}</template>
            </li>
          </ul>
        </template>
        <template v-if="trading && trading.nightly.length">
          <p class="qsu-hint qsv-sub">Last nightly run per system</p>
          <ul class="qsv-list">
            <li v-for="n in trading.nightly" :key="n.system_id" :class="{ 'qsv-warn': n.status !== 'done' }">
              {{ n.system_id }} · close {{ n.close_date ?? '—' }} · {{ n.status }} · {{ when(n.finished_at) }}
            </li>
          </ul>
        </template>
      </div>
    </div>
    <p v-if="error" class="qsu-hint qsv-error">{{ error }}</p>
  </div>
</template>

<style scoped>
.qsv-section { margin-top: 18px; }
.qsv-list { margin: 6px 0 0; padding-left: 18px; font-size: 12px; display: flex; flex-direction: column; gap: 3px; color: var(--qss-text-secondary, inherit); }
.qsv-table { border-collapse: collapse; font-size: 12px; margin-top: 6px; }
.qsv-table th { text-align: left; font-weight: 500; color: var(--qss-text-muted, inherit); padding: 3px 12px 3px 0; vertical-align: top; }
.qsv-table td { padding: 3px 0; }
.qsv-mono { font-family: var(--qss-font-mono, monospace); font-size: 11px; }
.qsv-sub { margin-top: 10px; }
.qsv-warn { color: var(--qss-warning, #d9a441); }
.qsv-error { color: var(--qss-error, #e5534b); }
</style>
