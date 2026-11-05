<script setup lang="ts">
import { useMcps } from '#mcp/composables/useMcps'
import { inActiveKeepAliveTree } from '@quantsuite/core'
definePageMeta({ layout: 'mcp' })

import type { McpEntry, McpFormData } from '#mcp/composables/useMcps'

const { mcps, loading, statuses, refresh, toggle, addMcp, updateMcp, deleteMcp, startMcp, stopMcp, refreshStatuses } = useMcps()

const showModal = ref(false)
const editingMcp = ref<McpEntry | null>(null)
const showConfig = ref(false)
const configMcp = ref<McpEntry | null>(null)
let statusInterval: ReturnType<typeof setInterval> | null = null

// V3 warm cache: poll statuses only while visible. Start in BOTH onMounted
// and onActivated — async pages mount after the stage's activation flush, so
// onActivated alone misses the first visit; the timer guard makes the
// overlap safe. That same late mount can land in a stage the user has already
// left, where the timer guard then blocks the real activation and the polling
// never starts — so onMounted starts it only inside an active tree. The
// one-off refresh() next to it is a plain load and stays unconditional.
function ensureStatusPolling() {
  if (statusInterval) return
  refreshStatuses()
  statusInterval = setInterval(refreshStatuses, 5000)
}

onMounted(() => {
  refresh()
  if (inActiveKeepAliveTree()) ensureStatusPolling()
})

onActivated(ensureStatusPolling)

onDeactivated(() => {
  if (statusInterval) {
    clearInterval(statusInterval)
    statusInterval = null
  }
})

onUnmounted(() => {
  if (statusInterval) clearInterval(statusInterval)
})

function openAdd() {
  editingMcp.value = null
  showModal.value = true
}

function openEdit(mcp: McpEntry) {
  editingMcp.value = mcp
  showModal.value = true
}

function openConfig(mcp: McpEntry) {
  configMcp.value = mcp
  showConfig.value = true
}

async function handleSubmit(data: McpFormData) {
  if (editingMcp.value) {
    await updateMcp(editingMcp.value.id, data)
  } else {
    await addMcp(data)
  }
}

async function handleDelete(mcp: McpEntry) {
  if (confirm(`Delete "${mcp.name}"?`)) {
    await deleteMcp(mcp.id)
  }
}

function getStatus(id: string) {
  const s = statuses.value[id]
  if (!s) return 'stopped'
  if (s === 'running') return 'running'
  if (s === 'stopped') return 'stopped'
  if (typeof s === 'object' && 'error' in s) return 'error'
  return 'stopped'
}

function isRunning(id: string) {
  return getStatus(id) === 'running'
}
</script>

<template>
  <div class="mcps-page">
    <div class="page-header">
      <h1 class="page-title">MCPs</h1>
      <div class="header-actions">
        <button class="btn-secondary" @click="refresh" :disabled="loading">
          {{ loading ? 'Loading...' : 'Refresh' }}
        </button>
        <button class="btn-add" @click="openAdd">+ Add MCP</button>
      </div>
    </div>

    <div v-if="mcps.length" class="mcp-list">
      <div v-for="mcp in mcps" :key="mcp.id" class="mcp-card">
        <span class="status-dot" :class="'dot-' + getStatus(mcp.id)"></span>
        <div class="mcp-info">
          <div class="mcp-name">{{ mcp.name }}</div>
          <div class="mcp-description">{{ mcp.description }}</div>
          <div class="mcp-meta">
            <span class="mcp-id">{{ mcp.id.slice(0, 8) }}</span>
            <span class="mcp-transport">
              {{ typeof mcp.transport === 'string' ? mcp.transport : `HTTP :${mcp.transport.http.port}` }}
            </span>
            <span v-if="mcp.command" class="mcp-cmd">{{ mcp.command }}</span>
          </div>
        </div>
        <div class="mcp-actions">
          <button
            v-if="mcp.command"
            class="btn-icon"
            @click="isRunning(mcp.id) ? stopMcp(mcp.id) : startMcp(mcp.id)"
            :title="isRunning(mcp.id) ? 'Stop' : 'Start'"
          >
            <span v-if="isRunning(mcp.id)">&#9632;</span>
            <span v-else>&#9654;</span>
          </button>
          <button class="btn-icon" @click="openConfig(mcp)" title="Config">{ }</button>
          <button class="btn-icon" @click="openEdit(mcp)" title="Edit">&#9998;</button>
          <button class="btn-icon btn-danger" @click="handleDelete(mcp)" title="Delete">&#10005;</button>
          <label class="toggle-switch">
            <input type="checkbox" :checked="mcp.enabled" @change="toggle(mcp.id, !mcp.enabled)" />
            <span class="toggle-slider"></span>
          </label>
        </div>
      </div>
    </div>

    <div v-else-if="!loading" class="empty-state">
      <p class="empty-text">No MCPs configured yet.</p>
      <button class="btn-add empty-btn" @click="openAdd">+ Add MCP</button>
    </div>

    <McpFormModal v-model="showModal" :edit-entry="editingMcp" @submit="handleSubmit" />
    <McpConfigModal v-model="showConfig" :mcp-id="configMcp?.id" :mcp-name="configMcp?.name" />
  </div>
</template>

<style scoped>
.mcps-page { width: 100%; }

.page-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 24px;
}

.page-title { font-size: 24px; font-weight: 600; }

.header-actions { display: flex; gap: 8px; }

.btn-secondary {
  padding: 8px 16px;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-secondary);
  font-size: 13px;
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
  transition: all var(--transition);
}

.btn-add:hover { background: var(--accent-hover); }

.mcp-list { display: flex; flex-direction: column; gap: 8px; }

.mcp-card {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 16px 20px;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  transition: all var(--transition);
}

.mcp-card:hover { border-color: var(--text-muted); }

.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}

.dot-running { background: var(--success); }
.dot-error { background: var(--error); }
.dot-stopped { background: var(--text-muted); }

.mcp-info { flex: 1; min-width: 0; }
.mcp-name { font-size: 15px; font-weight: 600; margin-bottom: 2px; }
.mcp-description { font-size: 13px; color: var(--text-secondary); margin-bottom: 6px; }

.mcp-meta {
  display: flex;
  gap: 12px;
  font-size: 12px;
  color: var(--text-muted);
  font-family: monospace;
}

.mcp-cmd {
  color: var(--text-secondary);
}

.mcp-actions {
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

/* Toggle switch */
.toggle-switch { position: relative; width: 44px; height: 24px; flex-shrink: 0; }
.toggle-switch input { display: none; }

.toggle-slider {
  position: absolute;
  inset: 0;
  background: var(--border);
  border-radius: 12px;
  cursor: pointer;
  transition: all var(--transition);
}

.toggle-slider::before {
  content: '';
  position: absolute;
  top: 3px;
  left: 3px;
  width: 18px;
  height: 18px;
  background: var(--text-secondary);
  border-radius: 50%;
  transition: all var(--transition);
}

.toggle-switch input:checked + .toggle-slider { background: var(--accent); }
.toggle-switch input:checked + .toggle-slider::before { transform: translateX(20px); background: white; }

.empty-state { text-align: center; padding: 60px 0; }
.empty-text { font-size: 16px; color: var(--text-secondary); }
.empty-sub { font-size: 14px; color: var(--text-muted); margin-top: 4px; }
.empty-btn { margin-top: 20px; }
</style>
