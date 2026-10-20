<script setup lang="ts">
/**
 * The Collection — indicators from a catalog (the public
 * QuantScript-Collection-Public, a private repository, or a local checkout),
 * browsed, inspected with their five versions and installed with their
 * requirements into your library (plugin:script|collection_*). List on the left, the item on the right; both
 * scroll inside their card, the page never scrolls. Install all installs and
 * updates everything of the selected source with one click — no plan, no
 * dialog; the module's notice reports the outcome.
 */
import { inActiveKeepAliveTree } from '@quantsuite/core'
import { Download, RefreshCw, Settings } from 'lucide-vue-next'
import { useCatalogStore } from '#script/stores/catalog'
import { useWorkbenchStore } from '#script/stores/workbench'
import { shortCommit } from '#script/utils/catalog'

definePageMeta({ layout: 'script' })

const store = useCatalogStore()
const wb = useWorkbenchStore()

const installingAll = computed(() => store.busy === 'install')
const installAllTitle = computed(() => {
  const n = store.installAll.length
  if (!n) return 'Everything in this catalog is installed and current'
  const updates = store.installAll.filter((i) => i.local?.installed_version).length
  return `${n - updates} new · ${updates} update(s) from ${store.source?.name ?? 'this catalog'}`
})

/**
 * Installs every missing or outdated item at once. Same-name files are
 * replaced (overwrite) — their current content stays in the history, so
 * Restore brings it back. Still all or nothing: the engine checks the staged
 * library before anything is written.
 */
async function installAll() {
  const keys = store.installAll.map((i) => i.key)
  if (!keys.length || store.busy) return
  const source = store.source?.name ?? 'the catalog'
  try {
    const outcome = await store.install(keys, true)
    const created = outcome.written.filter((w) => w.created).length
    const replaced = outcome.written.length - created
    const parts = [
      created ? `${created} new` : '',
      replaced ? `${replaced} replaced` : '',
      // Identical files that were already there: now recorded as installed.
      outcome.adopted.length ? `${outcome.adopted.length} already in your library (identical, now recorded)` : '',
    ].filter(Boolean)
    const restart = outcome.written.length ? ' Restart a running QuantSystems engine to use them there.' : ''
    wb.setNotice(`Installed all from ${source}: ${parts.join(', ') || 'nothing to do'}.${restart}`, 'ok', 8000)
  } catch (err) {
    wb.setNotice(`Install all failed — nothing was written. ${String(err)}`, 'error', 12000)
  }
  // The tree and the library list the new scripts.
  void wb.loadListing(true)
}

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
        <button
          v-if="store.catalog"
          class="qsc-btn is-sm is-primary"
          :disabled="!store.installAll.length || !!store.busy || store.catalogLoading"
          :title="installAllTitle"
          @click="installAll"
        >
          <template v-if="installingAll"><Download :size="13" class="qsc-pulse" /> Installing…</template>
          <template v-else><Download :size="13" /> Install all<span v-if="store.installAll.length" class="mono"> ({{ store.installAll.length }})</span></template>
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
