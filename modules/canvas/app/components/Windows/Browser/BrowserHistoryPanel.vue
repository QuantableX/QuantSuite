<script setup lang="ts">
import type { BrowserHistoryEntry } from '../../../../shared/types'

const props = defineProps<{
  history: BrowserHistoryEntry[]
}>()

const emit = defineEmits<{
  navigate: [url: string]
  close: []
  clear: []
}>()

const searchQuery = ref('')

// The store keeps up to 10 000 entries; rendering them all builds tens of
// thousands of DOM nodes on open and again on every keystroke. Only the newest
// matches are shown — the search box is how you reach the rest.
const MAX_VISIBLE_ENTRIES = 300

const filteredHistory = computed(() => {
  const query = searchQuery.value.toLowerCase().trim()
  if (!query) return props.history
  return props.history.filter(
    (entry) =>
      entry.url.toLowerCase().includes(query) ||
      entry.title.toLowerCase().includes(query),
  )
})

// History is stored newest-first, so the window is simply the head of the list.
const visibleHistory = computed(() => filteredHistory.value.slice(0, MAX_VISIBLE_ENTRIES))
const hiddenCount = computed(() => filteredHistory.value.length - visibleHistory.value.length)

interface GroupedHistory {
  label: string
  entries: BrowserHistoryEntry[]
}

const groupedHistory = computed<GroupedHistory[]>(() => {
  const now = new Date()
  const todayStart = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime()
  const yesterdayStart = todayStart - 86400000
  const weekStart = todayStart - 6 * 86400000

  const groups: Record<string, BrowserHistoryEntry[]> = {
    Today: [],
    Yesterday: [],
    'Last 7 days': [],
    Older: [],
  }

  for (const entry of visibleHistory.value) {
    const ts = new Date(entry.visitedAt).getTime()
    if (ts >= todayStart) {
      groups['Today'].push(entry)
    } else if (ts >= yesterdayStart) {
      groups['Yesterday'].push(entry)
    } else if (ts >= weekStart) {
      groups['Last 7 days'].push(entry)
    } else {
      groups['Older'].push(entry)
    }
  }

  return Object.entries(groups)
    .filter(([, entries]) => entries.length > 0)
    .map(([label, entries]) => ({ label, entries }))
})

function formatTime(dateStr: string): string {
  const date = new Date(dateStr)
  const now = new Date()
  const diffMs = now.getTime() - date.getTime()
  const diffMin = Math.floor(diffMs / 60000)

  if (diffMin < 1) return 'Just now'
  if (diffMin < 60) return `${diffMin}m ago`
  const diffHours = Math.floor(diffMin / 60)
  if (diffHours < 24) return `${diffHours}h ago`
  const diffDays = Math.floor(diffHours / 24)
  if (diffDays < 7) return `${diffDays}d ago`
  return date.toLocaleDateString()
}
</script>

<template>
  <div
    class="absolute inset-0 z-40 flex flex-col overflow-hidden"
    :style="{
      background: 'var(--qc-bg)',
    }"
  >
    <!-- Header -->
    <div
      class="flex items-center justify-between px-3 py-2 flex-shrink-0"
      :style="{ borderBottom: '1px solid var(--qc-border)' }"
    >
      <span class="text-xs font-medium" :style="{ color: 'var(--qc-text)' }">History</span>
      <button
        class="w-5 h-5 flex items-center justify-center rounded transition-colors hover:brightness-125"
        :style="{ color: 'var(--qc-text-dim)' }"
        title="Close"
        @click="emit('close')"
        @mousedown.stop
      >
        <svg class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M18 6L6 18" />
          <path d="M6 6l12 12" />
        </svg>
      </button>
    </div>

    <!-- Search input -->
    <div class="px-3 py-2 flex-shrink-0">
      <input
        v-model="searchQuery"
        type="text"
        class="w-full text-xs px-2.5 py-1 rounded outline-none"
        :style="{
          background: 'var(--qc-bg-surface)',
          color: 'var(--qc-text)',
          border: '1px solid var(--qc-border)',
        }"
        placeholder="Search history..."
        spellcheck="false"
        @mousedown.stop
      />
    </div>

    <!-- History list -->
    <div class="flex-1 min-h-0 overflow-y-auto px-3 pb-2">
      <div v-if="groupedHistory.length === 0" class="flex items-center justify-center py-8">
        <span class="text-xs" :style="{ color: 'var(--qc-text-dim)' }">No history found</span>
      </div>

      <div v-for="group in groupedHistory" :key="group.label" class="mb-3">
        <!-- Group label -->
        <div
          class="text-[10px] font-medium uppercase tracking-wide px-1 py-1"
          :style="{ color: 'var(--qc-text-dim)' }"
        >
          {{ group.label }}
        </div>

        <!-- Entries -->
        <div
          v-for="entry in group.entries"
          :key="entry.url + entry.visitedAt"
          class="flex items-start justify-between gap-2 px-2 py-1.5 rounded cursor-pointer transition-colors hover:brightness-110"
          :style="{ background: 'transparent' }"
          @click="emit('navigate', entry.url)"
        >
          <div class="min-w-0 flex-1">
            <div class="text-[11px] truncate" :style="{ color: 'var(--qc-text)' }">
              {{ entry.title || entry.url }}
            </div>
            <div class="text-[9px] truncate" :style="{ color: 'var(--qc-text-dim)' }">
              {{ entry.url }}
            </div>
          </div>
          <span class="text-[9px] flex-shrink-0 pt-0.5" :style="{ color: 'var(--qc-text-dim)' }">
            {{ formatTime(entry.visitedAt) }}
          </span>
        </div>
      </div>

      <div v-if="hiddenCount > 0" class="px-2 py-1.5 text-[9px]" :style="{ color: 'var(--qc-text-dim)' }">
        {{ hiddenCount }} older entries not shown &mdash; narrow the search to find them
      </div>
    </div>

    <!-- Footer with clear button -->
    <div
      class="flex items-center justify-center px-3 py-2 flex-shrink-0"
      :style="{ borderTop: '1px solid var(--qc-border)' }"
    >
      <button
        class="text-[10px] px-3 py-1 rounded transition-colors hover:brightness-125"
        :style="{
          color: '#ef4444',
          background: 'var(--qc-bg-surface)',
          border: '1px solid var(--qc-border)',
        }"
        @click="emit('clear')"
        @mousedown.stop
      >
        Clear all history
      </button>
    </div>
  </div>
</template>
