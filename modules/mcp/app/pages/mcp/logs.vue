<script setup lang="ts">
import { useLogs } from '#mcp/composables/useLogs'
import { inActiveKeepAliveTree } from '@quantsuite/core'
definePageMeta({ layout: 'mcp' })

import type { LogEntry } from '#mcp/composables/useLogs'

const { logs, loading, refresh, clear } = useLogs()

const filterSource = ref('all')
const filterDirection = ref('all')

let autoRefresh: ReturnType<typeof setInterval> | null = null

// V3 warm cache: poll only while visible. Start in BOTH onMounted and
// onActivated — async pages mount after the stage's activation flush, so
// onActivated alone misses the first visit; the timer guard makes the
// overlap safe. That same late mount can land in a stage the user has already
// left, where the timer guard then blocks the real activation and the polling
// never starts — so onMounted starts it only inside an active tree.
function ensureAutoRefresh() {
  if (autoRefresh) return
  refresh()
  autoRefresh = setInterval(refresh, 3000)
}

onMounted(() => {
  if (inActiveKeepAliveTree()) ensureAutoRefresh()
})
onActivated(ensureAutoRefresh)

onDeactivated(() => {
  if (autoRefresh) {
    clearInterval(autoRefresh)
    autoRefresh = null
  }
})

onUnmounted(() => {
  if (autoRefresh) clearInterval(autoRefresh)
})

const sources = computed(() => {
  const set = new Set(logs.value.map(l => l.source))
  return ['all', ...Array.from(set)]
})

const filtered = computed(() => {
  return logs.value.filter(l => {
    if (filterSource.value !== 'all' && l.source !== filterSource.value) return false
    if (filterDirection.value !== 'all' && l.direction !== filterDirection.value) return false
    return true
  }).slice().reverse()
})

function formatTime(ts: number): string {
  return new Date(ts * 1000).toLocaleTimeString()
}

async function handleClear() {
  await clear()
}
</script>

<template>
  <div class="logs-page">
    <div class="page-header">
      <div>
        <h1 class="page-title">Logs</h1>
      </div>
      <div class="header-actions">
        <button class="btn-secondary" @click="refresh" :disabled="loading">
          {{ loading ? 'Loading...' : 'Refresh' }}
        </button>
        <button class="btn-danger-outline" @click="handleClear">Clear</button>
      </div>
    </div>

    <div class="filter-bar">
      <div class="filter-group">
        <label class="filter-label">Source</label>
        <select v-model="filterSource" class="filter-select">
          <option v-for="s in sources" :key="s" :value="s">{{ s === 'all' ? 'All Sources' : s }}</option>
        </select>
      </div>
      <div class="filter-group">
        <label class="filter-label">Direction</label>
        <select v-model="filterDirection" class="filter-select">
          <option value="all">All</option>
          <option value="In">In</option>
          <option value="Out">Out</option>
        </select>
      </div>
      <span class="log-count">{{ filtered.length }} entries</span>
    </div>

    <div v-if="filtered.length" class="log-list">
      <div v-for="entry in filtered" :key="entry.id" class="log-entry">
        <div class="log-meta">
          <span class="log-time">{{ formatTime(entry.timestamp) }}</span>
          <span class="log-direction" :class="'dir-' + entry.direction.toLowerCase()">
            {{ entry.direction === 'In' ? '↓ IN' : '↑ OUT' }}
          </span>
          <span class="log-source-type" :class="'type-' + entry.source_type">{{ entry.source_type }}</span>
          <span class="log-source">{{ entry.source }}</span>
        </div>
        <pre class="log-content">{{ entry.content }}</pre>
      </div>
    </div>

    <div v-else-if="!loading" class="empty-state">
      <p class="empty-text">No log entries yet.</p>
    </div>
  </div>
</template>

<style scoped>
.logs-page { width: 100%; display: flex; flex-direction: column; height: 100%; }

.page-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  margin-bottom: 16px;
  flex-shrink: 0;
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

.btn-danger-outline {
  padding: 8px 16px;
  background: transparent;
  border: 1px solid var(--error);
  border-radius: var(--radius);
  color: var(--error);
  font-size: 13px;
  cursor: pointer;
  transition: all var(--transition);
}

.btn-danger-outline:hover { background: var(--error); color: white; }

.filter-bar {
  display: flex;
  align-items: center;
  gap: 16px;
  margin-bottom: 12px;
  flex-shrink: 0;
}

.filter-group { display: flex; align-items: center; gap: 6px; }
.filter-label { font-size: 12px; color: var(--text-muted); }

.filter-select {
  padding: 6px 10px;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-primary);
  font-size: 13px;
  outline: none;
}

.log-count { margin-left: auto; font-size: 12px; color: var(--text-muted); }

.log-list {
  flex: 1;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-height: 0;
}

.log-entry {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 12px 16px;
}

.log-meta {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 8px;
  font-size: 12px;
}

.log-time { color: var(--text-muted); font-family: monospace; }

.log-direction {
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 600;
  font-family: monospace;
}

.dir-in { background: rgba(46, 213, 115, 0.15); color: #2ed573; }
.dir-out { background: rgba(100, 150, 255, 0.15); color: #6496ff; }

.log-source-type {
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 11px;
  color: var(--text-muted);
  background: var(--bg-hover);
  font-family: monospace;
}

.type-tool { color: var(--accent); }

.log-source {
  font-weight: 500;
  font-size: 13px;
  font-family: monospace;
  color: var(--text-primary);
}

.log-content {
  font-size: 12px;
  font-family: monospace;
  color: var(--text-secondary);
  background: var(--bg-primary);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 10px 12px;
  overflow-x: auto;
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 200px;
  overflow-y: auto;
  margin: 0;
}

.empty-state { text-align: center; padding: 60px 0; }
.empty-text { font-size: 16px; color: var(--text-secondary); }
.empty-sub { font-size: 14px; color: var(--text-muted); margin-top: 4px; }
</style>
