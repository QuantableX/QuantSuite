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
import { useSidebarStore } from '#pilot/stores/sidebar'
import { chime } from '#pilot/utils/chime'
import { threadReference } from '#pilot/utils/format'
import type { PilotEnvelope, ThreadEnvelope } from '#pilot/types'

const app = useAppStore()
const store = usePilotStore()
const threads = useThreadStore()
const sidebar = useSidebarStore()

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

/** Typing in a field keeps its own Ctrl+Z, F2 and friends. */
function inEditable(e: KeyboardEvent): boolean {
  const el = e.target as HTMLElement | null
  return Boolean(el?.closest('input, textarea, select, [contenteditable="true"], .xterm'))
}
function onActive(e: KeyboardEvent, run: (id: string) => unknown) {
  const id = store.activeId
  if (!id) return
  e.preventDefault()
  void run(id)
}
const digit = (n: number) => ({
  key: String(n),
  ctrl: true,
  shift: false,
  alt: false,
  handler: (e: KeyboardEvent) => {
    e.preventDefault()
    void sidebar.openNth(n - 1)
  },
})
const step = (key: string, dir: 1 | -1) => ({
  key,
  ctrl: true,
  shift: true,
  handler: (e: KeyboardEvent) => {
    e.preventDefault()
    void sidebar.step(dir)
  },
})

// ── Files from Explorer anywhere in the module go to the open thread's composer (card 9) ──
function carriesFiles(e: DragEvent) {
  return Array.from(e.dataTransfer?.types ?? []).includes('Files')
}
function onFileDrag(e: DragEvent) {
  if (!carriesFiles(e) || !store.activeId) return
  e.preventDefault()
  if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy'
  threads.fileDrag = true
}
function onFileLeave(e: DragEvent) {
  // Leaving the module, not just moving between its children.
  if (!e.relatedTarget || !(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node)) threads.fileDrag = false
}
function onFileDrop(e: DragEvent) {
  threads.fileDrag = false
  if (!carriesFiles(e) || !store.activeId) return
  e.preventDefault()
  const files = Array.from(e.dataTransfer?.files ?? [])
  if (files.length) threads.dropFiles(store.activeId, files)
}

useShortcuts([
  // The sidebar's keys (card 7; docs/PLAN-QUANTPILOT.md §7). Explicit
  // shift/alt keep Ctrl+N clear of the suite's Ctrl+Shift+N (notes).
  {
    key: 'n',
    ctrl: true,
    shift: false,
    alt: true,
    handler: (e) => {
      e.preventDefault()
      void sidebar.newThread({ noProject: true })
    },
  },
  {
    key: 'n',
    ctrl: true,
    shift: false,
    alt: false,
    handler: (e) => {
      e.preventDefault()
      void sidebar.newThread()
    },
  },
  ...[1, 2, 3, 4, 5, 6, 7, 8, 9].map(digit),
  // Ctrl+Shift+[ reports "{" on US layouts.
  step('{', -1),
  step('[', -1),
  step('}', 1),
  step(']', 1),
  {
    key: 'S',
    ctrl: true,
    shift: true,
    alt: false,
    handler: (e) =>
      onActive(e, (id) => {
        const t = store.sessions.find((s) => s.id === id)
        return t?.settledAt ? sidebar.unsettle(id) : sidebar.settle(id)
      }),
  },
  {
    key: 'P',
    ctrl: true,
    shift: true,
    alt: false,
    handler: (e) =>
      onActive(e, (id) => {
        const t = store.sessions.find((s) => s.id === id)
        return t?.pinned ? sidebar.unpin(id) : sidebar.pin(id)
      }),
  },
  {
    key: 'C',
    ctrl: true,
    shift: true,
    alt: false,
    handler: (e) =>
      onActive(e, (id) => {
        const t = store.sessions.find((s) => s.id === id)
        if (t) return navigator.clipboard.writeText(threadReference(t)).catch(() => undefined)
      }),
  },
  {
    key: 'z',
    ctrl: true,
    shift: false,
    alt: false,
    handler: (e) => {
      if (inEditable(e) || !sidebar.latestUndo) return
      e.preventDefault()
      void sidebar.undoLatest()
    },
  },
  {
    key: 'F2',
    handler: (e) => {
      if (inEditable(e)) return
      onActive(e, (id) => {
        if (!app.sidebarLeftOpen) app.toggleSidebar('left')
        sidebar.renaming = id
      })
    },
  },
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
  <div class="qp-shell" @dragenter="onFileDrag" @dragover="onFileDrag" @dragleave="onFileLeave" @drop="onFileDrop">
    <PilotLayoutAppHeader />
    <div class="qp-body">
      <QSidebar :model-value="app.sidebarLeftOpen" resizable storage-key="pilot.left">
        <PilotLayoutLeftSidebar />
      </QSidebar>
      <main class="qp-main">
        <slot />
        <PilotLayoutUndoToast />
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
