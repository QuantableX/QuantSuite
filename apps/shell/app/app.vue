<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { currentWindowLabel, isSuiteWindow } from '@quantsuite/core'
import { appAvailability, isAppEnabled, isModuleEnabled, isRouteEnabled, appForModule, appById, apps, bus, moduleById, moduleForRoute, stageForRoute, modulesForApp, PROCESS_CHANGED, qs, routableModules } from '@quantsuite/core'

/**
 * The v2 shell (PLAN-V2 §2): one titlebar, the module rail, the stage, the
 * drawer. The window is a plain maximised undecorated rectangle — the
 * launcher circle, the veil and the iris are gone with the pivot; startup
 * lands on the workspace start screen at `/`.
 *
 * Modules render content only. All window chrome — dragging, window
 * controls, module switching — lives here and in the shell components.
 */

const router = useRouter()

type AlgoMode = 'manual' | 'automated'
const algoLastMode = useState<AlgoMode>('algo-last-mode', () => 'manual')
const algoModeRoutes = useState<Record<AlgoMode, string>>('algo-mode-routes', () => ({
  manual: '/algo/manual',
  automated: '/algo',
}))

/** `/algo` is the module's registry route. Entering through shell chrome uses
 * the last open interface; before either interface was opened, Manual wins. */
function entryRoute(route: string): string {
  return route === '/algo' || route === '/algo/manual'
    ? algoModeRoutes.value[algoLastMode.value]
    : route
}

/**
 * V3 warm cache: the shell must track the router's LIVE route, not Nuxt's
 * `useRoute()`. Nuxt defers `_route` until the incoming page resolves inside
 * a NuxtPage — but our stages render *pinned* routes, so a foreign page never
 * resolves in the old stage and `_route` would freeze, deadlocking the module
 * switch (the new stage only mounts once activeModule changes). ModuleStage
 * re-syncs `_route` on activation for everyone downstream who does use it.
 */
const currentPath = computed(() => router.currentRoute.value.path)

/**
 * QuantHUD's overlay windows (`dual-right`, `region-selector`, …) load
 * routes of this same app — but they are transparent, always-on-top
 * surfaces with their own shapes (the edge half-circles). They must render
 * **bare**, exactly like the standalone app's root did: NuxtLayout/NuxtPage
 * and nothing else. Wrapping them in the shell chrome painted an opaque
 * frame with titlebar, rail and drawer into a transparent overlay — which
 * is how the HUD "stopped working" after the E1 shell reset.
 *
 * Detected synchronously via the global Tauri handle (withGlobalTauri), so
 * the first paint is already bare — no chrome flash.
 */
const primaryWindow = currentWindowLabel() === 'main'
const overlayWindow = !isSuiteWindow(currentWindowLabel())

if (overlayWindow && import.meta.client) {
  // Everything from the document root down has to stop painting, or the
  // overlay sits in an opaque rectangle (same lesson as the old launcher).
  document.documentElement.classList.add('qss-overlay-window')
}

const paletteOpen = ref(false)

const activeModule = computed(() => moduleForRoute(currentPath.value))

/**
 * V3: the rail lists APPS. The active app is derived from the active module's
 * membership; the shell's own pages (`/`, `/settings`) belong to
 * the Dashboard. QuantHUD has no app and never appears here — its overlay
 * always runs alongside the suite, reachable via the palette.
 */
const activeApp = computed(() =>
  activeModule.value ? appForModule(activeModule.value.id) : appById('dashboard')
)

/** V3.1: the Dashboard has no rail entry — the titlebar's QuantableX mark is
 * the home button. The rail lists only the module apps. */
const railApps = computed(() => apps.filter((a) => a.id !== 'dashboard' && isAppEnabled(a.id)))

/**
 * Last-used module per app: clicking an app on the rail returns to where you
 * were inside it, not to a fixed first module. Persisted as one JSON object in
 * the `core` settings scope; localStorage keeps browser dev working.
 */
const lastModuleByApp = ref<Record<string, string>>({})
const LAST_MODULE_KEY = 'app.lastModule'

async function loadLastModules() {
  // Merge under the in-memory map: the immediate watcher below may already
  // have recorded the module this session started on, and that wins.
  try {
    const stored = await qs.core.getSetting<Record<string, string>>('core', LAST_MODULE_KEY)
    if (stored && typeof stored === 'object')
      lastModuleByApp.value = { ...stored, ...lastModuleByApp.value }
    return
  } catch {
    /* browser development */
  }
  try {
    const raw = localStorage.getItem('qss-last-modules')
    if (raw) lastModuleByApp.value = { ...JSON.parse(raw), ...lastModuleByApp.value }
  } catch {
    /* ignore */
  }
}

watch(activeModule, (m) => {
  if (!m || m.ownWindow) return
  const app = appForModule(m.id)
  if (!app || lastModuleByApp.value[app.id] === m.id) return
  lastModuleByApp.value = { ...lastModuleByApp.value, [app.id]: m.id }
  qs.core.setSetting('core', LAST_MODULE_KEY, lastModuleByApp.value).catch(() => {})
  try {
    localStorage.setItem('qss-last-modules', JSON.stringify(lastModuleByApp.value))
  } catch {
    /* ignore */
  }
}, { immediate: true })

/** Rail click: dashboard routes to `/`; an app opens its last-used module,
 * falling back to its first member (by appOrder). */
async function openApp(appId: string) {
  const app = appById(appId)
  if (!app || !isAppEnabled(app.id)) return
  if (app.route) {
    await router.push(app.route)
    return
  }
  const members = modulesForApp(app.id)
  const last = lastModuleByApp.value[app.id]
  const target = members.find((m) => m.id === last) ?? members[0]
  if (target) await router.push(entryRoute(target.route))
}

/** Suite workspaces. Loaded once here; the dashboard picker and the command
 * palette's file search read the same state. The titlebar no longer shows a
 * workspace chip (docs/PLAN-WORKSPACES.md) — a workspace binds QuantCode, not
 * the suite. */
const ws = useWorkspaces()

/**
 * The agent-call broker's shell half (E4): dispatches allowed calls, feeds
 * the approval queue in the drawer's Agent tab. Lives here so it runs even
 * while that tab is closed — an agent must not wait on a UI tab being open.
 */
const agentBroker = useAgentBroker(primaryWindow)

/**
 * The rail's process entry pointed at QuantConsole's suite session from P3.5
 * (docs/PLAN-CONSOLE.md) until 2026-08-26 — which meant pressing it while
 * working in QuantTerminal cost you the terminal. It goes to the shell's own
 * process screen again (`pages/processes.vue`, user decision): a page that
 * belongs to no module, so it takes none away. Every module stage stays warm
 * behind it and comes back instantly.
 */
const processesActive = computed(() => currentPath.value === '/processes')

/** Running process count for the rail's process dot (E4). Shared state, not a
 * local ref: the dashboard header shows the same count and used to fetch its
 * own snapshot once per activation, which then froze while the page was open.
 *
 * Read once at startup and again on every `core.process.changed` — never on a
 * clock. The count is what the modules last *reported* to qs-core's register
 * (it owns no handles and cannot observe them), and the register announces
 * each real transition, so the dot flips the moment a module reports; a module
 * that crashes without reporting keeps its process counted here until it says
 * otherwise. A suite that sits in the tray for days used to poll this every
 * five seconds for nobody. */
const runningProcesses = useState<number>('qss-running-processes', () => 0)
let offProcesses: (() => void) | undefined
async function countProcesses() {
  try {
    const list = await qs.core.processList()
    runningProcesses.value = list.filter((p) => p.status.state === 'running').length
  } catch {
    runningProcesses.value = 0 // browser development
  }
}

/**
 * A module either navigates the stage, or — for QuantHUD alone — opens its
 * always-on-top overlay, which by nature cannot live inside another window.
 * That exception is declared as `ownWindow` in its manifest, not special-
 * cased here.
 */
const openError = ref<{ module: string; message: string } | null>(null)

async function openModule(id: string) {
  const target = moduleById(id)
  if (!target || !isModuleEnabled(target.id)) return
  openError.value = null

  if (!target.ownWindow) {
    await router.push(entryRoute(target.route))
    return
  }

  try {
    await invoke(target.ownWindow.command)
  } catch (e) {
    // Surfaced, not logged: every silent failure in this project so far
    // looked exactly like "the click did nothing".
    const message = e instanceof Error ? e.message : String(e)
    console.error(`[shell] failed to open ${id}`, e)
    openError.value = { module: target.title, message }
  }
}

/** Rail gear → the unified settings modal, scoped to the active module. */
function openSettings() {
  window.dispatchEvent(
    new CustomEvent('qss:settings', { detail: { module: activeModule.value?.id ?? null } })
  )
}

function onKeydown(e: KeyboardEvent) {
  // A module may claim a shortcut the shell also wants — QuantNotes binds
  // Ctrl+K to its own palette. Modules mount below the shell, so their
  // listeners run first; if one has already called preventDefault, the
  // shell stands down.
  if (e.defaultPrevented) return

  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
    e.preventDefault()
    paletteOpen.value = !paletteOpen.value
  } else if (isModuleEnabled('notes') && (e.ctrlKey || e.metaKey) && e.shiftKey && e.key.toLowerCase() === 'n') {
    // Quick capture (E5): a fresh shared note, from anywhere in the suite.
    e.preventDefault()
    window.dispatchEvent(new CustomEvent('qss:drawer', { detail: { tab: 'notes', capture: true } }))
  } else if (e.key === 'Escape' && paletteOpen.value) {
    paletteOpen.value = false
  }
}

/**
 * A module asking for the palette.
 *
 * QuantConsole's header has the launcher field in the middle of the window, and
 * the palette it opens lives up here. The alternative is a module synthesising a
 * Ctrl+K keydown, which is a lie the shell would have to believe.
 */
function onPaletteEvent() {
  paletteOpen.value = true
}

/** Shell components (drawer chips, palette rows) navigate via this event —
 * they live in packages/ui and have no router of their own. */
function onNavigateEvent(e: Event) {
  const route = (e as CustomEvent).detail?.route
  if (typeof route === 'string') router.push(entryRoute(route))
}

/** The dashboard's app cards open apps the same way the rail does —
 * last-used module first. */
function onOpenAppEvent(e: Event) {
  const id = (e as CustomEvent).detail?.app
  if (typeof id === 'string') void openApp(id)
}

let offFocus: (() => void) | undefined

onMounted(async () => {
  // Overlay windows run none of the shell's machinery: no shortcuts (the
  // HUD has its own), no tray-focus routing (it would navigate the overlay),
  // no agent broker or process count.
  if (overlayWindow) return

  window.addEventListener('keydown', onKeydown)
  window.addEventListener('qss:navigate', onNavigateEvent)
  window.addEventListener('qss:open-app', onOpenAppEvent)
  window.addEventListener('qss:palette', onPaletteEvent)

  // The tray menu routes by emitting on the bus rather than reaching into
  // the router — the same path a module would use (ARCHITECTURE.md §10).
  // Tray actions restore main; other windows keep their own navigation.
  if (primaryWindow) {
    offFocus = bus.on<{ module: string }>('core.module.focused', (event) => {
      openModule(event.payload.module)
    })
  }


  await loadLastModules()

  // "Reopen last workspace" (setting, off by default): a cold start on the
  // picker jumps straight back into the folder you were in.
  await ws.load()
  if (primaryWindow && ws.reopenLast.value && ws.active.value && currentPath.value === '/') {
    void openApp('quantspace')
  }

  // Subscribed before the first read, so a transition landing in between is
  // not lost; the handler re-reads rather than patching a copy from the payload.
  offProcesses = bus.on(PROCESS_CHANGED, () => void countProcesses())
  void countProcesses()

  await agentBroker.start()

  // QuantHUD runs whenever the suite runs (user decision): start its
  // overlay with the app. Idempotent — an already-open overlay is left be.
  const hud = moduleById('hud')
  if (primaryWindow && hud?.ownWindow) {
    try {
      await invoke(hud.ownWindow.command)
    } catch {
      // Browser development, or the hud plugin is absent — not fatal.
    }
  }

  // V3 warm cache, part two: once startup has settled, prefetch the other
  // modules' JS chunks in idle time. This loads code only — nothing mounts
  // until the first visit, it just doesn't wait on the network anymore.
  const idle: (fn: () => void) => void =
    'requestIdleCallback' in window
      ? (fn) => window.requestIdleCallback(fn, { timeout: 10_000 })
      : (fn) => void setTimeout(fn, 2000)
  idle(() => {
    for (const m of routableModules) {
      if (m.ownWindow || !isModuleEnabled(m.id)) continue // hud renders in its own window
      preloadRouteComponents(m.route).catch(() => {
        // A failed prefetch is harmless — the route loads normally on visit.
      })
    }
  })
})

onUnmounted(() => {
  if (overlayWindow) return
  window.removeEventListener('keydown', onKeydown)
  window.removeEventListener('qss:navigate', onNavigateEvent)
  window.removeEventListener('qss:open-app', onOpenAppEvent)
  window.removeEventListener('qss:palette', onPaletteEvent)
  offFocus?.()
  offProcesses?.()
  agentBroker.stop()
})

provide('openModule', openModule)
</script>

<template>
  <!-- HUD overlay windows: bare and transparent, exactly the standalone
       app's root. data-module stays — it is what hud's stylesheet scopes to. -->
  <div v-if="overlayWindow" class="qss-overlay" :data-module="activeModule?.id">
    <NuxtLayout>
      <NuxtPage />
    </NuxtLayout>
  </div>

  <div v-else class="qss-app">
    <QTitlebar @home="router.push('/')" />

    <div class="qss-body">
      <QRail
        :apps="railApps"
        :active-app-id="activeApp?.id"
        :active-module-id="activeModule?.id"
        :running="runningProcesses"
        :processes-active="processesActive"
        @select="openApp"
        @open-module="openModule"
        @settings="openSettings"
        @processes="router.push('/processes')"
      />

      <main class="qss-stage">
        <div v-if="appAvailability.error.value" class="qss-error" role="alert">
          Could not load app settings. Open Settings > Apps to retry.
        </div>
        <div v-if="openError" class="qss-error" role="alert">
          <strong>Could not open {{ openError.module }}</strong>
          <span>{{ openError.message }}</span>
          <button class="qss-error-x" @click="openError = null">&times;</button>
        </div>

        <!-- V3 warm cache: one ModuleStage per module, keyed by module id and
             kept alive across switches. A module you have visited keeps its
             full tree (state, terminals, charts) and reactivates instantly;
             ModuleStage pins its route so hidden stages never render foreign
             pages. Bounded by the module count (~10 stages), so no `max`.
             The enter animation lives in ModuleStage and is replayed on
             activation — still a CSS keyframe, NOT a <Transition> (rAF never
             fires in a hidden window; see git history). -->
        <KeepAlive>
          <ModuleStage
            v-if="appAvailability.ready.value && isRouteEnabled(currentPath)"
            :key="stageForRoute(currentPath) ?? 'shell'"
            :stage-id="stageForRoute(currentPath)"
            :module-id="activeModule?.id ?? null"
          />
        </KeepAlive>
      </main>
    </div>

    <QDrawer
      :agent-queue="agentBroker.queue.value"
      @approve="agentBroker.approve"
      @reject="agentBroker.reject"
    />

    <QCommandPalette
      :open="paletteOpen"
      :workspace-path="ws.active.value?.path"
      @close="paletteOpen = false"
      @navigate="router.push(entryRoute($event))"
    />

    <!-- The ONE settings surface (V3). Opens on `qss:settings` from any
         module header's gear; suite sections + the module's sections. -->
    <QSettingsModal />
  </div>
</template>

<style scoped>
/*
  Flex column: titlebar / body / drawer. The drawer takes height from the
  body by relayout, never by overlaying it — a chart half-hidden behind a
  panel reads as broken (PLAN-V2 §7).

  The flowing look (E6, Rectury reference): the chrome — titlebar, rail,
  drawer strip — is ONE continuous raised surface with no separator lines;
  the module floats in it as a rounded panel. Tone and radius do the work
  borders used to do.
*/
.qss-app {
  display: flex;
  flex-direction: column;
  height: 100%;
  /* The frame: darkest layer, so the bars read as separate from content. */
  background: var(--qss-bg-chrome);
}

/* HUD overlay windows: the page paints only what the overlay paints. */
.qss-overlay {
  height: 100%;
  background: transparent;
}

.qss-body {
  flex: 1;
  min-height: 0;
  display: flex;
}

.qss-stage {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  /* No left padding — the rail carries its own symmetric gutters, so the
     module panel starts right at its edge and the dark strip is ONE piece.
     Right: a 5px gutter (user request 2026-08-21) so the panel's rounded
     edge reads clearly against the frame. Paired with --qss-rail-w dropping
     60→55 so the panel keeps its full width and shifts left rather than
     shrinking. No bottom gutter: the drawer strip owns that 8px itself
     (--qss-drawer-bar-h), so the tab labels sit centred in the whole strip
     the eye reads rather than low in a 33px slice of it. The module panel's
     bottom edge lands in exactly the same place either way. */
  padding: 0 5px 0 0;
}

/* .qss-module-root (incl. enter animation) lives in ModuleStage.vue now. */

.qss-error {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 12px;
  background: color-mix(in srgb, var(--qss-warning) 14%, transparent);
  border-bottom: 1px solid color-mix(in srgb, var(--qss-warning) 40%, transparent);
  color: var(--qss-text);
  font-size: 12px;
}
.qss-error span {
  color: var(--qss-text-muted);
  font-family: var(--qss-font-mono);
  font-size: 11px;
  min-width: 0;
  overflow-wrap: anywhere;
}
.qss-error-x {
  margin-left: auto;
  background: none;
  border: none;
  color: var(--qss-text-muted);
  font-size: 16px;
  line-height: 1;
  cursor: pointer;
}
</style>
