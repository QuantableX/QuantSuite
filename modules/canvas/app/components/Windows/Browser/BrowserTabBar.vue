<script setup lang="ts">
import type { BrowserTab } from '../../../../shared/types'

const props = defineProps<{
  tabs: BrowserTab[]
  activeTabId: string
}>()

const emit = defineEmits<{
  switch: [tabId: string]
  close: [tabId: string]
  add: []
}>()

function truncate(text: string, max: number): string {
  return text.length > max ? text.slice(0, max) + '...' : text
}
</script>

<template>
  <div
    class="flex items-center flex-shrink-0 overflow-x-auto"
    :style="{
      height: '28px',
      background: 'var(--qc-bg-header)',
      borderBottom: '1px solid var(--qc-border)',
    }"
  >
    <!-- Tabs -->
    <div class="flex items-stretch h-full min-w-0 overflow-x-auto no-scrollbar">
      <div
        v-for="tab in tabs"
        :key="tab.id"
        class="flex items-center gap-1 px-2 cursor-pointer flex-shrink-0 max-w-[160px] transition-colors"
        :style="{
          background: tab.id === activeTabId ? 'var(--qc-bg-surface)' : 'transparent',
          borderRight: '1px solid var(--qc-border)',
          color: tab.id === activeTabId ? 'var(--qc-text)' : 'var(--qc-text-dim)',
        }"
        @click="emit('switch', tab.id)"
        @auxclick.middle.prevent="emit('close', tab.id)"
      >
        <!-- Loading spinner -->
        <svg
          v-if="tab.isLoading"
          class="w-3 h-3 flex-shrink-0 animate-spin"
          :style="{ color: 'var(--qc-text-dim)' }"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8" />
        </svg>

        <!-- Tab title -->
        <span class="text-[10px] truncate select-none leading-none">
          {{ truncate(tab.title || 'New Tab', 20) }}
        </span>

        <!-- Close button (hidden when only one tab) -->
        <button
          v-if="tabs.length > 1"
          class="w-3.5 h-3.5 flex items-center justify-center flex-shrink-0 rounded opacity-50 hover:opacity-100 transition-opacity"
          :style="{ color: 'var(--qc-text-dim)' }"
          title="Close tab"
          @click.stop="emit('close', tab.id)"
          @mousedown.stop
        >
          <svg class="w-2.5 h-2.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M18 6L6 18" />
            <path d="M6 6l12 12" />
          </svg>
        </button>
      </div>
    </div>

    <!-- Add tab button -->
    <button
      class="w-6 h-full flex items-center justify-center flex-shrink-0 transition-colors hover:brightness-125"
      :style="{ color: 'var(--qc-text-dim)' }"
      title="New tab"
      @click="emit('add')"
      @mousedown.stop
    >
      <svg class="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M12 5v14" />
        <path d="M5 12h14" />
      </svg>
    </button>
  </div>
</template>

<style scoped>
.no-scrollbar::-webkit-scrollbar {
  display: none;
}
.no-scrollbar {
  -ms-overflow-style: none;
  scrollbar-width: none;
}
</style>
