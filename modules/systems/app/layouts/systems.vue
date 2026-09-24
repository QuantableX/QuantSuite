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
const route = router.currentRoute

const { systemId, view } = useActiveView()
const bootstrapped = ref(false)

onMounted(async () => {
  await app.loadSettings()
  await systems.load()
  bootstrapped.value = true
  void app.refreshEngineStatus()
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
useShortcuts(Array.from({ length: 9 }, (_, i) => ({
  key: String(i + 1),
  ctrl: true,
  handler: (e: KeyboardEvent) => {
    const target = systems.systems[i]
    if (!target) return
    e.preventDefault()
    gotoSystem(target.id)
  },
})))

// Keep the active-system setting in sync with the current route.
watch([bootstrapped, () => systems.loaded, () => systems.systems, () => route.value.path], () => {
  if (!bootstrapped.value || !systems.loaded || !route.value.path.startsWith('/algo/manual')) return
  const id = systemId.value
  if (route.value.path === '/algo/manual/new') return
  if (id && systems.byId(id)) {
    if (id !== app.activeSystemId) app.setActiveSystem(id)
    void config.load(id)
    return
  }
  const fallback = systems.byId(app.activeSystemId)?.id ?? systems.systems[0]?.id
  if (fallback) void router.replace(`/algo/manual/${fallback}/${app.viewFor(fallback)}`)
  else if (id) void router.replace('/algo/manual')
}, { immediate: true })

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
          <div v-if="systems.error" class="qs-shell__error" role="alert">
            {{ systems.error }}
            <button class="btn" @click="systems.load()">Retry</button>
          </div>
          <div v-else-if="!bootstrapped || !systems.loaded">Loading strategies…</div>
          <template v-else>
            <div v-if="config.errors[systemId]" class="qs-shell__error" role="alert">
              {{ config.errors[systemId] }}
              <button v-if="!config.configBySystem[systemId]" class="btn" @click="config.load(systemId)">Retry</button>
            </div>
            <slot v-if="!systemId || config.configBySystem[systemId]" />
            <div v-else-if="!config.errors[systemId]">Loading strategy settings…</div>
          </template>
        </main>
      </div>
      <!-- V3: the shared right panel — 220px default, same as every module.
           The old StatusBar footer is gone; no module owns a bottom band. -->
      <QRightPanel v-if="systems.byId(systemId)" storage-key="systems.right">
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

.qs-shell__error { display: flex; gap: 12px; align-items: center; padding: 12px; color: var(--qs-error); font-size: 13px; }

</style>
