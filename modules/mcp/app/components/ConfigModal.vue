<script setup lang="ts">
// PLAN-QUANTMCP-CONNECT §3.3: the same Connect as Settings, for one
// registered MCP entry — no tool picker, the standard snippet underneath.
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

const props = defineProps<{
  modelValue: boolean
  mcpId?: string | null
  mcpName?: string
}>()

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
}>()

const snippet = ref<ConnectSnippet | null>(null)
const connecting = ref(false)
const report = ref<ConnectResult[] | null>(null)
const connectError = ref('')
const copied = ref(false)

const done = computed(() => (report.value ?? []).filter(r => r.outcome === 'written' || r.outcome === 'ran'))
const failed = computed(() => (report.value ?? []).filter(r => r.outcome === 'failed'))
const manual = computed(() => (report.value ?? []).filter(r => r.outcome === 'manual'))
const skipped = computed(() => (report.value ?? []).filter(r => r.outcome === 'skipped'))

watch(() => props.modelValue, async (open) => {
  if (!open) return
  report.value = null
  connectError.value = ''
  snippet.value = null
  try {
    if (window.__TAURI_INTERNALS__ && props.mcpId) {
      const { invoke } = await import('@tauri-apps/api/core')
      snippet.value = await invoke<ConnectSnippet>('plugin:mcp|get_connect_snippet', { id: props.mcpId })
    }
  } catch (e) {
    console.error('Failed to load the snippet:', e)
  }
})

async function connect() {
  if (connecting.value || !props.mcpId) return
  connecting.value = true
  connectError.value = ''
  try {
    if (window.__TAURI_INTERNALS__) {
      const { invoke } = await import('@tauri-apps/api/core')
      report.value = await invoke<ConnectResult[]>('plugin:mcp|connect_mcp_entry', { id: props.mcpId })
    }
  } catch (e) {
    connectError.value = String(e)
  } finally {
    connecting.value = false
  }
}

async function copySnippet() {
  if (!snippet.value) return
  try {
    await navigator.clipboard.writeText(snippet.value.snippet)
    copied.value = true
    setTimeout(() => { copied.value = false }, 2000)
  } catch (e) {
    console.error('Failed to copy:', e)
  }
}

function close() {
  emit('update:modelValue', false)
}
</script>

<template>
  <Teleport to="body">
    <!-- data-module: teleported out of the module root, the overlay must
         carry the scope itself or it loses the theme-bridge palette. -->
    <div v-if="modelValue" class="modal-overlay" data-module="mcp" @click.self="close">
      <div class="modal-card">
        <h2 class="modal-title">Connect: {{ mcpName }}</h2>

        <div class="connect-section">
          <div class="connect-head">
            <div class="connect-info">
              <div class="connect-label">Add to the AI tools on this machine</div>
              <div class="connect-description">
                Every installed client gets this MCP; the rest is listed as not found.
              </div>
            </div>
            <button class="btn-connect" :disabled="connecting || !snippet" @click="connect">
              <span v-if="connecting" class="spinner" />
              <template v-else>&#9654;</template>
              {{ connecting ? 'Connecting...' : 'Connect' }}
            </button>
          </div>

          <div v-if="connectError" class="result-box error">{{ connectError }}</div>

          <div v-if="report" class="report">
            <div v-for="r in done" :key="r.id" class="report-row ok">
              <span class="report-mark">&#10003;</span>
              <span class="report-name">{{ r.name }}</span>
              <span class="report-detail" :title="r.detail">{{ r.detail }}</span>
              <span v-if="r.restart" class="report-restart">restart</span>
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
              Not found: {{ skipped.map(s => s.name).join(', ') }}
            </div>
          </div>

          <div class="snippet-block">
            <div class="snippet-head">
              <span class="connect-label">Any other MCP client</span>
              <button class="btn-copy" :disabled="!snippet" @click="copySnippet">
                {{ copied ? 'Copied!' : 'Copy' }}
              </button>
            </div>
            <pre class="config-block">{{ snippet?.snippet ?? '' }}</pre>
          </div>
        </div>

        <div class="modal-actions">
          <button class="btn-cancel" @click="close">Close</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.modal-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.6);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}

.modal-card {
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 28px;
  width: 100%;
  max-width: 560px;
  margin: 16px;
}

.modal-title { font-size: 18px; font-weight: 600; margin-bottom: 16px; }

.connect-section {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 18px;
}

.connect-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.connect-info { flex: 1; min-width: 0; }

.connect-label { font-size: 14px; font-weight: 500; }

.connect-description {
  font-size: 12px;
  color: var(--text-secondary);
  margin-top: 2px;
}

.btn-connect {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 16px;
  background: var(--bg-hover);
  border: 1px solid var(--success, #2ed573);
  border-radius: var(--radius);
  color: var(--success, #2ed573);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all var(--transition);
  flex-shrink: 0;
}

.btn-connect:hover:not(:disabled) {
  background: var(--success, #2ed573);
  color: var(--bg-primary);
}

.btn-connect:disabled { opacity: 0.6; cursor: not-allowed; }

.result-box {
  margin-top: 10px;
  padding: 8px 12px;
  border-radius: var(--radius);
  font-size: 12px;
  white-space: pre-line;
}

.result-box.error {
  background: rgba(255, 71, 87, 0.1);
  border: 1px solid rgba(255, 71, 87, 0.3);
  color: var(--error, #ff4757);
}

.report {
  margin-top: 12px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.report-row {
  display: flex;
  align-items: baseline;
  gap: 8px;
  font-size: 13px;
  min-width: 0;
}

.report-mark { width: 14px; flex-shrink: 0; text-align: center; }
.report-row.ok .report-mark { color: var(--success, #2ed573); }
.report-row.bad .report-mark { color: var(--error, #ff4757); }
.report-row.note .report-mark { color: var(--text-muted); }

.report-name {
  width: 130px;
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

.snippet-block {
  margin-top: 16px;
  padding-top: 12px;
  border-top: 1px solid var(--border);
}

.snippet-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 8px;
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
  max-height: 200px;
  overflow-y: auto;
}

.btn-copy {
  padding: 4px 10px;
  background: var(--bg-hover);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-secondary);
  font-size: 11px;
  cursor: pointer;
  transition: all var(--transition);
}

.btn-copy:hover:not(:disabled) { color: var(--text-primary); }
.btn-copy:disabled { opacity: 0.6; cursor: not-allowed; }

.spinner {
  display: inline-block;
  width: 10px;
  height: 10px;
  border: 2px solid var(--text-muted);
  border-top-color: var(--success, #2ed573);
  border-radius: 50%;
  animation: spin 0.6s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.modal-actions { display: flex; justify-content: flex-end; margin-top: 16px; }

.btn-cancel {
  padding: 8px 16px;
  background: transparent;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-secondary);
  font-size: 13px;
  cursor: pointer;
  transition: all var(--transition);
}

.btn-cancel:hover { background: var(--bg-hover); color: var(--text-primary); }
</style>
