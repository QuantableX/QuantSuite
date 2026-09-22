<script setup lang="ts">
import { FolderOpen, Plus, RotateCw, Search } from 'lucide-vue-next'
import { invoke } from '@tauri-apps/api/core'
import { useWorkbenchStore } from '#script/stores/workbench'
const wb = useWorkbenchStore()
const openingFolder = ref(false)

async function openScriptsFolder() {
  if (openingFolder.value) return
  openingFolder.value = true
  try {
    await invoke('plugin:script|open_scripts_folder')
  } catch (error) {
    wb.setNotice(`Could not open the scripts folder: ${String(error)}`, 'error')
  } finally {
    openingFolder.value = false
  }
}
</script>
<template>
  <div class="qsc-sb-head">
    <div class="qsc-sb-title">
      <span>Workspace</span
      ><span class="qsc-sb-tools">
        <button
          class="qsc-icon-btn"
          aria-label="Open scripts folder"
          :title="wb.listing?.indicators_dir ? `Open scripts folder: ${wb.listing.indicators_dir}` : 'Open scripts folder'"
          :disabled="openingFolder"
          @click="openScriptsFolder"
        >
          <FolderOpen :size="15" />
        </button>
        <button
          class="qsc-icon-btn"
          aria-label="Reload scripts"
          title="Reload scripts"
          :disabled="wb.listingLoading"
          @click="wb.loadListing(true)"
        >
          <RotateCw :size="14" :class="{ 'qsc-pulse': wb.listingLoading }" />
        </button>
        <button class="qsc-icon-btn" aria-label="New script" title="New script" @click="wb.newScriptOpen = true">
          <Plus :size="16" />
        </button>
      </span>
    </div>
    <label class="qsc-sb-search"
      ><Search :size="14" /><input
        v-model="wb.search"
        type="search"
        placeholder="Find a script or indicator…"
        aria-label="Filter scripts"
        spellcheck="false"
    /></label>
  </div>
</template>
<style scoped>
.qsc-sb-head {
  width: 100%;
}
.qsc-sb-head {
  padding: 12px 12px 10px;
  border-bottom: 1px solid var(--qss-border);
}
.qsc-sb-title {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 10px;
  font-size: 12px;
  font-weight: 600;
}
.qsc-sb-tools {
  display: flex;
  gap: 3px;
}
.qsc-sb-search {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 0 8px;
  height: 32px;
  color: var(--qss-text-muted);
  border: 1px solid var(--qss-border);
  border-radius: 6px;
  background: var(--qss-bg);
}
.qsc-sb-search:focus-within {
  border-color: var(--qss-text-muted);
}
.qsc-sb-search input {
  width: 100%;
  min-width: 0;
  background: transparent;
  border: 0;
  outline: none;
  font-size: 11px;
}
.qsc-sb-search svg {
  flex-shrink: 0;
}
</style>
