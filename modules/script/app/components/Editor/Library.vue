<script setup lang="ts">
import { ArrowRight, FileCode2, Plus, Search, Star, X } from 'lucide-vue-next'
import { useWorkbenchStore } from '#script/stores/workbench'
import { LIBRARY_FILTERS, scriptStatus, scriptTitle } from '#script/utils/library'
import { timeAgo } from '#script/utils/format'
const wb = useWorkbenchStore()
const sort = ref<'name' | 'recent'>('name')
function resetFilters() {
  wb.search = ''
  wb.libraryFilter = 'all'
}
const rows = computed(() =>
  wb.filtered
    .filter((s) => s.kind === 'script')
    .slice()
    .sort((a, b) =>
      sort.value === 'recent'
        ? (b.modified ?? '').localeCompare(a.modified ?? '') || a.file.localeCompare(b.file)
        : scriptTitle(a).localeCompare(scriptTitle(b)),
    ),
)
</script>

<template>
  <div class="qsc-library">
    <QPageHeading title="Indicator library">
      <button class="qsc-btn is-primary" @click="wb.newScriptOpen = true"><Plus :size="15" /> New script</button>
    </QPageHeading>
    <div class="qsc-library-controls">
      <div class="qsc-library-filters" aria-label="Library filters">
        <button
          v-for="f in LIBRARY_FILTERS"
          :key="f.id"
          :aria-pressed="wb.libraryFilter === f.id"
          :class="{ 'is-active': wb.libraryFilter === f.id }"
          @click="wb.libraryFilter = f.id"
        >
          {{ f.label }}<span>{{ wb.filterCounts[f.id] }}</span>
        </button>
      </div>
      <div class="qsc-library-search-row">
        <label class="qsc-library-search"
          ><Search :size="15" /><input
            v-model="wb.search"
            type="search"
            aria-label="Search indicator library"
            placeholder="Search names, files, indicators…"
            spellcheck="false"
        /></label>
        <select v-model="sort" class="qsc-select" aria-label="Sort scripts">
          <option value="name">Name A–Z</option>
          <option value="recent">Recently modified</option>
        </select>
      </div>
    </div>
    <div v-if="wb.listingError" class="qsc-note is-error" role="alert">
      <strong>Scripts could not be loaded.</strong>
      <p>{{ wb.listingError }}</p>
      <button class="qsc-btn is-sm" :disabled="wb.listingLoading" @click="wb.loadListing(true)">Try again</button>
    </div>
    <div v-else-if="!wb.listing && wb.listingLoading" class="qsc-library-loading" role="status">
      <span class="qsc-pulse">Loading your scripts and indicators…</span>
    </div>
    <template v-else-if="wb.listing">
      <div class="qsc-library-caption">
        <span
          >{{ rows.length }} {{ rows.length === 1 ? 'script' : 'scripts'
          }}<template v-if="wb.search"> matching “{{ wb.search }}”</template></span
        ><span>{{ wb.indicatorCount }} registered indicators</span>
      </div>
      <div v-if="rows.length" class="qsc-script-grid">
        <article
          v-for="entry in rows"
          :key="entry.file"
          class="qsc-script-card"
          :class="{ 'is-opening': wb.openingFile === entry.file }"
        >
          <div class="qsc-card-top">
            <FileCode2 :size="18" /><span>{{
              entry.registered ? 'Indicator script' : 'Utility'
            }}</span
            ><button
              class="qsc-icon-btn"
              :aria-label="`${wb.favorites.includes(entry.file) ? 'Unfavorite' : 'Favorite'} ${entry.file}`"
              :aria-pressed="wb.favorites.includes(entry.file)"
              @click="wb.toggleFavorite(entry.file)"
            >
              <Star :size="14" :fill="wb.favorites.includes(entry.file) ? 'currentColor' : 'none'" />
            </button>
          </div>
          <button class="qsc-card-open" :aria-label="`Open ${entry.file}`" @click="wb.openScript(entry.file)">
            <h2>{{ scriptTitle(entry) }}</h2>
            <span class="mono qsc-card-file">{{ entry.file }}</span>
            <p v-if="entry.summary">{{ entry.summary }}</p>
          </button>
          <div class="qsc-card-bottom">
            <span class="qsc-chip" :class="wb.isDirty(entry.file) ? 'is-warn' : scriptStatus(entry).tone">{{
              wb.isDirty(entry.file) ? 'Unsaved changes' : scriptStatus(entry).label
            }}</span
            ><span :title="`Modified ${timeAgo(entry.modified)}`"
              >{{ entry.registered }} {{ entry.registered === 1 ? 'indicator' : 'indicators' }}</span
            >
          </div>
        </article>
      </div>
      <div v-else-if="!wb.listing.scripts.some((script) => script.kind === 'script')" class="qsc-library-empty">
        <FileCode2 :size="25" />
        <h2>Your script library is empty</h2>
        <p>Add your Python indicator files to this folder, or create a new script.</p>
        <p class="mono" style="overflow-wrap: anywhere">{{ wb.listing.indicators_dir }}</p>
        <button class="qsc-btn" :disabled="wb.listingLoading" @click="wb.loadListing(true)">Refresh library</button>
      </div>
      <div v-else class="qsc-library-empty">
        <Search :size="25" />
        <h2>
          {{ wb.libraryFilter === 'favorites' && !wb.search ? 'No favorites yet' : 'No matching scripts' }}
        </h2>
        <p>
          {{
            wb.libraryFilter === 'favorites' && !wb.search
              ? 'Star a script to save it here.'
              : 'Try another search or switch to all scripts.'
          }}
        </p>
        <button class="qsc-btn" @click="resetFilters">
          <X :size="13" /> Reset filters
        </button>
      </div>
    </template>
    <footer class="qsc-library-foot">
      <NuxtLink to="/script/forge">Open Forge <ArrowRight :size="13" /></NuxtLink>
    </footer>
  </div>
</template>

<style scoped>
.qsc-library {
  height: 100%;
  overflow: auto;
  padding: 22px clamp(16px, 3%, 32px);
  container-type: inline-size;
}
.qsc-library-filters {
  display: flex;
  gap: 20px;
  border-bottom: 1px solid var(--qss-border);
  margin-bottom: 16px;
  flex-wrap: wrap;
}
.qsc-library-filters button {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 0 0 12px;
  font-size: 12px;
  background: transparent;
  border: 0;
  border-bottom: 2px solid transparent;
  color: var(--qss-text-muted);
  cursor: pointer;
}
.qsc-library-filters button.is-active {
  border-bottom-color: var(--qss-text);
  color: var(--qss-text);
}
.qsc-library-filters button span {
  font-size: 10px;
  padding: 0 5px;
  border-radius: 4px;
  background: var(--qss-bg-hover);
  color: var(--qss-text-secondary);
}
.qsc-library-search-row {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  margin-bottom: 18px;
}
.qsc-library-search {
  min-width: min(220px, 100%);
  display: flex;
  align-items: center;
  gap: 9px;
  flex: 1;
  border: 1px solid var(--qss-border);
  border-radius: 7px;
  padding: 0 11px;
  color: var(--qss-text-muted);
}
.qsc-library-search:focus-within {
  border-color: var(--qss-text-muted);
}
.qsc-library-search input {
  width: 100%;
  border: 0;
  outline: 0;
  background: none;
  min-width: 0;
  height: 34px;
  font-size: 12px;
}
.qsc-library-search-row select {
  height: 36px;
}
.qsc-library-caption {
  display: flex;
  justify-content: space-between;
  color: var(--qss-text-muted);
  font-size: 11px;
  margin-bottom: 12px;
}
.qsc-script-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(min(230px, 100%), 1fr));
  gap: 12px;
}
.qsc-script-card {
  min-width: 0;
  display: flex;
  flex-direction: column;
  border: 1px solid var(--qss-border);
  border-radius: 9px;
  background: var(--qss-bg-raised);
  overflow: hidden;
  transition: border-color 0.12s;
}
.qsc-script-card:hover,
.qsc-script-card.is-opening {
  border-color: var(--qss-text-muted);
}
.qsc-card-top {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 14px 14px 0;
  color: var(--qss-text-muted);
  font-size: 10px;
}
.qsc-card-top .qsc-icon-btn {
  margin-left: auto;
}
.qsc-card-open {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  justify-content: flex-start;
  text-align: left;
  border: 0;
  background: transparent;
  padding: 10px 16px 16px;
  cursor: pointer;
  flex: 1;
}
.qsc-card-open h2 {
  font-size: 14px;
  font-weight: 600;
  color: var(--qss-text);
  overflow-wrap: anywhere;
}
.qsc-card-file {
  display: block;
  font-size: 10px;
  color: var(--qss-text-muted);
  margin-top: 3px;
  overflow-wrap: anywhere;
}
.qsc-card-open p {
  margin-top: 12px;
  font-size: 12px;
  color: var(--qss-text-secondary);
  line-height: 1.65;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.qsc-card-bottom {
  padding: 10px 14px;
  border-top: 1px solid var(--qss-border-subtle);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
  font-size: 10px;
  color: var(--qss-text-muted);
}
.qsc-library-empty,
.qsc-library-loading {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  padding: 50px 20px;
  color: var(--qss-text-muted);
  text-align: center;
}
.qsc-library-empty h2 {
  color: var(--qss-text);
  font-size: 15px;
}
.qsc-library-foot {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  gap: 12px;
  font-size: 11px;
  color: var(--qss-text-muted);
  padding-top: 24px;
}
.qsc-library-foot a {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  color: var(--qss-text-secondary);
  text-decoration: none;
}
@container (max-width: 660px) {
  .qsc-library-filters {
    gap: 13px;
  }
}
</style>
