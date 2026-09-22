<script setup lang="ts">
import { ArrowRight, BookOpen, FileCode2, FlaskConical, Plus } from 'lucide-vue-next'
import { useWorkbenchStore } from '#script/stores/workbench'
const wb = useWorkbenchStore()
function attention() {
  wb.search = ''
  wb.libraryFilter = 'attention'
}
</script>

<template>
  <div class="qsc-workspace-overview">
    <section class="qsc-panel">
      <h3 class="qsc-panel-title">Library</h3>
      <div v-if="wb.listing" class="qsc-workspace-stats">
        <div><strong>{{ wb.filterCounts.custom }}</strong><span>Indicator scripts</span></div>
        <div><strong>{{ wb.indicatorCount }}</strong><span>Indicators</span></div>
      </div>
      <p v-else class="qsc-help">{{ wb.listingLoading ? 'Loading…' : 'Library unavailable.' }}</p>
      <button v-if="wb.filterCounts.attention" class="qsc-attention-link" @click="attention">
        <span>{{ wb.filterCounts.attention }} {{ wb.filterCounts.attention === 1 ? 'script needs' : 'scripts need' }} attention</span><ArrowRight :size="13" />
      </button>
    </section>
    <section v-if="wb.dirtyFiles.length" class="qsc-panel">
      <h3 class="qsc-panel-title">Unsaved changes <span>{{ wb.dirtyFiles.length }}</span></h3>
      <button v-for="file in wb.dirtyFiles" :key="file" class="qsc-workspace-file" @click="wb.openScript(file)">
        <span class="qsc-dot is-warn" /><span class="mono">{{ file }}</span><ArrowRight :size="13" />
      </button>
    </section>
    <section class="qsc-panel">
      <h3 class="qsc-panel-title">{{ wb.recentScripts.length ? 'Recent' : 'Create' }}</h3>
      <button v-for="script in wb.recentScripts" :key="script.file" class="qsc-workspace-file" @click="wb.openScript(script.file)">
        <FileCode2 :size="14" /><span class="mono">{{ script.file }}</span><ArrowRight :size="13" />
      </button>
      <button class="qsc-workspace-action" @click="wb.newScriptOpen = true">
        <Plus :size="16" /><span><strong>New indicator</strong><small>EMA template</small></span><ArrowRight :size="13" />
      </button>
    </section>
    <section class="qsc-panel">
      <h3 class="qsc-panel-title">Resources</h3>
      <button class="qsc-workspace-action" @click="wb.showInspector('guide')">
        <BookOpen :size="16" /><span><strong>Script guide</strong><small>Signal contract, registration & shortcuts</small></span><ArrowRight :size="13" />
      </button>
      <NuxtLink to="/script/forge" class="qsc-workspace-action">
        <FlaskConical :size="16" /><span><strong>Validate in the Forge</strong><small>Test saved indicators on market data</small></span><ArrowRight :size="13" />
      </NuxtLink>
    </section>
  </div>
</template>

<style scoped>
.qsc-workspace-overview { display: flex; flex-direction: column; gap: 22px; }
.qsc-help { font-size: 11px; line-height: 1.7; color: var(--qss-text-muted); }
.qsc-workspace-stats { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; margin: 8px 0; }
.qsc-workspace-stats > div { padding: 12px; border: 1px solid var(--qss-border); border-radius: 7px; background: var(--qss-bg); }
.qsc-workspace-stats strong { display: block; font-size: 22px; font-weight: 500; line-height: 1.3; }
.qsc-workspace-stats span { font-size: 10px; color: var(--qss-text-muted); }
.qsc-workspace-file, .qsc-attention-link { display: flex; align-items: center; gap: 9px; width: 100%; padding: 8px 4px; border: 0; background: none; text-align: left; cursor: pointer; font-size: 11px; }
.qsc-workspace-file span:nth-child(2), .qsc-attention-link span { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qsc-workspace-file svg, .qsc-workspace-action svg { flex-shrink: 0; color: var(--qss-text-muted); }
.qsc-attention-link { color: var(--qss-warning); }
.qsc-workspace-action { display: flex; align-items: center; gap: 10px; border: 1px solid var(--qss-border-subtle); border-radius: 7px; padding: 11px 9px; text-align: left; background: var(--qss-bg); text-decoration: none; cursor: pointer; }
.qsc-workspace-action > span { flex: 1; min-width: 0; }
.qsc-workspace-action strong { display: block; font-weight: 500; font-size: 11px; }
.qsc-workspace-action small { display: block; color: var(--qss-text-muted); font-size: 10px; line-height: 1.5; margin-top: 4px; }
.qsc-workspace-action:hover, .qsc-workspace-file:hover { background: var(--qss-bg-hover); }
</style>
