<script setup lang="ts">
/**
 * The webhook listener (round 4, card 3): off by default, on one address the
 * user chooses. The backend checks the address; this only shows and asks.
 */
import { computed, onMounted, ref } from 'vue'
import { pilot } from '#pilot/utils/invoke'

interface Status {
  settings: { enabled: boolean; address: string; port: number; lanOk: boolean }
  defaultPort: number
  running: boolean
  base: string | null
  error: string | null
  addresses: { name: string; address: string; kind: string }[]
  tailnet: { installed: boolean; running: boolean }
}

const props = defineProps<{ scheduleId: string }>()
const status = ref<Status | null>(null)
const address = ref('')
const port = ref(47111)
const lanOk = ref(false)
const notice = ref('')
const busy = ref(false)
const secret = ref('')

const kindLabel: Record<string, string> = { tailnet: 'Tailscale', lan: 'Local network', other: 'Public', saved: 'Saved', loopback: 'This PC only' }
const choices = computed(() => {
  const rank: Record<string, number> = { tailnet: 0, lan: 1, other: 2 }
  const list = [{ name: 'this PC', address: '127.0.0.1', kind: 'loopback' }, ...[...(status.value?.addresses ?? [])].filter((a) => a.kind !== 'other').sort((a, b) => (rank[a.kind] ?? 3) - (rank[b.kind] ?? 3))]
  const saved = status.value?.settings.address
  if (saved && !list.some((a) => a.address === saved)) list.unshift({ name: 'saved', address: saved, kind: 'saved' })
  return list
})
const kindOf = computed(() => choices.value.find((a) => a.address === address.value)?.kind ?? '')
const url = computed(() => (status.value?.base && props.scheduleId ? `${status.value.base}/hook/${props.scheduleId}` : ''))

function adopt(next: Status | null) {
  if (!next) return
  const first = !status.value
  status.value = next
  if (first || next.running) {
    address.value = next.settings.address || '127.0.0.1'
    port.value = next.settings.port || next.defaultPort
    lanOk.value = next.settings.lanOk
  }
}

async function run(enabled: boolean) {
  busy.value = true
  notice.value = ''
  try {
    adopt(await pilot<Status>('plugin:pilot|pilot_triggers_listen', { settings: { enabled, address: address.value, port: Number(port.value), lanOk: lanOk.value } }))
  } catch (e) {
    notice.value = String(e)
  } finally {
    busy.value = false
  }
}

async function newSecret() {
  busy.value = true
  notice.value = ''
  try {
    secret.value = (await pilot<string>('plugin:pilot|pilot_trigger_new_secret', { id: props.scheduleId })) ?? ''
  } catch (e) {
    notice.value = String(e)
  } finally {
    busy.value = false
  }
}

onMounted(async () => adopt(await pilot<Status>('plugin:pilot|pilot_triggers_status')))
</script>

<template>
  <div v-if="status" class="qp-hook">
    <div class="qp-hook-row">
      <select v-model="address" class="qp-input qp-hook-grow" aria-label="Listen on" :disabled="status.running">
        <option v-for="a in choices" :key="a.address" :value="a.address">{{ a.address }} · {{ kindLabel[a.kind] ?? a.kind }}</option>
      </select>
      <input v-model.number="port" class="qp-input qp-hook-port" type="number" min="1024" max="65535" aria-label="Port" :disabled="status.running" />
      <button type="button" class="qp-btn" :disabled="busy" @click="run(!status.running)">{{ status.running ? 'Stop listening' : 'Start listening' }}</button>
    </div>
    <label v-if="kindOf === 'lan'" class="qp-check">
      <input v-model="lanOk" type="checkbox" :disabled="status.running" />
      <span>I understand this is plain HTTP on the local network.</span>
    </label>
    <p v-if="status.error" class="qp-hook-bad">{{ status.error }}</p>

    <template v-if="scheduleId">
      <div v-if="url" class="qp-hook-row">
        <code class="qp-hook-url">{{ url }}</code>
      </div>
      <div class="qp-hook-row">
        <button type="button" class="qp-btn" :disabled="busy" @click="newSecret">New secret</button>
        <span class="qp-hook-note">Sign the body with HMAC-SHA256 (GitHub's <code>X-Hub-Signature-256</code> header works) or send it as a bearer token.</span>
      </div>
      <p v-if="secret" class="qp-hook-secret">Copy it now, it is not shown again: <code>{{ secret }}</code></p>
    </template>
    <p v-else class="qp-hook-note">Save the schedule first to get its address and secret.</p>
    <p class="qp-hook-note">It only listens on this address. Reaching it from the internet is your own choice, for example with Tailscale Funnel; QuantSuite never sets that up.</p>
    <p v-if="notice" class="qp-hook-bad">{{ notice }}</p>
  </div>
</template>

<style scoped>
.qp-hook { display: flex; flex-direction: column; gap: 8px; }
.qp-hook-row { display: flex; align-items: center; gap: 8px; min-width: 0; }
.qp-hook-grow { flex: 1; min-width: 0; }
.qp-hook-port { flex: none; width: 90px; }
.qp-hook-url, .qp-hook-secret code, .qp-hook-note code { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 12px; overflow-wrap: anywhere; }
.qp-hook-note { margin: 0; font-size: 11.5px; line-height: 1.45; color: var(--qss-text-muted, #6e6e7a); }
.qp-hook-secret { margin: 0; font-size: 12px; overflow-wrap: anywhere; }
.qp-hook-bad { margin: 0; font-size: 12px; color: var(--qss-error, #f87171); }
.qp-input { min-width: 0; background: var(--qss-bg, #18181e); border: 1px solid var(--qss-border, #47474f); border-radius: 6px; color: var(--qss-text, #d4d4d8); font: inherit; font-size: 12.5px; padding: 6px 9px; outline: none; }
.qp-input:focus { border-color: var(--qss-accent, #a0a0a8); }
.qp-check { display: flex; align-items: flex-start; gap: 7px; font-size: 12px; line-height: 1.4; cursor: pointer; }
</style>
