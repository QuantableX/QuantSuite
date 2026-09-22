<script setup lang="ts">
/**
 * The vault's Output folder: every gauntlet report, newest first, rendered
 * in the app. The reports are the forge's own markdown (dated per-axis
 * tables and the verdict), written by the gauntlet — never edited here.
 * Rendered through the suite's sanitised preview, like every markdown that
 * comes from a folder on disk.
 */
import { useForgeStore } from '#script/stores/forge'
import { formatP } from '#script/utils/forge'
import { gradeClass } from '#script/utils/format'

const forge = useForgeStore()
const filter = ref('')

const list = computed(() => {
  const q = filter.value.trim().toLowerCase()
  const all = forge.reports
  return q ? all.filter((r) => r.name.toLowerCase().includes(q)) : all
})

const selected = computed(() => forge.selectedReport)
const markdown = computed(() => (selected.value ? (forge.reportMarkdown[selected.value] ?? null) : null))

function open(name: string) {
  void forge.loadReport(name)
}

// The first visit shows the newest report.
watch(
  () => forge.reports,
  (reports) => {
    if (!forge.selectedReport && reports.length) open(reports[0]!.name)
  },
  { immediate: true },
)
</script>

<template>
  <div class="qsf-reports">
    <div class="qsc-card qsf-list">
      <input v-model="filter" class="qsc-input" type="search" placeholder="Filter by name…" spellcheck="false" aria-label="Filter reports" />
      <div v-if="!forge.reports.length" class="muted qsf-empty">
        {{ forge.infoLoading ? 'Reading the vault…' : 'No reports in the vault yet — run a gauntlet.' }}
      </div>
      <button
        v-for="r in list"
        :key="r.name"
        class="qsf-item"
        :class="{ 'is-active': selected === r.name }"
        @click="open(r.name)"
      >
        <span class="qsf-item-name">{{ r.indicator ?? r.name }}</span>
        <span class="qsf-item-meta mono">
          {{ r.date ?? '' }}
          <span v-if="r.timeframe && r.timeframe !== '1d'" class="qsc-tag qsf-item-tf">{{ r.timeframe }}</span>
          <template v-if="r.score != null"> · {{ r.score.toFixed(0) }}</template>
          <span v-if="r.grade" class="qsc-grade qsf-item-grade" :class="gradeClass(r.grade, r.certified ?? false)">{{ r.grade.charAt(0) }}</span>
          <template v-if="r.perm_p != null"> p {{ formatP(r.perm_p) }}</template>
          <span v-if="r.fast" class="muted"> · fast</span>
        </span>
        <span v-if="r.certified" class="qsf-item-cert" title="Certified on this track">✓</span>
      </button>
    </div>

    <div class="qsc-card qsf-body">
      <div v-if="!selected" class="muted qsf-empty">Pick a report.</div>
      <div v-else-if="forge.reportLoading === selected" class="muted qsf-empty qsc-pulse">Reading…</div>
      <div v-else-if="forge.reportError && !markdown" class="qsc-err qsf-empty">{{ forge.reportError }}</div>
      <QMarkdownPreview v-else :source="markdown ?? ''" tokens="qss" />
    </div>
  </div>
</template>

<style scoped>
.qsf-reports {
  display: grid;
  grid-template-columns: 300px minmax(0, 1fr);
  gap: 12px;
  align-items: start;
}
.qsf-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 10px;
  max-height: 640px;
  overflow-y: auto;
}
.qsf-list .qsc-input {
  margin-bottom: 6px;
}
.qsf-empty {
  padding: 24px 8px;
  font-size: 13px;
  text-align: center;
}
.qsf-item {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 2px;
  position: relative;
  width: 100%;
  padding: 7px 10px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text-secondary);
  text-align: left;
  cursor: pointer;
}
.qsf-item:hover {
  background: var(--qss-bg-hover);
}
.qsf-item.is-active {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}
.qsf-item-name {
  font-size: 12.5px;
  font-weight: 600;
}
.qsf-item-meta {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
  color: var(--qss-text-muted);
}
.qsf-item-tf {
  height: 16px;
  padding: 0 5px;
}
.qsf-item-grade {
  min-width: 18px;
  height: 16px;
  padding: 0 4px;
  font-size: 10px;
}
.qsf-item-cert {
  position: absolute;
  top: 8px;
  right: 10px;
  font-size: 12px;
  color: var(--qss-success);
}
.qsf-body {
  min-height: 300px;
  padding: 18px 22px;
  overflow-x: auto;
}
</style>
