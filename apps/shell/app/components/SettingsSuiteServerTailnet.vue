<script setup lang="ts">
/**
 * Suite → Server → Use it from anywhere (PLAN-SERVER-CLIENT §4.2, card Server
 * 6/7): Tailscale on this PC and the steps for the server and each client.
 * Nothing is opened to the internet: the PCs reach each other inside your
 * private tailnet, and QuantSuite's own pairing still decides who gets in.
 */
import { open } from '@tauri-apps/plugin-shell'

interface Peer { name: string; dns_name: string | null; ips: string[]; online: boolean; os: string | null }
interface Tailnet {
  installed: boolean
  state: string | null
  running: boolean
  name: string | null
  dns_name: string | null
  ips: string[]
  magic_dns: boolean
  peers: Peer[]
  error: string | null
}

const props = defineProps<{ tailnet: Tailnet; role: 'local' | 'server' | 'client'; listening: string | null }>()
const emit = defineEmits<{ (e: 'pick', address: string): void }>()

const ipv4 = computed(() => props.tailnet.ips.find((ip) => ip.includes('.')) ?? props.tailnet.ips[0] ?? null)
const reachAs = computed(() => props.tailnet.dns_name ?? ipv4.value)
const onTailnet = computed(() => !!ipv4.value && !!props.listening?.startsWith(`${ipv4.value}:`))
const isServer = computed(() => props.role === 'server')

function download() {
  void open('https://tailscale.com/download').catch(() => undefined)
}
</script>

<template>
  <div class="qsu-row qsv-section">
    <div class="qsu-row-text">
      <p class="qsu-label">Use it from anywhere</p>
      <p class="qsu-hint">
        Tailscale connects your PCs privately over the internet — end-to-end encrypted, no open port on your router, free
        for personal use. QuantSuite's pairing still decides who gets in.
      </p>

      <template v-if="!tailnet.installed">
        <p class="qsu-hint qsv-warn">Tailscale is not installed on this PC.</p>
        <div class="qsv-line"><button class="qsu-btn" @click="download">Download Tailscale</button></div>
      </template>
      <p v-else-if="tailnet.error" class="qsu-hint qsv-warn">Tailscale: {{ tailnet.error }}</p>
      <p v-else-if="!tailnet.running" class="qsu-hint qsv-warn">
        Tailscale is installed but not connected ({{ tailnet.state ?? 'unknown' }}). Open Tailscale and sign in — with the
        same account on every PC.
      </p>
      <template v-else>
        <p class="qsu-hint">
          This PC in your tailnet: <strong>{{ tailnet.name }}</strong>
          <span class="qsv-mono"> · {{ tailnet.ips.join(' · ') }}</span>
          <template v-if="tailnet.dns_name"> · <span class="qsv-mono">{{ tailnet.dns_name }}</span></template>
        </p>
        <template v-if="isServer">
          <p v-if="onTailnet" class="qsu-hint">
            The server listens on the tailnet. Your other PCs connect with <span class="qsv-mono">{{ reachAs }}</span>.
          </p>
          <div v-else-if="ipv4" class="qsv-line">
            <button class="qsu-btn qsu-btn--primary" @click="emit('pick', ipv4)">Listen on the tailnet address</button>
          </div>
        </template>
        <template v-else-if="tailnet.peers.length">
          <p class="qsu-hint qsv-sub">Your devices — pick the server to fill in its address:</p>
          <table class="qsv-table">
            <tbody>
              <tr v-for="p in tailnet.peers" :key="p.name + p.ips.join()">
                <td><span class="qsv-dot" :class="{ 'is-on': p.online }" />{{ p.name }}</td>
                <td class="qsv-mono">{{ p.dns_name ?? p.ips[0] }}</td>
                <td class="qsv-muted">{{ p.online ? 'online' : 'offline' }}</td>
                <td class="qsv-actions">
                  <button class="qsu-btn qsv-small" @click="emit('pick', p.dns_name ?? p.ips.find((ip) => ip.includes('.')) ?? p.ips[0] ?? '')">Use</button>
                </td>
              </tr>
            </tbody>
          </table>
        </template>
      </template>

      <ol class="qsv-steps">
        <template v-if="isServer">
          <li>Install Tailscale here and sign in.</li>
          <li>"Listen on the tailnet address" above (100.x.y.z). The server is then reachable only inside your tailnet.</li>
          <li>Optional: turn on MagicDNS in Tailscale's admin page, so PCs can use this server's name instead of its address.</li>
          <li>On each other PC: install Tailscale, sign in with the same account, then pair it here ("Pair a device").</li>
        </template>
        <template v-else>
          <li>Install Tailscale on this PC and sign in with the same account as the server.</li>
          <li>Check that the server shows "online" in the list above (or in the Tailscale app).</li>
          <li>Below, connect with the server's tailnet name or 100.x address and the pairing code from the server.</li>
        </template>
        <li>
          Prefer no third party at all? Headscale is a self-hosted coordination server for the same Tailscale apps (see
          docs/PLAN-SERVER-CLIENT.md §4.2).
        </li>
      </ol>
    </div>
  </div>
</template>

<style scoped>
.qsv-section { margin-top: 18px; }
.qsv-line { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; margin-top: 8px; }
.qsv-table { border-collapse: collapse; font-size: 12px; margin: 6px 0; width: 100%; }
.qsv-table td { padding: 3px 10px 3px 0; vertical-align: middle; }
.qsv-actions { white-space: nowrap; text-align: right; }
.qsv-small { padding: 4px 10px; min-width: 0; font-size: 11px; }
.qsv-dot { display: inline-block; width: 7px; height: 7px; border-radius: 50%; background: var(--qss-text-muted, #777); margin-right: 6px; }
.qsv-dot.is-on { background: var(--qss-success, #3fb950); }
.qsv-muted { color: var(--qss-text-muted, inherit); }
.qsv-mono { font-family: var(--qss-font-mono, monospace); font-size: 11px; }
.qsv-warn { color: var(--qss-warning, #d9a441); }
.qsv-sub { margin-top: 8px; }
.qsv-steps { margin: 8px 0 0; padding-left: 18px; list-style: decimal; font-size: 12px; display: flex; flex-direction: column; gap: 3px; color: var(--qss-text-secondary, inherit); }
</style>
