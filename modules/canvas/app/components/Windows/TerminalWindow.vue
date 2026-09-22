<script setup lang="ts">
/**
 * A canvas terminal window — one QuantConsole session (P4.5,
 * docs/PLAN-CONSOLE.md).
 *
 * This used to be its own xterm instance plus its own PTY commands in the canvas
 * plugin. There is one emulator in the suite now: the session is opened by
 * `plugin:console`, parsed by its block engine, and rendered by `ConsolePane` —
 * so a canvas terminal gets blocks, per-block copy, search and history for free,
 * and the PTY details have one implementation rather than two that drift apart.
 *
 * The deliberate coupling: this module renders a component that belongs to
 * `console`. Nuxt registers components globally across layers so it resolves, but
 * it is a cross-module dependency — the only one in the suite — and the
 * alternative was keeping a second block renderer alive here. Recorded in
 * docs/PLAN-CONSOLE.md §11 rather than left for someone to discover.
 *
 * The same coupling carries the keymap: the pane chords this window answers
 * (split, close, zoom, find) are derived live from the console's one action
 * table via `consolePaneKeys()` — imported relatively from
 * `modules/console/app/keymap.ts`, which module isolation permits (it is not a
 * composable and there is no cross-module-import rule; the component tag above
 * crosses the same line). A copy is how this window ended up zooming on `Alt+Z`
 * while the console shipped `Ctrl+Shift+Enter`, deaf to every rebind (q1 #22).
 *
 * `owner: 'canvas'` is what keeps this session out of the console's tab bar: both
 * live in one registry, and the owner is how the console's adoption sweep tells
 * them apart.
 *
 * The window frame around this is the pane's chrome, so everything the pane's
 * own title bar would show — what is running, where, whether it is still alive —
 * arrives through `@state` and is drawn on the frame instead.
 */
import { computed, inject, onMounted, onUnmounted, ref, watch } from 'vue'
import { bus, qs, type ConsoleShell } from '@quantsuite/core'
import { consolePaneKeys, type ConsolePaneKey } from '../../../../console/app/keymap'
import { useWorkspacesStore } from '../../../stores/workspaces'
import { useCanvasStore } from '../../../stores/canvas'
import { useAppStore } from '../../../stores/app'
import type { CanvasWindow, ShellType, WindowStatus } from '../../../shared/types'

const props = defineProps<{
  window: CanvasWindow
}>()

const workspacesStore = useWorkspacesStore()
const canvasStore = useCanvasStore()
const appStore = useAppStore()

const workspaceId = computed(() => workspacesStore.contentWorkspaceId!)
const cwd = computed(() => workspacesStore.contentWorkspace?.path ?? '.')

/** Shells this machine actually has, as reported by the console plugin. */
const shells = ref<ConsoleShell[]>([])
const shellPath = ref<string | null>(null)
const shellLabel = ref<string | null>(null)
const started = ref(false)
const failure = ref<string | null>(null)

/**
 * A fresh window asks which shell to run before anything is spawned; one that
 * already carries a session — or a shell chosen earlier — skips the picker.
 */
const pickerOpen = ref(false)

/**
 * The session to re-attach to, or `null` to open a new one.
 *
 * `null` is not a fallback: it is the difference between the pane calling
 * `attach` and calling `open`, and an id that names no live session sends it
 * down the attach branch to fail there. Only an id the plugin minted and that
 * `alive()` has just confirmed belongs in here.
 */
const sessionId = ref<string | null>(props.window.terminalId ?? null)

const pane = ref<{
  pushSize: () => void
  prefill: (text: string) => void
  openSearch: () => void
} | null>(null)

/**
 * The canvas window stores a *kind* (`powershell`, `git-bash`); the plugin
 * reports paths it verified exist. Matching by id keeps a reopened window on the
 * shell it had, and drops silently when that shell is no longer installed.
 */
function matchShell(kind: ShellType | undefined): ConsoleShell | null {
  if (!kind) return null
  const wanted = kind === 'git-bash' ? 'gitbash' : kind
  return shells.value.find((shell) => shell.id === wanted) ?? null
}

const defaultShell = computed(
  () => shells.value.find((shell) => shell.isDefault) ?? shells.value[0] ?? null
)

/** The app's configured default, when it names something that exists. */
const preferred = computed(() => matchShell(appStore.defaultShell) ?? defaultShell.value)

onMounted(async () => {
  // The pane chords, resolved against the user's rebinds — and kept resolved:
  // the settings panel writes, the bus says so, this window re-derives.
  void applyKeymap()
  offKeymap = bus.on<{ scope: string; key: string }>('core.setting.changed', (event) => {
    if (event.payload?.scope === 'console' && event.payload.key === 'keymap') void applyKeymap()
  })

  try {
    shells.value = await qs.console.listShells()
  } catch (e) {
    // No bridge (browser development), or the command was refused. Either way the
    // picker has nothing to offer, and "nothing to offer" must not look like an
    // empty window — the reason goes on screen.
    // Browser development: the same stand-in the console page uses, so the
    // window can be laid out and driven without the suite binary. Nothing is
    // spawned — the pane renders its fixture.
    if (hasTauriBridge()) {
      shells.value = []
      failure.value = `No shells could be listed: ${e instanceof Error ? e.message : String(e)}`
    } else {
      shells.value = DEMO_SHELLS
    }
  }

  const existing = props.window.terminalId
  if (existing) {
    let live = false
    try {
      live = await qs.console.alive(existing)
    } catch {
      // Gone; fall through and open a fresh one.
    }
    if (live) {
      // A session from a previous mount (workspace switch) wins — never re-ask.
      sessionId.value = existing
      shellLabel.value = matchShell(props.window.shellType)?.label ?? null
      started.value = true
      return
    }
    // Forget it here *and* on disk: a dead id that survives the check is the one
    // that reaches the pane's re-attach branch.
    sessionId.value = null
    canvasStore.updateWindow(workspaceId.value, props.window.id, { terminalId: undefined })
  }

  const chosen = matchShell(props.window.shellType)
  if (!chosen && props.window.shellType && shells.value.length) {
    // The workspace remembers a shell this machine no longer has. Saying so beats
    // silently falling through to a picker that has lost the user's choice.
    failure.value = `${props.window.shellType} is not installed on this machine — pick another shell.`
  }
  if (chosen) {
    // The shell was picked before (its PTY died, or this is a reopened
    // workspace): respawn it without asking again.
    shellPath.value = chosen.path
    shellLabel.value = chosen.label
    sessionId.value = null
    started.value = true
    return
  }

  canvasStore.updateWindow(workspaceId.value, props.window.id, { status: 'idle' })
  pickerOpen.value = true
})

/**
 * Whether the Tauri bridge is here at all.
 *
 * A failed `list_shells` in the app is a real failure and has to be reported; the
 * same failure in a browser is simply the absence of a backend, and there the
 * stand-in is what makes the surface reachable. Telling the two apart by the
 * bridge, not by the error, keeps a stand-in shell out of the real picker — its
 * path is empty, and spawning it would fail in a way nobody could read.
 */
function hasTauriBridge(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

/** Shells for browser development, where nothing can be detected. */
const DEMO_SHELLS: ConsoleShell[] = [
  { id: 'powershell', label: 'Windows PowerShell', path: '', isDefault: true },
  { id: 'cmd', label: 'Command Prompt', path: '', isDefault: false },
]

function chooseShell(shell: ConsoleShell) {
  if (started.value) return
  pickerOpen.value = false
  shellPath.value = shell.path
  shellLabel.value = shell.label
  sessionId.value = null

  const kind = (shell.id === 'gitbash' ? 'git-bash' : shell.id) as ShellType
  canvasStore.updateWindow(workspaceId.value, props.window.id, {
    shellType: kind,
    status: 'live',
  })
  applyTitle(shell.label)
  started.value = true
}

function onOpened(id: string) {
  // Persisted on the window, so a workspace switch reconnects instead of
  // spawning a second shell.
  canvasStore.updateWindow(workspaceId.value, props.window.id, {
    terminalId: id,
    status: 'live',
  })
}

function onExited() {
  canvasStore.updateWindow(workspaceId.value, props.window.id, { status: 'idle' })
}

// ── frame title and status ───────────────────────────────────────────────────

/**
 * The last title this window wrote for itself.
 *
 * A title is only overwritten while the stored one is still the one written from
 * here, so anything the user typed survives. Titles from before the shell was
 * ever picked (`Terminal`) and from the earlier `Terminal · <shell>` scheme
 * count as ours, or a saved workspace would freeze on its first label forever.
 */
const autoTitle = ref<string | null>(
  /^Terminal(\s·\s.+)?$/.test(props.window.title) ? props.window.title : null
)

function applyTitle(next: string) {
  if (autoTitle.value === null || props.window.title !== autoTitle.value) return
  if (next === autoTitle.value) return
  autoTitle.value = next
  canvasStore.updateWindow(workspaceId.value, props.window.id, { title: next })
}

function applyStatus(next: WindowStatus) {
  if (props.window.status === next) return
  canvasStore.updateWindow(workspaceId.value, props.window.id, { status: next })
}

/** The last path segment — a canvas header has room for a folder, not a path. */
function leafOf(path: string): string {
  return path.split(/[/\\]/).filter(Boolean).pop() ?? path
}

/**
 * What the pane knows about its session, drawn on the window frame: `live`
 * while the shell runs, `idle` once it exited. A plain terminal reports no
 * more — the running command lives on the shell's own line.
 */
function onState(state: { cwd: string; exited: boolean }) {
  applyStatus(state.exited ? 'idle' : 'live')
  const shell = shellLabel.value ?? 'Terminal'
  applyTitle(state.cwd ? `${shell} · ${leafOf(state.cwd)}` : shell)
}

watch(failure, (message) => {
  if (message) applyStatus('error')
})

/** The copy confirmation the pane's own chrome would have shown. */
const copied = ref<string | null>(null)
let copiedTimer: ReturnType<typeof setTimeout> | undefined

function onCopied() {
  copied.value = 'Copied'
  clearTimeout(copiedTimer)
  copiedTimer = setTimeout(() => {
    copied.value = null
  }, 1600)
}

// ── focus ────────────────────────────────────────────────────────────────────

/**
 * Whether this window is the one the keyboard belongs to.
 *
 * `bringToFront` already stacks the clicked window above the rest, so the
 * topmost non-minimized window is the focused one without a second notion of
 * focus to keep in step with the first.
 */
const isFocused = computed(() => {
  const windows = canvasStore.activeCanvasState?.windows ?? []
  let top: CanvasWindow | null = null
  for (const win of windows) {
    if (win.minimized) continue
    if (!top || win.zIndex > top.zIndex) top = win
  }
  return top?.id === props.window.id
})

// ── pane actions ─────────────────────────────────────────────────────────────

const spawnTerminalBeside = inject<
  (source: CanvasWindow, axis: 'row' | 'col', before?: boolean) => void
>(
  'spawnTerminalBeside'
)
const windowActions = inject<{ close: () => unknown; toggleZoom: () => void }>(
  'canvasWindowActions'
)

/**
 * The pane's shortcuts — labels for its menus' right column, chords for the
 * keydown below.
 *
 * Derived from the console's keymap rather than copied (q1 #22), because a
 * terminal that answers `Ctrl+Shift+D` in one surface and not the other is two
 * terminals: the same table the console page binds, resolved through the same
 * `console`-scope `keymap` setting, so a rebind made in the console's settings
 * reaches this window too — live, via the same `core.setting.changed` event the
 * page listens for. They are bound on this window rather than on the document:
 * a canvas key that fires while the user is somewhere else on the canvas is the
 * bug in the layer above.
 */
const paneKeys = ref<ConsolePaneKey[]>(consolePaneKeys())

/** What `ConsolePane` prints in its menus — the shape the console page hands it. */
const paneKeyLabels = computed<Record<string, string>>(() =>
  Object.fromEntries(paneKeys.value.map((key) => [key.id, key.label]))
)

async function applyKeymap() {
  let overrides: Record<string, unknown> | null = null
  try {
    overrides = await qs.core.getSetting<Record<string, unknown>>('console', 'keymap')
  } catch {
    // No backend (browser development): the shipped chords are already in place.
    return
  }
  paneKeys.value = consolePaneKeys(overrides && typeof overrides === 'object' ? overrides : {})
}

let offKeymap: (() => void) | undefined

/**
 * A pane split, read for a canvas.
 *
 * There is no frame to divide here — the window *is* the frame — so the honest
 * reading is a second terminal against the matching edge. `before` picks which
 * edge: left and up rather than right and down.
 */
function onSplit(direction: 'right' | 'down' | 'left' | 'up') {
  const axis = direction === 'right' || direction === 'left' ? 'row' : 'col'
  const before = direction === 'left' || direction === 'up'
  spawnTerminalBeside?.(props.window, axis, before)
}

function onZoom() {
  windowActions?.toggleZoom()
}

function onClose() {
  windowActions?.close()
}

function onKeydown(event: KeyboardEvent) {
  // The same rule `useShortcuts` applies: a handler closer to the key — the
  // editor accepting its suggestion on Ctrl+F — marks the event handled with
  // preventDefault, and matching it anyway would run the action a second time
  // on the same keystroke.
  if (event.defaultPrevented) return

  for (const entry of paneKeys.value) {
    const hit = entry.chords.some(
      (chord) =>
        chord.key.toLowerCase() === event.key.toLowerCase() &&
        (chord.ctrl ?? false) === (event.ctrlKey || event.metaKey) &&
        (chord.alt ?? false) === event.altKey &&
        (chord.shift ?? false) === event.shiftKey
    )
    if (!hit) continue
    event.preventDefault()
    switch (entry.id) {
      case 'splitRight':
        onSplit('right')
        break
      case 'splitDown':
        onSplit('down')
        break
      case 'closePane':
        onClose()
        break
      case 'zoomPane':
        onZoom()
        break
    }
    return
  }
}

// ── PTY size ─────────────────────────────────────────────────────────────────

/**
 * A canvas zoom moves only a CSS transform: no layout box changes, so nothing
 * the pane watches for a resize fires, and the shell keeps the column count it
 * was given at the previous scale — `git log` and every progress bar then wrap
 * against a width that no longer exists.
 */
let resizeTimer: ReturnType<typeof setTimeout> | undefined

watch(
  () => canvasStore.activeCanvasState?.transform.scale,
  () => {
    clearTimeout(resizeTimer)
    // One resize per gesture rather than one per wheel notch: each reaches a
    // real ConPTY, and a shell redrawing at every intermediate width flickers.
    resizeTimer = setTimeout(() => pane.value?.pushSize(), 120)
  }
)

onUnmounted(() => {
  clearTimeout(copiedTimer)
  clearTimeout(resizeTimer)
  offKeymap?.()
})

// The frame's "attach file" button prefills the pane's input rather than writing
// to the PTY, so the block engine sees the same command line the shell does.
defineExpose({
  prefill: (text: string) => pane.value?.prefill(text),
})
</script>

<template>
  <!--
    `qc-terminal` is what makes the pane belong to this window.

    `ConsolePane` and everything under it is written against the suite's own
    tokens (`--qss-*`), which are a different, warmer grey than the canvas
    palette. Rendered inside a canvas window that reads as a slab of the wrong
    colour bolted into the frame — an opaque `#212121` pane on a `#18181e`
    window, with borders and muted text from a palette nothing else here uses.

    Custom properties inherit, so redefining them on this wrapper re-skins the
    whole subtree — including xterm, which resolves the same tokens through
    `getComputedStyle` when a full-screen program opens the alternate screen.
    Nothing in QuantConsole has to know about it, and the console page is
    untouched: this only holds where a canvas window is the host.
  -->
  <div
    class="qc-terminal w-full h-full overflow-hidden relative"
    @keydown="onKeydown"
  >
    <ConsolePane
      v-if="started"
      ref="pane"
      :session-id="sessionId"
      :shell="shellPath"
      :cwd="cwd"
      :visible="!window.minimized"
      :label="shellLabel ?? undefined"
      :active="isFocused"
      :keys="paneKeyLabels"
      owner="canvas"
      @opened="onOpened"
      @exited="onExited"
      @failed="failure = $event"
      @state="onState"
      @copied="onCopied"
      @split="onSplit"
      @zoom="onZoom"
      @close="onClose"
    />

    <!-- Shell picker — shown before any session is opened. -->
    <div
      v-if="pickerOpen"
      class="absolute inset-0 z-20 flex flex-col items-center justify-center gap-3 px-4"
      :style="{ background: 'var(--qc-bg)' }"
    >
      <p class="text-[10px] uppercase tracking-wider" :style="{ color: 'var(--qc-text-dim)' }">
        Choose terminal type
      </p>
      <div class="flex flex-col gap-1 w-full max-w-[240px]">
        <button
          v-for="shell in shells"
          :key="shell.id"
          class="w-full flex items-center justify-between gap-2 px-3 py-2 text-xs rounded transition-colors hover:brightness-125"
          :style="{
            background: 'var(--qc-bg-header)',
            border: '1px solid var(--qc-border)',
            color: 'var(--qc-text)',
          }"
          @click="chooseShell(shell)"
        >
          <span>{{ shell.label }}</span>
          <span
            v-if="shell.id === preferred?.id"
            class="text-[9px] uppercase tracking-wider"
            :style="{ color: 'var(--qc-text-dim)' }"
          >default</span>
        </button>
      </div>
      <p v-if="!shells.length" class="text-xs" :style="{ color: 'var(--qc-text-dim)' }">
        No shell backend available — the terminal needs the suite binary.
      </p>
    </div>

    <!-- Copy confirmation. The pane emits it for its own chrome, which is off
         here, so the window says it instead — a copy with no acknowledgement
         reads as a copy that did not happen. -->
    <div
      v-if="copied"
      class="absolute bottom-2 left-1/2 -translate-x-1/2 z-20 px-2.5 py-1 text-[11px] rounded-full pointer-events-none"
      :style="{
        background: 'var(--qc-bg-header)',
        border: '1px solid var(--qc-border)',
        color: 'var(--qc-text-muted)',
      }"
    >
      Copied {{ copied }}
    </div>

    <div
      v-if="failure"
      class="absolute bottom-0 left-0 right-0 px-3 py-2 text-xs"
      :style="{ background: 'var(--qc-bg-header)', color: 'var(--qc-text)' }"
    >
      {{ failure }}
    </div>
  </div>
</template>

<style scoped>
/* See the comment on the root element. Only the tokens the pane subtree
   actually reads are mapped; anything else keeps the suite value, which is
   correct — a duration or an easing curve has no palette. */
.qc-terminal {
  --qss-bg: var(--qc-bg);
  --qss-bg-chrome: var(--qc-bg-titlebar);
  --qss-bg-raised: var(--qc-bg-window);
  --qss-bg-card: var(--qc-bg-header);
  --qss-bg-hover: var(--qc-bg-surface);
  --qss-bg-overlay: var(--qc-bg-header);
  --qss-border: var(--qc-border);
  --qss-border-subtle: var(--qc-border-subtle);
  --qss-text: var(--qc-text);
  --qss-text-secondary: var(--qc-text-muted);
  --qss-text-muted: var(--qc-text-dim);
  --qss-accent: var(--qc-accent);
  --qss-font-mono: var(--qc-font-mono);
  background: var(--qc-bg);
}
</style>
