<script setup lang="ts">
/**
 * The open scripts as tabs above the editor. A dirty tab carries a dot
 * where its close mark sits; closing one asks before dropping unsaved work.
 */
import { useWorkbenchStore } from '#script/stores/workbench'
import { Library, Plus } from 'lucide-vue-next'

const wb = useWorkbenchStore()

function close(file: string) {
  const o = wb.open.find((s) => s.file === file)
  if (o?.saving || o?.checking) return
  if (wb.closeScript(file)) return
  if (confirm(`Discard the unsaved changes to ${file}?`)) wb.closeScript(file, true)
}

function moveTab(e: KeyboardEvent, file: string) {
  const index = wb.open.findIndex((s) => s.file === file)
  const next = e.key === 'ArrowRight' ? index + 1 : e.key === 'ArrowLeft' ? index - 1 : e.key === 'Home' ? 0 : e.key === 'End' ? wb.open.length - 1 : null
  if (next === null) return
  e.preventDefault()
  const target = wb.open[(next + wb.open.length) % wb.open.length]
  if (target) { wb.activate(target.file); nextTick(() => document.getElementById(`qsc-tab-${target.file}`)?.focus()) }
}

function onAux(e: MouseEvent, file: string) {
  // Middle click closes, like every editor.
  if (e.button === 1) {
    e.preventDefault()
    close(file)
  }
}
</script>

<template>
  <div v-if="wb.open.length" class="qsc-tabs" role="tablist" aria-label="Open scripts">
    <button class="qsc-tab qsc-library-tab" role="tab" :aria-selected="wb.libraryOpen" :class="{ 'is-active': wb.libraryOpen }" aria-label="Library" @click="wb.showLibrary()"><Library :size="14" /></button>
    <div
      v-for="o in wb.open"
      :key="o.file"
      class="qsc-tab"
      :class="{ 'is-active': o.file === wb.activeFile && !wb.libraryOpen, 'is-readonly': !o.editable }"
      role="tab"
      :id="`qsc-tab-${o.file}`"
      :tabindex="o.file === wb.activeFile ? 0 : -1"
      :aria-selected="o.file === wb.activeFile && !wb.libraryOpen"
      :title="o.path"
      @keydown="moveTab($event, o.file)"
      @keydown.enter.prevent="wb.activate(o.file)"
      @keydown.space.prevent="wb.activate(o.file)"
      @click="wb.activate(o.file)"
      @auxclick="onAux($event, o.file)"
    >
      <span class="qsc-tab-name mono">{{ o.file }}</span>
      <button
        class="qsc-tab-close"
        :class="{ 'is-dirty': o.content !== o.saved }"
        :title="o.content !== o.saved ? 'Unsaved changes — close' : 'Close'"
        :aria-label="`Close ${o.file}`"
        :disabled="o.saving || !!o.checking"
        @click.stop="close(o.file)"
      >
        <span class="qsc-tab-dot" />
        <svg class="qsc-tab-x" width="10" height="10" viewBox="0 0 14 14" fill="none" aria-hidden="true">
          <path d="M3 3l8 8M11 3l-8 8" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
        </svg>
      </button>
    </div>
    <button class="qsc-icon-btn qsc-new-tab" aria-label="Create script" title="New script" @click="wb.newScriptOpen = true"><Plus :size="14" /></button>
  </div>
</template>

<style scoped>
.qsc-tabs {
  display: flex;
  align-items: stretch;
  height: 34px;
  flex-shrink: 0;
  overflow-x: auto;
  overflow-y: hidden;
  border-bottom: 1px solid var(--qss-border);
  background: var(--qss-bg-raised);
  scrollbar-width: none;
}
.qsc-library-tab { border: 0; background: none; border-right: 1px solid var(--qss-border); padding: 0 12px; }
.qsc-new-tab { align-self: center; margin: 0 6px; flex-shrink: 0; }
.qsc-tab:focus-visible { outline: 1px solid var(--qss-text-muted); outline-offset: -2px; }
.qsc-tabs::-webkit-scrollbar {
  display: none;
}

.qsc-tab {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 8px 0 12px;
  border-right: 1px solid var(--qss-border-subtle);
  color: var(--qss-text-secondary);
  font-size: 12px;
  cursor: pointer;
  user-select: none;
  white-space: nowrap;
  position: relative;
}
.qsc-tab:hover {
  color: var(--qss-text);
  background: var(--qss-bg-hover);
}
.qsc-tab.is-active {
  color: var(--qss-text);
  background: var(--qss-bg);
}
.qsc-tab.is-active::after {
  content: '';
  position: absolute;
  left: 0;
  right: 0;
  bottom: -1px;
  height: 1px;
  background: var(--qss-bg);
}
.qsc-tab.is-readonly .qsc-tab-name {
  font-style: italic;
}

.qsc-tab-close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--qss-text-muted);
  cursor: pointer;
  position: relative;
}
.qsc-tab-close:hover {
  background: var(--qss-bg-card);
  color: var(--qss-text);
}
.qsc-tab-dot {
  display: none;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--qss-warning);
}
.qsc-tab-close.is-dirty .qsc-tab-dot {
  display: block;
}
.qsc-tab-close.is-dirty .qsc-tab-x {
  display: none;
}
.qsc-tab-close.is-dirty:hover .qsc-tab-dot {
  display: none;
}
.qsc-tab-close.is-dirty:hover .qsc-tab-x {
  display: block;
}
</style>
