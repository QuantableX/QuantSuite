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
  return new Date(ts * 1000).toLocaleDateString('en-US', { dateStyle: 'medium' })
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

    <QEmptyState v-else-if="!loading" title="No scripts stored yet.">
      <button class="btn-add" @click="openAdd">+ Add Script</button>
    </QEmptyState>

    <McpScriptFormModal
      v-model="showModal"
      :edit-script="editingScript"
      @submit="handleSubmit"
    />
  </div>
</template>

<style scoped>
.scripts-page { display: flex; flex-direction: column; gap: var(--qss-gap); width: 100%; height: 100%; min-height: 0; container-type: inline-size; }

.page-header {
  flex-shrink: 0;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--qss-gap-tight) var(--qss-gap);
  min-height: var(--qss-ctl-h);
}

.page-title { font-size: var(--qss-text-title); font-weight: 600; letter-spacing: -.01em; }
.header-actions { display: flex; gap: var(--qss-gap-tight); flex-shrink: 0; }

.btn-secondary {
  height: var(--qss-ctl-h);
  padding: 0 12px;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--qss-ctl-radius);
  color: var(--text-secondary);
  font-size: var(--qss-text-label);
  cursor: pointer;
  transition: all var(--transition);
}

.btn-secondary:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
.btn-secondary:disabled { opacity: 0.5; }

.btn-add {
  height: var(--qss-ctl-h);
  padding: 0 12px;
  background: var(--accent);
  border: none;
  border-radius: var(--qss-ctl-radius);
  color: var(--qss-accent-ink);
  font-size: var(--qss-text-label);
  font-weight: 600;
  cursor: pointer;
  transition: all var(--transition);
}

.btn-add:hover { background: var(--accent-hover); }

.script-list {
  flex: 0 1 auto;
  min-height: 0;
  overflow-y: auto;
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: var(--qss-panel-radius);
}

.script-card {
  display: flex;
  align-items: center;
  gap: var(--qss-gap);
  min-height: var(--qss-row-h);
  padding: 6px var(--qss-panel-pad);
  border-bottom: 1px solid var(--qss-border-subtle);
  transition: background var(--transition);
}

.script-card:last-child { border-bottom: none; }
.script-card:hover { background: var(--bg-hover); }

/* Name, its first lines and its date read across, not down. */
.script-info {
  flex: 1;
  min-width: 0;
  display: grid;
  grid-template-columns: minmax(180px, 1fr) minmax(0, 3fr) auto;
  column-gap: var(--qss-gap);
  align-items: center;
}

.script-header-row {
  display: flex;
  align-items: center;
  gap: var(--qss-gap-tight);
  min-width: 0;
}

.script-name {
  font-size: var(--qss-text-label);
  font-weight: 500;
  font-family: var(--qss-font-mono);
  overflow-wrap: anywhere;
}

.lang-badge {
  flex-shrink: 0;
  padding: 0 6px;
  border-radius: 6px;
  background: var(--bg-hover);
  color: var(--text-secondary);
  font-size: 11px;
}

.script-preview {
  font-size: var(--qss-text-meta);
  font-family: var(--qss-font-mono);
  color: var(--text-muted);
  overflow: hidden;
  white-space: pre;
  max-height: 36px;
  line-height: 18px;
}

.script-meta { font-size: var(--qss-text-meta); color: var(--text-muted); font-variant-numeric: tabular-nums; white-space: nowrap; }

.script-actions {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}

.btn-icon {
  background: transparent;
  border: 1px solid var(--border);
  color: var(--text-secondary);
  font-size: 13px;
  cursor: pointer;
  transition: all var(--transition);
  line-height: 1;
}

.btn-icon:hover { background: var(--bg-hover); color: var(--text-primary); }
.btn-danger:hover { border-color: var(--error); color: var(--error); }

@container (max-width: 760px) {
  .script-info { grid-template-columns: minmax(0, 1fr); row-gap: 2px; }
}
</style>
