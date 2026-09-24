<script setup lang="ts">
/**
 * The catalog as a list: search, tag, state and a score filter on one
 * track; every row with its version, the Standard's and the general
 * version's score per track, and the library's state.
 */
import { Search } from 'lucide-vue-next'
import { useCatalogStore } from '#script/stores/catalog'
import { DEFAULT_STORE_FILTERS, STORE_TRACKS, allTags, itemState } from '#script/utils/catalog'
import type { StoreCatalogItem } from '#script/types'

const store = useCatalogStore()
const tags = computed(() => allTags(store.items))

const STATES = [
  { id: 'all', label: 'All' },
  { id: 'installed', label: 'Installed' },
  { id: 'update', label: 'Updates' },
  { id: 'available', label: 'Not installed' },
] as const

function count(id: string): number {
  if (id === 'installed') return store.installedCount
  if (id === 'update') return store.updateCount
  if (id === 'available') return store.items.length - store.installedCount
  return store.items.length
}

function score(item: StoreCatalogItem, role: string, track: string): string {
  const v = item.scores?.[role]?.[track]
  return v ? v.score.toFixed(0) : '–'
}

function certified(item: StoreCatalogItem, role: string, track: string): boolean {
  return !!item.scores?.[role]?.[track]?.certified
}

function reset() {
  store.filters = { ...DEFAULT_STORE_FILTERS, track: store.filters.track }
}
</script>

<template>
  <section class="qst-list qsc-card is-flush" aria-label="Catalog">
    <div class="qst-filters">
      <div class="qst-states" role="tablist" aria-label="Library state">
        <button
          v-for="s in STATES"
          :key="s.id"
          role="tab"
          :aria-selected="store.filters.state === s.id"
          :class="{ 'is-active': store.filters.state === s.id }"
          @click="store.filters.state = s.id"
        >
          {{ s.label }}<span class="mono">{{ count(s.id) }}</span>
        </button>
      </div>
      <label class="qst-search">
        <Search :size="14" />
        <input v-model="store.filters.search" type="search" placeholder="Search indicators…" aria-label="Search the catalog" spellcheck="false" />
      </label>
      <div class="qst-filter-row">
        <select v-model="store.filters.tag" class="qsc-select" aria-label="Tag">
          <option value="">All tags</option>
          <option v-for="t in tags" :key="t" :value="t">{{ t }}</option>
        </select>
        <select v-model="store.filters.track" class="qsc-select" aria-label="Score track">
          <option v-for="t in STORE_TRACKS" :key="t" :value="t">{{ t }} track</option>
        </select>
        <label class="qst-min" title="The best version's score on the chosen track">
          <span>Min score</span>
          <input v-model.number="store.filters.minScore" class="qsc-input mono" type="number" min="0" max="100" step="5" />
        </label>
        <label class="qsc-check"><input v-model="store.filters.certifiedOnly" type="checkbox" /> Certified</label>
      </div>
    </div>

    <div class="qst-rows">
      <button
        v-for="item in store.visible"
        :key="item.key"
        class="qst-row"
        :class="{ 'is-selected': store.selectedKey === item.key }"
        :aria-pressed="store.selectedKey === item.key"
        @click="store.openItem(item.key)"
      >
        <span class="qst-row-top">
          <span class="qst-row-name">{{ item.name }}</span>
          <span class="mono qst-row-version">{{ item.version }}</span>
        </span>
        <span class="qst-row-key"><span class="mono">{{ item.key }}</span><template v-if="item.summary"> · {{ item.summary }}</template></span>
        <span class="qst-row-bottom">
          <span v-if="item.type === 'library'" class="qst-scores muted">Library — needed by other indicators</span>
          <span v-else class="qst-scores" :title="'Standard / general version on 1h · 4h · 1d'">
            <span v-for="role in ['standard', 'optimized']" :key="role" class="qst-score-group">
              <span class="qst-role">{{ role === 'standard' ? 'S' : 'G' }}</span>
              <span v-for="tf in STORE_TRACKS" :key="tf" class="mono" :class="{ 'is-ok': certified(item, role, tf) }">{{ score(item, role, tf) }}</span>
            </span>
          </span>
          <span class="qsc-chip" :class="itemState(item.local, item.version).tone" :title="itemState(item.local, item.version).title">
            {{ itemState(item.local, item.version).label }}
          </span>
        </span>
      </button>
      <div v-if="!store.visible.length" class="qst-none">
        <p>No indicator matches these filters.</p>
        <button class="qsc-btn is-sm" @click="reset">Reset filters</button>
      </div>
    </div>
    <p class="qst-foot">
      <span class="mono">{{ store.visible.length }}</span> of <span class="mono">{{ store.items.length }}</span> · scores: S Standard, G general version, 1h · 4h · 1d
    </p>
  </section>
</template>

<style scoped>
.qst-list {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
}
.qst-filters {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 12px 14px 10px;
  border-bottom: 1px solid var(--qss-border-subtle);
}
.qst-states {
  display: flex;
  gap: 16px;
  flex-wrap: wrap;
}
.qst-states button {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 0 6px;
  border: 0;
  border-bottom: 2px solid transparent;
  background: transparent;
  font-size: 12px;
  color: var(--qss-text-muted);
  cursor: pointer;
}
.qst-states button.is-active {
  border-bottom-color: var(--qss-text);
  color: var(--qss-text);
}
.qst-states button span {
  font-size: 10.5px;
  padding: 0 5px;
  border-radius: 4px;
  background: var(--qss-bg-hover);
  color: var(--qss-text-secondary);
}
.qst-search {
  display: flex;
  align-items: center;
  gap: 8px;
  border: 1px solid var(--qss-border);
  border-radius: 7px;
  padding: 0 10px;
  color: var(--qss-text-muted);
}
.qst-search:focus-within {
  border-color: var(--qss-text-muted);
}
.qst-search input {
  flex: 1;
  min-width: 0;
  height: 30px;
  border: 0;
  outline: 0;
  background: none;
  font-size: 12px;
}
.qst-filter-row {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  font-size: 12px;
}
.qst-min {
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--qss-text-muted);
}
.qst-min input {
  width: 58px;
}
.qst-rows {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 6px;
}
.qst-row {
  display: flex;
  flex-direction: column;
  gap: 4px;
  width: 100%;
  padding: 9px 10px;
  border: 1px solid transparent;
  border-radius: 7px;
  background: transparent;
  text-align: left;
  cursor: pointer;
}
.qst-row:hover {
  background: var(--qss-bg-hover);
}
.qst-row.is-selected {
  border-color: var(--qss-border);
  background: var(--qss-bg-hover);
}
.qst-row-top {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 8px;
}
.qst-row-name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 13px;
  font-weight: 600;
  color: var(--qss-text);
}
.qst-row-version {
  flex-shrink: 0;
  font-size: 11px;
  color: var(--qss-text-muted);
}
.qst-row-key {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 11.5px;
  color: var(--qss-text-muted);
}
.qst-row-bottom {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.qst-scores {
  display: flex;
  gap: 12px;
  font-size: 11px;
  color: var(--qss-text-secondary);
  min-width: 0;
}
.qst-score-group {
  display: inline-flex;
  gap: 6px;
}
.qst-score-group .is-ok {
  color: var(--qss-success);
}
.qst-role {
  color: var(--qss-text-muted);
}
.qst-none {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 30px 10px;
  font-size: 12px;
  color: var(--qss-text-muted);
}
.qst-foot {
  padding: 8px 14px;
  border-top: 1px solid var(--qss-border-subtle);
  font-size: 11px;
  color: var(--qss-text-muted);
}
</style>
