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
    <!-- Title, filters, search, sort and the action on one row where they fit. -->
    <div class="qsc-library-head">
      <QPageHeading class="qsc-library-title" title="Indicator library" />
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
      <label class="qsc-library-search"
        ><Search :size="14" /><input
          v-model="wb.search"
          type="search"
          aria-label="Search indicator library"
          placeholder="Search names, files, indicators…"
          spellcheck="false"
      /></label>
      <select v-model="sort" class="qsc-select qsc-library-sort" aria-label="Sort scripts">
        <option value="name">Name A–Z</option>
        <option value="recent">Recently modified</option>
      </select>
      <button class="qsc-btn is-primary qsc-library-new" @click="wb.newScriptOpen = true"><Plus :size="15" /> New script</button>
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
      <!-- The list scrolls here; the title row and the Forge link stay on screen. -->
      <div class="qsc-library-scroll">
        <div v-if="rows.length" class="qsc-script-grid">
          <article
            v-for="entry in rows"
            :key="entry.file"
            class="qsc-script-card"
            :class="{ 'is-opening': wb.openingFile === entry.file }"
          >
            <div class="qsc-card-top">
              <FileCode2 :size="14" /><span>{{
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
        <QEmptyState
          v-else-if="!wb.listing.scripts.some((script) => script.kind === 'script')"
          title="Your script library is empty"
          description="Add your Python indicator files to this folder, or create a new script."
        >
          <span class="mono qsc-library-path">{{ wb.listing.indicators_dir }}</span>
          <button class="qsc-btn" :disabled="wb.listingLoading" @click="wb.loadListing(true)">Refresh library</button>
        </QEmptyState>
        <QEmptyState
          v-else
          :title="wb.libraryFilter === 'favorites' && !wb.search ? 'No favorites yet' : 'No matching scripts'"
          :description="
            wb.libraryFilter === 'favorites' && !wb.search
              ? 'Star a script to save it here.'
              : 'Try another search or switch to all scripts.'
          "
        >
          <button class="qsc-btn" @click="resetFilters">
            <X :size="13" /> Reset filters
          </button>
        </QEmptyState>
      </div>
    </template>
    <footer class="qsc-library-foot">
      <NuxtLink to="/script/forge">Open Forge <ArrowRight :size="13" /></NuxtLink>
    </footer>
  </div>
</template>

<style scoped>
.qsc-library {
  --qss-page-head-gap: 0px;
  height: 100%;
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap-tight);
  padding: var(--qss-page-edge);
  overflow: auto;
  container-type: inline-size;
}
.qsc-library > * {
  flex-shrink: 0;
}
/* One row where it fits. Narrower, the title keeps sort and action, and the filters and search get their own row. */
.qsc-library-head {
  display: grid;
  grid-template-columns: auto auto minmax(160px, 1fr) auto auto;
  grid-template-areas: 'title filters search sort new';
  align-items: center;
  gap: var(--qss-gap-tight) var(--qss-gap);
}
.qsc-library-title { grid-area: title; }
.qsc-library-filters { grid-area: filters; }
.qsc-library-search { grid-area: search; }
.qsc-library-sort { grid-area: sort; }
.qsc-library-new { grid-area: new; }
@container (max-width: 1040px) {
  .qsc-library-head {
    grid-template-columns: auto minmax(0, 1fr) auto auto;
    grid-template-areas:
      'title title sort new'
      'filters search search search';
  }
}
@container (max-width: 660px) {
  .qsc-library-head {
    grid-template-columns: minmax(0, 1fr) auto auto;
    grid-template-areas:
      'title sort new'
      'filters filters filters'
      'search search search';
  }
}
.qsc-library-filters {
  display: flex;
  flex-wrap: wrap;
  gap: 2px;
}
.qsc-library-filters button {
  display: flex;
  align-items: center;
  gap: 6px;
  height: var(--qss-ctl-h);
  padding: 0 8px;
  border: 1px solid transparent;
  border-radius: var(--qss-ctl-radius);
  background: transparent;
  color: var(--qss-text-secondary);
  font-size: var(--qss-ctl-font);
  cursor: pointer;
}
.qsc-library-filters button:hover {
  color: var(--qss-text);
}
.qsc-library-filters button.is-active {
  border-color: var(--qss-border);
  background: var(--qss-bg-card);
  color: var(--qss-text);
}
.qsc-library-filters button span {
  color: var(--qss-text-muted);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}
.qsc-library-search {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 8px;
  height: var(--qss-ctl-h);
  border: 1px solid var(--qss-border);
  border-radius: var(--qss-ctl-radius);
  padding: 0 10px;
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
  font-size: var(--qss-ctl-font);
}
.qsc-library-caption {
  display: flex;
  justify-content: space-between;
  gap: var(--qss-gap);
  color: var(--qss-text-secondary);
  font-size: var(--qss-text-meta);
  font-variant-numeric: tabular-nums;
}
.qsc-library > .qsc-library-scroll {
  flex: 1 1 0;
  min-height: 120px;
  overflow-y: auto;
}
.qsc-library-path {
  flex: 1 0 100%;
  font-size: var(--qss-text-meta);
  color: var(--qss-text-secondary);
  overflow-wrap: anywhere;
}
.qsc-script-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(min(240px, 100%), 1fr));
  gap: var(--qss-gap);
}
.qsc-script-card {
  min-width: 0;
  display: flex;
  flex-direction: column;
  border: 1px solid var(--qss-border);
  border-radius: var(--qss-panel-radius);
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
  gap: 6px;
  padding: var(--qss-gap-tight) var(--qss-panel-pad) 0;
  color: var(--qss-text-secondary);
  font-size: 11px;
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
  padding: 4px var(--qss-panel-pad) var(--qss-gap-tight);
  cursor: pointer;
  flex: 1;
}
.qsc-card-open h2 {
  font-size: var(--qss-text-section);
  font-weight: 600;
  color: var(--qss-text);
  overflow-wrap: anywhere;
}
.qsc-card-file {
  display: block;
  font-size: 11px;
  color: var(--qss-text-secondary);
  margin-top: 1px;
  overflow-wrap: anywhere;
}
.qsc-card-open p {
  margin-top: 6px;
  font-size: var(--qss-text-label);
  color: var(--qss-text-secondary);
  line-height: 1.45;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.qsc-card-bottom {
  padding: 6px var(--qss-panel-pad);
  border-top: 1px solid var(--qss-border-subtle);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
  font-size: 11px;
  color: var(--qss-text-secondary);
  font-variant-numeric: tabular-nums;
}
.qsc-library-loading {
  padding: var(--qss-page-edge) 0;
  color: var(--qss-text-secondary);
  font-size: var(--qss-text-label);
}
.qsc-library-foot {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  gap: var(--qss-gap);
  font-size: var(--qss-text-meta);
  color: var(--qss-text-muted);
}
.qsc-library-foot a {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  color: var(--qss-text-secondary);
  text-decoration: none;
}
</style>
