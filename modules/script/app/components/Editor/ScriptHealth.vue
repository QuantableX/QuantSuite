<script setup lang="ts">
import { ArrowRight, CircleAlert, CircleCheck, Clock3, History } from 'lucide-vue-next'
import { useWorkbenchStore } from '#script/stores/workbench'
const wb = useWorkbenchStore()
const diagnostics = computed(() => {
  const o = wb.active
  if (o?.lintError || !wb.python?.ok) return { text: 'Diagnostics unavailable', tone: 'warn' }
  if (o?.linting || !o?.lint) return { text: 'Checking syntax…', tone: 'pending' }
  const errors = o.lint.markers.filter(m => m.severity === 'error').length
  const warnings = o.lint.markers.length - errors
  return errors ? { text: `${errors} syntax errors`, tone: 'warn' }
    : warnings ? { text: `${warnings} warnings`, tone: 'warn' }
    : { text: 'No syntax problems', tone: 'ok' }
})
const validation = computed(() => {
  const o = wb.active
  if (o?.checking) return { text: 'Check running…', tone: 'pending' }
  if (wb.checkStale) return { text: 'Code changed · check again', tone: 'warn' }
  if (!o?.check) return { text: 'Not checked yet', tone: 'pending' }
  if (!o.check.ok) return { text: o.check.blocking ? 'Check blocked' : 'Validation failed', tone: 'warn' }
  return { text: o.check.depth === 'quick' ? 'Quick check passed' : 'Full check passed', tone: 'ok' }
})
function results(tab: 'problems' | 'check') {
  wb.showResults(tab)
}
</script>

<template>
  <section v-if="wb.active?.editable" class="qsc-health">
    <h3 class="qsc-panel-title">Current script</h3>
    <button class="qsc-health-row" @click="results('problems')">
      <component :is="diagnostics.tone === 'ok' ? CircleCheck : diagnostics.tone === 'warn' ? CircleAlert : Clock3" :size="14" :class="`is-${diagnostics.tone}`" />
      <span>{{ diagnostics.text }}</span><ArrowRight :size="12" />
    </button>
    <button class="qsc-health-row" @click="results('check')">
      <component :is="validation.tone === 'ok' ? CircleCheck : validation.tone === 'warn' ? CircleAlert : Clock3" :size="14" :class="`is-${validation.tone}`" />
      <span>{{ validation.text }}</span><ArrowRight :size="12" />
    </button>
    <button class="qsc-health-row" @click="wb.inspectorTab = 'history'">
      <History :size="14" /><span>{{ wb.active.saving ? 'Saving…' : wb.isDirty(wb.active.file) ? 'Unsaved changes' : 'All changes saved' }}</span>
      <span class="mono">{{ wb.active.version ? `v${wb.active.version.version}` : '—' }}</span>
    </button>
    <p v-if="wb.checkStale || !wb.active.check" class="qsc-health-hint">Run a full check before taking this indicator to the Forge.</p>
    <p v-else-if="wb.active.check.depth === 'quick'" class="qsc-health-hint">Quick checks exclude causality and scale invariance. Run a full check to test both.</p>
    <p v-else-if="wb.active.check.ok" class="qsc-health-hint">Code checks passed. Validate the saved indicator in the Forge to evaluate its evidence.</p>
  </section>
</template>

<style scoped>
.qsc-health { border: 1px solid var(--qss-border); border-radius: 8px; padding: 12px; background: var(--qss-bg); }
.qsc-health .qsc-panel-title { margin-bottom: 8px; }
.qsc-health-row { display: flex; align-items: center; width: 100%; gap: 8px; border: 0; background: none; padding: 7px 0; font-size: 11px; text-align: left; cursor: pointer; color: var(--qss-text-secondary); }
.qsc-health-row > span:first-of-type { flex: 1; }
.qsc-health-row svg { flex-shrink: 0; color: var(--qss-text-muted); }
.qsc-health-row svg.is-ok { color: var(--qss-success); }
.qsc-health-row svg.is-warn { color: var(--qss-warning); }
.qsc-health-row:hover { color: var(--qss-text); }
.qsc-health-hint { font-size: 10px; line-height: 1.6; color: var(--qss-text-muted); border-top: 1px solid var(--qss-border-subtle); padding-top: 9px; margin-top: 7px; }
</style>
