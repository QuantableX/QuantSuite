<script setup lang="ts">
/**
 * QuantMemory module shell — header, the two 220px panels, the main stage.
 *
 * The vault is shared with agents and with external editors (Obsidian), so
 * the layout listens on the bus: any `memory.*` change reloads the list.
 * Warm-cache rules apply (V3): subscriptions arm on mount only inside an
 * active KeepAlive tree, re-arm on activation, stand down on deactivation.
 */
import { bus, inActiveKeepAliveTree, useShortcuts } from '@quantsuite/core'
import { useAppStore } from '#memory/stores/app'
import { useVaultStore } from '#memory/stores/vault'
import MemoryLayoutGraphSidebar from '#memory/components/Layout/GraphSidebar.vue'

const app = useAppStore()
const vault = useVaultStore()
const route = useRoute()
const mapOpen = computed(() => route.path.startsWith('/memory/graph'))
watch(() => [route.path, route.query.scope], () => {
  const scope = route.query.scope
  if (route.path.startsWith('/memory') && typeof scope === 'string' && (scope === 'general' || scope.startsWith('core:workspace:'))) vault.setScope(scope)
}, { immediate: true })

let searchTimer: ReturnType<typeof setTimeout> | null = null
watch(() => [vault.query, vault.searchMode], () => {
  if (searchTimer) clearTimeout(searchTimer)
  vault.hits = []
  vault.searching = !!vault.query.trim()
  searchTimer = setTimeout(() => void vault.search(vault.query), 200)
})
onBeforeUnmount(() => { if (searchTimer) clearTimeout(searchTimer) })

let unsubs: Array<() => void> = []

function arm() {
  if (unsubs.length) return
  const reload = () => {
    void vault.loadAll()
    void vault.refreshActive()
    if (mapOpen.value) void vault.loadGraph()
  }
  unsubs = [
    bus.on('memory.vault.changed', reload),
    bus.on('memory.note.created', reload),
    bus.on('memory.note.updated', reload),
    bus.on('memory.note.deleted', reload),
  ]
}

function disarm() {
  unsubs.forEach((u) => u())
  unsubs = []
}

onMounted(() => {
  if (!inActiveKeepAliveTree()) return
  arm()
  void vault.loadAll()
})
onActivated(() => {
  arm()
  void vault.loadAll()
})
onDeactivated(disarm)
onBeforeUnmount(disarm)

useShortcuts([
  // The wildcard-modifier rule: Ctrl+Shift+B must come before plain Ctrl+B.
  {
    key: 'B',
    ctrl: true,
    shift: true,
    handler: (e) => {
      e.preventDefault()
      app.toggleSidebar('right')
    },
  },
  {
    key: 'b',
    ctrl: true,
    handler: (e) => {
      e.preventDefault()
      app.toggleSidebar('left')
    },
  },
  {
    key: ',',
    ctrl: true,
    handler: (e) => {
      e.preventDefault()
      window.dispatchEvent(new CustomEvent('qss:settings', { detail: { module: 'memory' } }))
    },
  },
])
</script>

<template>
  <div class="qm-shell">
    <MemoryLayoutAppHeader />
    <div class="qm-body">
      <QSidebar v-model="app.sidebarLeftOpen" resizable storage-key="memory.left">
        <MemoryLayoutLeftSidebar />
      </QSidebar>
      <main class="qm-main">
        <slot />
      </main>
      <QRightPanel v-model="app.sidebarRightOpen" storage-key="memory.right">
        <MemoryLayoutGraphSidebar v-if="mapOpen" />
        <MemoryLayoutRightSidebar v-else />
      </QRightPanel>
    </div>
    <MemoryCreateDialog />
  </div>
</template>

<style scoped>
.qm-shell {
  display: flex;
  flex-direction: column;
  /* The module fills its container, never the viewport (ARCHITECTURE.md §9). */
  width: 100%;
  height: 100%;
  overflow: hidden;
}

.qm-body {
  display: flex;
  flex: 1;
  min-height: 0;
}

.qm-main {
  position: relative;
  flex: 1;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  background: var(--qm-bg);
}
</style>
