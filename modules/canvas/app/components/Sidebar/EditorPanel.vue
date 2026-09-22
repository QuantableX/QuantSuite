<script setup lang="ts">
/**
 * QuantCanvas' editor panel: the tab bar, the markdown preview and the image
 * viewer. Monaco itself lives in `QCodeEditor` (packages/ui) since 2026-08-26 —
 * QuantCode needed the same editor, and two Monaco setups would have drifted
 * (docs/PLAN-QUANTSPACE.md). Behaviour here is unchanged.
 */
import { invoke } from '@tauri-apps/api/core'
import { useAppStore } from '../../../stores/app'
import { marked } from 'marked'

const appStore = useAppStore()

const tabs = computed(() => appStore.activeEditorTabs)
const activeTabId = computed(() => appStore.activeTabId)
const activeTab = computed(() => appStore.activeTab)

// Markdown preview toggle
const showMarkdownPreview = ref(false)

const isMarkdownTab = computed(() => activeTab.value?.language === 'markdown')
const isImageTab = computed(() => activeTab.value?.language === 'image')

const renderedMarkdown = computed(() => {
  if (!isMarkdownTab.value || !activeTab.value) return ''
  return marked.parse(activeTab.value.content) as string
})

// Reset preview when switching away from markdown tabs
watch(activeTabId, () => {
  if (!isMarkdownTab.value) {
    showMarkdownPreview.value = false
  }
})

function onEditorInput(content: string) {
  if (activeTabId.value) appStore.updateTabContent(activeTabId.value, content)
}

function onCursor(position: { line: number; column: number }) {
  if (activeTab.value) activeTab.value.cursorPosition = position
}

async function saveFile(content: string) {
  if (!activeTab.value) return
  try {
    await invoke('plugin:canvas|write_file', { path: activeTab.value.filePath, content })
    appStore.markTabClean(activeTab.value.id)
    appStore.notifyFileSaved(activeTab.value.filePath, content)
  } catch (e) {
    console.error('Failed to save file:', e)
  }
}

function switchTab(tabId: string) {
  appStore.setActiveTab(tabId)
}

function closeTab(tabId: string, e: Event) {
  e.stopPropagation()
  appStore.closeTab(tabId)
}
</script>

<template>
  <div class="flex flex-col h-full" :style="{ background: 'var(--qc-bg-titlebar)' }">
    <!-- Tab bar -->
    <div class="flex items-center overflow-x-auto flex-shrink-0" :style="{ borderBottom: '1px solid var(--qc-border)', background: 'var(--qc-bg-titlebar)' }">
      <div
        v-for="tab in tabs"
        :key="tab.id"
        class="flex items-center gap-1.5 px-3 py-1.5 text-xs cursor-pointer transition-colors flex-shrink-0"
        :class="tab.id === activeTabId ? 'border-b-2 border-b-[#a0a0a8]' : ''"
        :style="{
          borderRight: '1px solid var(--qc-border)',
          color: tab.id === activeTabId ? 'var(--qc-text)' : 'var(--qc-text-muted)',
          background: tab.id === activeTabId ? 'var(--qc-bg-window)' : 'transparent',
        }"
        @click="switchTab(tab.id)"
        @mousedown.middle.prevent="closeTab(tab.id, $event)"
      >
        <!-- Dirty indicator -->
        <span
          v-if="tab.isDirty"
          class="w-1.5 h-1.5 rounded-full bg-[#a0a0a8] flex-shrink-0"
        />

        <span class="truncate max-w-[120px]">{{ tab.fileName }}</span>

        <!-- Close button -->
        <button
          class="w-4 h-4 flex items-center justify-center rounded text-[10px] flex-shrink-0"
          :style="{ color: 'var(--qc-text-muted)' }"
          @click="closeTab(tab.id, $event)"
        >
          &#10005;
        </button>
      </div>

      <!-- Empty state hint -->
      <div
        v-if="tabs.length === 0"
        class="px-3 py-1.5 text-xs"
        :style="{ color: 'var(--qc-text-muted)' }"
      >
        No file open
      </div>

      <!-- Spacer -->
      <div class="flex-1" />

      <!-- Markdown preview toggle -->
      <button
        v-if="isMarkdownTab"
        class="flex-shrink-0 px-2 py-1 text-[10px] transition-colors mr-1"
        :style="{
          color: showMarkdownPreview ? 'var(--qc-text)' : 'var(--qc-text-muted)',
          background: showMarkdownPreview ? 'var(--qc-bg-window)' : 'transparent',
          borderRadius: '4px',
        }"
        :title="showMarkdownPreview ? 'Show source' : 'Show preview'"
        @click="showMarkdownPreview = !showMarkdownPreview"
      >
        {{ showMarkdownPreview ? '&lt;/&gt; Source' : 'Preview' }}
      </button>
    </div>

    <!-- Image viewer -->
    <div
      v-if="isImageTab"
      class="flex-1 min-h-0 overflow-auto flex items-center justify-center p-4"
      :style="{ background: 'repeating-conic-gradient(var(--qc-bg-surface) 0% 25%, var(--qc-bg) 0% 50%) 50% / 16px 16px' }"
    >
      <img
        v-if="activeTab?.content"
        :src="activeTab.content"
        :alt="activeTab.fileName"
        class="max-w-full max-h-full object-contain"
        draggable="false"
      />
    </div>

    <!-- The shared editor (hidden when markdown preview or image is active).
         `tokens="qc"` makes it read QuantCanvas' palette, not the suite's. -->
    <div v-show="!showMarkdownPreview && !isImageTab" class="flex-1 min-h-0">
      <QCodeEditor
        :model-value="activeTab?.content ?? ''"
        :path="activeTab?.filePath ?? ''"
        :language="activeTab?.language ?? 'plaintext'"
        :theme="appStore.theme"
        :minimap="appStore.editorMinimap"
        tokens="qc"
        @update:model-value="onEditorInput"
        @save="saveFile"
        @cursor="onCursor"
      />
    </div>

    <!-- Markdown rendered preview -->
    <div
      v-if="showMarkdownPreview && isMarkdownTab"
      class="flex-1 min-h-0 overflow-auto markdown-body p-4"
      :style="{ background: 'var(--qc-bg-window)' }"
      v-html="renderedMarkdown"
    />

  </div>
</template>

<style scoped>
/* Markdown rendered output styling */
.markdown-body {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif;
  font-size: 13px;
  line-height: 1.6;
  color: var(--qc-text);
}

.markdown-body :deep(h1) {
  font-size: 1.6em;
  font-weight: 700;
  margin: 0.6em 0 0.4em;
  padding-bottom: 0.3em;
  border-bottom: 1px solid var(--qc-border);
}

.markdown-body :deep(h2) {
  font-size: 1.3em;
  font-weight: 600;
  margin: 0.6em 0 0.3em;
  padding-bottom: 0.2em;
  border-bottom: 1px solid var(--qc-border);
}

.markdown-body :deep(h3) {
  font-size: 1.1em;
  font-weight: 600;
  margin: 0.5em 0 0.3em;
}

.markdown-body :deep(h4),
.markdown-body :deep(h5),
.markdown-body :deep(h6) {
  font-size: 1em;
  font-weight: 600;
  margin: 0.4em 0 0.2em;
}

.markdown-body :deep(p) {
  margin: 0.4em 0;
}

.markdown-body :deep(a) {
  color: #60a5fa;
  text-decoration: none;
}

.markdown-body :deep(a:hover) {
  text-decoration: underline;
}

.markdown-body :deep(strong) {
  font-weight: 700;
  color: var(--qc-text);
}

.markdown-body :deep(em) {
  font-style: italic;
}

.markdown-body :deep(code) {
  font-family: "JetBrains Mono", "Fira Code", monospace;
  font-size: 0.88em;
  padding: 0.15em 0.35em;
  border-radius: 4px;
  background: var(--qc-bg-surface);
  color: #f59e0b;
}

.markdown-body :deep(pre) {
  margin: 0.5em 0;
  padding: 10px 14px;
  border-radius: 6px;
  background: var(--qc-bg-surface);
  overflow-x: auto;
}

.markdown-body :deep(pre code) {
  padding: 0;
  background: none;
  color: var(--qc-text);
  font-size: 12px;
  line-height: 1.5;
}

.markdown-body :deep(blockquote) {
  margin: 0.5em 0;
  padding: 0.3em 1em;
  border-left: 3px solid var(--qc-border);
  color: var(--qc-text-muted);
}

.markdown-body :deep(ul),
.markdown-body :deep(ol) {
  margin: 0.4em 0;
  padding-left: 1.5em;
}

.markdown-body :deep(li) {
  margin: 0.15em 0;
}

.markdown-body :deep(hr) {
  border: none;
  border-top: 1px solid var(--qc-border);
  margin: 0.8em 0;
}

.markdown-body :deep(table) {
  border-collapse: collapse;
  width: 100%;
  margin: 0.5em 0;
}

.markdown-body :deep(th),
.markdown-body :deep(td) {
  padding: 6px 10px;
  border: 1px solid var(--qc-border);
  font-size: 12px;
}

.markdown-body :deep(th) {
  font-weight: 600;
  background: var(--qc-bg-surface);
}

.markdown-body :deep(img) {
  max-width: 100%;
  border-radius: 4px;
}

.markdown-body :deep(input[type="checkbox"]) {
  margin-right: 0.4em;
}
</style>
