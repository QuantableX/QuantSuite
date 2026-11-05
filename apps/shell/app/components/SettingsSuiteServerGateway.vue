<script setup lang="ts">
/**
 * Suite → Server → Gateway, devices and connections (PLAN-SERVER-CLIENT §4,
 * card Server 3/7).
 *
 * On the server: where it listens, the server PIN, pairing a device (code +
 * the four words), the paired devices (rename, role, revoke) and the audit
 * log. On every PC: pairing with a server and the saved connections.
 */
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useServerLink } from '@quantsuite/core'

type DeviceRole = 'full' | 'no_trading' | 'read_only'
interface Iface { name: string; address: string; kind: 'tailnet' | 'lan' | 'loopback' | 'other' }
interface GatewayStatus {
  role: 'local' | 'server' | 'client'
  address: string | null
  every_interface: boolean
  listening: string | null
  error: string | null
  fingerprint: string | null
  pin_set: boolean
  default_port: number
  interfaces: Iface[]
  computer: string
}
interface PendingDevice { id: string; name: string; fingerprint: string; words: string[] }
interface Pairing { code: string | null; secs_left: number; locked_secs: number; pending: PendingDevice[]; qr_svg: string | null }
interface Device {
  id: string; name: string; role: DeviceRole; state: 'pending' | 'paired' | 'revoked'; fingerprint: string
  created_at: string; paired_at: string | null; revoked_at: string | null; last_seen: string | null; connected: boolean
}
interface AuditEntry { id: number; at: string; device: string; command: string; outcome: string; detail: string | null }
interface Connection { id: string; name: string; address: string; fingerprint: string; device_id: string; role: DeviceRole; paired_at: string }
interface PairBegun { server: string; address: string; words: string[]; server_fingerprint: string }
interface ConnTest { server: string; app: string; role: DeviceRole; millis: number; health: { role: string; version: string; uptime_secs: number } }

const ROLES: { id: DeviceRole; label: string; hint: string }[] = [
  { id: 'full', label: 'Full', hint: 'everything; trading and keys ask for the server PIN' },
  { id: 'no_trading', label: 'No trading', hint: 'read and change data, never trade or touch keys' },
  { id: 'read_only', label: 'Read-only', hint: 'look, never change' },
]
const roleLabel = (r: DeviceRole) => ROLES.find((x) => x.id === r)?.label ?? r

const gw = ref<GatewayStatus | null>(null)
const pairing = ref<Pairing | null>(null)
const devices = ref<Device[]>([])
const audit = ref<AuditEntry[]>([])
const connections = ref<Connection[]>([])
const error = ref<string | null>(null)
const busy = ref(false)

// The listen address form.
const pickAddress = ref('')
const port = ref(47100)
const everyInterface = ref(false)

// The PIN form.
const pin1 = ref('')
const pin2 = ref('')
const pinMsg = ref<string | null>(null)

// Pending pairing: the role the server's user grants.
const grant = ref<Record<string, DeviceRole>>({})

// This PC pairing with a server.
const joinAddress = ref('')
const joinCode = ref('')
const begun = ref<PairBegun | null>(null)
const joinMsg = ref<string | null>(null)
const tests = ref<Record<string, string>>({})

const isServer = computed(() => gw.value?.role === 'server')
// A client sees the server's devices (and may revoke one with the PIN).
const isClient = computed(() => gw.value?.role === 'client')
const link = useServerLink()
const activeId = computed(() => link.status.value?.connection?.id ?? null)
const switching = ref(false)

async function useConnection(id: string | null) {
  switching.value = true
  error.value = null
  try {
    // QuantSuite restarts: the role decides at start whether this PC may trade.
    await invoke('plugin:qs|gateway_use_connection', { id })
  } catch (e) {
    error.value = String(e)
    switching.value = false
  }
}
const showAudit = ref(false)

function inTauri(): boolean {
  return import.meta.client && '__TAURI__' in window
}

function when(iso: string | null): string {
  if (!iso) return '—'
  const d = new Date(iso)
  return `${d.toISOString().slice(0, 10)} ${d.toISOString().slice(11, 16)} UTC`
}

function mmss(s: number): string {
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`
}

function spacedCode(c: string): string {
  return `${c.slice(0, 4)} ${c.slice(4)}`
}

async function run<T>(f: () => Promise<T>): Promise<T | undefined> {
  error.value = null
  busy.value = true
  try {
    return await f()
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}

async function loadStatus() {
  gw.value = await invoke<GatewayStatus>('plugin:qs|gateway_status')
  const a = gw.value.address
  if (a) {
    const i = a.lastIndexOf(':')
    pickAddress.value = a.slice(0, i).replace(/^\[|\]$/g, '')
    port.value = Number(a.slice(i + 1)) || gw.value.default_port
  } else if (!pickAddress.value) {
    pickAddress.value = gw.value.interfaces.find((x) => x.kind === 'tailnet' || x.kind === 'lan')?.address ?? ''
    port.value = gw.value.default_port
  }
  everyInterface.value = gw.value.every_interface
}

async function loadServerSide() {
  if (isClient.value) {
    devices.value = (await invoke<Device[]>('plugin:qs|gateway_devices').catch(() => [])).filter((d) => d.state !== 'pending')
    if (showAudit.value) audit.value = await invoke<AuditEntry[]>('plugin:qs|gateway_audit', { limit: 100 }).catch(() => [])
    return
  }
  if (!isServer.value) return
  pairing.value = await invoke<Pairing>('plugin:qs|gateway_pairing_status').catch(() => null)
  for (const p of pairing.value?.pending ?? []) grant.value[p.id] ??= 'full'
  // A device waiting for the words is shown under "Pair a device" only.
  devices.value = (await invoke<Device[]>('plugin:qs|gateway_devices').catch(() => [])).filter((d) => d.state !== 'pending')
  if (showAudit.value) audit.value = await invoke<AuditEntry[]>('plugin:qs|gateway_audit', { limit: 100 }).catch(() => [])
}

async function loadConnections() {
  connections.value = await invoke<Connection[]>('plugin:qs|gateway_connections').catch(() => [])
  // A pairing still in progress (the page was closed while comparing).
  begun.value = await invoke<PairBegun | null>('plugin:qs|gateway_pair_current').catch(() => null)
}

async function load() {
  if (!inTauri()) return
  try {
    await loadStatus()
    await Promise.all([loadServerSide(), loadConnections()])
  } catch (e) {
    error.value = String(e)
  }
}

const listenTarget = computed(() => {
  const host = everyInterface.value ? '0.0.0.0' : pickAddress.value.trim()
  return host ? `${host.includes(':') ? `[${host}]` : host}:${port.value}` : null
})

async function saveListen(off = false) {
  await run(async () => {
    gw.value = await invoke<GatewayStatus>('plugin:qs|gateway_configure', {
      address: off ? null : listenTarget.value,
      everyInterface: everyInterface.value,
    })
  })
  // After an error too: show what the server really does now.
  await loadStatus().catch(() => undefined)
}

async function savePin(remove = false) {
  pinMsg.value = null
  if (!remove && pin1.value !== pin2.value) {
    pinMsg.value = 'The two PINs differ.'
    return
  }
  await run(async () => {
    await invoke('plugin:qs|gateway_set_pin', { pin: remove ? null : pin1.value })
    pin1.value = ''
    pin2.value = ''
    pinMsg.value = remove ? 'PIN removed: trading and keys can no longer be changed from other devices.' : 'PIN saved.'
    await loadStatus()
  })
}

async function startPairing() {
  await run(async () => {
    pairing.value = await invoke<Pairing>('plugin:qs|gateway_pairing_start')
  })
}

async function cancelPairing() {
  await run(async () => {
    await invoke('plugin:qs|gateway_pairing_cancel')
    await loadServerSide()
  })
}

async function confirmDevice(id: string, allow: boolean) {
  await run(async () => {
    await invoke('plugin:qs|gateway_pairing_confirm', { deviceId: id, role: allow ? (grant.value[id] ?? 'full') : null })
    await loadServerSide()
  })
}

const editing = ref<string | null>(null)
const editName = ref('')
function startRename(d: Device) {
  editing.value = d.id
  editName.value = d.name
}
async function saveRename(d: Device) {
  const name = editName.value.trim()
  editing.value = null
  if (!name || name === d.name) return
  await run(async () => {
    await invoke('plugin:qs|gateway_device_rename', { id: d.id, name })
    await loadServerSide()
  })
}

async function setRole(d: Device, role: DeviceRole) {
  await run(async () => {
    await invoke('plugin:qs|gateway_device_set_role', { id: d.id, role })
    await loadServerSide()
  })
}

const confirmRevoke = ref<string | null>(null)
async function revoke(d: Device) {
  if (confirmRevoke.value !== d.id) {
    confirmRevoke.value = d.id
    return
  }
  confirmRevoke.value = null
  await run(async () => {
    await invoke('plugin:qs|gateway_device_revoke', { id: d.id })
    await loadServerSide()
  })
}

async function forget(d: Device) {
  await run(async () => {
    await invoke('plugin:qs|gateway_device_forget', { id: d.id })
    await loadServerSide()
  })
}

async function toggleAudit() {
  showAudit.value = !showAudit.value
  if (showAudit.value) await loadServerSide()
}

async function joinBegin() {
  joinMsg.value = null
  await run(async () => {
    begun.value = await invoke<PairBegun>('plugin:qs|gateway_pair_begin', { address: joinAddress.value, code: joinCode.value })
  })
}

async function joinFinish(accept: boolean) {
  joinMsg.value = accept ? 'Waiting for the server: allow this PC there…' : null
  const r = await run(() => invoke<Connection | null>('plugin:qs|gateway_pair_finish', { accept }))
  begun.value = null
  joinCode.value = ''
  if (r) {
    joinMsg.value = `Paired with ${r.name} as ${roleLabel(r.role)}.`
    await loadConnections()
  } else if (accept) {
    joinMsg.value = null
  } else {
    joinMsg.value = 'Pairing ended: the words did not match. Do not use that connection.'
  }
}

async function testConnection(c: Connection) {
  tests.value = { ...tests.value, [c.id]: 'Connecting…' }
  try {
    const t = await invoke<ConnTest>('plugin:qs|gateway_connection_test', { id: c.id })
    tests.value = { ...tests.value, [c.id]: `Connected in ${t.millis} ms · ${t.server} runs ${t.app} · this PC: ${roleLabel(t.role)}` }
  } catch (e) {
    tests.value = { ...tests.value, [c.id]: String(e) }
  }
}

async function forgetConnection(c: Connection) {
  await run(async () => {
    await invoke('plugin:qs|gateway_connection_forget', { id: c.id })
    await loadConnections()
  })
}

let unlisten: UnlistenFn | null = null
let tick: ReturnType<typeof setInterval> | null = null
onMounted(async () => {
  await load()
  if (inTauri()) {
    unlisten = await listen('server:gateway', () => loadServerSide())
  }
  // The code's countdown.
  tick = setInterval(() => {
    const p = pairing.value
    if (p && p.secs_left > 0) {
      p.secs_left -= 1
      if (p.secs_left === 0) loadServerSide()
    }
    if (p && p.locked_secs > 0) p.locked_secs -= 1
  }, 1000)
})
onUnmounted(() => {
  unlisten?.()
  if (tick) clearInterval(tick)
})
</script>

<template>
  <div>
    <template v-if="gw && isServer">
      <div class="qsu-row qsv-section">
        <div class="qsu-row-text">
          <p class="qsu-label">Where the server listens</p>
          <p class="qsu-hint">
            Your other PCs connect to this address. Choose the tailnet address (Tailscale, 100.x.y.z) to reach the server
            from anywhere, or the LAN address for home only. Nothing is opened to the internet.
          </p>
          <div class="qsv-line">
            <select v-model="pickAddress" class="qsu-select" :disabled="everyInterface || busy">
              <option value="">Choose an address…</option>
              <option v-for="i in gw.interfaces.filter((x) => x.kind !== 'loopback')" :key="i.address" :value="i.address">
                {{ i.address }} — {{ i.kind === 'tailnet' ? 'tailnet' : i.kind === 'lan' ? 'home network' : 'other' }} ({{ i.name }})
              </option>
              <option value="127.0.0.1">127.0.0.1 — this PC only (testing)</option>
            </select>
            <input v-model.number="port" class="qsv-input qsv-port" type="number" min="1024" max="65535" :disabled="busy">
            <button class="qsu-btn qsu-btn--primary" :disabled="busy || !listenTarget" @click="saveListen()">Listen</button>
            <button v-if="gw.listening" class="qsu-btn" :disabled="busy" @click="saveListen(true)">Stop</button>
          </div>
          <label class="qsv-check">
            <input v-model="everyInterface" type="checkbox" :disabled="busy">
            Every network of this PC (0.0.0.0)
          </label>
          <p v-if="everyInterface" class="qsu-hint qsv-warn">
            Anyone who reaches this PC on any network can knock. Paired keys still keep them out, but prefer the tailnet
            or LAN address.
          </p>
          <p class="qsu-hint">
            <template v-if="gw.listening">Listening on <span class="qsv-mono">{{ gw.listening }}</span>.</template>
            <template v-else>Not listening: no other PC can connect.</template>
            <span v-if="gw.fingerprint"> Server key <span class="qsv-mono">{{ gw.fingerprint.slice(0, 19) }}…</span></span>
          </p>
          <p v-if="gw.error" class="qsu-hint qsv-error">{{ gw.error }}</p>
        </div>
      </div>

      <div class="qsu-row qsv-section">
        <div class="qsu-row-text">
          <p class="qsu-label">Server PIN</p>
          <p class="qsu-hint">
            Other devices must enter it before they trade, close positions, start a live bot, change a key or a device's
            role. It stays valid 10 minutes per device; 5 wrong PINs lock that device for 15 minutes.
            {{ gw.pin_set ? 'A PIN is set.' : 'No PIN is set: those actions are impossible from other devices.' }}
          </p>
          <div class="qsv-line">
            <input v-model="pin1" class="qsv-input" type="password" inputmode="numeric" autocomplete="new-password" placeholder="New PIN (6–12 digits)" :disabled="busy">
            <input v-model="pin2" class="qsv-input" type="password" inputmode="numeric" autocomplete="new-password" placeholder="The PIN again" :disabled="busy">
            <button class="qsu-btn" :disabled="busy || pin1.length < 6" @click="savePin()">{{ gw.pin_set ? 'Change' : 'Set' }}</button>
            <button v-if="gw.pin_set" class="qsu-btn" :disabled="busy" @click="savePin(true)">Remove</button>
          </div>
          <p v-if="pinMsg" class="qsu-hint">{{ pinMsg }}</p>
        </div>
      </div>

      <div class="qsu-row qsv-section">
        <div class="qsu-row-text">
          <p class="qsu-label">Pair a device</p>
          <p class="qsu-hint">
            On the other PC: Settings → Server → Connect to a server, enter this server's address and the code. Then both
            screens show four words — allow the device only if they are the same.
          </p>
          <template v-if="pairing">
            <p v-if="pairing.locked_secs > 0" class="qsu-hint qsv-warn">
              Pairing is locked for {{ mmss(pairing.locked_secs) }} after 5 wrong codes.
            </p>
            <div v-else-if="pairing.code" class="qsv-pair">
              <div>
                <p class="qsv-code">{{ spacedCode(pairing.code) }}</p>
                <p class="qsu-hint">Valid {{ mmss(pairing.secs_left) }} · once</p>
                <button class="qsu-btn" :disabled="busy" @click="cancelPairing">Cancel</button>
              </div>
              <!-- eslint-disable-next-line vue/no-v-html -- an SVG generated by the server, no input of others -->
              <div v-if="pairing.qr_svg" class="qsv-qr" v-html="pairing.qr_svg" />
            </div>
            <button v-else class="qsu-btn qsu-btn--primary" :disabled="busy || !gw.listening" @click="startPairing">
              Show a pairing code
            </button>
            <div v-for="p in pairing.pending" :key="p.id" class="qsv-pending">
              <p class="qsu-label">“{{ p.name }}” wants to connect</p>
              <p class="qsv-words">{{ p.words.join(' · ') }}</p>
              <p class="qsu-hint">Does the other PC show exactly these words? Key <span class="qsv-mono">{{ p.fingerprint.slice(0, 19) }}…</span></p>
              <p class="qsu-hint">As {{ ROLES.find((r) => r.id === (grant[p.id] ?? 'full'))?.label }}: {{ ROLES.find((r) => r.id === (grant[p.id] ?? 'full'))?.hint }}.</p>
              <div class="qsv-line">
                <select v-model="grant[p.id]" class="qsu-select" :title="ROLES.find((r) => r.id === grant[p.id])?.hint">
                  <option v-for="r in ROLES" :key="r.id" :value="r.id">{{ r.label }}</option>
                </select>
                <button class="qsu-btn qsu-btn--primary" :disabled="busy" @click="confirmDevice(p.id, true)">Same words — allow</button>
                <button class="qsu-btn" :disabled="busy" @click="confirmDevice(p.id, false)">Different — turn away</button>
              </div>
            </div>
          </template>
        </div>
      </div>

    </template>

    <template v-if="gw && (isServer || (isClient && link.online.value))">
      <div class="qsu-row qsv-section">
        <div class="qsu-row-text">
          <p class="qsu-label">Devices{{ isClient ? ` paired with ${link.status.value?.connection?.name ?? 'the server'}` : '' }}</p>
          <p v-if="!devices.length" class="qsu-hint">No device is paired yet.</p>
          <table v-else class="qsv-table qsv-devices">
            <tbody>
              <tr v-for="d in devices" :key="d.id" :class="{ 'qsv-dim': d.state === 'revoked' }">
                <td>
                  <span class="qsv-dot" :class="{ 'is-on': d.connected }" :title="d.connected ? 'connected now' : 'not connected'" />
                  <input v-if="editing === d.id" v-model="editName" class="qsv-input qsv-small" @keydown.enter="saveRename(d)" @keydown.esc="editing = null" @blur="saveRename(d)">
                  <template v-else>{{ d.name }}</template>
                </td>
                <td>
                  <select v-if="d.state === 'paired'" class="qsu-select qsv-small" :value="d.role" :disabled="busy" @change="setRole(d, ($event.target as HTMLSelectElement).value as DeviceRole)">
                    <option v-for="r in ROLES" :key="r.id" :value="r.id">{{ r.label }}</option>
                  </select>
                  <span v-else>{{ d.state }}</span>
                </td>
                <td class="qsv-muted">{{ d.state === 'revoked' ? `revoked ${when(d.revoked_at)}` : `last seen ${when(d.last_seen)}` }}</td>
                <td class="qsv-actions">
                  <button class="qsu-btn qsv-small" :disabled="busy || editing === d.id" @click="startRename(d)">Rename</button>
                  <button v-if="d.state !== 'revoked'" class="qsu-btn qsv-small" :class="{ 'qsv-danger': confirmRevoke === d.id }" :disabled="busy" @click="revoke(d)">
                    {{ confirmRevoke === d.id ? 'Revoke now' : 'Revoke' }}
                  </button>
                  <button v-else class="qsu-btn qsv-small" :disabled="busy" @click="forget(d)">Remove</button>
                </td>
              </tr>
            </tbody>
          </table>
          <p class="qsu-hint">Revoking drops the device's connections at once; it must pair again to return.</p>
          <button class="qsu-btn qsv-small" @click="toggleAudit">{{ showAudit ? 'Hide the audit log' : 'Show the audit log' }}</button>
          <table v-if="showAudit" class="qsv-table qsv-audit">
            <tbody>
              <tr v-for="a in audit" :key="a.id">
                <td class="qsv-muted">{{ when(a.at) }}</td>
                <td>{{ a.device }}</td>
                <td class="qsv-mono">{{ a.command }}</td>
                <td :class="{ 'qsv-warn': a.outcome !== 'ok' && a.outcome !== 'paired' }">{{ a.outcome }}</td>
                <td class="qsv-muted">{{ a.detail ?? '' }}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </template>

    <div v-if="gw && gw.role !== 'server'" class="qsu-row qsv-section">
      <div class="qsu-row-text">
        <p class="qsu-label">Connect to a server</p>
        <p class="qsu-hint">
          Pair this PC with your QuantSuite server once. On the server: Settings → Server → Pair a device shows the code.
          This PC's key stays in its own vault; the server's key is pinned.
        </p>
        <div v-if="!begun" class="qsv-line">
          <input v-model="joinAddress" class="qsv-input" placeholder="Server address, e.g. 100.101.102.103 or homeserver" :disabled="busy">
          <input v-model="joinCode" class="qsv-input qsv-port" inputmode="numeric" placeholder="Code" :disabled="busy">
          <button class="qsu-btn qsu-btn--primary" :disabled="busy || !joinAddress.trim() || joinCode.replace(/\D/g, '').length !== 8" @click="joinBegin">Pair</button>
        </div>
        <div v-else class="qsv-pending">
          <p class="qsu-label">Compare with the server's screen ({{ begun.server }})</p>
          <p class="qsv-words">{{ begun.words.join(' · ') }}</p>
          <p class="qsu-hint">Allow only if the server shows exactly these four words. Server key <span class="qsv-mono">{{ begun.server_fingerprint.slice(0, 19) }}…</span></p>
          <div class="qsv-line">
            <button class="qsu-btn qsu-btn--primary" :disabled="busy" @click="joinFinish(true)">Same words</button>
            <button class="qsu-btn" :disabled="busy" @click="joinFinish(false)">Different — stop</button>
          </div>
        </div>
        <p v-if="joinMsg" class="qsu-hint">{{ joinMsg }}</p>
        <table v-if="connections.length" class="qsv-table">
          <tbody>
            <tr>
              <td>This PC</td>
              <td class="qsv-muted">its own data</td>
              <td />
              <td class="qsv-actions">
                <span v-if="!activeId" class="qsv-badge">in use</span>
                <button v-else class="qsu-btn qsv-small" :disabled="switching" @click="useConnection(null)">Use</button>
              </td>
              <td />
            </tr>
            <tr v-for="c in connections" :key="c.id">
              <td>{{ c.name }}</td>
              <td class="qsv-mono">{{ c.address }}</td>
              <td>{{ roleLabel(c.role) }}</td>
              <td class="qsv-actions">
                <span v-if="activeId === c.id" class="qsv-badge">in use</span>
                <button v-else class="qsu-btn qsv-small" :disabled="switching" title="QuantSuite restarts and works on this server" @click="useConnection(c.id)">Use</button>
                <button class="qsu-btn qsv-small" @click="testConnection(c)">Test</button>
                <button class="qsu-btn qsv-small" :disabled="busy || activeId === c.id" @click="forgetConnection(c)">Forget</button>
              </td>
              <td class="qsv-muted">{{ tests[c.id] ?? '' }}</td>
            </tr>
          </tbody>
        </table>
        <p v-if="switching" class="qsu-hint">QuantSuite restarts…</p>
      </div>
    </div>
    <p v-if="error" class="qsu-hint qsv-error">{{ error }}</p>
  </div>
</template>

<style scoped>
.qsv-section { margin-top: 18px; }
.qsv-line { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; margin-top: 8px; }
.qsv-input { flex: 1; min-width: 140px; padding: 7px 10px; border-radius: 8px; border: 1px solid var(--qss-border); background: var(--qss-bg-card); color: var(--qss-text); font: inherit; font-size: 12px; }
.qsv-port { flex: 0 0 96px; min-width: 0; }
.qsv-check { display: flex; gap: 6px; align-items: center; font-size: 12px; margin-top: 8px; color: var(--qss-text-secondary); }
.qsv-pair { display: flex; gap: 24px; align-items: center; margin-top: 8px; }
.qsv-code { font-family: var(--qss-font-mono, monospace); font-size: 28px; letter-spacing: 0.08em; margin: 0 0 4px; }
.qsv-qr { width: 132px; height: 132px; background: #fff; border-radius: 8px; padding: 4px; }
.qsv-qr :deep(svg) { width: 100%; height: 100%; }
.qsv-pending { margin-top: 12px; padding: 10px 12px; border: 1px solid var(--qss-border); border-radius: 8px; }
.qsv-words { font-size: 18px; font-weight: 600; margin: 4px 0; }
.qsv-table { border-collapse: collapse; font-size: 12px; margin: 8px 0; width: 100%; }
.qsv-table td { padding: 4px 10px 4px 0; vertical-align: middle; }
.qsv-audit td { padding: 2px 10px 2px 0; }
.qsv-actions { white-space: nowrap; text-align: right; }
.qsv-actions .qsu-btn + .qsu-btn { margin-left: 6px; }
.qsv-small { padding: 4px 10px; min-width: 0; font-size: 11px; }
.qsv-dot { display: inline-block; width: 7px; height: 7px; border-radius: 50%; background: var(--qss-text-muted, #777); margin-right: 6px; }
.qsv-dot.is-on { background: var(--qss-success, #3fb950); }
.qsv-dim { opacity: 0.55; }
.qsv-muted { color: var(--qss-text-muted, inherit); }
.qsv-mono { font-family: var(--qss-font-mono, monospace); font-size: 11px; }
.qsv-warn { color: var(--qss-warning, #d9a441); }
.qsv-error { color: var(--qss-error, #e5534b); }
.qsv-danger { border-color: var(--qss-error, #e5534b); color: var(--qss-error, #e5534b); }
.qsv-badge { display: inline-block; padding: 2px 8px; margin-right: 6px; border-radius: 999px; font-size: 11px; color: var(--qss-accent-ink); background: var(--qss-accent); }
</style>
