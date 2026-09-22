import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { useVaultStore } from '#memory/stores/vault'
import { graphNeighborhood, scopeColor } from '#memory/utils/graph'

/** Shared presentation state for the map and its docked context panel. */
export const useMemoryGraphStore = defineStore('memory/graph', () => {
  const vault = useVaultStore()
  const selectedId = ref<string | null>(null)
  const connectionsOnly = ref(false)
  const kind = ref<string | null>(null)
  const showLabels = ref(true)
  const focusRequest = ref(0)
  const scopeFocusRequest = ref(0)
  const scopeToFocus = ref<string | null>(null)
  const nodes = computed(() => vault.graph?.nodes ?? [])
  const edges = computed(() => vault.graph?.edges ?? [])
  const selected = computed(() => nodes.value.find(n => n.id === selectedId.value) ?? null)
  const selectedMeta = computed(() => vault.memories.find(m => m.id === selectedId.value))
  const neighborhood = computed(() => graphNeighborhood(nodes.value, edges.value, selectedId.value ?? ''))
  const connected = computed(() => nodes.value.filter(n => n.id !== selectedId.value && neighborhood.value.has(n.id)).sort((a, b) => a.title.localeCompare(b.title)))
  const kinds = computed(() => [...new Set(nodes.value.map(n => n.kind).filter((k): k is string => !!k))].sort())
  const scopes = computed(() => [...new Set(nodes.value.map(n => n.scope))].sort().map(scope => ({
    scope, color: scopeColor(scope), name: vault.scopeName(scope), count: nodes.value.filter(n => n.scope === scope && !n.missing).length,
  })))
  const hubs = computed(() => nodes.value.filter(n => !n.missing).sort((a, b) => (b.incoming + b.outgoing) - (a.incoming + a.outgoing)).slice(0, 5))

  function inspect(id: string | null, center = true) {
    selectedId.value = id
    connectionsOnly.value = false
    if (id && kind.value && selected.value?.kind !== kind.value) kind.value = null
    if (id && center) focusRequest.value++
  }
  function focusScope(scope: string) {
    scopeToFocus.value = scope
    scopeFocusRequest.value++
  }
  return { selectedId, selected, selectedMeta, neighborhood, connected, nodes, edges, scopes, kinds, hubs,
    connectionsOnly, kind, showLabels, focusRequest, scopeFocusRequest, scopeToFocus, inspect, focusScope }
})
