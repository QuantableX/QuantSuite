<script setup lang="ts">
/**
 * Suite → Data → Move this installation to the server (PLAN-MIGRATION §2.3,
 * card Migration 4/4).
 *
 * 1. Export the bundle (as "Export everything", with its defaults).
 * 2. The server PIN, then the bundle over the paired connection — checksummed
 *    and resumable: after an interruption "Continue" sends only the rest.
 * 3. The server checks it and proposes the path table; you correct it here.
 * 4. The server imports, verifies, and restarts onto the moved data, keeping
 *    its own gateway (its key, the paired devices, where it listens).
 * 5. Here: the move is confirmed with the report's code (this PC stops
 *    trading for good, nothing is deleted) and this PC uses the server.
 */
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

interface Conn { id: string; name: string; address: string; role: 'full' | 'no_trading' | 'read_only' }
interface Info { bundle: string | null; bytes: number | null; connection: string | null; created_at: string | null }
interface Row { kind: string; name: string; old: string; new: string | null }
interface Proposal { machine: string; created_at: string; suite_version: string; files: number; bytes: number; secrets: number; rows: Row[] }
interface Check { name: string; ok: boolean; detail: string }
interface Report { ok: boolean; checks: Check[]; manual_steps: string[]; move_code: string; report_file: string }
interface Progress { phase: 'export' | 'send'; done: number; total: number }

type Step = 'start' | 'export' | 'send' | 'check' | 'paths' | 'import' | 'report' | 'finish'

const connections = ref<Conn[]>([])
const info = ref<Info | null>(null)
const chosen = ref('')
const pass1 = ref('')
const pass2 = ref('')
const pin = ref('')
const step = ref<Step>('start')
const progress = ref<Progress | null>(null)
const proposal = ref<Proposal | null>(null)
const report = ref<Report | null>(null)
const error = ref<string | null>(null)

const full = computed(() => connections.value.filter((c) => c.role === 'full'))
const target = computed(() => connections.value.find((c) => c.id === chosen.value) ?? null)
const resumable = computed(() => !!info.value?.bundle && info.value.connection === chosen.value)
const busy = computed(() => !['start', 'paths', 'report'].includes(step.value))
const canStart = computed(
  () => !!chosen.value && pass1.value.length >= 12 && (resumable.value || pass1.value === pass2.value) && pin.value.length >= 6 && !busy.value,
)

function inTauri(): boolean {
  return import.meta.client && '__TAURI__' in window
}

function mb(n: number | null | undefined): string {
  return n == null ? '—' : n >= 1e9 ? `${(n / 1e9).toFixed(2)} GB` : `${(n / 1e6).toFixed(1)} MB`
}

const progressText = computed(() => {
  const p = progress.value
  if (!p || p.total <= 0) return ''
  const pct = Math.min(100, Math.round((p.done / p.total) * 100))
  return `${p.phase === 'export' ? 'Exporting' : 'Sending'} ${mb(p.done)} of ${mb(p.total)} (${pct} %)`
})

async function load() {
  if (!inTauri()) return
  connections.value = await invoke<Conn[]>('plugin:qs|gateway_connections').catch(() => [])
  info.value = await invoke<Info>('plugin:qs|move_info').catch(() => null)
  if (!chosen.value) chosen.value = info.value?.connection ?? full.value[0]?.id ?? ''
}

async function run() {
  error.value = null
  report.value = null
  try {
    if (!resumable.value) {
      step.value = 'export'
      info.value = await invoke<Info>('plugin:qs|move_export', { connection: chosen.value, passphrase: pass1.value })
    }
    step.value = 'send'
    await invoke('plugin:qs|move_send', { connection: chosen.value, pin: pin.value })
    step.value = 'check'
    proposal.value = await invoke<Proposal>('plugin:qs|move_remote_propose', { passphrase: pass1.value })
    step.value = 'paths'
  } catch (e) {
    error.value = String(e)
    step.value = 'start'
    info.value = await invoke<Info>('plugin:qs|move_info').catch(() => info.value)
  }
}

async function cancel() {
  await invoke('plugin:qs|move_cancel').catch(() => undefined)
}

async function importNow() {
  if (!proposal.value) return
  error.value = null
  step.value = 'import'
  try {
    report.value = await invoke<Report>('plugin:qs|move_remote_import', { passphrase: pass1.value, rows: proposal.value.rows })
    step.value = 'report'
  } catch (e) {
    error.value = String(e)
    step.value = 'paths'
  }
}

async function finish() {
  if (!report.value?.ok) return
  error.value = null
  step.value = 'finish'
  try {
    // This PC stops trading for good (nothing is sold or deleted)…
    await invoke('plugin:algo|trading_confirm_move', { code: report.value.move_code })
    // …and uses the server from now on: QuantSuite restarts.
    await invoke('plugin:qs|move_finish', { connection: chosen.value })
  } catch (e) {
    error.value = String(e)
    step.value = 'report'
  }
}

let unlisten: UnlistenFn | null = null
onMounted(async () => {
  await load()
  if (inTauri()) unlisten = await listen<Progress>('qs:move-progress', (e) => (progress.value = e.payload))
})
onUnmounted(() => unlisten?.())
</script>

<template>
  <div class="qsu-row qsm-section">
    <div class="qsu-row-text">
      <p class="qsu-label">Move this installation to the server</p>
      <p class="qsu-hint">
        Everything goes to the server over the paired connection — encrypted, checksummed, and continued where it stopped
        if the connection drops. The server imports it, checks it and restarts on it, keeping its own pairing. This PC then
        stops trading for good and uses the server; its own data stays here as an archive, nothing is deleted.
      </p>

      <p v-if="!full.length" class="qsu-hint qsm-warn">
        Pair this PC with your server first (Settings → Server → Connect to a server) — with the role Full.
      </p>
      <div v-else class="qsm-form">
        <div class="qsm-line">
          <select v-model="chosen" class="qsu-select" :disabled="busy">
            <option v-for="c in full" :key="c.id" :value="c.id">{{ c.name }} ({{ c.address }})</option>
          </select>
        </div>
        <p v-if="resumable" class="qsu-hint">
          A bundle from {{ info?.created_at?.slice(0, 16).replace('T', ' ') }} ({{ mb(info?.bytes) }}) is ready: it is sent
          from where it stopped. Enter its passphrase.
        </p>
        <input v-model="pass1" class="qsm-input" type="password" autocomplete="new-password" placeholder="Bundle passphrase (at least 12 characters)" :disabled="busy">
        <input v-if="!resumable" v-model="pass2" class="qsm-input" type="password" autocomplete="new-password" placeholder="The passphrase again" :disabled="busy">
        <input v-model="pin" class="qsm-input" type="password" inputmode="numeric" autocomplete="off" placeholder="The server's PIN" :disabled="busy">
        <div class="qsm-line">
          <button class="qsu-btn qsu-btn--primary" :disabled="!canStart" @click="run">
            {{ resumable ? 'Continue the move' : 'Move to ' + (target?.name ?? 'the server') }}
          </button>
          <button v-if="step === 'send'" class="qsu-btn" @click="cancel">Stop sending</button>
        </div>
        <p v-if="step === 'export' || step === 'send'" class="qsu-hint">{{ progressText || (step === 'export' ? 'Exporting…' : 'Connecting…') }}</p>
        <p v-if="step === 'check'" class="qsu-hint">The server checks the bundle…</p>

        <template v-if="proposal && (step === 'paths' || step === 'import')">
          <p class="qsu-hint">
            The server read the bundle: {{ proposal.files }} files, {{ mb(proposal.bytes) }}, {{ proposal.secrets }} secret(s).
            Where are these folders on the server? Empty leaves a workspace unlinked.
          </p>
          <table class="qsm-paths">
            <thead><tr><th>Folder</th><th>Here</th><th>On the server</th></tr></thead>
            <tbody>
              <tr v-for="row in proposal.rows" :key="row.old">
                <td>{{ row.name }}</td>
                <td class="qsm-mono">{{ row.old }}</td>
                <td><input v-model="row.new" class="qsm-input" :placeholder="row.kind === 'workspace' ? 'Leave empty to link later' : ''" :disabled="row.kind === 'data-root' || step === 'import'"></td>
              </tr>
            </tbody>
          </table>
          <div class="qsm-line">
            <button class="qsu-btn qsu-btn--primary" :disabled="step === 'import'" @click="importNow">
              {{ step === 'import' ? 'The server imports…' : 'Import on the server' }}
            </button>
          </div>
        </template>

        <template v-if="report">
          <p class="qsu-hint" :class="report.ok ? 'qsm-ok' : 'qsm-error'">
            {{ report.ok ? 'The server checked everything and restarts on the moved data.' : 'Something did not check out on the server; nothing switched there.' }}
          </p>
          <ul class="qsm-checks">
            <li v-for="(c, i) in report.checks" :key="i" :class="c.ok ? '' : 'qsm-error'">{{ c.ok ? '✓' : '✗' }} {{ c.detail }}</li>
          </ul>
          <template v-if="report.ok">
            <p class="qsu-hint">Still to do by you:</p>
            <ul class="qsm-checks">
              <li v-for="(s, i) in report.manual_steps" :key="i">{{ s }}</li>
            </ul>
            <div class="qsm-line">
              <button class="qsu-btn qsu-btn--primary" :disabled="step === 'finish'" @click="finish">Finish: this PC uses the server</button>
            </div>
            <p class="qsu-hint">This stops trading here for good (move code {{ report.move_code }}) and restarts QuantSuite.</p>
          </template>
        </template>
        <p v-if="error" class="qsu-hint qsm-error">{{ error }}</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.qsm-section { margin-top: 18px; }
.qsm-form { display: flex; flex-direction: column; gap: 8px; max-width: 620px; margin-top: 8px; }
.qsm-line { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
.qsm-input { padding: 6px 8px; border-radius: 6px; border: 1px solid var(--qss-border, #444); background: transparent; color: inherit; font: inherit; font-size: 13px; }
.qsm-paths { width: 100%; border-collapse: collapse; font-size: 12px; }
.qsm-paths th { text-align: left; font-weight: 500; color: var(--qss-text-muted, inherit); padding: 4px 8px 4px 0; }
.qsm-paths td { padding: 3px 8px 3px 0; }
.qsm-paths .qsm-input { width: 100%; }
.qsm-checks { list-style: none; padding: 0; margin: 4px 0 0; display: flex; flex-direction: column; gap: 3px; font-size: 12px; color: var(--qss-text-secondary, inherit); }
.qsm-mono { font-family: var(--qss-font-mono, monospace); font-size: 11px; }
.qsm-warn { color: var(--qss-warning, #d9a441); }
.qsm-error { color: var(--qss-error, #e5534b); }
.qsm-ok { color: var(--qss-success, #3fb950); }
</style>
