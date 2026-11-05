<script setup lang="ts">
/**
 * Settings → Phone access (round 4, card 1): approvals and messages from a
 * phone's browser. Off by default. The backend checks every address and keeps
 * the PIN as a hash in the vault; this page only shows the state and asks.
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { pilot } from '#pilot/utils/invoke'

interface Settings { enabled: boolean; address: string; port: number; lanOk: boolean }
interface Iface { name: string; address: string; kind: string }
interface Device { id: string; name: string; createdAt: number; lastSeen: number }
interface AuditRow { at: number; device: string; action: string; threadId: string | null; ok: boolean; note: string }
interface Pairing { code: string; secondsLeft: number; url: string; qr: string | null }
interface Status {
  settings: Settings
  defaultPort: number
  running: boolean
  url: string | null
  error: string | null
  hasPin: boolean
  addresses: Iface[]
  tailnet: { installed: boolean; running: boolean; state: string | null; ips: string[] }
  pairing: Pairing | null
  devices: Device[]
  audit: AuditRow[]
}

const status = ref<Status | null>(null)
const notice = ref('')
const busy = ref(false)
const pin = ref('')
const address = ref('')
const port = ref(47110)
const lanOk = ref(false)
let timer: ReturnType<typeof setInterval> | null = null

const kindOf = computed(() => status.value?.addresses.find((a) => a.address === address.value)?.kind ?? '')
const kindLabel: Record<string, string> = { tailnet: 'Tailscale', lan: 'Local network', other: 'Public', saved: 'Saved' }
const choices = computed(() => {
  const all = status.value?.addresses ?? []
  const rank: Record<string, number> = { tailnet: 0, lan: 1, other: 2 }
  const sorted = all.filter((a) => a.kind !== 'other').sort((a, b) => (rank[a.kind] ?? 3) - (rank[b.kind] ?? 3))
  // The saved address stays in the list, even when it is not a network's right now.
  const saved = status.value?.settings.address
  if (saved && !all.some((a) => a.address === saved)) sorted.unshift({ name: 'saved', address: saved, kind: 'saved' })
  return sorted
})
const pairingCode = computed(() => {
  const c = status.value?.pairing?.code ?? ''
  return c ? `${c.slice(0, 4)} ${c.slice(4)}` : ''
})

function adopt(next: Status) {
  const first = !status.value
  status.value = next
  if (first || next.running) {
    address.value = next.settings.address || choices.value[0]?.address || ''
    port.value = next.settings.port || next.defaultPort
    lanOk.value = next.settings.lanOk
  }
}

async function run(command: string, args: Record<string, unknown> = {}) {
  busy.value = true
  notice.value = ''
  try {
    const next = await pilot<Status>(command, args)
    if (next) adopt(next)
  } catch (e) {
    notice.value = String(e)
  } finally {
    busy.value = false
  }
}

const load = () => run('plugin:pilot|pilot_phone_status')
const turnOn = () => run('plugin:pilot|pilot_phone_set', { settings: { enabled: true, address: address.value, port: Number(port.value), lanOk: lanOk.value } })
const turnOff = () => run('plugin:pilot|pilot_phone_set', { settings: { enabled: false, address: address.value, port: Number(port.value), lanOk: lanOk.value } })
async function setPin() {
  await run('plugin:pilot|pilot_phone_set_pin', { pin: pin.value })
  if (!notice.value) pin.value = ''
}
const pair = () => run('plugin:pilot|pilot_phone_pair')
const cancelPair = () => run('plugin:pilot|pilot_phone_pair_cancel')
const revoke = (id: string) => run('plugin:pilot|pilot_phone_revoke', { id })

/** The code's countdown and a new device showing up, while the page is open. */
watch(
  () => status.value?.pairing?.code,
  (code) => {
    if (timer) clearInterval(timer)
    timer = null
    if (code) {
      timer = setInterval(async () => {
        const next = await pilot<Status>('plugin:pilot|pilot_phone_status')
        if (next) adopt(next)
      }, 3000)
    }
  },
)

const when = (ms: number) => new Date(ms).toLocaleString('en-US', { dateStyle: 'medium', timeStyle: 'short', hour12: false })
const leftText = computed(() => {
  const s = status.value?.pairing?.secondsLeft ?? 0
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`
})

onMounted(load)
onBeforeUnmount(() => {
  if (timer) clearInterval(timer)
})
</script>

<template>
  <div class="qp-settings">
    <p class="qp-settings-desc">
      Answer approvals and message your agents from your phone's browser. It runs on this PC, with no relay and no account, and it is off until you turn it on.
    </p>

    <div v-if="status" class="qp-gh" :class="status.running ? 'ok' : status.error ? 'bad' : ''">
      <span class="qp-dot" />
      <span v-if="status.running">On at <code>{{ status.url }}</code></span>
      <span v-else-if="status.error">{{ status.error }}</span>
      <span v-else>Off</span>
      <button v-if="status.running" class="qp-btn" :disabled="busy" @click="turnOff">Turn off</button>
    </div>

    <template v-if="status">
      <h3 class="qp-settings-title">1. A PIN</h3>
      <p class="qp-settings-desc">
        A paired phone can read your threads. Sending a message, answering an approval or stopping an agent also needs this PIN, which stays valid for 10 minutes. Only its hash is kept, in the vault.
      </p>
      <div class="qp-row">
        <input v-model="pin" class="qp-input" type="password" inputmode="numeric" autocomplete="off" placeholder="6 to 12 digits" aria-label="PIN" @keydown.enter="setPin">
        <button class="qp-btn" :disabled="busy || !pin" @click="setPin">{{ status.hasPin ? 'Change PIN' : 'Set PIN' }}</button>
        <span v-if="status.hasPin" class="qp-ok">Set</span>
      </div>

      <h3 class="qp-settings-title">2. The address</h3>
      <p v-if="!status.tailnet.running" class="qp-settings-desc">
        {{ status.tailnet.installed ? 'Tailscale is installed but not connected.' : 'Tailscale is not installed.' }}
        It is the recommended way: a private encrypted network that also works when you are out, with nothing opened to the internet. Install it on this PC and your phone from tailscale.com, sign in to the same account on both, and its address appears here.
      </p>
      <div class="qp-row">
        <select v-model="address" class="qp-input qp-grow" aria-label="Address" :disabled="status.running">
          <option v-if="!choices.length" value="">No network address found</option>
          <option v-for="a in choices" :key="a.address" :value="a.address">{{ a.address }} · {{ kindLabel[a.kind] ?? a.kind }} · {{ a.name }}</option>
        </select>
        <input v-model.number="port" class="qp-input qp-port" type="number" min="1024" max="65535" aria-label="Port" :disabled="status.running">
      </div>
      <label v-if="kindOf === 'lan'" class="qp-check">
        <input v-model="lanOk" type="checkbox" :disabled="status.running">
        <span>I understand this is plain HTTP on the local network: anyone on the same Wi-Fi could read my PIN and what the phone shows. It also only works at home.</span>
      </label>
      <p v-if="kindOf === 'other'" class="qp-settings-desc">This looks like a public address, and phone access refuses to listen on one.</p>
      <div v-if="!status.running" class="qp-row">
        <button class="qp-btn qp-primary" :disabled="busy || !address || !status.hasPin" @click="turnOn">Turn on</button>
        <span v-if="!status.hasPin" class="qp-settings-notice">Set a PIN first.</span>
      </div>

      <template v-if="status.running">
        <h3 class="qp-settings-title">3. Pair a phone</h3>
        <div v-if="status.pairing" class="qp-pair">
          <!-- eslint-disable-next-line vue/no-v-html -- the QR SVG comes from our own backend and holds no outside text -->
          <div v-if="status.pairing.qr" class="qp-qr" aria-label="QR code with the pairing link" v-html="status.pairing.qr" />
          <div class="qp-pair-text">
            <p class="qp-settings-desc">Scan this with your phone, or open the address and type the code.</p>
            <code class="qp-url">{{ status.url }}</code>
            <div class="qp-code">{{ pairingCode }}</div>
            <p class="qp-settings-desc">Valid for {{ leftText }}, and works once.</p>
            <button class="qp-btn" :disabled="busy" @click="cancelPair">Cancel</button>
          </div>
        </div>
        <button v-else class="qp-btn qp-self" :disabled="busy" @click="pair">Pair a phone</button>
        <p class="qp-settings-desc">The page only alerts you while it is open on the phone. A closed page cannot show that an agent is waiting.</p>

        <h3 class="qp-settings-title">Paired phones</h3>
        <p v-if="!status.devices.length" class="qp-settings-desc">None yet.</p>
        <div v-for="d in status.devices" :key="d.id" class="qp-gh">
          <span class="qp-dot qp-dot-on" />
          <span>{{ d.name }} <em>· paired {{ when(d.createdAt) }} · last seen {{ when(d.lastSeen) }}</em></span>
          <button class="qp-btn" :disabled="busy" @click="revoke(d.id)">Revoke</button>
        </div>
      </template>

      <template v-if="status.audit.length">
        <h3 class="qp-settings-title">What phones did</h3>
        <div class="qp-audit">
          <div v-for="(a, i) in status.audit" :key="i" class="qp-audit-row">
            <span class="qp-num">{{ when(a.at) }}</span>
            <span>{{ a.device }}</span>
            <span :class="{ 'qp-bad': !a.ok }">{{ a.action }}{{ a.ok ? '' : ' refused' }}</span>
          </div>
        </div>
      </template>
    </template>

    <p v-if="notice" class="qp-settings-notice qp-bad">{{ notice }}</p>
  </div>
</template>

<style scoped>
.qp-settings { display: flex; flex-direction: column; gap: 10px; font-size: 12.5px; color: var(--qss-text, #e8e8ec); }
.qp-settings-title { margin: 6px 0 0; font-size: 13px; font-weight: 600; }
.qp-settings-desc { margin: 0; color: var(--qss-text-secondary, #9a9aa5); line-height: 1.45; }
.qp-settings-notice { color: var(--qss-text-secondary, #9a9aa5); font-size: 12px; }
.qp-gh { display: flex; align-items: center; gap: 8px; padding: 8px 10px; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; }
.qp-gh > span:nth-child(2) { flex: 1; min-width: 0; overflow-wrap: anywhere; }
.qp-gh em { font-style: normal; color: var(--qss-text-muted, #6e6e7a); }
.qp-gh code, .qp-url { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 12px; }
.qp-dot { flex: none; width: 7px; height: 7px; border-radius: 50%; background: var(--qss-border, #47474f); }
.qp-gh.ok .qp-dot, .qp-dot-on { background: var(--qss-success, #34d399); }
.qp-gh.bad .qp-dot { background: var(--qss-error, #f87171); }
.qp-row { display: flex; align-items: center; gap: 8px; }
.qp-grow { flex: 1; min-width: 0; }
.qp-port { flex: none; width: 90px; }
.qp-input { background: var(--qss-bg, #18181e); border: 1px solid var(--qss-border, #47474f); border-radius: 6px; padding: 6px 8px; color: inherit; font: inherit; outline: none; }
.qp-input:focus { border-color: var(--qss-accent, #a0a0a8); }
.qp-input::placeholder { color: var(--qss-text-muted, #6e6e7a); }
.qp-btn { flex: none; padding: 5px 10px; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; background: none; color: inherit; font: inherit; cursor: pointer; }
.qp-btn:hover:not(:disabled) { background: var(--qss-bg-hover, rgba(255, 255, 255, 0.06)); }
.qp-btn:disabled { opacity: 0.5; cursor: default; }
.qp-primary { background: var(--qss-text, #e8e8ec); border-color: var(--qss-text, #e8e8ec); color: var(--qss-bg, #18181e); font-weight: 600; }
.qp-primary:hover:not(:disabled) { background: var(--qss-text, #e8e8ec); }
.qp-self { align-self: flex-start; }
.qp-ok { color: var(--qss-success, #34d399); }
.qp-bad { color: var(--qss-error, #f87171); }
.qp-check { display: flex; align-items: flex-start; gap: 8px; line-height: 1.4; }
.qp-pair { display: flex; gap: 16px; padding: 12px; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; }
.qp-qr { flex: none; width: 168px; height: 168px; padding: 8px; border-radius: 6px; background: #fff; }
.qp-qr :deep(svg) { display: block; width: 100%; height: 100%; }
.qp-pair-text { display: flex; flex-direction: column; align-items: flex-start; gap: 8px; min-width: 0; }
.qp-url { overflow-wrap: anywhere; }
.qp-code { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 24px; font-weight: 600; letter-spacing: 0.06em; font-variant-numeric: tabular-nums; }
.qp-audit { display: flex; flex-direction: column; gap: 2px; max-height: 180px; overflow: auto; }
.qp-audit-row { display: grid; grid-template-columns: 150px 1fr 1fr; gap: 8px; padding: 2px 0; color: var(--qss-text-secondary, #9a9aa5); }
.qp-num { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11.5px; font-variant-numeric: tabular-nums; }
</style>
