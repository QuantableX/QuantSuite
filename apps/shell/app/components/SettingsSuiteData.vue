<script setup lang="ts">
/**
 * Suite → Data: "Export everything" (PLAN-MIGRATION §2.1). One encrypted
 * `.qsbundle` with every module's data, settings and the vault's secrets,
 * re-encrypted under a passphrase that cannot be recovered. Optional parts
 * start as the data inventory says (artifacts, logs and models unticked).
 */
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

interface LeftOut { pattern: string; reason: string; files: number; bytes: number }
interface Plan { files: number; bytes: number; left_out: LeftOut[]; unclassified: string[]; optional: [string, string, boolean][] }
interface Progress { done_bytes: number; total_bytes: number; current: string }
interface Result { path: string; files: number; bytes: number; secrets: number }

const plan = ref<Plan | null>(null)
const include = ref<Record<string, boolean>>({})
const dest = ref('')
const pass1 = ref('')
const pass2 = ref('')
const busy = ref(false)
const planning = ref(false)
const progress = ref<Progress | null>(null)
const result = ref<Result | null>(null)
const error = ref<string | null>(null)
let unlisten: UnlistenFn | null = null

function inTauri(): boolean {
  return import.meta.client && '__TAURI__' in window
}

function gb(bytes: number): string {
  return bytes >= 1e9 ? `${(bytes / 1e9).toFixed(2)} GB` : `${(bytes / 1e6).toFixed(1)} MB`
}

const passOk = computed(() => pass1.value.length >= 12 && pass1.value === pass2.value)
const canExport = computed(() => !!plan.value && !plan.value.unclassified.length && !!dest.value && passOk.value && !busy.value)

async function loadPlan() {
  if (!inTauri()) return
  planning.value = true
  error.value = null
  try {
    plan.value = await invoke<Plan>('plugin:qs|bundle_plan', { include: include.value })
    for (const [pattern, , on] of plan.value.optional) {
      if (!(pattern in include.value)) include.value[pattern] = on
    }
  } catch (e) {
    error.value = String(e)
  } finally {
    planning.value = false
  }
}

async function toggle(pattern: string) {
  include.value = { ...include.value, [pattern]: !include.value[pattern] }
  await loadPlan()
}

async function chooseFolder() {
  const { open } = await import('@tauri-apps/plugin-dialog')
  const picked = await open({ directory: true, title: 'Where to write the bundle' })
  if (typeof picked === 'string') dest.value = picked
}

async function runExport() {
  busy.value = true
  error.value = null
  result.value = null
  progress.value = null
  try {
    result.value = await invoke<Result>('plugin:qs|bundle_export', { dest_dir: dest.value, passphrase: pass1.value, include: include.value })
    pass1.value = ''
    pass2.value = ''
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}

async function cancel() {
  await invoke('plugin:qs|bundle_export_cancel')
}

// ── Import (PLAN-MIGRATION §2.2) ──
interface PathRow { kind: string; name: string; old: string; new: string | null }
interface Proposal { machine: string; created_at: string; suite_version: string; files: number; bytes: number; secrets: number; rows: PathRow[] }
interface Check { name: string; ok: boolean; detail: string }
interface ImportReport { ok: boolean; from_machine: string; checks: Check[]; manual_steps: string[]; move_code: string; report_file: string }

const bundlePath = ref('')
const importPass = ref('')
const proposal = ref<Proposal | null>(null)
const importBusy = ref(false)
const importReport = ref<ImportReport | null>(null)
const importError = ref<string | null>(null)
const pendingImport = ref<string | null>(null)
const moveCode = ref('')
const moveResult = ref<string | null>(null)

async function chooseBundle() {
  const { open } = await import('@tauri-apps/plugin-dialog')
  const picked = await open({ title: 'A QuantSuite bundle', filters: [{ name: 'QuantSuite bundle', extensions: ['qsbundle'] }] })
  if (typeof picked === 'string') bundlePath.value = picked
}

async function checkBundle() {
  importBusy.value = true
  importError.value = null
  importReport.value = null
  try {
    proposal.value = await invoke<Proposal>('plugin:qs|import_propose', { bundle_path: bundlePath.value, passphrase: importPass.value })
  } catch (e) {
    importError.value = String(e)
  } finally {
    importBusy.value = false
  }
}

async function runImport() {
  if (!proposal.value) return
  importBusy.value = true
  importError.value = null
  try {
    importReport.value = await invoke<ImportReport>('plugin:qs|import_bundle', { bundle_path: bundlePath.value, passphrase: importPass.value, rows: proposal.value.rows })
    pendingImport.value = await invoke<string | null>('plugin:qs|import_status')
    importPass.value = ''
  } catch (e) {
    importError.value = String(e)
  } finally {
    importBusy.value = false
  }
}

async function confirmMove() {
  moveResult.value = null
  try {
    const r = await invoke<{ bots_stopped: number; systems_disarmed: number }>('plugin:algo|trading_confirm_move', { code: moveCode.value })
    moveResult.value = `Moved: ${r.bots_stopped} bot(s) stopped, ${r.systems_disarmed} system(s) disarmed. Trading stays off on this machine; nothing was deleted or sold.`
    moveCode.value = ''
  } catch (e) {
    moveResult.value = String(e)
  }
}

onMounted(async () => {
  if (!inTauri()) return
  unlisten = await listen<Progress>('qs:bundle-progress', (e) => { progress.value = e.payload })
  pendingImport.value = await invoke<string | null>('plugin:qs|import_status').catch(() => null)
  await loadPlan()
})
onUnmounted(() => { unlisten?.() })
</script>

<template>
  <div>
    <div class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">Export everything</p>
        <p class="qsu-hint">
          One encrypted file with every module's data, settings and keys, to move QuantSuite to another machine.
          Databases are copied as a consistent snapshot while the app runs. Live trading must be stopped first; stopping never sells.
        </p>
      </div>
    </div>

    <div v-if="plan" class="qsd-plan">
      <p class="qsu-hint">{{ plan.files.toLocaleString('en-US') }} files, {{ gb(plan.bytes) }} before compression.</p>
      <p v-if="plan.unclassified.length" class="qsu-hint qsd-error">
        {{ plan.unclassified.length }} file(s) have no rule in the data inventory yet (e.g. {{ plan.unclassified.slice(0, 3).join(', ') }}); the export refuses until they do.
      </p>
      <ul class="qsd-optional">
        <li v-for="[pattern, reason] in plan.optional" :key="pattern">
          <label>
            <input type="checkbox" :checked="include[pattern]" :disabled="planning || busy" @change="toggle(pattern)">
            <code>{{ pattern }}</code> — {{ reason }}
          </label>
        </li>
      </ul>
    </div>

    <div class="qsd-form">
      <div class="qsd-line">
        <input v-model="dest" class="qsd-input" placeholder="Destination folder" :disabled="busy">
        <button class="qsd-btn" :disabled="busy" @click="chooseFolder">Choose…</button>
      </div>
      <input v-model="pass1" class="qsd-input" type="password" autocomplete="new-password" placeholder="Passphrase (at least 12 characters)" :disabled="busy">
      <input v-model="pass2" class="qsd-input" type="password" autocomplete="new-password" placeholder="The passphrase again" :disabled="busy">
      <p class="qsu-hint qsd-warn">The passphrase cannot be recovered. Without it the bundle cannot be opened by anyone, you included.</p>
      <div class="qsd-line">
        <button class="qsd-btn qsd-btn--primary" :disabled="!canExport" @click="runExport">{{ busy ? 'Exporting…' : 'Export' }}</button>
        <button v-if="busy" class="qsd-btn" @click="cancel">Cancel</button>
        <span v-if="busy && progress" class="qsu-hint">{{ gb(progress.done_bytes) }} / {{ gb(progress.total_bytes) }} · {{ progress.current }}</span>
      </div>
      <p v-if="result" class="qsu-hint qsd-ok">Written: {{ result.path }} — {{ result.files }} files, {{ gb(result.bytes) }}, {{ result.secrets }} secret(s).</p>
      <p v-if="error" class="qsu-hint qsd-error">{{ error }}</p>
    </div>

    <div class="qsu-row qsd-section">
      <div class="qsu-row-text">
        <p class="qsu-label">Import from a QuantSuite bundle</p>
        <p class="qsu-hint">
          Checks the passphrase, every checksum and the version, rewrites the folders below, puts the keys into this machine's vault and verifies everything.
          The switch happens at the next start; today's data is kept beside it, nothing is deleted. Trading arrives switched off.
        </p>
      </div>
    </div>
    <p v-if="pendingImport" class="qsu-hint qsd-warn">An import waits for the next start ({{ pendingImport }}). Restart QuantSuite to switch.</p>
    <div class="qsd-form">
      <div class="qsd-line">
        <input v-model="bundlePath" class="qsd-input" placeholder="The .qsbundle file" :disabled="importBusy">
        <button class="qsd-btn" :disabled="importBusy" @click="chooseBundle">Choose…</button>
      </div>
      <input v-model="importPass" class="qsd-input" type="password" autocomplete="off" placeholder="The bundle's passphrase" :disabled="importBusy">
      <div class="qsd-line">
        <button class="qsd-btn" :disabled="importBusy || !bundlePath || !importPass" @click="checkBundle">{{ importBusy && !proposal ? 'Checking…' : 'Check the bundle' }}</button>
      </div>
      <template v-if="proposal">
        <p class="qsu-hint">From {{ proposal.machine }}, {{ proposal.created_at.slice(0, 10) }}, QuantSuite {{ proposal.suite_version }}: {{ proposal.files }} files, {{ gb(proposal.bytes) }}, {{ proposal.secrets }} secret(s).</p>
        <table class="qsd-paths">
          <thead><tr><th>Folder</th><th>On the old machine</th><th>Here</th></tr></thead>
          <tbody>
            <tr v-for="row in proposal.rows" :key="row.old">
              <td>{{ row.name }}</td>
              <td class="qsd-mono">{{ row.old }}</td>
              <td><input v-model="row.new" class="qsd-input" :placeholder="row.kind === 'workspace' ? 'Leave empty to link later' : ''" :disabled="row.kind === 'data-root' || importBusy"></td>
            </tr>
          </tbody>
        </table>
        <div class="qsd-line">
          <button class="qsd-btn qsd-btn--primary" :disabled="importBusy || !importPass" @click="runImport">{{ importBusy ? 'Importing…' : 'Import' }}</button>
        </div>
      </template>
      <template v-if="importReport">
        <p class="qsu-hint" :class="importReport.ok ? 'qsd-ok' : 'qsd-error'">
          {{ importReport.ok ? 'Everything checked out. Restart QuantSuite to switch.' : 'Something did not check out; nothing will switch.' }} Report: {{ importReport.report_file }}
        </p>
        <ul class="qsd-optional">
          <li v-for="(c, i) in importReport.checks" :key="i" :class="c.ok ? '' : 'qsd-error'">{{ c.ok ? '✓' : '✗' }} {{ c.detail }}</li>
        </ul>
        <p class="qsu-hint">Move code for the old machine: <strong class="qsd-mono">{{ importReport.move_code }}</strong></p>
        <ul class="qsd-optional">
          <li v-for="(step, i) in importReport.manual_steps" :key="`s${i}`">• {{ step }}</li>
        </ul>
      </template>
      <p v-if="importError" class="qsu-hint qsd-error">{{ importError }}</p>
    </div>

    <div class="qsu-row qsd-section">
      <div class="qsu-row-text">
        <p class="qsu-label">This installation moved to another machine</p>
        <p class="qsu-hint">Enter the move code the new machine's import showed. Trading stops here for good (Stop all trading); nothing is deleted or sold.</p>
      </div>
    </div>
    <div class="qsd-form">
      <div class="qsd-line">
        <input v-model="moveCode" class="qsd-input qsd-mono" placeholder="e.g. K7Q2-M9XD">
        <button class="qsd-btn" :disabled="moveCode.replace(/[^a-z0-9]/gi, '').length !== 8" @click="confirmMove">Confirm the move</button>
      </div>
      <p v-if="moveResult" class="qsu-hint">{{ moveResult }}</p>
    </div>
  </div>
</template>

<style scoped>
.qsd-plan { margin: 8px 0 12px; }
.qsd-optional { list-style: none; padding: 0; margin: 6px 0 0; display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--qss-text-secondary, inherit); }
.qsd-optional code { font-size: 11px; }
.qsd-form { display: flex; flex-direction: column; gap: 8px; max-width: 560px; }
.qsd-line { display: flex; gap: 8px; align-items: center; }
.qsd-input { flex: 1; padding: 6px 8px; border-radius: 6px; border: 1px solid var(--qss-border, #444); background: transparent; color: inherit; font: inherit; font-size: 13px; }
.qsd-btn { padding: 6px 12px; border-radius: 6px; border: 1px solid var(--qss-border, #444); background: transparent; color: inherit; cursor: pointer; font-size: 13px; }
.qsd-btn:disabled { opacity: 0.5; cursor: default; }
.qsd-btn--primary { border-color: var(--qss-accent, #6b8afd); }
.qsd-warn { color: var(--qss-warning, #d9a441); }
.qsd-error { color: var(--qss-error, #e5534b); }
.qsd-ok { color: var(--qss-success, #3fb950); }
.qsd-section { margin-top: 18px; }
.qsd-mono { font-family: var(--qss-font-mono, monospace); font-size: 12px; }
.qsd-paths { width: 100%; border-collapse: collapse; font-size: 12px; }
.qsd-paths th { text-align: left; color: var(--qss-text-muted, inherit); font-weight: 500; padding: 4px; }
.qsd-paths td { padding: 4px; vertical-align: middle; }
</style>
