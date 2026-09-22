<script setup lang="ts">
import type { Ref } from 'vue'
import AgentRecipients from '../../components/AgentRecipients.vue'
import type { AgentRecipient } from '../../types/agentRecipients'

definePageMeta({ layout: 'mcp' })

// MCP activation uses an explicit selection from the shared agent scanner.
// The manual URL/snippet remains available for every other MCP client.
interface ConnectResult {
  id: string
  name: string
  outcome: 'written' | 'ran' | 'skipped' | 'failed' | 'manual'
  detail: string
  restart: boolean
}

interface ConnectSnippet {
  name: string
  url: string | null
  snippet: string
}

// V3: no per-module theme toggle — the suite is dark-monochrome by decision.
const runtimeConfig = useRuntimeConfig()
const appVersion = runtimeConfig.public.appVersion

const snippet = ref<ConnectSnippet | null>(null)
const connecting = ref(false)
const report = ref<ConnectResult[] | null>(null)
const reportAt = ref('')
const connectError = ref('')
const urlCopied = ref(false)
const snippetCopied = ref(false)
const copyError = ref('')
const recipients = ref<AgentRecipient[]>([])
const selectedIds = ref<string[]>([])
const scanning = ref(false)
const scanError = ref('')
let scanToken = 0
const canConnect = computed(() => !!snippet.value && !connecting.value && !scanning.value
  && !scanError.value && selectedIds.value.length > 0)

const serverUrl = computed(() => snippet.value?.url ?? '')
const done = computed(() => (report.value ?? []).filter(r => r.outcome === 'written' || r.outcome === 'ran'))
const failed = computed(() => (report.value ?? []).filter(r => r.outcome === 'failed'))
const manual = computed(() => (report.value ?? []).filter(r => r.outcome === 'manual'))
const skipped = computed(() => (report.value ?? []).filter(r => r.outcome === 'skipped'))

onMounted(() => {
  void loadSnippet()
  void detectAgents()
})
onBeforeUnmount(() => { ++scanToken })

async function loadSnippet() {
  try {
    if (window.__TAURI_INTERNALS__) {
      const { invoke } = await import('@tauri-apps/api/core')
      snippet.value = await invoke<ConnectSnippet>('plugin:mcp|get_connect_snippet', { id: null })
    }
  } catch (e) {
    connectError.value = `Could not load the MCP connection details: ${e}`
  }
}

async function detectAgents() {
  if (!window.__TAURI_INTERNALS__ || connecting.value) return
  const token = ++scanToken
  scanning.value = true
  scanError.value = ''
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    // AgentOS custom Markdown targets are not MCP configuration files.
    const rows = await invoke<AgentRecipient[]>('plugin:mcp|scan_clients', { customRecipients: [] })
    if (token !== scanToken) return
    const clients = rows.filter(row => !row.custom)
    if (clients.some(row => row.mcp_selection_supported !== true || typeof row.manual !== 'boolean')) {
      throw new Error('Restart QuantSuite with the updated backend to connect only selected agents. Manual setup is still available.')
    }
    recipients.value = clients
    selectedIds.value = selectedIds.value.filter(id => clients.some(row => row.id === id && row.installed))
  } catch (e) {
    if (token !== scanToken) return
    recipients.value = []
    selectedIds.value = []
    scanError.value = String(e)
  } finally {
    if (token === scanToken) scanning.value = false
  }
}

async function connect() {
  if (!canConnect.value || !window.__TAURI_INTERNALS__) return
  const clientIds = [...selectedIds.value]
  connecting.value = true
  connectError.value = ''
  report.value = null
  try {
    if (window.__TAURI_INTERNALS__) {
      const { invoke } = await import('@tauri-apps/api/core')
      report.value = await invoke<ConnectResult[]>('plugin:mcp|connect_clients', { instructionsOnly: false, clientIds })
      reportAt.value = new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
    }
  } catch (e) {
    connectError.value = String(e)
  } finally {
    connecting.value = false
  }
  await detectAgents()
}

async function copyText(text: string, flag: Ref<boolean>) {
  copyError.value = ''
  try {
    await navigator.clipboard.writeText(text)
    flag.value = true
    setTimeout(() => { flag.value = false }, 2000)
  } catch (e) {
    copyError.value = `Could not copy. Select and copy the connection details below: ${e}`
  }
}

function copyUrl() {
  if (serverUrl.value) copyText(serverUrl.value, urlCopied)
}

function copySnippet() {
  if (snippet.value) copyText(snippet.value.snippet, snippetCopied)
}

</script>

<template>
  <div class="settings-page">
    <h1 class="page-title">Settings</h1>

    <div class="settings-list">
      <!-- Selective activation plus the universal manual connection path. -->
      <div class="connect-card">
        <div class="connect-head">
          <div class="connect-title">
            <div class="setting-label">Connect QuantMCP</div>
            <div class="setting-description">
              Choose the agents that receive QuantMCP. Manage shared instructions in General → AgentOS.
            </div>
          </div>
          <div class="connect-actions">
            <button class="setting-action" :disabled="!serverUrl" @click="copyUrl">
              {{ urlCopied ? 'Copied!' : 'Copy URL' }}
            </button>
          </div>
        </div>
        <AgentRecipients
          v-model="selectedIds"
          mode="mcp"
          :agents="recipients"
          :scanning="scanning"
          :busy="connecting"
          :error="scanError"
          :can-import="canConnect"
          :import-label="connecting ? 'Connecting…' : 'Connect selected'"
          @refresh="detectAgents"
          @import="connect"
        >
          <template #manual-setup>
            <div class="manual-panel">
              <p>For a custom agent or manual connection, add this server in your client’s MCP settings. Use the configuration format your client accepts.</p>
              <p>Cloud clients need a public HTTPS URL; they cannot reach localhost.</p>
              <div class="manual-actions">
                <button class="setting-action" :disabled="!serverUrl" @click="copyUrl">{{ urlCopied ? 'Copied!' : 'Copy URL' }}</button>
                <button class="setting-action" :disabled="!snippet" @click="copySnippet">{{ snippetCopied ? 'Copied!' : 'Copy config' }}</button>
              </div>
              <code class="connect-url">{{ serverUrl || 'Loading connection details…' }}</code>
              <pre class="config-block">{{ snippet?.snippet ?? '' }}</pre>
              <p v-if="copyError" class="manual-error" role="alert">{{ copyError }}</p>
            </div>
          </template>
        </AgentRecipients>
        <code class="connect-url">{{ serverUrl || '…' }}</code>

        <div v-if="connectError || copyError" class="result-box error" role="alert">{{ connectError || copyError }}</div>

        <div v-if="report" class="report">
          <div class="report-title">Result · {{ reportAt }}</div>
          <p class="setting-description">Connection results, manual setup and failures are listed below. Import instructions separately in General → AgentOS.</p>
          <div v-for="r in done" :key="r.id" class="report-row ok">
            <span class="report-mark">&#10003;</span>
            <span class="report-name">{{ r.name }}</span>
            <span class="report-detail" :title="r.detail">{{ r.detail }}</span>
            <span v-if="r.restart" class="report-restart">restart {{ r.name }}</span>
          </div>
          <div v-for="r in failed" :key="r.id" class="report-row bad">
            <span class="report-mark">&#10007;</span>
            <span class="report-name">{{ r.name }}</span>
            <span class="report-detail" :title="r.detail">{{ r.detail }}</span>
          </div>
          <div v-for="r in manual" :key="r.id" class="report-row note">
            <span class="report-mark">&ndash;</span>
            <span class="report-name">{{ r.name }}</span>
            <span class="report-detail">{{ r.detail }}</span>
          </div>
          <div v-if="skipped.length" class="report-skipped">
            Not found on this machine: {{ skipped.map(s => s.name).join(', ') }}
          </div>
        </div>

        <!-- The universal path: plain MCP for any client the table does not know -->
        <div class="snippet-block">
          <div class="snippet-head">
            <div>
              <div class="setting-label">Manual setup · Any MCP client</div>
              <div class="setting-description">
                Plain MCP over Streamable HTTP — paste it where your client keeps its servers.
                Cloud clients such as ChatGPT need a public HTTPS URL; localhost is out of their reach.
              </div>
            </div>
            <button class="setting-action" :disabled="!snippet" @click="copySnippet">
              {{ snippetCopied ? 'Copied!' : 'Copy' }}
            </button>
          </div>
          <pre class="config-block">{{ snippet?.snippet ?? '' }}</pre>
        </div>
      </div>

      <div class="setting-item">
        <div class="setting-info">
          <div class="setting-label">Version</div>
          <div class="setting-description">
            QuantSuite v{{ appVersion }}
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.settings-page { width: 100%; }

.page-title { font-size: 24px; font-weight: 600; margin-bottom: 24px; }

.settings-list { display: flex; flex-direction: column; gap: 8px; }

.setting-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px 20px;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
}

.setting-label { font-size: 15px; font-weight: 500; }

.setting-description {
  font-size: 13px;
  color: var(--text-secondary);
  margin-top: 2px;
}

.setting-action {
  padding: 8px 16px;
  background: var(--bg-hover);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-primary);
  font-size: 13px;
  cursor: pointer;
  transition: all var(--transition);
  flex-shrink: 0;
}

.setting-action:hover:not(:disabled) { border-color: var(--accent); color: var(--accent); }
.setting-action:disabled { opacity: 0.6; cursor: not-allowed; }

.result-box {
  margin-top: 10px;
  padding: 8px 12px;
  border-radius: var(--radius);
  font-size: 12px;
  white-space: pre-line;
  background: rgba(46, 213, 115, 0.1);
  border: 1px solid rgba(46, 213, 115, 0.3);
  color: var(--success, #2ed573);
}
.result-box.error {
  background: rgba(255, 71, 87, 0.1);
  border: 1px solid rgba(255, 71, 87, 0.3);
  color: var(--error, #ff4757);
}

/* Connect card */
.connect-card {
  padding: 20px;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
}

.connect-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 12px;
}

.connect-title { flex: 1; min-width: 0; }

.connect-actions {
  display: flex;
  gap: 6px;
  flex-shrink: 0;
}

.connect-url {
  display: block;
  padding: 8px 12px;
  background: var(--bg-primary);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  font-family: monospace;
  font-size: 12px;
  color: var(--text-primary);
  user-select: all;
  overflow-wrap: anywhere;
}

.manual-panel { display: flex; flex-direction: column; gap: 12px; }
.manual-panel p { font-size: 12px; line-height: 1.7; color: var(--text-secondary); }
.manual-panel .manual-error { color: var(--error, #ff4757); overflow-wrap: anywhere; }
.manual-actions { display: flex; flex-wrap: wrap; gap: 8px; }
.manual-panel .config-block { max-height: none; flex-shrink: 0; }

/* The report */
.report {
  margin-top: 14px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.report-title {
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--text-muted);
  margin-bottom: 4px;
}

.report-row {
  display: flex;
  align-items: baseline;
  gap: 10px;
  font-size: 13px;
  min-width: 0;
}

.report-mark { width: 14px; flex-shrink: 0; text-align: center; }
.report-row.ok .report-mark { color: var(--success, #2ed573); }
.report-row.bad .report-mark { color: var(--error, #ff4757); }
.report-row.note .report-mark { color: var(--text-muted); }

.report-name {
  width: 150px;
  flex-shrink: 0;
  color: var(--text-primary);
  font-weight: 500;
}

.report-detail {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: monospace;
  font-size: 12px;
  color: var(--text-secondary);
}

.report-row.note .report-detail {
  font-family: inherit;
  font-size: 12px;
  white-space: normal;
}

.report-restart {
  flex-shrink: 0;
  font-size: 11px;
  color: var(--accent);
}

.report-skipped {
  margin-top: 6px;
  font-size: 12px;
  color: var(--text-muted);
}

/* The standard snippet */
.snippet-block {
  margin-top: 18px;
  padding-top: 14px;
  border-top: 1px solid var(--border);
}

.snippet-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 10px;
}

.config-block {
  margin: 0;
  padding: 12px;
  background: var(--bg-primary);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  font-family: monospace;
  font-size: 12px;
  color: var(--text-primary);
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 180px;
  overflow-y: auto;
}

.spinner {
  display: inline-block;
  width: 12px;
  height: 12px;
  border: 2px solid var(--text-muted);
  border-top-color: var(--success, #2ed573);
  border-radius: 50%;
  animation: spin 0.6s linear infinite;
}

@media (max-width: 640px) {
  .connect-card { padding: 16px; }
  .connect-head, .setting-item { flex-wrap: wrap; }
  .report-row { flex-wrap: wrap; }
  .report-name { width: auto; }
  .report-detail { flex-basis: 100%; white-space: normal; overflow-wrap: anywhere; }
}

@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
