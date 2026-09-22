<script setup lang="ts">
/**
 * QuantCanvas → General — the module's main section in the unified settings
 * modal (V3). Extracted from the old inline `sm-*` modal in the canvas page;
 * controls use the shared `.qsu-*` form language from shell.css.
 *
 * The old dark/light theme picker did not survive the move: the suite is
 * dark-monochrome by decision (2026-08-14) and the theme bridge paints every
 * module — a per-module light switch would fight it.
 */
import { useAppStore, SHELL_OPTIONS } from '#canvas-root/stores/app'
import type { ShellType, SidebarInitialWidth } from '#canvas-root/stores/app'
import { useBrowserStore } from '#canvas-root/stores/browser'
import type { SearchEngine } from '#canvas-root/shared/types'

const appStore = useAppStore()
const browserStore = useBrowserStore()

const SEARCH_ENGINES = [
  { value: 'google' as const, label: 'Google' },
  { value: 'duckduckgo' as const, label: 'DuckDuckGo' },
  { value: 'bing' as const, label: 'Bing' },
  { value: 'brave' as const, label: 'Brave Search' },
]

function setFontSize(delta: number) {
  appStore.setFontSize(Math.min(22, Math.max(11, appStore.fontSize + delta)))
}
</script>

<template>
  <div>
    <div class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">Font size</p>
        <p class="qsu-hint">Base size for the canvas UI, 11–22px.</p>
      </div>
      <div class="cs-font">
        <button class="qsu-btn cs-stepper" @click="setFontSize(-1)">&minus;</button>
        <span class="cs-font-value">{{ appStore.fontSize }}px</span>
        <button class="qsu-btn cs-stepper" @click="setFontSize(1)">+</button>
      </div>
    </div>

    <div class="qsu-row">
      <p class="qsu-label">Code minimap</p>
      <button
        class="qsu-toggle"
        :class="{ 'is-on': appStore.editorMinimap }"
        role="switch"
        :aria-checked="appStore.editorMinimap"
        @click="appStore.toggleEditorMinimap()"
      />
    </div>

    <div class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">Default terminal shell</p>
      </div>
      <select
        class="qsu-select"
        :value="appStore.defaultShell"
        @change="appStore.setDefaultShell(($event.target as HTMLSelectElement).value as ShellType)"
      >
        <option v-for="shell in SHELL_OPTIONS" :key="shell.value" :value="shell.value">{{ shell.label }}</option>
      </select>
    </div>

    <div class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">Browser search engine</p>
      </div>
      <select
        class="qsu-select"
        :value="browserStore.searchEngine"
        @change="browserStore.setSearchEngine(($event.target as HTMLSelectElement).value as SearchEngine)"
      >
        <option v-for="engine in SEARCH_ENGINES" :key="engine.value" :value="engine.value">{{ engine.label }}</option>
      </select>
    </div>

    <div class="qsu-row">
      <p class="qsu-label">Snap windows to grid</p>
      <button class="qsu-toggle" :class="{ 'is-on': appStore.snapToGrid }" role="switch" :aria-checked="appStore.snapToGrid" @click="appStore.toggleSnapToGrid()" />
    </div>
    <div class="qsu-row">
      <p class="qsu-label">Snap camera to grid</p>
      <button class="qsu-toggle" :class="{ 'is-on': appStore.snapCameraToGrid }" role="switch" :aria-checked="appStore.snapCameraToGrid" @click="appStore.toggleSnapCameraToGrid()" />
    </div>
    <div class="qsu-row">
      <p class="qsu-label">Canvas minimap</p>
      <button class="qsu-toggle" :class="{ 'is-on': appStore.canvasMinimap }" role="switch" :aria-checked="appStore.canvasMinimap" @click="appStore.toggleCanvasMinimap()" />
    </div>

    <div class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">Sidebar initial width</p>
      </div>
      <select
        class="qsu-select"
        :value="appStore.sidebarInitialWidth"
        @change="appStore.setSidebarInitialWidth(($event.target as HTMLSelectElement).value as SidebarInitialWidth)"
      >
        <option value="default">Default</option>
        <option value="wide">Wide</option>
      </select>
    </div>
    <div class="qsu-row">
      <p class="qsu-label">File explorer</p>
      <button class="qsu-toggle" :class="{ 'is-on': appStore.fileExplorerVisible }" role="switch" :aria-checked="appStore.fileExplorerVisible" @click="appStore.toggleFileExplorer()" />
    </div>
    <div class="qsu-row">
      <p class="qsu-label">Editor panel</p>
      <button class="qsu-toggle" :class="{ 'is-on': appStore.editorVisible }" role="switch" :aria-checked="appStore.editorVisible" @click="appStore.toggleEditor()" />
    </div>
  </div>
</template>

<style scoped>
.cs-font {
  display: flex;
  align-items: center;
  gap: 8px;
}
.cs-stepper {
  width: 28px;
  padding: 4px 0;
  text-align: center;
}
.cs-font-value {
  min-width: 42px;
  text-align: center;
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  color: var(--qss-text);
}
</style>
