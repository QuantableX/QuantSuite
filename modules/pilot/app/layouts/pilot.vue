<script setup lang="ts">
/**
 * QuantPilot module shell — header, the two 220px panels, the main stage.
 *
 * Three Tauri events feed the store here, deliberately not keep-alive
 * gated: a session keeps running while another module is in front, and
 * the face and the sidebar dots must be right when you come back.
 *   `pilot-thread-event` — V3 threads: timeline items, deltas, turns, state
 *   `pilot-event`     — V2 signals (hooks, tailed session files)
 *   `console-output`  — the pilot PTYs' bytes, for activity and the bell
 *   `console-session` — a PTY exited
 * Loading follows the warm-cache rule (V3): on mount only inside an active
 * KeepAlive tree, again on activation.
 */
import { inActiveKeepAliveTree, setModuleAttention, useShortcuts, useTauriEvent } from '@quantsuite/core'
import { useAppStore } from '#pilot/stores/app'
import { usePilotStore } from '#pilot/stores/pilot'
import { useThreadStore } from '#pilot/stores/thread'
import { chime } from '#pilot/utils/chime'
import type { PilotEnvelope, ThreadEnvelope } from '#pilot/types'

const app = useAppStore()
const store = usePilotStore()
const threads = useThreadStore()

useTauriEvent<ThreadEnvelope>('pilot-thread-event', (envelope) => {
  threads.apply(envelope)
})
// A "needs you" toast was clicked (crate/src/notify.rs).
useTauriEvent<{ threadId: string }>('pilot-open-thread', (payload) => {
  void navigateTo('/pilot')
  void store.openSession(payload.threadId)
})

/** Threads waiting on the user: the rail's badge, and the optional sound. */
const waitingCount = computed(
  () =>
    store.sessions.filter((s) => {
      const v = threads.views[s.id]
      return (v?.loaded ? v.state : s.state) === 'waiting'
    }).length
)
watch(waitingCount, (n, old) => {
  setModuleAttention('pilot', n)
  if (n > (old ?? 0) && store.settings?.notifySound) chime()
})
onUnmounted(() => setModuleAttention('pilot', 0))
useTauriEvent<PilotEnvelope>('pilot-event', (envelope) => {
  store.applyEvent(envelope.sessionId, envelope.event)
})
useTauriEvent<{ id: string; data: string }>('console-output', (payload) => {
  store.onOutput(payload.id, payload.data)
})
useTauriEvent<{ id: string; state: string }>('console-session', (payload) => {
  if (payload.state === 'exited') store.ptyExited(payload.id)
})

onMounted(() => {
  if (!inActiveKeepAliveTree()) return
  void store.load()
})
onActivated(() => {
  if (store.loaded) {
    void store.refreshSessions()
    void store.refreshContexts()
    void store.syncRuntime()
  } else {
    void store.load()
  }
})

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
      window.dispatchEvent(new CustomEvent('qss:settings', { detail: { module: 'pilot' } }))
    },
  },
])
</script>

<template>
  <div class="qp-shell">
    <PilotLayoutAppHeader />
    <div class="qp-body">
      <QSidebar :model-value="app.sidebarLeftOpen" resizable storage-key="pilot.left">
        <PilotLayoutLeftSidebar />
      </QSidebar>
      <main class="qp-main">
        <slot />
      </main>
      <QRightPanel :model-value="app.sidebarRightOpen" storage-key="pilot.right">
        <PilotLayoutRightSidebar />
      </QRightPanel>
    </div>
  </div>
</template>

<style scoped>
.qp-shell {
  display: flex;
  flex-direction: column;
  /* The module fills its container, never the viewport (ARCHITECTURE.md §9). */
  width: 100%;
  height: 100%;
  overflow: hidden;
}
.qp-body {
  display: flex;
  flex: 1;
  min-height: 0;
}
.qp-main {
  position: relative;
  flex: 1;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  background: var(--qss-bg, #18181e);
  display: flex;
  flex-direction: column;
}
</style>
