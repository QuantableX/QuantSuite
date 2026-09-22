<script setup lang="ts">
import { BookOpen, Code2, FlaskConical, PanelLeft, PanelRight, Scan, Shrink } from 'lucide-vue-next'
import { useForgeStore } from '#script/stores/forge'
import { useWorkbenchStore } from '#script/stores/workbench'
const wb = useWorkbenchStore()
const forge = useForgeStore()
const route = useRoute()
const onForge = computed(() => route.path.startsWith('/script/forge'))
const editing = computed(() => !onForge.value && !!wb.active && !wb.libraryOpen)
function togglePanel(side: 'left' | 'right') {
  wb.toggleSidebar(side)
}
</script>

<template>
  <div class="qsc-toolbar">
    <nav class="qsc-places" aria-label="QuantScript">
      <NuxtLink
        to="/script"
        class="qsc-place"
        :class="{ 'is-active': !onForge }"
        :aria-current="!onForge ? 'page' : undefined"
        ><Code2 :size="14" /> Scripts</NuxtLink
      >
      <NuxtLink
        to="/script/forge"
        class="qsc-place"
        :class="{ 'is-active': onForge }"
        :aria-current="onForge ? 'page' : undefined"
        ><FlaskConical :size="14" /> Forge<span v-if="forge.isRunning" class="qsc-dot is-ok qsc-pulse"
      /></NuxtLink>
    </nav>
    <span class="qsc-toolbar-context">{{
      onForge ? 'Test & validate' : editing ? 'Indicator editor' : 'Indicator library'
    }}</span>
    <div class="qsc-toolbar-actions">
      <span class="qsc-runtime" :title="wb.python?.error || wb.python?.command || 'Resolving Python…'"
        ><span class="qsc-dot" :class="wb.python ? (wb.python.ok ? 'is-ok' : 'is-bad') : ''" />{{
          wb.python?.ok ? 'Python ready' : wb.python ? 'Python unavailable' : 'Connecting…'
        }}</span
      >
      <button
        class="qsc-icon-btn"
        :aria-pressed="wb.sidebarLeftOpen && (!wb.focusMode || onForge)"
        aria-label="Toggle script sidebar"
        title="Script sidebar · Ctrl+B"
        @click="togglePanel('left')"
      >
        <PanelLeft :size="16" />
      </button>
      <button
        class="qsc-icon-btn"
        :aria-pressed="wb.sidebarRightOpen && (!wb.focusMode || onForge)"
        aria-label="Toggle inspector"
        title="Inspector · Ctrl+Shift+B"
        @click="togglePanel('right')"
      >
        <PanelRight :size="16" />
      </button>
      <button
        v-if="editing"
        class="qsc-icon-btn"
        :aria-pressed="wb.focusMode"
        :aria-label="wb.focusMode ? 'Exit focus mode' : 'Focus editor'"
        title="Focus editor"
        @click="wb.focusMode = !wb.focusMode"
      >
        <component :is="wb.focusMode ? Shrink : Scan" :size="16" />
      </button>
      <button
        v-if="editing"
        class="qsc-icon-btn"
        aria-label="Open script guide"
        title="Script guide"
        @click="wb.showInspector('guide')"
      >
        <BookOpen :size="16" />
      </button>
    </div>
  </div>
</template>

<style scoped>
.qsc-toolbar svg {
  flex-shrink: 0;
}
.qsc-toolbar {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 0 12px;
}
.qsc-places {
  display: flex;
  gap: 3px;
  padding: 3px;
  border: 1px solid var(--qss-border);
  border-radius: 8px;
  flex-shrink: 0;
}
.qsc-place {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 4px 10px;
  border-radius: 5px;
  color: var(--qss-text-secondary);
  font-size: 12px;
  text-decoration: none;
}
.qsc-place.is-active {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}
.qsc-toolbar-context {
  color: var(--qss-text-muted);
  font-size: 12px;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.qsc-toolbar-actions {
  display: flex;
  align-items: center;
  gap: 5px;
  margin-left: auto;
  flex-shrink: 0;
}
.qsc-runtime {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-right: 8px;
  color: var(--qss-text-muted);
  font-size: 11px;
}
@media (max-width: 1050px) {
  .qsc-toolbar-context,
  .qsc-runtime {
    display: none;
  }
}
</style>
