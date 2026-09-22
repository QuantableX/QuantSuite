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
    <QPageHeading title="Forge">
      <div class="qsf-actions">
        <span v-if="forge.registry" class="qsc-chip">{{ forge.certified.length }} overall passes / {{ forge.indicators.length }}</span>
        <span v-if="forge.isRunning" class="qsc-chip is-warn qsc-pulse">forge running</span>
        <button class="qsc-btn is-sm" :disabled="forge.registryLoading || forge.infoLoading" @click="reload">
          {{ forge.registryLoading || forge.infoLoading ? 'Reading…' : 'Reload' }}
        </button>
      </div>
    </QPageHeading>
    <div class="qsf-overview"><button @click="forge.tab = 'roster'"><strong>{{ forge.indicators.length }}</strong><span>Indicators</span></button><button @click="forge.tab = 'reports'"><strong>{{ forge.reports.length }}</strong><span>Reports</span></button><button @click="forge.tab = 'shelf'"><strong>{{ forge.shelf.length }}</strong><span>Market series</span></button><div><strong>{{ forge.isRunning ? 'Running' : 'Ready' }}</strong><span>{{ forge.isRunning ? 'A test is in progress' : 'Select indicators to begin' }}</span></div></div>

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

    <div v-if="forge.registryError" class="qsc-note is-error qsf-error">
      <p>The registry could not be read.</p>
      <pre class="qsc-pre">{{ forge.registryError }}</pre>
      <p class="muted">The forge needs numpy, pandas and scipy for the Python interpreter set in QuantAlgo's settings (<span class="mono">sidecars/python/requirements.txt</span>).</p>
    </div>
    <div v-else-if="forge.infoError" class="qsc-note is-error qsf-error">
      <p>The vault could not be read.</p>
      <pre class="qsc-pre">{{ forge.infoError }}</pre>
    </div>

    <ScriptForgeRoster v-show="forge.tab === 'roster'" />
    <ScriptForgeJobs v-show="forge.tab === 'forge'" />
    <ScriptForgeReports v-show="forge.tab === 'reports'" />
    <ScriptForgeShelf v-show="forge.tab === 'shelf'" />
  </div>
</template>

<style scoped>
.qsf-page {
  height: 100%;
  overflow-y: auto;
  padding: 28px 30px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.qsf-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  justify-content: flex-end;
}
.qsf-overview { display: grid; grid-template-columns: repeat(4, 1fr); border: 1px solid var(--qss-border); border-radius: 9px; background: var(--qss-bg-raised); flex-shrink: 0; }
.qsf-overview > * { display: flex; flex-direction: column; align-items: flex-start; gap: 5px; padding: 16px 20px; border: 0; border-right: 1px solid var(--qss-border); background: none; text-align: left; }
.qsf-overview > button { cursor: pointer; }
.qsf-overview > button:hover { background: var(--qss-bg-hover); }
.qsf-overview > :last-child { border-right: none; }
.qsf-overview strong { font-size: 20px; font-weight: 500; }
.qsf-overview span { font-size: 11px; color: var(--qss-text-muted); }
.qsf-page > .qsc-tabbar { align-self: flex-start; }
@media (max-width: 1100px) { .qsf-overview > * { padding: 12px; } }
.qsf-error {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
</style>
