<script setup lang="ts">
import { ArrowUpRight, Eye, Focus, Link2, Network, Star, X, FilePlus2 } from 'lucide-vue-next'
import { useMemoryGraphStore } from '#memory/stores/graph'
import { useVaultStore } from '#memory/stores/vault'
import { useAppStore } from '#memory/stores/app'
import { scopeColor } from '#memory/utils/graph'
import { memoryPreview } from '#memory/utils/library'
import { timeAgo } from '#memory/utils/format'

const graph = useMemoryGraphStore()
const vault = useVaultStore()
const app = useAppStore()
const router = useRouter()
function openSelected() {
  const node = graph.selected
  if (!node) return
  if (node.missing) app.startCreate(node.scope, node.title)
  else void router.push('/memory/m/' + encodeURIComponent(node.id))
}
</script>

<template>
  <div class="qm-graph-context" aria-label="Graph sidebar">
    <section class="qm-map-controls">
      <h2>Map controls</h2>
      <label class="qm-map-type">Memory type<select v-model="graph.kind" aria-label="Filter graph by type"><option :value="null">All types</option><option v-for="kind in graph.kinds" :key="kind" :value="kind">{{ kind }}</option></select></label>
      <label class="qm-map-labels"><Eye :size="15" /> Memory labels<input v-model="graph.showLabels" type="checkbox" aria-label="Show memory labels" /></label>
    </section>

    <section v-if="graph.selected" class="qm-map-inspector" :style="{ '--node-color': scopeColor(graph.selected.scope) }" aria-label="Selected memory">
      <div class="qm-inspector-top"><span><i />{{ vault.scopeName(graph.selected.scope) }}</span><button aria-label="Close memory details" @click="graph.inspect(null)"><X :size="16" /></button></div>
      <p class="qm-inspector-kind">{{ graph.selected.missing ? 'Not written yet' : graph.selected.kind || 'Memory' }}</p>
      <h2>{{ graph.selected.title }}</h2>
      <div class="qm-inspector-actions"><button class="qm-inspector-open" @click="openSelected"><component :is="graph.selected.missing ? FilePlus2 : ArrowUpRight" :size="15" />{{ graph.selected.missing ? 'Write this memory' : 'Open memory' }}</button><button v-if="!graph.selected.missing" class="qm-inspector-star" :aria-label="app.starred.includes(graph.selected.id) ? 'Unstar selected memory' : 'Star selected memory'" :aria-pressed="app.starred.includes(graph.selected.id)" @click="app.toggleStar(graph.selected.id)"><Star :size="16" /></button></div>
      <p v-if="graph.selected.missing" class="qm-inspector-preview">Linked from another memory. Write this note to complete the connection.</p>
      <p v-else-if="graph.selectedMeta?.preview" class="qm-inspector-preview">{{ memoryPreview(graph.selectedMeta.preview, graph.selected.title) }}</p>
      <div v-if="graph.selected.tags.length" class="qm-inspector-tags"><span v-for="tag in graph.selected.tags" :key="tag">#{{ tag }}</span></div>
      <div v-if="graph.selectedMeta" class="qm-inspector-meta"><span>{{ graph.selectedMeta.wordCount }} words</span><span>Updated {{ timeAgo(graph.selectedMeta.updatedAt) }}</span></div>
      <button class="qm-focus-connections" :aria-pressed="graph.connectionsOnly" aria-label="Focus connections" @click="graph.connectionsOnly = !graph.connectionsOnly"><Focus :size="15" />{{ graph.connectionsOnly ? 'Show full map' : 'Focus connections' }}</button>
      <div class="qm-inspector-connections"><h3>Connections <span>{{ graph.connected.length }}</span></h3><p v-if="!graph.connected.length">Use [[a memory title]] in a note to connect it.</p><button v-for="node in graph.connected" :key="node.id" class="qm-connection" @click="graph.inspect(node.id)"><Link2 :size="14" :style="{ color: scopeColor(node.scope) }" /><span>{{ node.title }}</span><ArrowUpRight :size="13" /></button></div>
    </section>
    <section v-else class="qm-map-discover">
      <h2><Network :size="15" /> Most connected</h2>
      <button v-for="node in graph.hubs" :key="node.id" class="qm-connection" @click="graph.inspect(node.id)"><i :style="{ background: scopeColor(node.scope) }" /><span>{{ node.title }}</span><small>{{ node.incoming + node.outgoing }}</small></button>
      <p v-if="graph.hubs.length">Select a memory to see its context and follow its connections.</p>
      <p v-else>Your memories and connections will appear here.</p>
    </section>
    <section v-if="graph.scopes.length" class="qm-map-legend" aria-label="Workspace constellations"><h2>Workspaces</h2><button v-for="scope in graph.scopes" :key="scope.scope" :title="'Center ' + scope.name" @click="graph.focusScope(scope.scope)"><i :style="{ background: scope.color, color: scope.color }" /><span>{{ scope.name }}</span><small>{{ scope.count }}</small><Focus :size="13" /></button></section>
  </div>
</template>

<style scoped>
.qm-graph-context { min-height: 0; height: 100%; overflow-y: auto; padding: 18px 14px; color: var(--qm-text-secondary); font-size: 13px; }
.qm-graph-context section + section { margin-top: 22px; padding-top: 20px; border-top: 1px solid var(--qm-border-subtle); }
.qm-graph-context h2 { display: flex; align-items: center; gap: 7px; margin: 0 0 15px; color: var(--qm-text); font-size: 13px; font-weight: 600; }
.qm-graph-context button { cursor: pointer; }.qm-graph-context button:focus-visible, .qm-graph-context input:focus-visible, .qm-graph-context select:focus-visible { outline: 2px solid #b9aaff; outline-offset: 2px; }
.qm-map-type { display: grid; gap: 7px; font-size: 12px; }.qm-map-type select { width: 100%; border: 1px solid var(--qm-border); border-radius: 6px; padding: 7px; color: var(--qm-text); background: var(--qm-bg); font: inherit; }
.qm-map-labels { display: flex; align-items: center; gap: 7px; margin-top: 13px; font-size: 12px; cursor: pointer; }.qm-map-labels input { margin-left: auto; accent-color: #ac9bdd; }
.qm-inspector-top { display: flex; align-items: center; justify-content: space-between; gap: 8px; }.qm-inspector-top > span { display: flex; align-items: center; gap: 7px; min-width: 0; font-size: 12px; }.qm-inspector-top i, .qm-connection i, .qm-map-legend i { width: 6px; height: 6px; flex-shrink: 0; border-radius: 50%; background: var(--node-color); }
.qm-inspector-top button { display: grid; place-items: center; flex-shrink: 0; width: 25px; height: 25px; border: 0; border-radius: 5px; background: transparent; color: var(--qm-text-muted); }.qm-inspector-top button:hover { background: var(--qm-bg-hover); color: var(--qm-text); }
.qm-inspector-kind { margin: 18px 0 7px; color: var(--node-color); font-size: 11px; text-transform: uppercase; letter-spacing: 1px; }.qm-map-inspector h2 { font-size: 18px; line-height: 1.4; overflow-wrap: anywhere; margin-bottom: 12px; }.qm-inspector-preview { margin: 0 0 14px; line-height: 1.7; overflow-wrap: anywhere; }
.qm-inspector-tags { display: flex; flex-wrap: wrap; gap: 5px; }.qm-inspector-tags span { padding: 3px 6px; border: 1px solid var(--qm-border); border-radius: 5px; font-size: 11px; }.qm-inspector-meta { display: flex; flex-wrap: wrap; gap: 6px 12px; margin: 15px 0; color: var(--qm-text-muted); font-size: 11px; }
.qm-inspector-actions { display: flex; gap: 6px; margin: 15px 0 8px; }.qm-inspector-open { display: flex; align-items: center; justify-content: center; gap: 6px; flex: 1; padding: 9px 6px; border: 1px solid color-mix(in srgb, var(--node-color) 35%, transparent); border-radius: 6px; background: color-mix(in srgb, var(--node-color) 10%, transparent); color: var(--qm-text); font-size: 12px; }.qm-inspector-open:hover { background: color-mix(in srgb, var(--node-color) 20%, transparent); }.qm-inspector-star { display: grid; place-items: center; flex-shrink: 0; width: 32px; border: 1px solid var(--qm-border); border-radius: 6px; background: none; color: var(--qm-text-muted); }.qm-inspector-star[aria-pressed=true] { color: #e8be7a; }.qm-inspector-star[aria-pressed=true] svg { fill: #e8be7a25; }
.qm-focus-connections { display: flex; align-items: center; justify-content: center; gap: 7px; width: 100%; padding: 8px; border: 1px solid var(--qm-border); border-radius: 6px; background: none; color: var(--qm-text-secondary); font-size: 12px; }.qm-focus-connections[aria-pressed=true] { color: var(--node-color); background: color-mix(in srgb, var(--node-color) 8%, transparent); }
.qm-inspector-connections { padding-top: 20px; }.qm-inspector-connections h3 { margin: 0 0 10px; font-size: 13px; font-weight: 500; }.qm-inspector-connections h3 span { margin-left: 6px; color: var(--qm-text-muted); }
.qm-connection { display: flex; align-items: center; gap: 7px; width: 100%; padding: 9px 3px; border: 0; border-radius: 5px; background: none; color: var(--qm-text-secondary); text-align: left; font-size: 12px; }.qm-connection:hover { background: var(--qm-bg-hover); color: var(--qm-text); }.qm-connection span { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }.qm-connection svg { flex-shrink: 0; }.qm-connection small { color: var(--qm-text-muted); font-size: 11px; }
.qm-map-discover p, .qm-inspector-connections > p { color: var(--qm-text-muted); font-size: 12px; line-height: 1.7; margin: 12px 0 0; }
.qm-map-legend button { display: flex; align-items: center; gap: 7px; width: 100%; margin-top: 6px; padding: 8px; border: 1px solid var(--qm-border-subtle); border-radius: 6px; background: none; color: var(--qm-text-secondary); text-align: left; font-size: 12px; }.qm-map-legend button:hover { background: var(--qm-bg-hover); }.qm-map-legend button > span { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }.qm-map-legend i { box-shadow: 0 0 8px currentColor; }.qm-map-legend small { color: var(--qm-text-muted); font-size: 11px; }
</style>
