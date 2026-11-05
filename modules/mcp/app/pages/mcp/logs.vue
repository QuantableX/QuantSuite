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
  return new Date(ts * 1000).toLocaleTimeString('en-US', { hourCycle: 'h23' })
}

async function handleClear() {
  await clear()
}
</script>

<template>
  <div class="logs-page">
    <!-- The title, the filters and the actions share one row. -->
    <div class="page-header">
      <h1 class="page-title">Logs</h1>
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
      <div class="header-actions">
        <button class="btn-secondary" @click="refresh" :disabled="loading">
          {{ loading ? 'Loading...' : 'Refresh' }}
        </button>
        <button class="btn-danger-outline" @click="handleClear">Clear</button>
      </div>
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

    <QEmptyState v-else-if="!loading" title="No log entries yet." />
  </div>
</template>

<style scoped>
.logs-page { display: flex; flex-direction: column; gap: var(--qss-gap); width: 100%; height: 100%; min-height: 0; container-type: inline-size; }

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

.btn-danger-outline {
  height: var(--qss-ctl-h);
  padding: 0 12px;
  background: transparent;
  border: 1px solid var(--error);
  border-radius: var(--qss-ctl-radius);
  color: var(--error);
  font-size: var(--qss-text-label);
  cursor: pointer;
  transition: all var(--transition);
}

.btn-danger-outline:hover { background: var(--error); color: white; }

.filter-bar {
  flex: 1 1 auto;
  min-width: 0;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--qss-gap-tight) var(--qss-gap);
}

.filter-group { display: flex; align-items: center; gap: 6px; }
.filter-label { font-size: var(--qss-text-label); color: var(--text-muted); }

.filter-select {
  height: var(--qss-ctl-h);
  padding: 0 8px;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--qss-ctl-radius);
  color: var(--text-primary);
  font-size: var(--qss-text-label);
  outline: none;
}

.log-count { font-size: var(--qss-text-meta); color: var(--text-muted); font-variant-numeric: tabular-nums; }

/* One panel of rows: who and when on the left, the payload beside it. */
.log-list {
  flex: 0 1 auto;
  min-height: 0;
  overflow-y: auto;
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: var(--qss-panel-radius);
}

.log-entry {
  display: grid;
  grid-template-columns: minmax(0, 380px) minmax(0, 1fr);
  column-gap: var(--qss-gap);
  align-items: start;
  min-height: var(--qss-row-h);
  padding: 6px var(--qss-panel-pad);
  border-bottom: 1px solid var(--qss-border-subtle);
}

.log-entry:last-child { border-bottom: none; }

.log-meta {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 2px var(--qss-gap-tight);
  font-size: var(--qss-text-meta);
}

.log-time { color: var(--text-muted); font-family: var(--qss-font-mono); font-variant-numeric: tabular-nums; }

.log-direction,
.log-source-type {
  padding: 0 6px;
  border-radius: 6px;
  background: var(--bg-hover);
  color: var(--text-secondary);
  font-size: 11px;
  font-family: var(--qss-font-mono);
}

.log-source-type { color: var(--text-muted); }
.type-tool { color: var(--text-secondary); }

.log-source {
  font-weight: 500;
  font-size: var(--qss-text-label);
  font-family: var(--qss-font-mono);
  color: var(--text-primary);
  overflow-wrap: anywhere;
}

.log-content {
  font-size: var(--qss-text-meta);
  font-family: var(--qss-font-mono);
  line-height: 1.5;
  color: var(--text-secondary);
  overflow-x: auto;
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 200px;
  overflow-y: auto;
  margin: 0;
}

@container (max-width: 760px) {
  .log-entry { grid-template-columns: minmax(0, 1fr); row-gap: 2px; }
  .filter-bar { order: 3; flex-basis: 100%; }
}
</style>
