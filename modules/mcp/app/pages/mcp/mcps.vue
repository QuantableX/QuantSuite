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

    <QEmptyState v-else-if="!loading" title="No MCPs configured yet.">
      <button class="btn-add" @click="openAdd">+ Add MCP</button>
    </QEmptyState>

    <McpFormModal v-model="showModal" :edit-entry="editingMcp" @submit="handleSubmit" />
    <McpConfigModal v-model="showConfig" :mcp-id="configMcp?.id" :mcp-name="configMcp?.name" />
  </div>
</template>

<style scoped>
.mcps-page { display: flex; flex-direction: column; gap: var(--qss-gap); width: 100%; height: 100%; min-height: 0; container-type: inline-size; }

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

/* One panel; each server is a two-line row. */
.mcp-list {
  flex: 0 1 auto;
  min-height: 0;
  overflow-y: auto;
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: var(--qss-panel-radius);
}

.mcp-card {
  display: flex;
  align-items: center;
  gap: var(--qss-gap);
  min-height: var(--qss-row-h);
  padding: 6px var(--qss-panel-pad);
  border-bottom: 1px solid var(--qss-border-subtle);
  transition: background var(--transition);
}

.mcp-card:last-child { border-bottom: none; }
.mcp-card:hover { background: var(--bg-hover); }

.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}

.dot-running { background: var(--success); }
.dot-error { background: var(--error); }
.dot-stopped { background: var(--text-muted); }

.mcp-info {
  flex: 1;
  min-width: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  column-gap: var(--qss-gap);
  align-items: center;
}
.mcp-name { grid-column: 1; font-size: var(--qss-text-section); font-weight: 600; }
.mcp-description { grid-column: 1; font-size: var(--qss-text-label); color: var(--text-secondary); }

.mcp-meta {
  grid-column: 2;
  grid-row: 1 / span 2;
  display: flex;
  flex-wrap: wrap;
  gap: 2px var(--qss-gap);
  font-size: var(--qss-text-meta);
  color: var(--text-muted);
  font-family: var(--qss-font-mono);
  overflow-wrap: anywhere;
}

.mcp-cmd {
  color: var(--text-secondary);
}

.mcp-actions {
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

/* Toggle switch */
.toggle-switch { position: relative; width: 36px; height: 20px; flex-shrink: 0; margin-left: 4px; }
.toggle-switch input { display: none; }

.toggle-slider {
  position: absolute;
  inset: 0;
  background: var(--border);
  border-radius: 10px;
  cursor: pointer;
  transition: all var(--transition);
}

.toggle-slider::before {
  content: '';
  position: absolute;
  top: 2px;
  left: 2px;
  width: 16px;
  height: 16px;
  background: var(--text-secondary);
  border-radius: 50%;
  transition: all var(--transition);
}

.toggle-switch input:checked + .toggle-slider { background: var(--accent); }
.toggle-switch input:checked + .toggle-slider::before { transform: translateX(16px); background: white; }

@container (max-width: 760px) {
  .mcp-info { grid-template-columns: minmax(0, 1fr); }
  .mcp-meta { grid-column: 1; grid-row: auto; }
}
</style>
