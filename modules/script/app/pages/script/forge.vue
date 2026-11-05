<script setup lang="ts">
/**
 * The forge — the Indicator Smithery's certification side, next to the
 * scripts it certifies (docs/PLAN-QUANTSCRIPT.md §2; moved here from
 * QuantAlgo on 2026-09-12): the registry's roster with the one score per
 * indicator, the jobs (gauntlet, walk-forward, comparison, shelf refresh),
 * the vault's reports and the price shelf. One page, four tabs.
 */
import { inActiveKeepAliveTree } from '@quantsuite/core'
import { useForgeStore, type ForgeTab } from '#script/stores/forge'

definePageMeta({ layout: 'script' })

const forge = useForgeStore()

const tabs: { id: ForgeTab; label: string }[] = [
  { id: 'research', label: 'Research' },
  { id: 'roster', label: 'Indicators' },
  { id: 'forge', label: 'Run tests' },
  { id: 'reports', label: 'Reports' },
  { id: 'shelf', label: 'Market data' },
]

function reload() {
  void forge.loadRegistry(true)
  void forge.loadInfo(true)
}

// V3 warm cache: the page stays mounted while another module is active —
// the store polls only while it is on screen.
function activate() {
  if (!forge.registry) void forge.loadRegistry()
  if (!forge.info) void forge.loadInfo()
  void forge.resume()
}

onMounted(() => {
  if (inActiveKeepAliveTree()) activate()
})
onActivated(activate)
onDeactivated(() => forge.pause())
onUnmounted(() => forge.pause())
</script>

<template>
  <div class="qsf-page">
    <!-- Title, tabs and actions on one row. -->
    <QPageHeading class="qsf-heading" title="Forge">
      <div class="qsc-tabbar" role="tablist" aria-label="The forge">
        <button
          v-for="t in tabs"
          :key="t.id"
          class="qsc-tabbtn"
          :class="{ 'is-active': forge.tab === t.id }"
          role="tab"
          :aria-selected="forge.tab === t.id"
          @click="forge.tab = t.id"
        >
          {{ t.label }}
          <span v-if="t.id === 'forge' && forge.isRunning" class="qsc-tab-live" />
        </button>
      </div>
      <div class="qsf-actions">
        <span v-if="forge.registry" class="qsc-chip">{{ forge.certified.length }} overall passes / {{ forge.indicators.length }}</span>
        <span v-if="forge.isRunning" class="qsc-chip is-warn qsc-pulse">forge running</span>
        <button class="qsc-btn is-sm" :disabled="forge.registryLoading || forge.infoLoading" @click="reload">
          {{ forge.registryLoading || forge.infoLoading ? 'Reading…' : 'Reload' }}
        </button>
      </div>
    </QPageHeading>
    <div class="qsf-overview"><button @click="forge.tab = 'roster'"><strong>{{ forge.indicators.length }}</strong><span>Indicators</span></button><button @click="forge.tab = 'reports'"><strong>{{ forge.reports.length }}</strong><span>Reports</span></button><button @click="forge.tab = 'shelf'"><strong>{{ forge.shelf.length }}</strong><span>Market series</span></button><div><strong>{{ forge.isRunning ? 'Running' : 'Ready' }}</strong><span>{{ forge.isRunning ? 'A test is in progress' : 'Select indicators to begin' }}</span></div></div>

    <div v-if="forge.registryError" class="qsc-note is-error qsf-error">
      <p>The registry could not be read.</p>
      <pre class="qsc-pre">{{ forge.registryError }}</pre>
      <p class="muted">The forge needs numpy, pandas and scipy for the Python interpreter set in QuantAlgo's settings (<span class="mono">sidecars/python/requirements.txt</span>).</p>
    </div>
    <div v-else-if="forge.infoError" class="qsc-note is-error qsf-error">
      <p>The vault could not be read.</p>
      <pre class="qsc-pre">{{ forge.infoError }}</pre>
    </div>

    <ScriptForgeResearch v-if="forge.tab === 'research'" />
    <ScriptForgeRoster v-show="forge.tab === 'roster'" />
    <ScriptForgeJobs v-show="forge.tab === 'forge'" />
    <ScriptForgeReports v-show="forge.tab === 'reports'" />
    <ScriptForgeShelf v-show="forge.tab === 'shelf'" />
  </div>
</template>

<style scoped>
.qsf-page {
  --qss-page-head-gap: 0px;
  height: 100%;
  overflow-y: auto;
  padding: var(--qss-page-edge);
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap);
  container-type: inline-size;
}
.qsf-page > * {
  flex-shrink: 0;
}
.qsf-heading :deep(.qph-actions) {
  flex: 1 1 auto;
  justify-content: space-between;
  gap: var(--qss-gap-tight) var(--qss-gap);
}
.qsf-actions {
  display: flex;
  align-items: center;
  gap: var(--qss-gap-tight);
  flex-wrap: wrap;
  justify-content: flex-end;
}
/* One slim strip: each figure beside its label. */
.qsf-overview { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); overflow: hidden; border: 1px solid var(--qss-border); border-radius: var(--qss-panel-radius); background: var(--qss-bg-raised); }
.qsf-overview > * { display: flex; align-items: baseline; flex-wrap: wrap; gap: 0 var(--qss-gap-tight); min-width: 0; padding: 6px var(--qss-panel-pad); border: 0; box-shadow: -1px 0 0 var(--qss-border), 0 -1px 0 var(--qss-border); background: none; text-align: left; }
.qsf-overview > button { cursor: pointer; }
.qsf-overview > button:hover { background: var(--qss-bg-hover); }
.qsf-overview strong { font-size: var(--qss-text-title); font-weight: 600; font-variant-numeric: tabular-nums; }
.qsf-overview span { font-size: var(--qss-text-meta); color: var(--qss-text-secondary); }
@container (max-width: 640px) { .qsf-overview { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
.qsf-error {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
</style>
