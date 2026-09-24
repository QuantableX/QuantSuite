<script setup lang="ts">
/**
 * The Collection — indicators from a catalog (the private QuantScript-Collection
 * repo, or a local checkout), browsed, inspected with their five versions
 * and installed with their requirements into your library
 * (plugin:script|store_*). List on the left, the item on the right; both
 * scroll inside their card, the page never scrolls.
 */
import { inActiveKeepAliveTree } from '@quantsuite/core'
import { RefreshCw, Settings } from 'lucide-vue-next'
import { useCatalogStore } from '#script/stores/catalog'
import { shortCommit } from '#script/utils/catalog'

definePageMeta({ layout: 'script' })

const store = useCatalogStore()

async function activate() {
  if (!store.sources.length && !store.sourcesLoading) await store.loadSources()
  if (!store.catalog && !store.catalogLoading) void store.loadCatalog(false)
}

function openSettings() {
  window.dispatchEvent(new CustomEvent('qss:settings', { detail: { module: 'script' } }))
}

function pickSource(event: Event) {
  store.selectSource((event.target as HTMLSelectElement).value)
}

onMounted(() => {
  if (inActiveKeepAliveTree()) void activate()
})
onActivated(() => void activate())
</script>

<template>
  <div class="qst-page">
    <QPageHeading title="Collection">
      <div class="qst-head">
        <select
          v-if="store.sources.length > 1"
          class="qsc-select"
          aria-label="Catalog source"
          :value="store.sourceId ?? ''"
          @change="pickSource"
        >
          <option v-for="s in store.sources" :key="s.id" :value="s.id" :disabled="!s.enabled">{{ s.name }}</option>
        </select>
        <span v-else-if="store.source" class="qst-source">{{ store.source.name }}</span>
        <span v-if="store.catalog" class="qsc-chip" :title="store.catalog.commit">
          <span class="mono">{{ shortCommit(store.catalog.commit) }}</span>
        </span>
        <button class="qsc-btn is-sm" :disabled="store.catalogLoading || !store.sourceId" title="Read the newest commit of the source" @click="store.loadCatalog(true)">
          <RefreshCw :size="13" /> {{ store.catalogLoading ? 'Reading…' : 'Refresh' }}
        </button>
        <button class="qsc-icon-btn" aria-label="Collection settings" title="Sources and tokens: Settings → Collection" @click="openSettings">
          <Settings :size="15" />
        </button>
      </div>
    </QPageHeading>

    <div v-if="store.sourcesError" class="qsc-note is-error" role="alert">{{ store.sourcesError }}</div>
    <div v-else-if="!store.sourcesLoading && !store.sources.length" class="qst-empty">
      <h2>No catalog source</h2>
      <p>Add a GitHub repository or a local catalog folder under Settings → Collection.</p>
      <button class="qsc-btn is-primary" @click="openSettings">Open settings</button>
    </div>
    <div v-else-if="store.catalogError" class="qsc-note is-error qst-error" role="alert">
      <strong>The catalog could not be read.</strong>
      <p>{{ store.catalogError }}</p>
      <div class="qst-error-actions">
        <button class="qsc-btn is-sm" :disabled="store.catalogLoading" @click="store.loadCatalog(true)">Try again</button>
        <button class="qsc-btn is-sm" @click="openSettings">Sources and tokens</button>
      </div>
    </div>

    <div v-if="store.catalog" class="qst-split">
      <ScriptCollectionList />
      <ScriptCollectionDetail />
    </div>
    <div v-else-if="store.catalogLoading" class="qst-empty qsc-pulse" role="status">Reading the catalog…</div>
  </div>
</template>

<style scoped>
.qst-page {
  height: 100%;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 22px clamp(16px, 3%, 30px);
  overflow: hidden;
}
.qst-head {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  justify-content: flex-end;
}
.qst-source {
  font-size: 12px;
  color: var(--qss-text-secondary);
}
.qst-split {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(300px, 5fr) minmax(0, 7fr);
  gap: 14px;
}
@container (max-width: 640px) {
  .qst-split {
    grid-template-columns: 1fr;
    grid-template-rows: minmax(0, 1fr) minmax(0, 1fr);
  }
}
.qst-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  padding: 48px 20px;
  text-align: center;
  font-size: 12px;
  color: var(--qss-text-muted);
}
.qst-empty h2 {
  font-size: 15px;
  color: var(--qss-text);
}
.qst-error {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.qst-error-actions {
  display: flex;
  gap: 6px;
}
</style>
