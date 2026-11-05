<script setup lang="ts">
import { Search, Plus, ArrowUpRight, FileText, RefreshCw, Link2Off, Network, Star, LayoutGrid, List, SlidersHorizontal, X, BookOpen, Lightbulb, Compass, Check, Folder } from 'lucide-vue-next'
import { selectKanbanBoard } from '@quantsuite/core'
import { useVaultStore } from '#memory/stores/vault'
import { useAppStore } from '#memory/stores/app'
import { timeAgo } from '#memory/utils/format'
import { memoryLibraryRows, memoryPreview, type MemorySort } from '#memory/utils/library'
import ReviewQueue from '#memory/components/ReviewQueue.vue'
import RecallPanel from '#memory/components/RecallPanel.vue'

definePageMeta({ layout: 'memory' })
const vault = useVaultStore()
const app = useAppStore()
const router = useRouter()
const route = useRoute()
type LibraryView = 'library' | 'starred' | 'unlinked' | 'broken' | 'review' | 'recall'
const views: LibraryView[] = ['library', 'starred', 'unlinked', 'broken', 'review', 'recall']
const view = computed<LibraryView>({
  get: () => views.includes(route.query.view as LibraryView) ? route.query.view as LibraryView : 'library',
  set: value => { void router.replace({ query: { ...route.query, view: value === 'library' ? undefined : value } }) },
})
const sort = ref<MemorySort>('relevance')
const pageSize = ref(24)
const filtersOpen = ref(false)
const browsing = computed(() => ['library', 'starred', 'unlinked'].includes(view.value))
const rows = computed(() => memoryLibraryRows(vault.memories, {
  scope: vault.scope, tag: vault.filterTag, kind: vault.filterKind,
  unlinked: view.value === 'unlinked', searching: !!vault.query.trim(), hits: vault.hits, sort: sort.value,
}).filter(({ meta }) => view.value !== 'starred' || app.starred.includes(meta.id)))
const topics = computed(() => {
  const counts = new Map<string, number>()
  for (const memory of vault.scopeMemories) for (const tag of memory.tags) counts.set(tag, (counts.get(tag) ?? 0) + 1)
  return [...counts].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
})
const unlinkedCount = computed(() => vault.scopeMemories.filter(m => m.incomingLinks + m.outgoingLinks === 0).length)
const starredCount = computed(() => vault.scopeMemories.filter(m => app.starred.includes(m.id)).length)
const scopeLabel = computed(() => vault.scope === 'general' ? 'Across your workspaces' : vault.scopeName(vault.scope))
const heading = computed(() => view.value === 'starred' ? 'Starred memories' : view.value === 'unlinked' ? 'Not connected yet' : vault.filterTag ? '#' + vault.filterTag : vault.query.trim() ? 'Search results' : 'Your memories')
const icons = { decision: Compass, insight: Lightbulb, reference: BookOpen, project: Folder }
function iconFor(kind: string | null) { return icons[kind as keyof typeof icons] ?? FileText }
watch(() => [vault.query, vault.scope, vault.filterKind, vault.filterTag, view.value, sort.value], () => { pageSize.value = 24 })
function clearFilters() { vault.query = ''; vault.filterTag = null; vault.filterKind = null }
function toggleTopic(tag: string) { vault.filterTag = vault.filterTag === tag ? null : tag; if (!browsing.value) view.value = 'library' }
function open(id: string) { void router.push('/memory/m/' + encodeURIComponent(id)) }
function onMap(id?: string, scope?: string) { void router.push({ path: '/memory/graph', query: id ? { focus: id, scope } : {} }) }
function openMcp() { selectKanbanBoard(vault.scope); void router.push('/mcp/projects') }
</script>

<template>
  <div class="qm-library">
    <!-- One slim row: name, search, the way out. Content starts right below. -->
    <header class="qm-library-head">
      <div class="qm-library-name"><h1>Vault</h1><p class="qm-library-sub">{{ scopeLabel }} <span>·</span> {{ vault.scopeMemories.length }} memories</p></div>
      <div v-if="browsing" class="qm-library-search">
        <Search :size="14" />
        <input v-model="vault.query" aria-label="Search memory content" placeholder="Find a thought, decision or idea…" />
        <button v-if="vault.query" aria-label="Clear search" @click="vault.query = ''"><X :size="14" /></button>
      </div>
      <div class="qm-library-head-actions">
        <button class="qm-library-btn qm-map-link" @click="onMap()"><Network :size="14" /> Explore graph <ArrowUpRight :size="14" /></button>
        <button class="qm-icon-action" title="Open MCP workspace" aria-label="Open MCP workspace" @click="openMcp"><Folder :size="15" /></button>
      </div>
    </header>
    <div v-if="vault.error" class="qm-library-error" role="alert">{{ vault.error }}<button :disabled="vault.loading" @click="vault.loadAll()">Retry</button></div>

    <!-- Second row: the views, the topics and what shapes the list. -->
    <div class="qm-library-bar">
      <nav class="qm-library-tabs" aria-label="Memory views">
        <button :aria-pressed="view === 'library'" @click="view = 'library'">All memories <span>{{ vault.scopeMemories.length }}</span></button>
        <button :aria-pressed="view === 'starred'" @click="view = 'starred'"><Star :size="13" /> Starred <span>{{ starredCount }}</span></button>
        <button :aria-pressed="view === 'unlinked'" @click="view = 'unlinked'">Unlinked <span>{{ unlinkedCount }}</span></button>
        <button :aria-pressed="view === 'broken'" @click="view = 'broken'">Broken links <span>{{ vault.unresolved.length }}</span></button>
        <button :aria-pressed="view === 'review'" @click="view = 'review'">Review</button>
        <button :aria-pressed="view === 'recall'" @click="view = 'recall'">Recall</button>
        <button class="qm-library-refresh" :disabled="vault.loading" aria-label="Refresh memories" @click="vault.loadAll()"><RefreshCw :size="14" /></button>
      </nav>

      <div v-if="browsing && topics.length" class="qm-topic-shelf" aria-label="Browse topics">
        <span>Topics</span><button v-for="[tag, count] in topics.slice(0, 8)" :key="tag" :aria-pressed="vault.filterTag === tag" @click="toggleTopic(tag)">#{{ tag }}<span>{{ count }}</span></button>
      </div>

      <div v-if="browsing" class="qm-library-tools">
        <h2>{{ heading }} <span>{{ rows.length }}</span></h2>
        <div class="qm-library-controls">
          <select v-model="sort" aria-label="Sort memories"><option value="relevance">{{ vault.query.trim() ? 'Best match' : 'Recently updated' }}</option><option value="title">Title A–Z</option><option value="links">Most connected</option><option v-if="vault.query.trim()" value="updated">Recently updated</option></select>
          <button class="qm-library-btn qm-filter-toggle" :aria-expanded="filtersOpen" :class="{ 'is-active': vault.filterKind || vault.filterTag }" @click="filtersOpen = !filtersOpen"><SlidersHorizontal :size="14" /> Filters</button>
          <div class="qm-layout-switch" aria-label="Memory layout"><button aria-label="Card view" :aria-pressed="app.libraryLayout === 'cards'" @click="app.setLibraryLayout('cards')"><LayoutGrid :size="14" /></button><button aria-label="List view" :aria-pressed="app.libraryLayout === 'list'" @click="app.setLibraryLayout('list')"><List :size="14" /></button></div>
        </div>
      </div>
    </div>

    <ReviewQueue v-if="view === 'review'" />
    <RecallPanel v-else-if="view === 'recall'" />
    <template v-else-if="browsing">
      <div v-if="filtersOpen" class="qm-library-filters">
        <label>Type<select v-model="vault.filterKind" aria-label="Filter memory type"><option :value="null">All types</option><option v-for="kind in vault.kinds" :key="kind">{{ kind }}</option></select></label>
        <label>Topic<select v-model="vault.filterTag" aria-label="Filter memory tag"><option :value="null">All topics</option><option v-for="[tag] in topics" :key="tag" :value="tag">#{{ tag }}</option></select></label>
        <label>Search matching<select v-model="vault.searchMode" aria-label="Search match mode"><option value="all">All words</option><option value="any">Any word</option></select></label>
      </div>
      <div v-if="vault.query || vault.filterTag || vault.filterKind" class="qm-active-filters">
        <button v-if="vault.filterTag" @click="vault.filterTag = null">#{{ vault.filterTag }} <X :size="12" /></button><button v-if="vault.filterKind" @click="vault.filterKind = null">{{ vault.filterKind }} <X :size="12" /></button>
        <button class="qm-clear-filters" @click="clearFilters">Clear filters</button>
      </div>
      <p v-if="vault.searchError" class="qm-library-error" role="alert">{{ vault.searchError }}</p>
      <!-- The list scrolls here; the two rows above stay in view. -->
      <div class="qm-library-body">
        <p v-if="vault.loading || vault.searching" class="qm-library-status" role="status">{{ vault.searching ? 'Searching…' : 'Loading your memories…' }}</p>
        <div v-else-if="rows.length" class="qm-library-list" :class="{ 'is-cards': app.libraryLayout === 'cards' }">
          <article v-for="{ meta, snippet } in rows.slice(0, pageSize)" :key="meta.id" class="qm-library-row">
            <div class="qm-memory-top"><span class="qm-memory-kind"><component :is="iconFor(meta.kind)" :size="14" />{{ meta.kind || 'memory' }}</span><button class="qm-star" :class="{ 'is-starred': app.starred.includes(meta.id) }" :aria-label="(app.starred.includes(meta.id) ? 'Unstar ' : 'Star ') + meta.title" :aria-pressed="app.starred.includes(meta.id)" @click="app.toggleStar(meta.id)"><Star :size="14" /></button></div>
            <NuxtLink class="qm-memory-open" :to="'/memory/m/' + encodeURIComponent(meta.id)">
              <h3 class="qm-library-title">{{ meta.title }}</h3>
              <p v-if="snippet || memoryPreview(meta.preview ?? '', meta.title)" class="qm-library-snippet">{{ snippet || memoryPreview(meta.preview ?? '', meta.title) }}</p>
            </NuxtLink>
            <div v-if="meta.tags.length" class="qm-library-tags"><button v-for="tag in meta.tags.slice(0, 3)" :key="tag" @click="toggleTopic(tag)">#{{ tag }}</button><span v-if="meta.tags.length > 3">+{{ meta.tags.length - 3 }}</span></div>
            <footer class="qm-memory-footer"><span class="qm-memory-scope" :title="vault.scopeName(meta.scope)">{{ vault.scopeName(meta.scope) }}</span><time :datetime="meta.updatedAt" :title="new Date(meta.updatedAt).toLocaleString('en-US', { dateStyle: 'medium', timeStyle: 'short', hourCycle: 'h23' })">{{ timeAgo(meta.updatedAt) }}</time><span v-if="meta.quality?.supersededBy" class="qm-memory-state">Superseded</span><Check v-else-if="meta.quality?.reviewed" :size="13" aria-label="Reviewed memory" class="qm-reviewed" /><button class="qm-card-map" :aria-label="'Show ' + meta.title + ' on graph'" :title="(meta.incomingLinks + meta.outgoingLinks) + ' connections · Show on graph'" @click="onMap(meta.id, meta.scope)"><Network :size="13" /><span>{{ meta.incomingLinks + meta.outgoingLinks }}</span></button></footer>
          </article>
        </div>
        <QEmptyState v-else-if="!vault.error && !vault.searchError" :title="view === 'starred' ? 'Keep the important things close' : vault.scopeMemories.length ? 'No matching memories' : 'Start with something worth remembering'" :description="view === 'starred' ? 'Star any memory to find it here.' : vault.scopeMemories.length ? 'Try a different search or remove a filter.' : 'A decision, a useful reference, or an idea for later.'">
          <button v-if="vault.query || vault.filterTag || vault.filterKind" class="qm-library-btn" @click="clearFilters">Clear filters</button>
          <button v-else-if="view === 'starred'" class="qm-library-btn" @click="view = 'library'">Browse memories</button>
          <button v-else class="qm-library-btn" @click="app.startCreate(vault.createScope)"><Plus :size="14" /> New memory</button>
        </QEmptyState>
        <footer v-if="rows.length && !vault.loading" class="qm-library-list-foot"><span>{{ Math.min(rows.length, pageSize) }} of {{ rows.length }}{{ vault.query.trim() && vault.hits.length === 200 ? ' · top 200 matches' : '' }}</span><button v-if="rows.length > pageSize" class="qm-library-btn" @click="pageSize += 24">Show more</button></footer>
      </div>
    </template>

    <section v-else class="qm-library-repair" aria-label="Broken memory links">
      <p class="qm-library-status">These links point to memories that haven’t been written yet.</p>
      <article v-for="link in vault.unresolved" :key="link.scope + ':' + link.target" class="qm-repair-row">
        <Link2Off :size="14" /><div><h2>{{ link.target }}</h2><p>{{ vault.scopeName(link.scope) }} · {{ link.count }} references</p><div class="qm-repair-sources"><span>From</span><button v-for="source in link.sources" :key="source.id" @click="open(source.id)">{{ source.title }}</button></div></div>
        <button class="qm-library-btn" @click="app.startCreate(link.scope, link.target)"><Plus :size="14" /> Create target</button>
      </article>
      <QEmptyState v-if="!vault.unresolved.length && !vault.loading && !vault.error" title="All links lead somewhere" />
    </section>
  </div>
</template>

<style scoped>
.qm-library { display: flex; flex-direction: column; gap: var(--qss-gap); height: 100%; min-height: 0; padding: var(--qss-page-edge); container-type: inline-size; }
button, select, input { font: inherit; }
button { cursor: pointer; }
button:disabled { opacity: .5; cursor: default; }
button:focus-visible, a:focus-visible, input:focus-visible, select:focus-visible { outline: 2px solid var(--qm-text-secondary); outline-offset: 2px; }

.qm-library-head { flex-shrink: 0; display: flex; flex-wrap: wrap; align-items: center; gap: var(--qss-gap-tight) var(--qss-gap); }
.qm-library-name { display: flex; align-items: baseline; gap: var(--qss-gap); min-width: 0; }
h1 { margin: 0; font-size: var(--qss-text-title); font-weight: 600; letter-spacing: -.01em; line-height: 1.25; white-space: nowrap; }
.qm-library-sub { margin: 0; color: var(--qm-text-muted); font-size: var(--qss-text-meta); font-variant-numeric: tabular-nums; white-space: nowrap; }
.qm-library-sub span { margin: 0 4px; }
.qm-library-head-actions { display: flex; align-items: center; gap: 4px; margin-left: auto; }
.qm-library-btn { display: inline-flex; justify-content: center; align-items: center; gap: 6px; height: var(--qss-ctl-h); border: 1px solid var(--qm-border-subtle); border-radius: 8px; background: var(--qm-bg-raised); color: var(--qm-text-secondary); padding: 0 10px; font-size: var(--qss-text-label); white-space: nowrap; }
.qm-library-btn:hover { color: var(--qm-text); background: var(--qm-bg-hover); }
.qm-filter-toggle.is-active { color: var(--qm-text); border-color: var(--qm-border); }
.qm-icon-action { display: grid; place-items: center; height: var(--qss-ctl-h); width: var(--qss-ctl-h); border: 0; border-radius: 8px; background: none; color: var(--qm-text-muted); }
.qm-icon-action:hover { color: var(--qm-text); background: var(--qm-bg-hover); }
.qm-library-search { flex: 1 1 240px; min-width: 0; display: flex; align-items: center; gap: var(--qss-gap-tight); height: var(--qss-ctl-h); padding: 0 8px; border: 1px solid var(--qm-border-subtle); border-radius: 8px; background: var(--qm-bg-raised); color: var(--qm-text-muted); }
.qm-library-search input { flex: 1; width: 0; border: 0; background: transparent; color: var(--qm-text); font-size: var(--qss-text-label); outline: none; }
.qm-library-search:focus-within { border-color: var(--qm-border); }
.qm-library-search button { display: flex; border: 0; padding: 2px; background: transparent; color: var(--qm-text-muted); }
.qm-library-search button:hover { color: var(--qm-text); }

.qm-library-bar { flex-shrink: 0; display: flex; flex-wrap: wrap; align-items: center; gap: var(--qss-gap-tight) var(--qss-gap); }
.qm-library-tabs { display: flex; flex-wrap: wrap; align-items: center; gap: 2px; }
.qm-library-tabs > button { display: flex; align-items: center; gap: 6px; height: var(--qss-ctl-h); border: 0; border-radius: 8px; padding: 0 8px; background: transparent; color: var(--qm-text-muted); font-size: var(--qss-text-label); white-space: nowrap; }
.qm-library-tabs > button:hover { color: var(--qm-text); }
.qm-library-tabs > button[aria-pressed=true] { background: var(--qm-bg-hover); color: var(--qm-text); }
.qm-library-tabs span { font-size: var(--qss-text-meta); color: var(--qm-text-muted); font-variant-numeric: tabular-nums; }
.qm-library-tabs .qm-library-refresh { padding: 0; width: var(--qss-ctl-h); justify-content: center; }
.qm-topic-shelf { flex: 1 1 200px; min-width: 0; display: flex; align-items: center; flex-wrap: wrap; gap: 4px 6px; }
.qm-topic-shelf > span { color: var(--qm-text-muted); font-size: var(--qss-text-meta); margin-right: 2px; }
.qm-topic-shelf button { display: flex; gap: 6px; align-items: center; height: 22px; font-size: var(--qss-text-meta); color: var(--qm-text-secondary); background: transparent; border: 1px solid var(--qm-border-subtle); border-radius: 11px; padding: 0 8px; white-space: nowrap; }
.qm-topic-shelf button:hover { color: var(--qm-text); border-color: var(--qm-border); }
.qm-topic-shelf button > span { color: var(--qm-text-muted); font-variant-numeric: tabular-nums; }
.qm-topic-shelf button[aria-pressed=true] { color: var(--qm-text); border-color: var(--qm-text-muted); background: var(--qm-bg-hover); }
.qm-library-tools { display: flex; align-items: center; gap: var(--qss-gap); margin-left: auto; min-width: 0; }
.qm-library-tools h2 { margin: 0; font-size: var(--qss-text-section); font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.qm-library-tools h2 > span { margin-left: 4px; color: var(--qm-text-muted); font-size: var(--qss-text-meta); font-weight: 400; font-variant-numeric: tabular-nums; }
.qm-library-controls { display: flex; align-items: center; gap: var(--qss-gap-tight); }
select { max-width: 180px; height: var(--qss-ctl-h); padding: 0 6px; border: 1px solid var(--qm-border-subtle); border-radius: 8px; background: var(--qm-bg); color: var(--qm-text-secondary); font-size: var(--qss-text-label); }
.qm-library-controls > select { border-color: transparent; background: transparent; }
.qm-layout-switch { display: flex; gap: 2px; height: var(--qss-ctl-h); padding: 2px; border: 1px solid var(--qm-border-subtle); border-radius: 8px; }
.qm-layout-switch button { display: grid; place-items: center; width: 24px; border: 0; border-radius: 6px; color: var(--qm-text-muted); background: transparent; }
.qm-layout-switch button:hover { color: var(--qm-text); }
.qm-layout-switch button[aria-pressed=true] { background: var(--qm-bg-hover); color: var(--qm-text); }
.qm-library-filters { flex-shrink: 0; display: flex; flex-wrap: wrap; gap: var(--qss-gap); }
.qm-library-filters label { display: flex; align-items: center; gap: var(--qss-gap-tight); color: var(--qm-text-muted); font-size: var(--qss-text-label); }
.qm-active-filters { flex-shrink: 0; display: flex; flex-wrap: wrap; gap: var(--qss-gap-tight); }
.qm-active-filters button { display: inline-flex; align-items: center; gap: 4px; height: 22px; border: 1px solid var(--qm-border); border-radius: 6px; background: var(--qm-bg-raised); color: var(--qm-text-secondary); font-size: var(--qss-text-meta); padding: 0 8px; }
.qm-active-filters .qm-clear-filters { border: 0; background: none; color: var(--qm-text-muted); }

.qm-library-body { flex: 1 1 0; min-height: 0; overflow-y: auto; }
.qm-library-list.is-cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: var(--qss-gap); align-items: stretch; }
.qm-library-row { position: relative; min-width: 0; display: flex; flex-direction: column; }
.is-cards .qm-library-row { gap: 4px; padding: var(--qss-panel-pad); border: 1px solid var(--qm-border-subtle); border-radius: var(--qss-panel-radius); background: var(--qm-bg-raised); transition: border-color .15s, background-color .15s; }
.is-cards .qm-library-row:hover { border-color: var(--qm-border); background: var(--qm-bg-card); }
.qm-memory-top { display: flex; align-items: center; justify-content: space-between; gap: var(--qss-gap-tight); }
.qm-memory-kind { display: inline-flex; align-items: center; gap: 6px; color: var(--qm-text-muted); font-size: var(--qss-text-meta); text-transform: capitalize; }
.qm-star { display: grid; place-items: center; border: 0; background: transparent; color: var(--qm-text-muted); padding: 2px; }
.qm-star:hover { color: var(--qm-text); }
.qm-star.is-starred { color: var(--qm-text); }
.qm-star.is-starred svg { fill: currentColor; }
.qm-memory-open { display: block; color: inherit; text-decoration: none; min-width: 0; flex: 1; }
.qm-library-title { margin: 0; color: var(--qm-text); font-size: var(--qss-text-section); font-weight: 600; overflow-wrap: anywhere; line-height: 1.4; }
.qm-memory-open:hover .qm-library-title { text-decoration: underline; text-underline-offset: 2px; }
.qm-library-snippet { display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; color: var(--qm-text-secondary); font-size: var(--qss-text-label); line-height: 1.5; margin: 2px 0 0; overflow-wrap: anywhere; }
.qm-library-tags { display: flex; gap: var(--qss-gap-tight); flex-wrap: wrap; }
.qm-library-tags button, .qm-library-tags > span { border: 0; background: transparent; padding: 0; color: var(--qm-text-muted); font-size: var(--qss-text-meta); }
.qm-library-tags button:hover { color: var(--qm-text); }
.qm-memory-footer { display: flex; align-items: center; gap: var(--qss-gap-tight); margin-top: auto; font-size: var(--qss-text-meta); color: var(--qm-text-muted); }
.qm-memory-scope { overflow: hidden; white-space: nowrap; text-overflow: ellipsis; max-width: 38%; }
.qm-memory-footer time { white-space: nowrap; font-variant-numeric: tabular-nums; }
.qm-card-map { margin-left: auto; display: inline-flex; align-items: center; gap: 4px; border: 0; background: transparent; color: var(--qm-text-muted); padding: 2px; font-size: var(--qss-text-meta); font-variant-numeric: tabular-nums; }
.qm-card-map:hover { color: var(--qm-text); }
.qm-reviewed { color: var(--qss-success); }
.qm-memory-state { color: var(--qss-warning); }

/* The list view reads as a table: one column each, the snippet under its title. */
.qm-library-list:not(.is-cards) .qm-library-row { display: grid; grid-template-columns: 120px minmax(0, 1fr) minmax(0, 220px) 220px; column-gap: var(--qss-gap); align-items: center; min-height: var(--qss-row-h); padding: 4px 8px; border-bottom: 1px solid var(--qm-border-subtle); }
.qm-library-list:not(.is-cards) .qm-library-row:hover { background: var(--qm-bg-raised); }
.qm-library-list:not(.is-cards) .qm-library-tags { grid-column: 3; }
.qm-library-list:not(.is-cards) .qm-memory-footer { grid-column: 4; margin: 0; }
.qm-library-list:not(.is-cards) .qm-memory-scope { max-width: 110px; }
.qm-library-list:not(.is-cards) .qm-library-snippet { -webkit-line-clamp: 1; margin: 0; }

.qm-library-status, .qm-library-list-foot { margin: 0; color: var(--qm-text-muted); font-size: var(--qss-text-meta); }
.qm-library-list-foot { display: flex; align-items: center; justify-content: space-between; gap: var(--qss-gap); padding-block: var(--qss-gap); font-variant-numeric: tabular-nums; }
.qm-library-error { flex-shrink: 0; display: flex; gap: var(--qss-gap); align-items: center; margin: 0; padding: 6px 12px; border: 1px solid var(--qss-error); border-radius: 8px; font-size: var(--qss-text-label); overflow-wrap: anywhere; }
.qm-library-error button { margin-left: auto; color: var(--qm-text); border: 0; background: transparent; text-decoration: underline; }

.qm-library-repair { flex: 1 1 0; min-height: 0; overflow-y: auto; }
.qm-library-repair > .qm-library-status { padding-bottom: var(--qss-gap-tight); }
.qm-repair-row { display: flex; align-items: flex-start; gap: var(--qss-gap); padding: var(--qss-gap-tight) 0; border-bottom: 1px solid var(--qm-border-subtle); }
.qm-repair-row > svg { color: var(--qm-link-broken); margin-top: 3px; flex-shrink: 0; }
.qm-repair-row > div { flex: 1; min-width: 0; }
.qm-repair-row h2 { font-size: var(--qss-text-section); font-weight: 600; margin: 0; overflow-wrap: anywhere; }
.qm-repair-row p { font-size: var(--qss-text-meta); color: var(--qm-text-muted); margin: 2px 0 0; }
.qm-repair-sources { display: flex; flex-wrap: wrap; gap: 4px 8px; font-size: var(--qss-text-meta); color: var(--qm-text-muted); margin-top: 2px; }
.qm-repair-sources button { border: 0; padding: 0; background: transparent; color: var(--qm-text-secondary); text-decoration: underline; }

/* The tabs take their own line once the topics and the list controls no longer fit beside them. */
@container (max-width: 1180px) {
  .qm-library-tabs { flex-basis: 100%; }
}
@container (max-width: 760px) {
  .qm-library-search { order: 3; flex-basis: 100%; }
  .qm-library-tabs > button { padding: 0 6px; }
  .qm-topic-shelf { flex-basis: 100%; }
  .qm-library-tools { flex-wrap: wrap; margin-left: 0; }
  .qm-library-list:not(.is-cards) .qm-library-row { grid-template-columns: 96px minmax(0, 1fr); row-gap: 2px; padding-block: 6px; }
  .qm-library-list:not(.is-cards) .qm-library-tags, .qm-library-list:not(.is-cards) .qm-memory-footer { grid-column: 2; }
}
</style>
