<script setup lang="ts">
import { useScripts } from '#mcp/composables/useScripts'
definePageMeta({ layout: 'mcp' })

import type { ScriptEntry, ScriptFormData } from '#mcp/composables/useScripts'

const { scripts, loading, refresh, addScript, updateScript, deleteScript } = useScripts()

const showModal = ref(false)
const editingScript = ref<ScriptEntry | null>(null)

onMounted(() => refresh())

function openAdd() {
  editingScript.value = null
  showModal.value = true
}

function openEdit(script: ScriptEntry) {
  editingScript.value = script
  showModal.value = true
}

async function handleSubmit(data: ScriptFormData) {
  if (editingScript.value) {
    await updateScript(editingScript.value.id, data)
  } else {
    await addScript(data)
  }
}

async function handleDelete(script: ScriptEntry) {
  if (confirm(`Delete script "${script.name}"?`)) {
    await deleteScript(script.id)
  }
}

function langBadgeClass(lang: string) {
  return {
    Python: 'lang-python',
    Javascript: 'lang-js',
    Bash: 'lang-bash',
    Powershell: 'lang-ps',
  }[lang] || ''
}

function previewContent(script: ScriptEntry): string {
  return script.content.split('\n').slice(0, 2).join('\n')
}

function formatDate(ts: number): string {
  return new Date(ts * 1000).toLocaleDateString()
}
</script>

<template>
  <div class="scripts-page">
    <div class="page-header">
      <div>
        <h1 class="page-title">Scripts</h1>
      </div>
      <div class="header-actions">
        <button class="btn-secondary" @click="refresh" :disabled="loading">
          {{ loading ? 'Loading...' : 'Refresh' }}
        </button>
        <button class="btn-add" @click="openAdd">+ Add Script</button>
      </div>
    </div>

    <div v-if="scripts.length" class="script-list">
      <div v-for="script in scripts" :key="script.id" class="script-card">
        <div class="script-info">
          <div class="script-header-row">
            <span class="script-name">{{ script.name }}</span>
            <span class="lang-badge" :class="langBadgeClass(script.language)">{{ script.language }}</span>
          </div>
          <pre class="script-preview">{{ previewContent(script) }}</pre>
          <div class="script-meta">
            <span class="meta-date">Created {{ formatDate(script.created_at) }}</span>
          </div>
        </div>
        <div class="script-actions">
          <button class="btn-icon" @click="openEdit(script)" title="Edit">&#9998;</button>
          <button class="btn-icon btn-danger" @click="handleDelete(script)" title="Delete">&#10005;</button>
        </div>
      </div>
    </div>

    <div v-else-if="!loading" class="empty-state">
      <p class="empty-text">No scripts stored yet.</p>
      <button class="btn-add empty-btn" @click="openAdd">+ Add Script</button>
    </div>

    <McpScriptFormModal
      v-model="showModal"
      :edit-script="editingScript"
      @submit="handleSubmit"
    />
  </div>
</template>

<style scoped>
.scripts-page { width: 100%; }

.page-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  margin-bottom: 24px;
}

.page-title { font-size: 24px; font-weight: 600; }
.page-subtitle { font-size: 13px; color: var(--text-muted); margin-top: 4px; }
.header-actions { display: flex; gap: 8px; flex-shrink: 0; }

.btn-secondary {
  padding: 8px 16px;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-secondary);
  font-size: 13px;
  cursor: pointer;
  transition: all var(--transition);
}

.btn-secondary:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
.btn-secondary:disabled { opacity: 0.5; }

.btn-add {
  padding: 8px 16px;
  background: var(--accent);
  border: none;
  border-radius: var(--radius);
  color: var(--bg-primary);
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: all var(--transition);
}

.btn-add:hover { background: var(--accent-hover); }

.script-list { display: flex; flex-direction: column; gap: 8px; }

.script-card {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 16px 20px;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  transition: all var(--transition);
}

.script-card:hover { border-color: var(--text-muted); }

.script-info { flex: 1; min-width: 0; }

.script-header-row {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 6px;
}

.script-name {
  font-size: 15px;
  font-weight: 600;
  font-family: monospace;
}

.lang-badge {
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 600;
}

.lang-python { background: rgba(55, 118, 171, 0.2); color: #4da6ff; }
.lang-js { background: rgba(240, 219, 79, 0.2); color: #f0db4f; }
.lang-bash { background: rgba(46, 213, 115, 0.2); color: #2ed573; }
.lang-ps { background: rgba(0, 122, 204, 0.2); color: #5cb3ff; }

.script-preview {
  font-size: 12px;
  font-family: monospace;
  color: var(--text-muted);
  background: var(--bg-primary);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 6px 10px;
  overflow: hidden;
  white-space: pre;
  margin: 0 0 6px;
  max-height: 40px;
}

.script-meta { font-size: 12px; color: var(--text-muted); }

.script-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

.btn-icon {
  padding: 6px 8px;
  background: transparent;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-secondary);
  font-size: 14px;
  cursor: pointer;
  transition: all var(--transition);
  line-height: 1;
}

.btn-icon:hover { background: var(--bg-hover); color: var(--text-primary); }
.btn-danger:hover { border-color: var(--error); color: var(--error); }

.empty-state { text-align: center; padding: 60px 0; }
.empty-text { font-size: 16px; color: var(--text-secondary); }
.empty-sub { font-size: 14px; color: var(--text-muted); margin-top: 4px; }
.empty-btn { margin-top: 20px; }
</style>
