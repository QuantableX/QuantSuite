<script setup lang="ts">
import { ArrowLeft, Search, X, Plus, Minus, Maximize, RefreshCw } from 'lucide-vue-next'
import { inActiveKeepAliveTree } from '@quantsuite/core'
import { useVaultStore } from '#memory/stores/vault'
import { useAppStore } from '#memory/stores/app'
import { scopeColor, findGraphNodes } from '#memory/utils/graph'
import { storeToRefs } from 'pinia'
import { useMemoryGraphStore } from '#memory/stores/graph'

definePageMeta({ layout: 'memory' })
const vault = useVaultStore()
const app = useAppStore()
const router = useRouter()
const route = useRoute()
const graph = useMemoryGraphStore()
const { kind, selectedId, connectionsOnly, selected, neighborhood, scopes } = storeToRefs(graph)
const map = ref<{ fit: () => void; reset: () => void; zoomStep: (direction: number) => void; focusNode: (id: string) => void; focusScope: (scope: string) => void } | null>(null)
const query = ref('')
const searchOpen = ref(false)
const searchIndex = ref(0)
const searchBox = ref<HTMLElement | null>(null)
const zoom = ref(1)
const pendingFocus = ref<string | null>(null)
const allNodes = computed(() => vault.graph?.nodes ?? [])
const allEdges = computed(() => vault.graph?.edges ?? [])
const nodes = computed(() => allNodes.value.filter(n => (!kind.value || n.kind === kind.value || n.missing) && (!connectionsOnly.value || neighborhood.value.has(n.id))))
const edges = computed(() => {
  const visible = new Set(nodes.value.map(n => n.id))
  return allEdges.value.filter(e => visible.has(e.source) && visible.has(e.target))
})
const matches = computed(() => findGraphNodes(allNodes.value, query.value))
const highlights = computed(() => query.value.trim() ? matches.value.map(n => n.id) : null)

const scopeColors = computed(() => Object.fromEntries(scopes.value.map(s => [s.scope, s.color])))
const memoryCount = computed(() => nodes.value.filter(n => !n.missing).length)

onMounted(() => { if (inActiveKeepAliveTree()) void vault.loadGraph() })
onActivated(() => void vault.loadGraph())
watch(() => vault.scope, async () => {
  selectedId.value = null; connectionsOnly.value = false; kind.value = null; query.value = ''
  await vault.loadGraph()
  await nextTick()
  if (!pendingFocus.value && !selectedId.value) map.value?.reset()
})
watch(query, () => { searchIndex.value = 0; searchOpen.value = true })
watch(kind, async () => {
  if (selected.value && !nodes.value.some(n => n.id === selectedId.value)) select(null)
  await nextTick(); map.value?.reset()
})
watch(() => route.query.focus, id => { pendingFocus.value = typeof id === 'string' ? id : null }, { immediate: true })
watch([pendingFocus, () => vault.graph, () => vault.graphLoading, () => vault.scope, map, () => route.path], async () => {
  if (route.path !== '/memory/graph' || !map.value || vault.graphLoading) return
  if (typeof route.query.scope === 'string' && route.query.scope !== vault.scope) return
  const id = pendingFocus.value
  if (id && allNodes.value.some(n => n.id === id)) { pendingFocus.value = null; await select(id, true) }
  else if (selectedId.value && !selected.value) select(null)
}, { immediate: true })

function select(id: string | null, center = false) {
  graph.inspect(id, center)
  if (id) app.sidebarRightOpen = true
}
watch(() => graph.focusRequest, async () => {
  if (route.path !== '/memory/graph' || !selectedId.value) return
  await nextTick(); await nextTick()
  map.value?.focusNode(selectedId.value)
})
watch(() => graph.scopeFocusRequest, () => {
  if (route.path === '/memory/graph' && graph.scopeToFocus) map.value?.focusScope(graph.scopeToFocus)
})
watch(connectionsOnly, async () => {
  if (route.path !== '/memory/graph') return
  await nextTick(); map.value?.reset()
})
async function chooseResult(id: string) {
  query.value = ''; searchOpen.value = false
  await select(id, true)
  searchOpen.value = false
}
function searchKeys(event: KeyboardEvent) {
  const count = Math.min(matches.value.length, 8)
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault(); searchOpen.value = true
    searchIndex.value = Math.max(0, Math.min(count - 1, searchIndex.value + (event.key === 'ArrowDown' ? 1 : -1)))
  } else if (event.key === 'Enter' && count) {
    event.preventDefault(); void chooseResult(matches.value[searchIndex.value]!.id)
  } else if (event.key === 'Escape') searchOpen.value = false
}
function leaveSearch(event: FocusEvent) {
  if (!searchBox.value?.contains(event.relatedTarget as Node | null)) searchOpen.value = false
}

function openNode(id: string) {
  const node = allNodes.value.find(n => n.id === id)
  if (!node) return
  if (node.missing) app.startCreate(node.scope, node.title)
  else void router.push('/memory/m/' + encodeURIComponent(id))
}
function reset() { kind.value = null; query.value = ''; select(null); nextTick(() => map.value?.reset()) }
</script>

<template>
  <section class="qm-map" aria-label="Your second brain" @keydown.esc="select(null)">
    <MemoryGraphCanvas ref="map" :nodes="nodes" :edges="edges" :scope-colors="scopeColors" :focus-id="selectedId" :highlight-ids="highlights" immersive @select="select($event)" @open="openNode" @zoom="zoom = $event" />

    <!-- One slim row over the map: the map itself keeps the room. -->
    <header class="qm-map-heading">
      <button class="qm-map-back" @click="router.push('/memory')"><ArrowLeft :size="13" /> Back to Vault</button>
      <h1>Memory map</h1>
      <p class="qm-map-count">{{ memoryCount }} memories <span>·</span> {{ edges.length }} connections</p>
    </header>

    <div class="qm-map-tools">
      <div ref="searchBox" class="qm-map-search-wrap" @focusout="leaveSearch">
        <div class="qm-map-search"><Search :size="14" /><input v-model="query" role="combobox" aria-label="Find a memory on the map" placeholder="Find a memory…" aria-autocomplete="list" aria-controls="qm-map-results" :aria-expanded="searchOpen && !!query.trim()" :aria-activedescendant="searchOpen && matches.length ? 'qm-map-result-' + searchIndex : undefined" @focus="searchOpen = true" @keydown="searchKeys" /><button v-if="query" aria-label="Clear graph search" @click="query = ''"><X :size="14" /></button><kbd v-else><Search :size="11" /></kbd></div>
        <div v-if="searchOpen && query.trim()" id="qm-map-results" class="qm-map-results" role="listbox" aria-label="Matching memories">
          <button v-for="(node, index) in matches.slice(0, 8)" :id="'qm-map-result-' + index" :key="node.id" role="option" :aria-selected="index === searchIndex" @click="chooseResult(node.id)"><span :style="{ background: scopeColor(node.scope) }" /><div>{{ node.title }}<small>{{ vault.scopeName(node.scope) }}{{ node.missing ? ' · Not written yet' : '' }}</small></div></button>
          <p v-if="!matches.length">No matching titles or topics.</p>
          <p v-else-if="matches.length > 8">{{ matches.length }} matches. Keep typing to narrow them down.</p>
        </div>
      </div>
    </div>

    <div v-if="vault.graphError" class="qm-map-state" role="alert"><h2>Couldn’t load your map</h2><p>{{ vault.graphError }}</p><button @click="vault.loadGraph()"><RefreshCw :size="14" /> Try again</button></div>
    <div v-else-if="vault.graphLoading && !allNodes.length" class="qm-map-state" role="status"><p>Mapping your memories…</p></div>
    <div v-else-if="!nodes.length" class="qm-map-state"><h2>{{ allNodes.length ? 'No memories of this type' : 'A space for your ideas' }}</h2><p>{{ allNodes.length ? 'Try another type to see its connections.' : 'Add your first memory. Links between notes will appear here.' }}</p><button v-if="allNodes.length" @click="reset">Show all memories</button><button v-else @click="app.startCreate(vault.createScope)"><Plus :size="15" /> Create a memory</button></div>

    <footer class="qm-map-footer">
      <span class="qm-map-hint">Click to explore <span>·</span> Drag to move <span>·</span> Scroll to zoom</span>
      <div class="qm-map-zoom"><button aria-label="Zoom out" @click="map?.zoomStep(-1)"><Minus :size="14" /></button><button class="qm-zoom-value" aria-label="Reset zoom to 100%" title="Reset zoom to 100%" @click="map?.reset()"><span aria-live="polite">{{ Math.round(zoom * 100) }}%</span></button><button aria-label="Zoom in" @click="map?.zoomStep(1)"><Plus :size="14" /></button><i /><button aria-label="Fit graph to view" @click="map?.fit()"><Maximize :size="14" /></button></div>
    </footer>
  </section>
</template>

<style scoped>
.qm-map { position: relative; height: 100%; min-height: 0; overflow: hidden; color: var(--qm-text); background: var(--qss-bg-chrome); font-family: var(--qss-font-sans, sans-serif); container-type: inline-size; }
.qm-map button { cursor: pointer; }
.qm-map button:focus-visible { outline: 2px solid var(--qm-text-secondary); outline-offset: 2px; }
/* The search sits at the right of the same row, so the title stays clear of it. */
.qm-map-heading { position: absolute; top: var(--qss-page-edge); left: var(--qss-page-edge); right: calc(var(--qss-page-edge) + 240px + var(--qss-gap)); display: flex; flex-wrap: wrap; align-items: baseline; gap: 2px var(--qss-gap); pointer-events: none; }
.qm-map-back { display: flex; align-items: center; gap: 6px; padding: 0; border: 0; background: none; color: var(--qm-text-secondary); font-size: var(--qss-text-label); white-space: nowrap; pointer-events: auto; }
.qm-map-back:hover { color: var(--qm-text); }
.qm-map-heading h1 { margin: 0; font-size: var(--qss-text-title); font-weight: 600; letter-spacing: -.01em; line-height: 28px; white-space: nowrap; }
.qm-map-count { margin: 0; color: var(--qm-text-secondary); font-size: var(--qss-text-meta); font-variant-numeric: tabular-nums; white-space: nowrap; }
.qm-map-count span { padding: 0 4px; }
.qm-map-tools { position: absolute; right: var(--qss-page-edge); top: var(--qss-page-edge); }
.qm-map-search-wrap { position: relative; width: 240px; }
.qm-map-search { display: flex; align-items: center; gap: var(--qss-gap-tight); height: var(--qss-ctl-h); padding: 0 8px; border: 1px solid var(--qm-border); border-radius: 8px; background: var(--qm-bg-raised); color: var(--qm-text-secondary); }
.qm-map-search:focus-within { border-color: var(--qm-text-muted); }
.qm-map-search input { min-width: 0; width: 100%; border: 0; background: none; color: var(--qm-text); outline: none; font: inherit; font-size: var(--qss-text-label); }
.qm-map-search input::placeholder { color: var(--qm-text-secondary); }
.qm-map-search button { display: grid; place-items: center; border: 0; padding: 0; background: none; color: var(--qm-text-secondary); }
.qm-map-search button:hover { color: var(--qm-text); }
.qm-map-search kbd { display: grid; place-items: center; border: 1px solid var(--qm-border); border-radius: 4px; padding: 2px; }
.qm-map-results { position: absolute; z-index: 3; top: 32px; left: 0; right: 0; padding: 4px; border: 1px solid var(--qm-border); border-radius: var(--qss-panel-radius); background: var(--qss-bg-overlay); box-shadow: var(--qss-shadow-md); }
.qm-map-results > button { display: flex; align-items: center; gap: var(--qss-gap-tight); width: 100%; border: 0; border-radius: 6px; padding: 6px 8px; background: none; color: var(--qm-text); text-align: left; font-size: var(--qss-text-label); }
.qm-map-results > button[aria-selected=true], .qm-map-results > button:hover { background: var(--qm-bg-hover); }
.qm-map-results > button > span { width: 6px; height: 6px; flex-shrink: 0; border-radius: 50%; }
.qm-map-results small { display: block; color: var(--qm-text-secondary); font-size: var(--qss-text-meta); }
.qm-map-results p { margin: 0; padding: 6px 8px; color: var(--qm-text-secondary); font-size: var(--qss-text-meta); }
.qm-map-footer { position: absolute; z-index: 1; bottom: var(--qss-page-edge); left: var(--qss-page-edge); right: var(--qss-page-edge); display: flex; align-items: center; justify-content: space-between; gap: var(--qss-gap); pointer-events: none; }
.qm-map-hint { margin: 0; color: var(--qm-text-secondary); font-size: var(--qss-text-meta); }
.qm-map-hint span { margin: 0 4px; color: var(--qm-text-muted); }
.qm-map-zoom { display: flex; align-items: center; gap: 2px; height: var(--qss-ctl-h); padding: 2px; border: 1px solid var(--qm-border); border-radius: 8px; background: var(--qm-bg-raised); pointer-events: auto; }
.qm-map-zoom button { display: grid; place-items: center; width: 24px; height: 22px; border: 0; border-radius: 6px; background: none; color: var(--qm-text-secondary); }
.qm-map-zoom button:hover { background: var(--qm-bg-hover); color: var(--qm-text); }
.qm-map-zoom .qm-zoom-value { width: 44px; font-size: var(--qss-text-label); font-variant-numeric: tabular-nums; }
.qm-map-zoom i { width: 1px; height: 14px; margin: 0 2px; background: var(--qm-border); }
.qm-map-state { position: absolute; top: 50%; left: 50%; transform: translate(-50%, -50%); display: flex; flex-direction: column; align-items: center; gap: var(--qss-gap-tight); width: min(370px, 80%); text-align: center; color: var(--qm-text-secondary); }
.qm-map-state h2 { margin: 0; color: var(--qm-text); font-size: 14px; font-weight: 600; }
.qm-map-state p { margin: 0; color: var(--qm-text-secondary); font-size: 13px; line-height: 1.65; overflow-wrap: anywhere; }
.qm-map-state button { display: flex; align-items: center; gap: 6px; height: var(--qss-ctl-h); padding: 0 10px; border: 1px solid var(--qm-border); border-radius: 8px; background: var(--qm-bg-raised); color: var(--qm-text); font-size: var(--qss-text-label); }
.qm-map-state button:hover { background: var(--qm-bg-hover); }
@container (max-width: 600px) {
  .qm-map-search-wrap { width: 180px; }
  .qm-map-search kbd { display: none; }
  .qm-map-heading { right: calc(var(--qss-page-edge) + 180px + var(--qss-gap)); }
}
</style>
