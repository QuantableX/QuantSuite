<script setup lang="ts">
import { qs, useShortcuts } from '@quantsuite/core'
import AgentRecipients from './AgentRecipients.vue'
import type { AgentRecipient, CustomRecipient } from '../types/agentRecipients'

// AGENT.md editor for one target: the global file (~/.quantmcp/AGENT.md) when
// `workspacePath` is null, otherwise <workspace>/AGENT.md. Concept.md is gone
// (2026-08-31) — AgentOS is Agent.md only.
const props = defineProps<{
  workspacePath: string | null
}>()

const content = ref('')
const loading = ref(false)
const saving = ref(false)
const dirty = ref(false)
const fileExists = ref(false)
const filePath = ref('')
const importing = ref(false)
const importError = ref('')
const importReport = ref<Array<{ id: string; name: string; outcome: string; detail: string }> | null>(null)
const copied = ref(false)
const recipients = ref<AgentRecipient[]>([])
const selectedIds = ref<string[]>([])
const scanning = ref(false)
const scanError = ref('')
const canImport = computed(() => !loading.value && !saving.value && !importing.value
  && !scanning.value && !scanError.value && selectedIds.value.length > 0 && !!content.value.trim())

const isGlobal = computed(() => props.workspacePath === null)
const hint = computed(() =>
  isGlobal.value
    ? 'Your shared instructions for every workspace. Import updates into your chosen agents, then start a new session.'
    : 'Workspace-specific agent instructions. Stored at <workspace>/AGENT.md',
)

// Target switches can overlap: a token marks the newest request, older ones
// drop their result instead of overwriting the current selection.
let loadToken = 0
let scanToken = 0

async function saveCustom(recipient: CustomRecipient) {
  const saved = await qs.core.getSetting<CustomRecipient[]>('mcp', 'agentos.custom_recipients') ?? []
  const name = recipient.name.trim()
  const path = recipient.path.trim()
  if (!name || name.length > 80 || /[\u0000-\u001f\u007f]/.test(name)) throw new Error('Enter a name of 1–80 characters.')
  if (!/^(?:[A-Za-z]:[\\/]|\/|\\\\)/.test(path) || !/\.md$/i.test(path)) throw new Error('Choose an absolute path to a Markdown (.md) file.')
  const pathKey = (value: string) => /^[A-Za-z]:|^\\\\/.test(value) ? value.replaceAll('\\', '/').toLowerCase() : value
  if (saved.some(item => item.id !== recipient.id && pathKey(item.path) === pathKey(path))) throw new Error('That instruction file already has a custom agent.')
  if (saved.length >= 100 && !saved.some(item => item.id === recipient.id)) throw new Error('You can save up to 100 custom agents.')
  const next = [...saved.filter(item => item.id !== recipient.id), { ...recipient, name, path }]
  const { invoke } = await import('@tauri-apps/api/core')
  await invoke('plugin:mcp|scan_clients', { customRecipients: next })
  await qs.core.setSetting('mcp', 'agentos.custom_recipients', next)
  await detectAgents()
  if (scanError.value) throw new Error(scanError.value)
}

async function removeCustom(id: string) {
  const saved = await qs.core.getSetting<CustomRecipient[]>('mcp', 'agentos.custom_recipients') ?? []
  await qs.core.setSetting('mcp', 'agentos.custom_recipients', saved.filter(item => item.id !== id))
  await detectAgents()
  if (scanError.value) throw new Error(scanError.value)
}

async function detectAgents() {
  if (!import.meta.client || !window.__TAURI_INTERNALS__ || !isGlobal.value || importing.value) return
  const token = ++scanToken
  const context = loadToken
  scanning.value = true
  scanError.value = ''
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const rows = await invoke<AgentRecipient[]>('plugin:mcp|scan_clients')
    if (token !== scanToken || context !== loadToken) return
    // An older backend ignores clientIds and imports every agent. Fail closed.
    if (rows.some(row => typeof row.instructions_manual !== 'boolean')) {
      throw new Error('Restart QuantSuite with the updated backend to enable selected imports.')
    }
    recipients.value = rows
    selectedIds.value = selectedIds.value.filter(id => rows.some(row => row.id === id && row.installed))
  } catch (e) {
    if (token !== scanToken || context !== loadToken) return
    recipients.value = []
    selectedIds.value = []
    scanError.value = String(e)
  } finally {
    if (token === scanToken && context === loadToken) scanning.value = false
  }
}

async function loadState() {
  if (!import.meta.client || !window.__TAURI_INTERNALS__) return
  const token = ++loadToken
  ++scanToken
  recipients.value = []
  selectedIds.value = []
  scanError.value = ''
  scanning.value = false
  if (isGlobal.value) void detectAgents()
  importReport.value = null
  importError.value = ''
  loading.value = true
  dirty.value = false
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const scope = isGlobal.value ? 'global' : 'project'
    const projectPath = props.workspacePath || null

    const paths = await invoke<{
      global: string
      project: string | null
      global_exists: boolean
      project_exists: boolean
    }>('plugin:mcp|get_agent_md_paths', { projectPath })
    if (token !== loadToken) return

    const text = await invoke<string>('plugin:mcp|read_agent_md', { scope, projectPath })
    if (token !== loadToken) return

    if (scope === 'global') {
      filePath.value = paths.global
      fileExists.value = paths.global_exists
    } else {
      filePath.value = paths.project ?? ''
      fileExists.value = paths.project_exists
    }

    content.value = text
  } catch (e) {
    if (token !== loadToken) return
    console.error('Failed to load AGENT.md:', e)
    content.value = ''
    fileExists.value = false
  } finally {
    if (token === loadToken) loading.value = false
  }
}

async function save() {
  if (!import.meta.client || !window.__TAURI_INTERNALS__) return
  saving.value = true
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const scope = isGlobal.value ? 'global' : 'project'
    const projectPath = props.workspacePath || null
    await invoke('plugin:mcp|write_agent_md', { scope, content: content.value, projectPath })
    dirty.value = false
    fileExists.value = true
  } catch (e) {
    console.error('Failed to save AGENT.md:', e)
  } finally {
    saving.value = false
  }
}

async function importToAgents() {
  if (!import.meta.client || !window.__TAURI_INTERNALS__ || !isGlobal.value || !canImport.value) return
  const token = loadToken
  const text = content.value
  const clientIds = [...selectedIds.value]
  importing.value = true
  importError.value = ''
  importReport.value = null
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    if (dirty.value) {
      await invoke('plugin:mcp|write_agent_md', { scope: 'global', content: text, projectPath: null })
      if (token === loadToken && content.value === text) dirty.value = false
    }
    if (token !== loadToken) return
    const results = await invoke<NonNullable<typeof importReport.value>>('plugin:mcp|connect_clients', { instructionsOnly: true, clientIds })
    if (token === loadToken) importReport.value = results
  } catch (e) {
    if (token === loadToken) importError.value = String(e)
  } finally {
    importing.value = false
  }
}

async function copyInstructions() {
  try {
    await navigator.clipboard.writeText(content.value)
    copied.value = true
  } catch (e) {
    importError.value = `Could not copy instructions: ${String(e)}`
  }
}

// "Reset to default": the global file goes back to the AGENT.md QuantSuite
// ships (the crate's template), so the operator can always return to a clean
// version. Confirmed first — it discards every edit, unsaved ones included.
const showResetConfirm = ref(false)

async function resetToDefault() {
  if (!import.meta.client || !window.__TAURI_INTERNALS__) return
  saving.value = true
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const scope = isGlobal.value ? 'global' : 'project'
    const projectPath = props.workspacePath || null
    content.value = await invoke<string>('plugin:mcp|reset_agent_md', { scope, projectPath })
    dirty.value = false
    fileExists.value = true
  } catch (e) {
    console.error('Failed to reset AGENT.md:', e)
  } finally {
    saving.value = false
  }
}

async function initFile() {
  if (!import.meta.client || !window.__TAURI_INTERNALS__) return
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const scope = isGlobal.value ? 'global' : 'project'
    const projectPath = props.workspacePath || null
    const result = await invoke<string>('plugin:mcp|init_agent_md', { scope, projectPath })
    if (result === 'created') await loadState()
  } catch (e) {
    console.error('Failed to init AGENT.md:', e)
  }
}

// The suite composable owns the window listener, including standing it down
// while this page sits in the module's warm cache — otherwise a hidden editor
// answers Ctrl+S and saves stale content.
useShortcuts([
  {
    key: 's',
    ctrl: true,
    handler: (e) => {
      e.preventDefault()
      if (dirty.value && !saving.value && !importing.value) save()
    },
  },
])

watch(() => props.workspacePath, () => loadState())
watch(content, () => { copied.value = false })
onMounted(() => loadState())
</script>

<template>
  <div class="agent-editor">
    <div class="editor-header">
      <div class="editor-header-left">
        <h2 class="editor-title">Agent.md</h2>
        <span class="editor-path" :title="filePath">{{ filePath }}</span>
      </div>
      <div class="editor-header-right">
        <span v-if="dirty" class="dirty-badge">Unsaved</span>
        <button
          v-if="isGlobal && fileExists"
          class="btn-secondary btn-sm"
          :disabled="loading || saving || importing"
          title="Replace this file with the AGENT.md QuantSuite ships"
          @click="showResetConfirm = true"
        >
          Reset to default
        </button>
        <button
          class="btn-primary btn-sm"
          :disabled="!dirty || saving || importing"
          @click="save"
        >
          {{ saving ? 'Saving...' : 'Save' }}
        </button>
      </div>
    </div>

    <McpConfirmModal
      v-model="showResetConfirm"
      title="Reset Agent.md"
      message="Replace the global Agent.md with the version QuantSuite ships?"
      submessage="Every edit to this file is discarded, unsaved ones included. Agents get the reset text on their next connect or get_instructions call."
      confirm-label="Reset"
      cancel-label="Cancel"
      danger
      @confirm="resetToDefault"
    />

    <div class="hint-bar">
      <span class="hint-text">{{ hint }}</span>
    </div>

    <AgentRecipients
      v-if="isGlobal && fileExists"
      v-model="selectedIds"
      :agents="recipients"
      :scanning="scanning"
      :busy="importing || saving"
      :error="scanError"
      :can-import="canImport"
      :import-label="importing ? 'Importing…' : dirty ? 'Save & import selected' : 'Import selected'"
      :save-custom="saveCustom"
      :remove-custom="removeCustom"
      @refresh="detectAgents"
      @import="importToAgents"
    />

    <div v-if="importError" class="import-error" role="alert">{{ importError }}</div>
    <div v-if="importReport" class="import-report" aria-live="polite">
      <p v-if="!importReport.length">No import results returned. Refresh detection and try again.</p>
      <template v-else>
        <p>Import results · Start a new agent session to load imported instructions.</p>
        <ul>
          <li v-for="row in importReport" :key="row.id" :class="{ 'import-error': row.outcome === 'failed' }">
            <strong>{{ row.name }}</strong> · {{ row.outcome === 'manual' ? 'Manual setup: ' : row.outcome === 'failed' ? 'Failed: ' : '' }}{{ row.detail }}
          </li>
        </ul>
        <button v-if="importReport.some(row => row.outcome === 'manual')" class="btn-secondary btn-sm" @click="copyInstructions">
          {{ copied ? 'Copied' : 'Copy instructions for manual setup' }}
        </button>
      </template>
    </div>

    <div v-if="!fileExists && !loading" class="editor-empty">
      <p class="empty-title">No AGENT.md file exists yet</p>
      <p class="empty-text">Create one with a default template to get started.</p>
      <button class="btn-primary" @click="initFile">
        Initialize with Template
      </button>
    </div>

    <div v-else class="editor-body">
      <textarea
        v-model="content"
        class="md-editor"
        :disabled="loading || saving || importing"
        placeholder="Write agent instructions in Markdown..."
        spellcheck="false"
        @input="dirty = true; importReport = null"
      />
    </div>
  </div>
</template>

<style scoped>
.agent-editor {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}

.editor-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
  padding-bottom: 12px;
  flex-shrink: 0;
  flex-wrap: wrap;
}

.editor-header-left {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
}

.editor-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin: 0;
}

.editor-path {
  font-size: 11px;
  color: var(--text-muted);
  font-family: monospace;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 500px;
}

.editor-header-right {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-shrink: 0;
  flex-wrap: wrap;
}

.import-report {
  max-height: 240px;
  overflow: auto;
  flex-shrink: 0;
  padding: 12px;
  margin-bottom: 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  font-size: 12px;
  overflow-wrap: anywhere;
}

.import-report p { margin: 0 0 8px; }
.import-report ul { margin: 0 0 10px; padding-left: 18px; }
.import-report li { margin-bottom: 6px; }
.import-error { color: var(--error, #ef4444); }


.dirty-badge {
  font-size: 11px;
  color: #f59e0b;
  font-weight: 600;
}

.hint-bar {
  padding-bottom: 14px;
  flex-shrink: 0;
}

.hint-text {
  font-size: 12px;
  color: var(--text-secondary);
  font-style: italic;
}

.editor-empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
}

.empty-title {
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary);
  margin: 0;
}

.empty-text {
  font-size: 13px;
  color: var(--text-muted);
  margin: 0;
}

.editor-body {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.md-editor {
  flex: 1;
  width: 100%;
  min-height: 0;
  padding: 14px 16px;
  background: var(--bg-primary);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-primary);
  font-family: 'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace;
  font-size: 13px;
  line-height: 1.7;
  resize: none;
  box-sizing: border-box;
  transition: border-color var(--transition);
  tab-size: 2;
}

.md-editor:focus {
  outline: none;
  border-color: var(--accent);
}

.md-editor:disabled {
  opacity: 0.5;
}

.btn-primary {
  background: var(--accent);
  color: #fff;
  border: none;
  border-radius: var(--radius);
  padding: 8px 16px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: background var(--transition);
}

.btn-primary:hover:not(:disabled) {
  background: var(--accent-hover);
}

.btn-primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-secondary {
  background: var(--bg-hover);
  color: var(--text-primary);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 8px 16px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all var(--transition);
}

.btn-secondary:hover:not(:disabled) {
  border-color: var(--accent);
  color: var(--accent);
}

.btn-secondary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-sm {
  padding: 5px 12px;
  font-size: 12px;
}
</style>
