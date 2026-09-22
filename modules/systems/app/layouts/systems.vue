<script setup lang="ts">
/**
 * QuantSystems module shell.
 *
 * This file is the merge of the standalone app's `app.vue` (store bootstrap,
 * engine event routing, shortcuts, route watches) and its `layouts/default.vue`
 * (the visual chrome). Both had to move here:
 *
 * - A layer's `app.vue` is ignored — the shell owns it — so the bootstrap logic
 *   would have silently disappeared.
 * - A layout named `default` would apply to shell pages too. Module layouts are
 *   named after the module (ARCHITECTURE.md §14).
 *
 * The layout fills its container, not the viewport. Nothing here may assume it
 * owns the window.
 */
import { useAppStore } from '#systems/stores/app'
import { useSystemsStore } from '#systems/stores/systems'
import { useConfigStore } from '#systems/stores/config'
import { useBacktestStore } from '#systems/stores/backtest'
import { useLiveStore } from '#systems/stores/live'
import { useShortcuts, useTauriEvent } from '@quantsuite/core'
import { useActiveView } from '#systems/composables/useActiveView'

const app = useAppStore()
const systems = useSystemsStore()
const config = useConfigStore()
const backtest = useBacktestStore()
const live = useLiveStore()
const router = useRouter()
const route = useRoute()

const activeSystemId = computed(() => (route.params.id as string) || app.activeSystemId)
const { systemId, view } = useActiveView()

onMounted(async () => {
  await app.loadSettings()
  await systems.load()
  await app.refreshEngineStatus()
  await config.load(app.activeSystemId)
})

// Route progress events from the engine into whichever job is running. The
// payload carries no job id, but runExclusive() keeps a single evaluation in
// flight, so at most one store holds a runningSystemId at any moment.
useTauriEvent<{ label: string; value: number }>('eval:progress', payload => {
  if (backtest.runningSystemId) backtest.setProgress(payload.label, payload.value)
  else if (live.runningSystemId) live.setProgress(payload.label, payload.value)
})

useTauriEvent<{ status: string; pid?: number }>('engine:status', payload => {
  app.engineStatus = { status: payload.status === 'running' ? 'running' : 'stopped', pid: payload.pid ?? null }
})

function gotoSystem(id: string) {
  const target = systems.byId(id)
  if (!target || target.status !== 'ready') return
  app.setActiveSystem(id)
  void config.load(id)
  void router.push(`/algo/manual/${id}/${app.viewFor(id)}`)
}

// Ctrl+1 / Ctrl+2 switch systems. Under the V3 warm cache the layout stays
// mounted while another module is open, so useShortcuts arms the window
// listener only between onActivated and onDeactivated — otherwise these would
// fight the shell's and other modules' bindings on the same keystroke.
useShortcuts([
  {
    key: '1',
    ctrl: true,
    handler: e => {
      e.preventDefault()
      gotoSystem(systems.systems[0]?.id ?? 'lces')
    },
  },
  {
    key: '2',
    ctrl: true,
    handler: e => {
      e.preventDefault()
      gotoSystem(systems.systems[1]?.id ?? 'sces')
    },
  },
])

// Keep the active-system setting in sync with the current route.
watch(activeSystemId, id => {
  if (id && id !== app.activeSystemId) {
    app.setActiveSystem(id)
    void config.load(id)
  }
})

// Remember each system's active view so switching systems restores its own
// last-open tab rather than carrying the current one across.
watch(
  [systemId, view],
  ([id, v]) => app.rememberView(id, v),
  { immediate: true },
)
</script>

<template>
  <div class="qs-shell" :style="app.rootStyle">
    <SystemsLayoutTitleBar />
    <div class="qs-body">
      <SystemsLayoutSystemRail />
      <div class="qs-content">
        <main class="qs-main">
          <slot />
        </main>
      </div>
      <!-- V3: the shared right panel — 220px default, same as every module.
           The old StatusBar footer is gone; no module owns a bottom band. -->
      <QRightPanel storage-key="systems.right">
        <SystemsLayoutRightSidebar />
      </QRightPanel>
    </div>
  </div>
</template>

<style scoped>
.qs-shell {
  display: flex;
  flex-direction: column;
  /* Was 100vw/100vh in the standalone app. The module now fills whatever box
     the shell gives it — that is what lets several modules share a window
     later without any change in here. */
  width: 100%;
  height: 100%;
  overflow: hidden;
}

.qs-body {
  display: flex;
  flex: 1;
  min-height: 0;
}

.qs-content {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  background: var(--qs-bg);
}

.qs-main {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 24px 28px;
}

</style>
