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
function toneFor(kind: string | null) { return kind === 'decision' ? 'amber' : kind === 'insight' ? 'violet' : kind === 'project' ? 'blue' : 'teal' }
watch(() => [vault.query, vault.scope, vault.filterKind, vault.filterTag, view.value, sort.value], () => { pageSize.value = 24 })
function clearFilters() { vault.query = ''; vault.filterTag = null; vault.filterKind = null }
function toggleTopic(tag: string) { vault.filterTag = vault.filterTag === tag ? null : tag; if (!browsing.value) view.value = 'library' }
function open(id: string) { void router.push('/memory/m/' + encodeURIComponent(id)) }
function onMap(id?: string, scope?: string) { void router.push({ path: '/memory/graph', query: id ? { focus: id, scope } : {} }) }
function openMcp() { selectKanbanBoard(vault.scope); void router.push('/mcp/projects') }
</script>

<template>
  <div class="qm-library">
    <header class="qm-library-head">
      <div><p class="qm-eyebrow"><span /> YOUR SECOND BRAIN</p><h1>Vault</h1><p class="qm-library-sub">{{ scopeLabel }} <span>·</span> {{ vault.scopeMemories.length }} memories</p></div>
      <div class="qm-library-head-actions">
        <button class="qm-library-btn qm-map-link" @click="onMap()"><Network :size="16" /> Explore graph <ArrowUpRight :size="14" /></button>
        <button class="qm-icon-action" title="Open MCP workspace" aria-label="Open MCP workspace" @click="openMcp"><Folder :size="16" /></button>
      </div>
    </header>
    <div v-if="vault.error" class="qm-library-error" role="alert">{{ vault.error }}<button :disabled="vault.loading" @click="vault.loadAll()">Retry</button></div>

    <div v-if="browsing" class="qm-library-search">
      <Search :size="19" />
      <input v-model="vault.query" aria-label="Search memory content" placeholder="Find a thought, decision or idea…" />
      <button v-if="vault.query" aria-label="Clear search" @click="vault.query = ''"><X :size="15" /></button>
      <span v-else class="qm-search-hint">Search your knowledge</span>
    </div>

    <div v-if="browsing && topics.length" class="qm-topic-shelf" aria-label="Browse topics">
      <span>Topics</span><button v-for="[tag, count] in topics.slice(0, 8)" :key="tag" :aria-pressed="vault.filterTag === tag" @click="toggleTopic(tag)">#{{ tag }}<span>{{ count }}</span></button>
    </div>

    <nav class="qm-library-tabs" aria-label="Memory views">
      <button :aria-pressed="view === 'library'" @click="view = 'library'">All memories <span>{{ vault.scopeMemories.length }}</span></button>
      <button :aria-pressed="view === 'starred'" @click="view = 'starred'"><Star :size="13" /> Starred <span>{{ starredCount }}</span></button>
      <button :aria-pressed="view === 'unlinked'" @click="view = 'unlinked'">Unlinked <span>{{ unlinkedCount }}</span></button>
      <button :aria-pressed="view === 'broken'" @click="view = 'broken'">Broken links <span>{{ vault.unresolved.length }}</span></button>
      <button :aria-pressed="view === 'review'" @click="view = 'review'">Review</button>
      <button :aria-pressed="view === 'recall'" @click="view = 'recall'">Recall</button>
      <button class="qm-library-refresh" :disabled="vault.loading" aria-label="Refresh memories" @click="vault.loadAll()"><RefreshCw :size="14" /></button>
    </nav>

    <ReviewQueue v-if="view === 'review'" />
    <RecallPanel v-else-if="view === 'recall'" />
    <template v-else-if="browsing">
      <div class="qm-library-tools">
        <h2>{{ heading }} <span>{{ rows.length }}</span></h2>
        <div class="qm-library-controls">
          <select v-model="sort" aria-label="Sort memories"><option value="relevance">{{ vault.query.trim() ? 'Best match' : 'Recently updated' }}</option><option value="title">Title A–Z</option><option value="links">Most connected</option><option v-if="vault.query.trim()" value="updated">Recently updated</option></select>
          <button class="qm-library-btn qm-filter-toggle" :aria-expanded="filtersOpen" :class="{ 'is-active': vault.filterKind || vault.filterTag }" @click="filtersOpen = !filtersOpen"><SlidersHorizontal :size="14" /> Filters</button>
          <div class="qm-layout-switch" aria-label="Memory layout"><button aria-label="Card view" :aria-pressed="app.libraryLayout === 'cards'" @click="app.setLibraryLayout('cards')"><LayoutGrid :size="15" /></button><button aria-label="List view" :aria-pressed="app.libraryLayout === 'list'" @click="app.setLibraryLayout('list')"><List :size="16" /></button></div>
        </div>
      </div>
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
      <p v-if="vault.loading || vault.searching" class="qm-library-status" role="status">{{ vault.searching ? 'Searching…' : 'Loading your memories…' }}</p>
      <div v-else-if="rows.length" class="qm-library-list" :class="{ 'is-cards': app.libraryLayout === 'cards' }">
        <article v-for="{ meta, snippet } in rows.slice(0, pageSize)" :key="meta.id" class="qm-library-row" :data-tone="toneFor(meta.kind)">
          <div class="qm-memory-top"><span class="qm-memory-kind"><component :is="iconFor(meta.kind)" :size="15" />{{ meta.kind || 'memory' }}</span><button class="qm-star" :class="{ 'is-starred': app.starred.includes(meta.id) }" :aria-label="(app.starred.includes(meta.id) ? 'Unstar ' : 'Star ') + meta.title" :aria-pressed="app.starred.includes(meta.id)" @click="app.toggleStar(meta.id)"><Star :size="15" /></button></div>
          <NuxtLink class="qm-memory-open" :to="'/memory/m/' + encodeURIComponent(meta.id)">
            <h3 class="qm-library-title">{{ meta.title }}</h3>
            <p v-if="snippet || memoryPreview(meta.preview ?? '', meta.title)" class="qm-library-snippet">{{ snippet || memoryPreview(meta.preview ?? '', meta.title) }}</p>
          </NuxtLink>
          <div v-if="meta.tags.length" class="qm-library-tags"><button v-for="tag in meta.tags.slice(0, 3)" :key="tag" @click="toggleTopic(tag)">#{{ tag }}</button><span v-if="meta.tags.length > 3">+{{ meta.tags.length - 3 }}</span></div>
          <footer class="qm-memory-footer"><span class="qm-memory-scope" :title="vault.scopeName(meta.scope)">{{ vault.scopeName(meta.scope) }}</span><time :datetime="meta.updatedAt" :title="new Date(meta.updatedAt).toLocaleString()">{{ timeAgo(meta.updatedAt) }}</time><span v-if="meta.quality?.supersededBy" class="qm-memory-state">Superseded</span><Check v-else-if="meta.quality?.reviewed" :size="13" aria-label="Reviewed memory" class="qm-reviewed" /><button class="qm-card-map" :aria-label="'Show ' + meta.title + ' on graph'" :title="(meta.incomingLinks + meta.outgoingLinks) + ' connections · Show on graph'" @click="onMap(meta.id, meta.scope)"><Network :size="13" /><span>{{ meta.incomingLinks + meta.outgoingLinks }}</span></button></footer>
        </article>
      </div>
      <div v-else-if="!vault.error && !vault.searchError" class="qm-library-empty">
        <div class="qm-empty-orbit"><Star v-if="view === 'starred'" :size="26" /><BookOpen v-else :size="26" /></div>
        <h2>{{ view === 'starred' ? 'Keep the important things close' : vault.scopeMemories.length ? 'No matching memories' : 'Start with something worth remembering' }}</h2>
        <p>{{ view === 'starred' ? 'Star any memory to find it here.' : vault.scopeMemories.length ? 'Try a different search or remove a filter.' : 'A decision, a useful reference, or an idea for later.' }}</p>
        <button v-if="vault.query || vault.filterTag || vault.filterKind" class="qm-library-btn" @click="clearFilters">Clear filters</button>
        <button v-else-if="view === 'starred'" class="qm-library-btn" @click="view = 'library'">Browse memories</button>
        <button v-else class="qm-library-btn" @click="app.startCreate(vault.createScope)"><Plus :size="14" /> New memory</button>
      </div>
      <footer v-if="rows.length && !vault.loading" class="qm-library-list-foot"><span>{{ Math.min(rows.length, pageSize) }} of {{ rows.length }}{{ vault.query.trim() && vault.hits.length === 200 ? ' · top 200 matches' : '' }}</span><button v-if="rows.length > pageSize" class="qm-library-btn" @click="pageSize += 24">Show more</button></footer>
    </template>

    <section v-else class="qm-library-repair" aria-label="Broken memory links">
      <p class="qm-library-status">These links point to memories that haven’t been written yet.</p>
      <article v-for="link in vault.unresolved" :key="link.scope + ':' + link.target" class="qm-repair-row">
        <Link2Off :size="16" /><div><h2>{{ link.target }}</h2><p>{{ vault.scopeName(link.scope) }} · {{ link.count }} references</p><div class="qm-repair-sources"><span>From</span><button v-for="source in link.sources" :key="source.id" @click="open(source.id)">{{ source.title }}</button></div></div>
        <button class="qm-library-btn" @click="app.startCreate(link.scope, link.target)"><Plus :size="14" /> Create target</button>
      </article>
      <div v-if="!vault.unresolved.length && !vault.loading && !vault.error" class="qm-library-empty"><Check :size="28" /><h2>All links lead somewhere</h2></div>
    </section>
  </div>
</template>

<style scoped>
.qm-library { height: 100%; overflow: auto; padding: clamp(24px, 3vw, 46px); background: radial-gradient(ellipse at 82% 0%, #a99bff07, transparent 50%); }
.qm-library-head { display: flex; align-items: center; justify-content: space-between; gap: 20px; margin-bottom: 30px; }
.qm-eyebrow { display: flex; align-items: center; gap: 8px; color: #b5a8ed; letter-spacing: .15em; font-size: 9px; font-weight: 600; margin: 0 0 10px; }
.qm-eyebrow > span { width: 5px; height: 5px; background: #ad9bff; box-shadow: 0 0 12px #ac99ff70; border-radius: 50%; }
h1 { font-size: 32px; font-weight: 550; letter-spacing: -.035em; margin: 0 0 7px; line-height: 1.2; }
.qm-library-sub { margin: 0; color: var(--qm-text-muted); font-size: 12px; }
.qm-library-sub span { margin: 0 6px; }
.qm-library-head-actions { display: flex; align-items: center; gap: 9px; }
button, select, input { font: inherit; }
button { cursor: pointer; }
button:disabled { opacity: .5; cursor: default; }
button:focus-visible, a:focus-visible, input:focus-visible, select:focus-visible { outline: 2px solid #aa9be3; outline-offset: 3px; }
.qm-library-btn { display: inline-flex; justify-content: center; align-items: center; gap: 8px; border: 1px solid var(--qm-border-subtle); border-radius: 7px; background: var(--qm-bg-raised); color: var(--qm-text-secondary); padding: 8px 12px; font-size: 11px; white-space: nowrap; }
.qm-library-btn:hover { color: var(--qm-text); background: var(--qm-bg-hover); }
.qm-map-link { color: #c5baf0; border-color: #a99bff30; background: #a99bff0a; padding: 10px 14px; }
.qm-icon-action { display: grid; place-items: center; height: 34px; width: 34px; color: var(--qm-text-muted); border: 0; background: none; }
.qm-library-search { display: flex; align-items: center; gap: 13px; padding: 15px 18px; border: 1px solid var(--qm-border-subtle); border-radius: 10px; background: color-mix(in srgb, var(--qm-bg-raised) 75%, transparent); color: var(--qm-text-muted); }
.qm-library-search input { flex: 1; width: 0; border: 0; background: transparent; color: var(--qm-text); font-size: 13px; outline: none; }
.qm-library-search:focus-within { border-color: #a99bff70; box-shadow: 0 0 0 3px #a99bff06; }
.qm-library-search button { border: 0; background: transparent; color: var(--qm-text-muted); display: flex; }
.qm-search-hint { font-size: 10px; opacity: .65; }
.qm-topic-shelf { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; padding: 16px 0 5px; }
.qm-topic-shelf > span { color: var(--qm-text-muted); font-size: 10px; margin-right: 3px; }
.qm-topic-shelf button { display: flex; gap: 7px; align-items: center; font-size: 10px; color: var(--qm-text-secondary); background: transparent; border: 1px solid var(--qm-border-subtle); border-radius: 20px; padding: 4px 10px; }
.qm-topic-shelf button > span { color: var(--qm-text-muted); font-size: 9px; }
.qm-topic-shelf button[aria-pressed=true] { color: #c8bdf5; border-color: #a99bff70; background: #a99bff0d; }
.qm-library-tabs { display: flex; gap: 23px; border-bottom: 1px solid var(--qm-border-subtle); margin-top: 17px; overflow-x: auto; }
.qm-library-tabs > button { display: flex; align-items: center; gap: 7px; border: 0; border-bottom: 2px solid transparent; padding: 13px 0; background: transparent; color: var(--qm-text-muted); font-size: 11px; white-space: nowrap; }
.qm-library-tabs > button[aria-pressed=true] { color: var(--qm-text); border-bottom-color: #aa9be3; }
.qm-library-tabs span { font-size: 10px; color: var(--qm-text-muted); }
.qm-library-tabs .qm-library-refresh { margin-left: auto; }
.qm-library-tools { display: flex; justify-content: space-between; gap: 14px; align-items: center; padding: 24px 0 18px; }
.qm-library-tools h2 { font-size: 13px; font-weight: 500; margin: 0; }
.qm-library-tools h2 > span { font-size: 10px; color: var(--qm-text-muted); margin-left: 6px; }
.qm-library-controls { display: flex; align-items: center; gap: 10px; }
select { max-width: 180px; padding: 6px 8px; border: 1px solid var(--qm-border-subtle); border-radius: 5px; background: var(--qm-bg); color: var(--qm-text-secondary); font-size: 11px; }
.qm-library-controls > select { border-color: transparent; background: transparent; }
.qm-layout-switch { display: flex; gap: 2px; border: 1px solid var(--qm-border-subtle); padding: 3px; border-radius: 6px; }
.qm-layout-switch button { display: grid; place-items: center; width: 27px; height: 25px; border: 0; border-radius: 3px; color: var(--qm-text-muted); background: transparent; }
.qm-layout-switch button[aria-pressed=true] { background: var(--qm-bg-hover); color: var(--qm-text); }
.qm-library-filters { display: flex; flex-wrap: wrap; gap: 14px; padding: 0 0 18px; }
.qm-library-filters label { display: flex; align-items: center; gap: 8px; color: var(--qm-text-muted); font-size: 11px; }
.qm-active-filters { display: flex; gap: 8px; padding: 0 0 14px; }
.qm-active-filters button { display: inline-flex; align-items: center; gap: 5px; border: 1px solid #a99bff40; border-radius: 4px; background: #a99bff08; color: #c3b7ee; font-size: 10px; padding: 4px 8px; }
.qm-active-filters .qm-clear-filters { border: 0; background: none; color: var(--qm-text-muted); }
.qm-library-list.is-cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(250px, 1fr)); gap: 16px; align-items: stretch; }
.qm-library-row { --card-tint: #81c4c6; position: relative; min-width: 0; display: flex; flex-direction: column; padding: 18px 0; border-bottom: 1px solid var(--qm-border-subtle); }
.qm-library-row[data-tone=amber] { --card-tint: #d7b184; }
.qm-library-row[data-tone=violet] { --card-tint: #b4a2e9; }
.qm-library-row[data-tone=blue] { --card-tint: #8cafd6; }
.is-cards .qm-library-row { min-height: 216px; padding: 19px; border: 1px solid var(--qm-border-subtle); border-radius: 11px; background: linear-gradient(145deg, color-mix(in srgb, var(--card-tint) 3%, var(--qm-bg-raised)), var(--qm-bg-raised) 75%); transition: border-color .15s, background-color .15s; }
.is-cards .qm-library-row:hover { border-color: color-mix(in srgb, var(--card-tint) 40%, var(--qm-border-subtle)); background: color-mix(in srgb, var(--card-tint) 4%, var(--qm-bg-raised)); }
.qm-memory-top { display: flex; align-items: center; justify-content: space-between; gap: 10px; margin-bottom: 14px; }
.qm-memory-kind { display: inline-flex; align-items: center; gap: 7px; color: var(--card-tint); font-size: 10px; text-transform: capitalize; }
.qm-star { display: grid; place-items: center; border: 0; background: transparent; color: var(--qm-text-muted); padding: 2px; }
.qm-star.is-starred { color: #d9bc81; }
.qm-star.is-starred svg { fill: #d9bc8130; }
.qm-memory-open { display: block; color: inherit; text-decoration: none; min-width: 0; flex: 1; }
.qm-library-title { margin: 0; color: var(--qm-text); font-size: 14px; font-weight: 550; letter-spacing: -.015em; overflow-wrap: anywhere; line-height: 1.5; }
.qm-memory-open:hover .qm-library-title { color: #ded8f4; }
.qm-library-snippet { display: -webkit-box; -webkit-line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; color: var(--qm-text-secondary); font-size: 12px; line-height: 1.7; margin: 8px 0 0; overflow-wrap: anywhere; }
.qm-library-tags { display: flex; gap: 8px; margin: 17px 0 12px; min-height: 15px; flex-wrap: wrap; }
.qm-library-tags button, .qm-library-tags > span { border: 0; background: transparent; padding: 0; color: var(--qm-text-muted); font-size: 10px; }
.qm-library-tags button:hover { color: var(--card-tint); }
.qm-memory-footer { display: flex; align-items: center; gap: 8px; padding-top: 12px; margin-top: auto; border-top: 1px solid color-mix(in srgb, var(--qm-border-subtle) 65%, transparent); font-size: 9px; color: var(--qm-text-muted); }
.qm-memory-scope { overflow: hidden; white-space: nowrap; text-overflow: ellipsis; max-width: 38%; }
.qm-memory-footer time { white-space: nowrap; }
.qm-card-map { margin-left: auto; display: inline-flex; align-items: center; gap: 5px; border: 0; background: transparent; color: var(--qm-text-muted); padding: 2px; font-size: 10px; }
.qm-card-map:hover { color: #b6a8f0; }
.qm-reviewed { color: #91cfa6; }
.qm-memory-state { color: #d7b184; }
.qm-library-list:not(.is-cards) .qm-library-row { display: grid; grid-template-columns: 120px minmax(0, 1fr) auto; column-gap: 18px; align-items: start; }
.qm-library-list:not(.is-cards) .qm-memory-top { margin: 0; }
.qm-library-list:not(.is-cards) .qm-library-tags { grid-column: 2; margin: 8px 0 0; }
.qm-library-list:not(.is-cards) .qm-memory-footer { grid-column: 3; grid-row: 1 / span 2; flex-direction: column; border: 0; padding: 0; align-items: flex-end; }
.qm-library-list:not(.is-cards) .qm-memory-scope { max-width: 110px; }
.qm-library-list:not(.is-cards) .qm-library-snippet { -webkit-line-clamp: 2; }
.qm-library-status, .qm-library-list-foot { color: var(--qm-text-muted); font-size: 11px; padding-block: 18px; }
.qm-library-list-foot { display: flex; align-items: center; justify-content: space-between; }
.qm-library-empty { display: flex; flex-direction: column; align-items: center; gap: 13px; padding: 64px 18px; text-align: center; color: var(--qm-text-muted); }
.qm-empty-orbit { display: grid; place-items: center; height: 76px; width: 76px; border: 1px solid #a99bff30; border-radius: 50%; box-shadow: 0 0 0 12px #a99bff04, 0 0 36px #a99bff08; color: #b7a7ec; margin-bottom: 14px; }
.qm-library-empty h2 { margin: 0; color: var(--qm-text); font-size: 17px; font-weight: 500; }
.qm-library-empty p { margin: 0 0 8px; font-size: 12px; }
.qm-library-error { display: flex; gap: 12px; align-items: center; padding: 12px; margin-bottom: 16px; border: 1px solid var(--qss-error); border-radius: 5px; font-size: 12px; overflow-wrap: anywhere; }
.qm-library-error button { margin-left: auto; color: var(--qm-text); border: 0; background: transparent; text-decoration: underline; }
.qm-repair-row { display: flex; align-items: flex-start; gap: 14px; padding: 20px 0; border-bottom: 1px solid var(--qm-border-subtle); }
.qm-repair-row > svg { color: var(--qm-link-broken); margin-top: 3px; flex-shrink: 0; }
.qm-repair-row > div { flex: 1; min-width: 0; }
.qm-repair-row h2 { font-size: 14px; font-weight: 500; margin: 0; overflow-wrap: anywhere; }
.qm-repair-row p { font-size: 11px; color: var(--qm-text-muted); margin-top: 7px; }
.qm-repair-sources { display: flex; flex-wrap: wrap; gap: 8px; font-size: 11px; color: var(--qm-text-muted); margin-top: 10px; }
.qm-repair-sources button { border: 0; padding: 0; background: transparent; color: var(--qm-text-secondary); text-decoration: underline; }
@media (max-width: 1150px) { .qm-library { padding: 26px; } .qm-library-tabs { gap: 17px; } .qm-search-hint { display: none; } }
@media (max-width: 850px) { .qm-library-head { align-items: flex-start; } .qm-library-tools { flex-wrap: wrap; } .qm-library-controls { flex-wrap: wrap; } .qm-library-head-actions .qm-icon-action { display: none; } }
</style>
