<script setup lang="ts">
import { Check, ChevronRight, FlaskConical, Play, Save, Star } from 'lucide-vue-next'
import { useWorkbenchStore } from '#script/stores/workbench'
import { useForgeStore } from '#script/stores/forge'
const wb = useWorkbenchStore()
const forge = useForgeStore()
const router = useRouter()
const busy = computed(() => !!wb.active?.saving || !!wb.active?.checking)
const indicatorKeys = computed(() => wb.activeEntry?.classes.flatMap((c) => (c.key ? [c.key] : [])) ?? [])
function openForge() {
  if (!wb.active || wb.isDirty(wb.active.file) || !indicatorKeys.value.length || busy.value || wb.listingLoading) return
  forge.openForge({ kind: 'gauntlet', indicators: indicatorKeys.value, fast: false, timeframe: 'all' })
  void router.push('/script/forge')
}
const state = computed(() => {
  if (wb.active?.saving) return 'Saving…'
  if (wb.active?.checking) return 'Checking…'
  if (wb.checkStale) return 'Code changed · check again'
  if (wb.active?.check)
    return wb.active.check.ok
      ? `${wb.active.check.depth === 'quick' ? 'Quick check' : 'Check'} passed`
      : wb.active.check.blocking
        ? 'Check blocked'
        : 'Validation failed'
  return wb.active?.editable ? 'Ready to check' : 'Read-only reference'
})
</script>
<template>
  <div v-if="wb.active" class="qsc-commandbar">
    <div class="qsc-command-file">
      <button class="qsc-breadcrumb" @click="wb.showLibrary()">Library</button><ChevronRight :size="12" /><strong
        class="mono"
        :title="wb.active.path"
        >{{ wb.active.file }}</strong
      ><button
        class="qsc-icon-btn"
        :aria-label="`Favorite ${wb.active.file}`"
        :aria-pressed="wb.favorites.includes(wb.active.file)"
        @click="wb.toggleFavorite(wb.active.file)"
      >
        <Star :size="13" :fill="wb.favorites.includes(wb.active.file) ? 'currentColor' : 'none'" />
      </button>
    </div>
    <div class="qsc-command-actions">
      <template v-if="wb.active.editable">
        <button
          class="qsc-btn"
          :disabled="busy || !wb.python?.ok"
          title="Syntax, import, contract, causality and scale invariance · Ctrl+Enter"
          @click="wb.check()"
        >
          <Play :size="12" />{{ wb.active.checking ? 'Checking…' : 'Run check' }}
        </button>
        <button
          class="qsc-btn is-primary"
          :disabled="busy || !wb.isDirty(wb.active.file)"
          title="Save a checked version · Ctrl+S"
          @click="wb.save()"
        >
          <Save :size="13" />{{ wb.active.saving ? 'Saving…' : 'Save' }}
        </button>
        <button
          v-if="indicatorKeys.length"
          class="qsc-btn"
          :disabled="busy || wb.listingLoading || wb.isDirty(wb.active.file)"
          :title="
            wb.isDirty(wb.active.file)
              ? 'Save your changes before validating them in the forge'
              : 'Set up a test for the indicators in this script'
          "
          @click="openForge"
        >
          <FlaskConical :size="13" /> Forge
        </button>
      </template>
    </div>
    <div class="qsc-command-meta">
      <button
        :class="{ 'qsc-warn': wb.checkStale, 'qsc-ok': wb.active.check?.ok && !wb.checkStale }"
        @click="wb.showResults('check')"
      >
        <Check v-if="wb.active.check?.ok && !wb.checkStale" :size="12" />{{ state }}</button
      ><span v-if="wb.active.version">v{{ wb.active.version.version }}</span
      ><span v-if="wb.isDirty(wb.active.file)" class="qsc-warn">Unsaved changes</span><span v-else>Saved</span
      ><span v-if="wb.activeEntry?.registered" class="qsc-command-indicators"
        >{{ wb.activeEntry.registered }} registered
        {{ wb.activeEntry.registered === 1 ? 'indicator' : 'indicators' }}</span
      >
    </div>
  </div>
</template>
<style scoped>
.qsc-commandbar {
  flex-shrink: 0;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 7px 12px;
  padding: 13px 16px 10px;
  border-bottom: 1px solid var(--qss-border);
}
.qsc-command-file {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  color: var(--qss-text-muted);
  flex: 1;
}
.qsc-command-file strong {
  color: var(--qss-text);
  font-size: 12px;
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.qsc-breadcrumb {
  color: var(--qss-text-muted);
  font-size: 12px;
  border: 0;
  background: none;
  padding: 0;
  cursor: pointer;
}
.qsc-breadcrumb:hover {
  color: var(--qss-text);
}
.qsc-command-actions {
  display: flex;
  align-items: center;
  gap: 7px;
  flex-shrink: 0;
}
.qsc-command-meta {
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
  color: var(--qss-text-muted);
  font-size: 10px;
}
.qsc-command-meta button {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  border: 0;
  background: none;
  padding: 0;
  cursor: pointer;
  font-size: 10px;
}
.qsc-command-indicators {
  margin-left: auto;
}
@container (max-width: 620px) {
  .qsc-command-file {
    flex-basis: 100%;
  }
  .qsc-commandbar {
    padding: 8px 12px;
  }
  .qsc-command-indicators {
    display: none;
  }
}
</style>
