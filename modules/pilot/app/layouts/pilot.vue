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
import { inActiveKeepAliveTree, setModuleAttention, useTauriEvent } from '@quantsuite/core'
import { useAppStore } from '#pilot/stores/app'
import { usePilotStore } from '#pilot/stores/pilot'
import { useThreadStore } from '#pilot/stores/thread'
import { useSidebarStore } from '#pilot/stores/sidebar'
import { useGitStore } from '#pilot/stores/git'
import { useInspectorStore } from '#pilot/stores/inspector'
import { useDrawerStore } from '#pilot/stores/drawer'
import { useKeysStore } from '#pilot/stores/keys'
import { useUsageStore } from '#pilot/stores/usage'
import { useGithubStore } from '#pilot/stores/github'
import { chordOf, resolve, SUITE_KEYS, type KeyContext } from '#pilot/utils/keybindings'
import { runCommand, useCommands } from '#pilot/utils/commands'
import { moveFocus, noteFocus } from '#pilot/utils/focus'
import { setMathEnabled } from '#pilot/utils/markdown'
import { chime } from '#pilot/utils/chime'
import { threadReference } from '#pilot/utils/format'
import { pilot } from '#pilot/utils/invoke'
import type { PilotEnvelope, SessionMeta, ThreadEnvelope } from '#pilot/types'

const app = useAppStore()
const store = usePilotStore()
const threads = useThreadStore()
const sidebar = useSidebarStore()
const git = useGitStore()
const inspector = useInspectorStore()
const drawer = useDrawerStore()

useTauriEvent<ThreadEnvelope>('pilot-thread-event', (envelope) => {
  threads.apply(envelope)
  // An agent's `gh pr create` links the pull request at turn end (card 12).
  if (envelope.event.type === 'turn' && envelope.event.status !== 'started') void git.loadLinks()
})
// A "needs you" toast was clicked (crate/src/notify.rs).
useTauriEvent<{ threadId: string }>('pilot-open-thread', (payload) => {
  void navigateTo('/pilot')
  void store.openSession(payload.threadId)
})
// QuantMCP's pilot tools (card 22): a file or page for the thread on screen, a thread an agent started.
useTauriEvent<{ threadId: string; kind: 'file' | 'preview'; path?: string; line?: number | null; url?: string }>('pilot-agent-open', (p) => {
  if (p.threadId !== store.activeId) return
  if (p.kind === 'file' && p.path) inspector.openFile(p.threadId, p.path, p.line ?? null)
  else if (p.kind === 'preview' && p.url) inspector.openBrowser(p.threadId, p.url)
})
useTauriEvent<SessionMeta>('pilot-thread-created', (meta) => store.upsertSession(meta))

/** The thread on screen with its inspector: the agent tools open files and pages only there. */
const moduleActive = ref(false)
const onScreen = computed(() => {
  const id = store.activeId
  return moduleActive.value && id && store.page === 'thread' && app.view === 'conversation' && !store.terminalShown[id] ? id : null
})
watch(onScreen, (id) => void pilot('plugin:pilot|pilot_ui_visible', { threadId: id }))
onMounted(() => (moduleActive.value = inActiveKeepAliveTree()))
onActivated(() => (moduleActive.value = true))
onDeactivated(() => (moduleActive.value = false))
onUnmounted(() => void pilot('plugin:pilot|pilot_ui_visible', { threadId: null }))

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
  drawer.onOutput(payload.id, payload.data)
})
useTauriEvent<{ id: string; state: string }>('console-session', (payload) => {
  if (payload.state === 'exited') store.ptyExited(payload.id)
})

// Whether GitHub is there: the GitHub view's toggle and the `#` menu (card 23).
const github = useGithubStore()
watch(() => store.settings?.github, () => void github.loadStatus(), { immediate: true })
onMounted(() => {
  if (!inActiveKeepAliveTree()) return
  void store.load()
  void git.loadLinks()
})
onActivated(() => {
  void git.loadLinks()
  if (store.loaded) {
    void store.refreshSessions()
    void store.refreshContexts()
    void store.syncRuntime()
  } else {
    void store.load()
  }
})

// ── Keys (card 17): one dispatcher over the command registry ──
// Each keydown inside the module becomes a chord; the keybindings table
// (defaults, then the user's keybindings.json) names the command for it in
// this context, and whoever registered that command runs it. Handlers closer
// to the key (menus, the find bar, the terminal drawer) claim it first with
// preventDefault. The suite's own keys (Ctrl+K) are taken in the capture
// phase, before the shell sees them.
const keys = useKeysStore()
const usage = useUsageStore()
const shell = ref<HTMLElement | null>(null)

// ── Appearance (card 19): text size, panel motion, math ──
const ZOOM: Record<string, number> = { small: 0.92, default: 1, large: 1.08, larger: 1.16 }
const reducedMotion = typeof matchMedia !== 'undefined' ? matchMedia('(prefers-reduced-motion: reduce)') : null
const appearance = computed(() => {
  const s = store.settings
  const motion = reducedMotion?.matches ? 0 : (s?.panelMotionMs ?? 160)
  return { '--qp-text-zoom': String(ZOOM[s?.textSize ?? 'default'] ?? 1), '--qp-motion': `${motion}ms` }
})
watch(() => store.settings?.renderMath ?? true, (on) => setMathEnabled(on), { immediate: true })

function keyContext(target: EventTarget | null): KeyContext {
  const t = target instanceof Element ? target : null
  const id = store.activeId
  return {
    terminalFocus: Boolean(t?.closest('.xterm')),
    terminalOpen: Boolean(id && drawer.open[id]),
    previewFocus: Boolean(t?.closest('.qp-br')),
    previewOpen: inspector.open && inspector.tab === 'browser',
    inspectorFocus: Boolean(t?.closest('.qp-insp')),
    inspectorOpen: inspector.open,
    composerFocus: Boolean(t?.closest('.qp-composer')),
    editableFocus: Boolean(t?.closest('input, textarea, select, [contenteditable="true"], .xterm')),
    turnRunning: Boolean(id && threads.views[id]?.turnId),
    modelPickerOpen: Boolean(shell.value?.querySelector('.qp-mp')),
    paletteOpen: keys.paletteOpen,
    usagePageOpen: store.page === 'usage',
  }
}

function dispatch(e: KeyboardEvent, capture: boolean) {
  if (keys.recording || e.defaultPrevented || e.isComposing) return
  // Keys typed into the suite around the module (its settings, its drawer) are not ours.
  const t = e.target
  if (t instanceof Node && t !== document.body && !shell.value?.contains(t)) return
  const chord = chordOf(e)
  if (!chord || (capture && !SUITE_KEYS[chord])) return
  const command = resolve(keys.rules, chord, keyContext(t))
  // The open palette keeps the keyboard: only its own key closes it.
  if (keys.paletteOpen && command !== 'commandPalette.toggle') return
  if (command && runCommand(command)) e.preventDefault()
}
const onKeyCapture = (e: KeyboardEvent) => dispatch(e, true)
const onKey = (e: KeyboardEvent) => dispatch(e, false)
const onFocusIn = (e: FocusEvent) => shell.value && noteFocus(e.target, shell.value)
function armKeys() {
  window.addEventListener('keydown', onKeyCapture, true)
  window.addEventListener('keydown', onKey)
}
function disarmKeys() {
  window.removeEventListener('keydown', onKeyCapture, true)
  window.removeEventListener('keydown', onKey)
}
onMounted(() => {
  if (!inActiveKeepAliveTree()) return
  armKeys()
  if (!keys.loaded) void keys.load()
})
onActivated(armKeys)
onDeactivated(disarmKeys)
onUnmounted(disarmKeys)

/** A command for the open thread; `false` (the key goes on) without one. */
const onThread = (run: (id: string) => unknown) => () => {
  const id = store.activeId
  if (!id) return false
  void run(id)
}
const contextBar = (which: 'mode' | 'branch' | 'reuse') =>
  onThread((id) => window.dispatchEvent(new CustomEvent('qp-context-bar', { detail: { threadId: id, which } })))
const sessionOf = (id: string) => store.sessions.find((s) => s.id === id)
// Every opened thread counts for the palette's ranking.
watch(
  () => store.activeId,
  (id) => id && keys.noteOpen(id)
)

useCommands({
  'commandPalette.toggle': () => {
    if (!keys.paletteOpen) return keys.openPalette()
    keys.closePalette()
    void nextTick(() => shell.value?.querySelector<HTMLElement>('.qp-c-input')?.focus())
  },
  'navigation.back': () => store.goHistory(-1),
  'navigation.forward': () => store.goHistory(1),
  'workspace.focusNext': () => shell.value && moveFocus(1, shell.value),
  'workspace.focusPrevious': () => shell.value && moveFocus(-1, shell.value),
  'settings.open': () => window.dispatchEvent(new CustomEvent('qss:settings', { detail: { module: 'pilot' } })),
  // Explicit Ctrl+Alt+N keeps clear of the suite's Ctrl+Shift+N (notes).
  'chat.new': () => sidebar.newThread(),
  'chat.newWithoutProject': () => sidebar.newThread({ noProject: true }),
  'thread.previous': () => sidebar.step(-1),
  'thread.next': () => sidebar.step(1),
  ...Object.fromEntries([1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) => [`thread.jump.${n}`, () => sidebar.openNth(n - 1)])),
  'thread.stop': onThread((id) => threads.interrupt(id)),
  'thread.settle': onThread((id) => (sessionOf(id)?.settledAt ? sidebar.unsettle(id) : sidebar.settle(id))),
  'thread.pin': onThread((id) => (sessionOf(id)?.pinned ? sidebar.unpin(id) : sidebar.pin(id))),
  'thread.copyReference': onThread((id) => {
    const t = sessionOf(id)
    if (t) return navigator.clipboard.writeText(threadReference(t)).catch(() => undefined)
  }),
  'thread.rename': onThread((id) => {
    if (!app.sidebarLeftOpen) app.toggleSidebar('left')
    sidebar.renaming = id
  }),
  'thread.undo': () => (sidebar.latestUndo ? sidebar.undoLatest() : false),
  'thread.restart': onThread((id) => threads.restart(id)),
  'thread.openTui': onThread((id) => threads.openTui(id)),
  'sidebar.toggle': () => app.toggleSidebar('left'),
  'threadPanel.toggle': () => app.toggleSidebar('right'),
  'rightPanel.toggle': () => inspector.toggle(),
  'rightPanel.close': () => (inspector.open ? inspector.closeTab() : false),
  'view.reopenClosed': () => (inspector.closed.length ? inspector.reopenTab() : false),
  'diff.toggle': onThread(() => inspector.toggleTab('diff')),
  'filePicker.toggle': onThread(() => (inspector.picker = true)),
  'projectSearch.toggle': onThread(() => inspector.showSearch()),
  'preview.toggle': onThread(() => inspector.toggleTab('browser')),
  'terminal.toggle': onThread((id) => drawer.toggle(id)),
  'board.toggle': () => app.setView(app.view === 'board' ? 'conversation' : 'board'),
  'composer.workspace': contextBar('mode'),
  'composer.branch': contextBar('branch'),
  'composer.previousWorktree': contextBar('reuse'),
  // The Usage page (card 18); its own keys apply while it is open.
  'usage.open': () => {
    store.page = 'usage'
    void (usage.tab === 'limits' ? usage.loadLimits() : usage.load())
  },
  'usage.close': () => (store.page === 'usage' ? void (store.page = 'thread') : false),
  'usage.cost': () => usage.setTab('cost'),
  'usage.tokens': () => usage.setTab('tokens'),
  'usage.limits': () => usage.setTab('limits'),
  'usage.period.day': () => (usage.tab === 'limits' ? false : usage.setPeriod('24h')),
  'usage.period.week': () => (usage.tab === 'limits' ? false : usage.setPeriod('7d')),
  'usage.period.month': () => (usage.tab === 'limits' ? false : usage.setPeriod('30d')),
  'usage.period.quarter': () => (usage.tab === 'limits' ? false : usage.setPeriod('90d')),
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

</script>

<template>
  <div ref="shell" class="qp-shell" :style="appearance" @focusin="onFocusIn" @dragenter="onFileDrag" @dragover="onFileDrag" @dragleave="onFileLeave" @drop="onFileDrop">
    <PilotLayoutAppHeader />
    <div class="qp-body">
      <!-- The board and GitHub fill the main area: no thread list, no thread panel (cards 21, 23). -->
      <QSidebar :model-value="app.sidebarLeftOpen && app.view === 'conversation'" resizable storage-key="pilot.left">
        <PilotLayoutLeftSidebar />
      </QSidebar>
      <main class="qp-main">
        <slot />
        <PilotLayoutUndoToast />
        <PilotPalette v-if="keys.paletteOpen" />
        <PilotLayoutWorktreeCleanup />
      </main>
      <QRightPanel :model-value="app.sidebarRightOpen && app.view === 'conversation'" storage-key="pilot.right">
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
