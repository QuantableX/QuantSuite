<script setup lang="ts">
import { ArrowLeft, Search, X, Plus, Minus, Maximize, Network, RefreshCw } from 'lucide-vue-next'
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
const { kind, selectedId, connectionsOnly, showLabels, selected, neighborhood, scopes } = storeToRefs(graph)
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
    <MemoryGraphCanvas ref="map" :nodes="nodes" :edges="edges" :scope-colors="scopeColors" :focus-id="selectedId" :highlight-ids="highlights" :show-labels="showLabels" :label-threshold="0.25" immersive @select="select($event)" @open="openNode" @zoom="zoom = $event" />

    <header class="qm-map-heading">
      <button class="qm-map-back" @click="router.push('/memory')"><ArrowLeft :size="13" /> Back to Vault</button>
      <p class="qm-map-eyebrow"><span /> YOUR SECOND BRAIN</p>
      <h1>Memory map<span>.</span></h1>
      <p class="qm-map-count">{{ memoryCount }} memories <span>·</span> {{ edges.length }} connections</p>
    </header>

    <div class="qm-map-tools">
      <div ref="searchBox" class="qm-map-search-wrap" @focusout="leaveSearch">
        <div class="qm-map-search"><Search :size="16" /><input v-model="query" role="combobox" aria-label="Find a memory on the map" placeholder="Find a memory…" aria-autocomplete="list" aria-controls="qm-map-results" :aria-expanded="searchOpen && !!query.trim()" :aria-activedescendant="searchOpen && matches.length ? 'qm-map-result-' + searchIndex : undefined" @focus="searchOpen = true" @keydown="searchKeys" /><button v-if="query" aria-label="Clear graph search" @click="query = ''"><X :size="14" /></button><kbd v-else><Search :size="11" /></kbd></div>
        <div v-if="searchOpen && query.trim()" id="qm-map-results" class="qm-map-results" role="listbox" aria-label="Matching memories">
          <button v-for="(node, index) in matches.slice(0, 8)" :id="'qm-map-result-' + index" :key="node.id" role="option" :aria-selected="index === searchIndex" @click="chooseResult(node.id)"><span :style="{ background: scopeColor(node.scope) }" /><div>{{ node.title }}<small>{{ vault.scopeName(node.scope) }}{{ node.missing ? ' · Not written yet' : '' }}</small></div></button>
          <p v-if="!matches.length">No matching titles or topics.</p>
          <p v-else-if="matches.length > 8">{{ matches.length }} matches. Keep typing to narrow them down.</p>
        </div>
      </div>
    </div>

    <div v-if="vault.graphError" class="qm-map-state" role="alert"><Network :size="30" /><h2>Couldn’t load your map</h2><p>{{ vault.graphError }}</p><button @click="vault.loadGraph()"><RefreshCw :size="14" /> Try again</button></div>
    <div v-else-if="vault.graphLoading && !allNodes.length" class="qm-map-state" role="status"><Network :size="30" /><p>Mapping your memories…</p></div>
    <div v-else-if="!nodes.length" class="qm-map-state"><Network :size="36" /><h2>{{ allNodes.length ? 'No memories of this type' : 'A space for your ideas' }}</h2><p>{{ allNodes.length ? 'Try another type to see its connections.' : 'Add your first memory. Links between notes will appear here.' }}</p><button v-if="allNodes.length" @click="reset">Show all memories</button><button v-else @click="app.startCreate(vault.createScope)"><Plus :size="15" /> Create a memory</button></div>

    <footer class="qm-map-footer">
      <span class="qm-map-hint">Click to explore <span>·</span> Drag to move <span>·</span> Scroll to zoom</span>
      <div class="qm-map-zoom"><button aria-label="Zoom out" @click="map?.zoomStep(-1)"><Minus :size="16" /></button><button class="qm-zoom-value" aria-label="Reset zoom to 100%" title="Reset zoom to 100%" @click="map?.reset()"><span aria-live="polite">{{ Math.round(zoom * 100) }}%</span></button><button aria-label="Zoom in" @click="map?.zoomStep(1)"><Plus :size="16" /></button><i /><button aria-label="Fit graph to view" @click="map?.fit()"><Maximize :size="16" /></button></div>
    </footer>
  </section>
</template>

<style scoped>
.qm-map { --map-muted: #9ba3bb; position: relative; height: 100%; min-height: 0; overflow: hidden; color: #ececf6; background: #0b0d16; font-family: var(--qss-font-sans, sans-serif); container-type: inline-size; }
.qm-map::after { content: ''; position: absolute; inset: auto 0 0; height: 100px; background: linear-gradient(transparent, #0b0d16 80%); pointer-events: none; }
.qm-map button { cursor: pointer; }.qm-map button:focus-visible, .qm-map input:focus-visible { outline: 2px solid #b9aaff; outline-offset: 3px; }
.qm-map-heading { position: absolute; top: 25px; left: 26px; pointer-events: none; }
.qm-map-back { display: flex; align-items: center; gap: 7px; padding: 0; border: 0; background: none; color: var(--map-muted); font-size: 13px; pointer-events: auto; }.qm-map-back:hover { color: #fff; }
.qm-map-eyebrow { display: flex; align-items: center; gap: 8px; margin: 27px 0 9px; color: #b9abeb; font-size: 11px; font-weight: 600; letter-spacing: 2px; }.qm-map-eyebrow span { width: 5px; height: 5px; border-radius: 50%; background: #b4a2ff; box-shadow: 0 0 12px #ad8fff; }
.qm-map-heading h1 { margin: 0; font-size: 32px; font-weight: 500; letter-spacing: -1.4px; line-height: 1.2; }.qm-map-heading h1 span { color: #b1a1f4; }
.qm-map-count { margin: 11px 0 0; color: var(--map-muted); font-size: 13px; }.qm-map-count span { padding: 0 6px; }
.qm-map-tools { position: absolute; right: 24px; top: 24px; }.qm-map-search-wrap { position: relative; width: 230px; }
.qm-map-search { display: flex; align-items: center; gap: 9px; height: 39px; padding: 0 11px; border: 1px solid #303346; border-radius: 9px; background: #131621; color: var(--map-muted); }.qm-map-search input { min-width: 0; width: 100%; border: 0; background: none; color: #ececf6; outline: none; font: inherit; font-size: 13px; }.qm-map-search input::placeholder { color: #a0a8c1; }.qm-map-search button { display: grid; place-items: center; border: 0; background: none; color: var(--map-muted); }.qm-map-search kbd { display: grid; place-items: center; border: 1px solid #303346; border-radius: 4px; padding: 3px; }
.qm-map-results { position: absolute; z-index: 3; top: 45px; left: 0; right: 0; padding: 5px; border: 1px solid #343349; border-radius: 10px; background: #151724; box-shadow: 0 12px 40px #0006; }.qm-map-results > button { display: flex; align-items: center; gap: 9px; width: 100%; border: 0; border-radius: 6px; padding: 10px; background: none; color: #e6e5f1; text-align: left; font-size: 13px; }.qm-map-results > button[aria-selected=true], .qm-map-results > button:hover { background: #242439; }.qm-map-results > button > span { width: 6px; height: 6px; flex-shrink: 0; border-radius: 50%; }.qm-map-results small { display: block; margin-top: 4px; color: #a1a8c2; font-size: 12px; }.qm-map-results p { padding: 8px; color: var(--map-muted); font-size: 12px; }
.qm-map-footer { position: absolute; z-index: 1; bottom: 18px; left: 24px; right: 24px; display: grid; grid-template-columns: 1fr auto; gap: 14px 0; pointer-events: none; }
.qm-map-hint { grid-row: 2; grid-column: 1 / -1; justify-self: center; margin: 0; color: #a4adc7; font-size: 13px; text-align: center; white-space: nowrap; }.qm-map-hint span { margin: 0 6px; color: #67718c; }
.qm-map-zoom { grid-row: 1; grid-column: 2; display: flex; align-items: center; gap: 3px; padding: 4px; border: 1px solid #303144; border-radius: 8px; background: #131621; pointer-events: auto; }.qm-map-zoom button { display: grid; place-items: center; width: 29px; height: 29px; border: 0; border-radius: 5px; background: none; color: #d5d2e8; }.qm-map-zoom button:hover { background: #242437; }.qm-map-zoom .qm-zoom-value { width: 49px; font-size: 13px; font-variant-numeric: tabular-nums; }.qm-map-zoom i { width: 1px; height: 14px; margin: 0 4px; background: #343348; }
.qm-map-state { position: absolute; top: 50%; left: 50%; transform: translate(-50%, -35%); display: flex; flex-direction: column; align-items: center; width: min(370px, 80%); text-align: center; color: #ad9fd6; }.qm-map-state h2 { margin: 20px 0 0; color: #e1ddef; font-size: 20px; font-weight: 500; }.qm-map-state p { color: #a3acc7; font-size: 13px; line-height: 1.8; overflow-wrap: anywhere; }.qm-map-state button { display: flex; align-items: center; gap: 7px; padding: 9px 13px; border: 1px solid #56476f; border-radius: 7px; background: #292038; color: #e3d5ff; font-size: 13px; }
@container (max-width: 600px) { .qm-map-heading { left: 20px; }.qm-map-tools { right: 20px; }.qm-map-search-wrap { width: 210px; }.qm-map-search kbd { display: none; }.qm-map-footer { left: 16px; right: 16px; } }
@container (max-width: 430px) { .qm-map-search-wrap { width: 175px; }.qm-map-search input { font-size: 12px; }.qm-map-hint { white-space: normal; line-height: 1.7; } }
@media (max-height: 800px) { .qm-map-heading { top: 20px; }.qm-map-eyebrow { margin-top: 18px; }.qm-map-heading h1 { font-size: 28px; }.qm-map-footer { bottom: 15px; } }
</style>
