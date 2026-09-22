<script setup lang="ts">
/**
 * One terminal pane — a PTY session rendered by xterm.js, nothing in between.
 *
 * This is the plain-terminal contract (2026-08-20 rollback, the user's call):
 * the block model, the input editor and everything built on them are gone; the
 * shell's own line editing is the input, the scrollback is xterm's, and the
 * one piece of UI kept beyond the terminal itself is **image paste** — a
 * screenshot pasted while a program like `claude` runs arrives as a bracketed
 * paste of the saved file's path, which is how such programs recognise an
 * attachment (measured against a real `claude`: bracketed → `[Image #1]`,
 * typed → literal text).
 *
 * Hosts: the console page (with `chrome`, inside tabs/splits) and a QuantCanvas
 * terminal window (`chrome` off — the window frame is the chrome). Both rely on
 * the V3 warm cache: `onActivated`/`onDeactivated` attach and detach the
 * stream, and `qs-pty` buffers while nobody listens.
 */
import { computed, nextTick, onActivated, onBeforeUnmount, onDeactivated, onMounted, ref } from 'vue'
import { qs, useTauriEvent } from '@quantsuite/core'
import type { Terminal } from '@xterm/xterm'
import { useAnsiPalette } from '../composables/useAnsiPalette'

const props = withDefaults(
  defineProps<{
    /** Re-attach to this session; `null` opens a new one. Never a fallback. */
    sessionId?: string | null
    /** Shell executable; `null` lets the backend pick the platform default. */
    shell?: string | null
    cwd?: string
    /** The pane is on a visible surface (tab in front, window not minimized). */
    visible?: boolean
    /**
     * The pane is laid out at real size. Defaults **true** on purpose: Vue
     * turns an absent boolean prop into `false`, and a host that never heard
     * of the flag would otherwise have its pane hidden by the very prop that
     * lets the console page hide background tabs.
     */
    staged?: boolean
    /** Draw the pane's own title bar. Canvas windows bring their own frame. */
    chrome?: boolean
    active?: boolean
    label?: string
    /** Chord labels for the menu, from the page's keymap. */
    keys?: Record<string, string | undefined>
    /**
     * Chords that belong to the page, not the terminal (tab switching): the
     * key handler lets them bubble instead of feeding them to the PTY.
     */
    terminalOverrides?: string[]
    /**
     * Which surface this pane belongs to. The console's tab bar adopts running
     * `'console'` sessions on a reload — a canvas window's shell must say
     * `'canvas'` or it grows a console tab it never asked for.
     */
    owner?: 'console' | 'canvas'
  }>(),
  { staged: true, visible: true, chrome: false, active: false, cwd: '.', owner: 'console' }
)

const emit = defineEmits<{
  (e: 'opened', sessionId: string): void
  (e: 'exited'): void
  (e: 'failed', message: string): void
  (e: 'state', payload: { cwd: string; exited: boolean }): void
  (e: 'copied'): void
  (e: 'split', direction: 'right' | 'down' | 'left' | 'up'): void
  (e: 'zoom'): void
  (e: 'close'): void
  (e: 'focus'): void
}>()

const root = ref<HTMLElement | null>(null)
const host = ref<HTMLElement | null>(null)
const sessionId = ref<string | null>(props.sessionId ?? null)
const exited = ref(false)
const paneMenu = ref(false)

const palette = useAnsiPalette()

let term: Terminal | null = null
let fit: { fit: () => void } | null = null
let resizeObserver: ResizeObserver | null = null
let detachTimer: ReturnType<typeof setTimeout> | null = null
/** Bytes that arrived before xterm existed. */
const pending: string[] = []

/** `var(--qss-*)`/palette entries resolved to real colours — xterm takes no `var()`. */
function resolveColour(value: string): string {
  const el = root.value ?? document.documentElement
  const probe = document.createElement('span')
  probe.style.color = value
  probe.style.display = 'none'
  el.appendChild(probe)
  const resolved = getComputedStyle(probe).color
  probe.remove()
  return resolved
}

function terminalTheme() {
  const styles = root.value ? getComputedStyle(root.value) : null
  const token = (name: string, fallback: string) =>
    styles?.getPropertyValue(name).trim() || fallback
  const ansi = Array.from({ length: 16 }, (_, i) => resolveColour(palette.colour(i)))
  return {
    background: token('--qss-bg', '#212121'),
    foreground: token('--qss-text', '#e3e3e6'),
    cursor: token('--qss-accent', '#c9c9d1'),
    selectionBackground: resolveColour('color-mix(in srgb, var(--qss-accent) 30%, transparent)'),
    black: ansi[0], red: ansi[1], green: ansi[2], yellow: ansi[3],
    blue: ansi[4], magenta: ansi[5], cyan: ansi[6], white: ansi[7],
    brightBlack: ansi[8], brightRed: ansi[9], brightGreen: ansi[10], brightYellow: ansi[11],
    brightBlue: ansi[12], brightMagenta: ansi[13], brightCyan: ansi[14], brightWhite: ansi[15],
  }
}

/** Parse "Ctrl+Shift+D" the way the keymap prints it, for the override check. */
function chordOf(event: KeyboardEvent): string {
  const parts: string[] = []
  if (event.ctrlKey) parts.push('Ctrl')
  if (event.altKey) parts.push('Alt')
  if (event.shiftKey) parts.push('Shift')
  const key = event.key.length === 1 ? event.key.toUpperCase() : event.key
  parts.push(key)
  return parts.join('+')
}

async function createTerminal() {
  if (term || !host.value) return
  const [{ Terminal }, { FitAddon }, { Unicode11Addon }] = await Promise.all([
    import('@xterm/xterm'),
    import('@xterm/addon-fit'),
    import('@xterm/addon-unicode11'),
  ])
  await import('@xterm/xterm/css/xterm.css')

  // The Nerd Font ships as a webfont (see shell.css). xterm measures cell
  // width at construction, so constructing before the face resolves bakes in
  // a fallback's metrics and the grid never lines up again.
  await document.fonts.ready

  const fontSize = Number.parseFloat(
    root.value ? getComputedStyle(root.value).getPropertyValue('--cpane-font-size') : ''
  ) || 13

  const instance = new Terminal({
    fontFamily: 'CaskaydiaCove Nerd Font, Cascadia Code, JetBrains Mono, ui-monospace, Consolas, monospace',
    fontSize,
    lineHeight: 1.2,
    cursorBlink: false,
    allowProposedApi: true,
    scrollback: 10_000,
    theme: terminalTheme(),
  })
  const fitAddon = new FitAddon()
  instance.loadAddon(fitAddon)
  instance.loadAddon(new Unicode11Addon())
  instance.unicode.activeVersion = '11'

  instance.attachCustomKeyEventHandler((event) => {
    if (event.type !== 'keydown') return true
    // Page-owned chords (tab switching etc.) bubble past xterm — returning
    // false here happens before xterm's stopPropagation, so `window` gets them.
    if (props.terminalOverrides?.includes(chordOf(event))) return false
    // Shift+Arrows select from the cursor, Windows-Terminal style — see below.
    if (
      event.shiftKey &&
      !event.ctrlKey &&
      !event.altKey &&
      (event.key === 'ArrowLeft' || event.key === 'ArrowRight' || event.key === 'ArrowUp' || event.key === 'ArrowDown')
    ) {
      event.preventDefault()
      extendKeyboardSelection(instance, event.key)
      return false
    }
    // Ctrl+C with a selection copies; without one it is the shell's interrupt.
    // The selection is cleared after the copy on purpose: the *next* Ctrl+C has
    // to interrupt again, not silently re-copy a selection from minutes ago.
    if (event.ctrlKey && !event.shiftKey && !event.altKey && event.key === 'c' && instance.hasSelection()) {
      void navigator.clipboard
        .writeText(instance.getSelection())
        .then(() => {
          instance.clearSelection()
          kbAnchor = null
          emit('copied')
        })
        .catch(() => {})
      return false
    }
    // Ctrl+V pastes. Handled by NOT letting xterm process the keydown (it
    // would send a literal ^V byte to the PTY) while also not cancelling it —
    // the browser then fires its native `paste` event: xterm's own textarea
    // handler pastes text (bracketed-paste aware), and the capture handler
    // above turns a pasted image into a saved file plus its path.
    if (event.ctrlKey && !event.shiftKey && !event.altKey && event.key === 'v') {
      return false
    }
    // Any other *real* key ends a keyboard selection: Escape (and typing)
    // clears it, so the next Shift+Arrow anchors at the cursor again. A bare
    // modifier press is not a key — pressing Ctrl on the way to Ctrl+C fires
    // its own keydown (`key: "Control"`) first, and treating that as "typing"
    // dissolved the very selection the user was about to copy.
    if (kbAnchor !== null && !MODIFIER_ONLY_KEYS.has(event.key)) {
      kbAnchor = null
      instance.clearSelection()
      if (event.key === 'Escape') return false
    }
    return true
  })

  instance.onData((data) => {
    const id = sessionId.value
    if (id && !exited.value) void qs.console.write(id, data).catch(() => {})
  })

  instance.open(host.value)
  term = instance
  fit = fitAddon
  for (const chunk of pending) instance.write(chunk)
  pending.length = 0
  pushSize()
}

// ── keyboard selection (Shift+Arrows, Windows-Terminal style) ───────────────
//
// xterm's own selection is mouse-only, so this drives `term.select()` by hand:
// the first Shift+Arrow anchors at the cursor cell, every further one moves a
// virtual caret one cell (left/right) or one row (up/down), and the linear
// span between anchor and caret becomes the selection. Positions are linear
// cell indices over the whole buffer (`row * cols + col`) so the span crosses
// line ends the way reading order does. Ctrl+C then copies it — the same path
// a mouse selection takes.

/** Keydowns that are only a modifier settling in, never a keypress of their own. */
const MODIFIER_ONLY_KEYS: ReadonlySet<string> = new Set([
  'Control',
  'Shift',
  'Alt',
  'Meta',
  'AltGraph',
  'CapsLock',
  'NumLock',
  'ScrollLock',
  'OS',
])

/** Linear anchor cell, or `null` while no keyboard selection is running. */
let kbAnchor: number | null = null
let kbCaret = 0

function extendKeyboardSelection(
  instance: Terminal,
  key: 'ArrowLeft' | 'ArrowRight' | 'ArrowUp' | 'ArrowDown'
) {
  const buffer = instance.buffer.active
  const cols = instance.cols
  const lastCell = (buffer.length * cols) - 1

  if (kbAnchor === null) {
    kbAnchor = (buffer.baseY + buffer.cursorY) * cols + buffer.cursorX
    kbCaret = kbAnchor
  }

  const step = key === 'ArrowLeft' ? -1 : key === 'ArrowRight' ? 1 : key === 'ArrowUp' ? -cols : cols
  kbCaret = Math.max(0, Math.min(lastCell, kbCaret + step))

  const start = Math.min(kbAnchor, kbCaret)
  const length = Math.max(1, Math.abs(kbCaret - kbAnchor))
  instance.select(start % cols, Math.floor(start / cols), length)

  // Keep the caret on screen — select() itself never scrolls.
  const caretRow = Math.floor(kbCaret / cols)
  const top = buffer.viewportY
  if (caretRow < top) instance.scrollLines(caretRow - top)
  else if (caretRow >= top + instance.rows) instance.scrollLines(caretRow - (top + instance.rows - 1))
}

function write(data: string) {
  if (term) term.write(data)
  else pending.push(data)
}

/** Fit the grid to the box and tell the PTY. Guarded against 0×0 (hidden panes). */
function pushSize() {
  const el = host.value
  if (!el || !term || !fit) return
  if (el.clientWidth < 20 || el.clientHeight < 20) return
  fit.fit()
  const id = sessionId.value
  if (id && !exited.value) {
    void qs.console.resize(id, term.cols, term.rows).catch(() => {})
  }
}

// ── session lifecycle ────────────────────────────────────────────────────────

useTauriEvent<{ id: string; data: string }>('console-output', (payload) => {
  if (payload.id !== sessionId.value) return
  if (payload.data === '') return // EOF travels on console-session too
  write(payload.data)
})

useTauriEvent<{ id: string; state: string }>('console-session', (payload) => {
  if (payload.id !== sessionId.value || payload.state !== 'exited') return
  exited.value = true
  emit('exited')
  emit('state', { cwd: props.cwd ?? '.', exited: true })
})

async function boot() {
  await createTerminal()
  const el = host.value
  if (!el) return

  try {
    if (props.sessionId) {
      const alive = await qs.console.alive(props.sessionId)
      if (alive) {
        sessionId.value = props.sessionId
        const missed = await qs.console.attach(props.sessionId)
        if (missed.raw) write(missed.raw)
        pushSize()
        emit('opened', props.sessionId)
        emit('state', { cwd: props.cwd ?? '.', exited: false })
        return
      }
    }
    const cols = term?.cols ?? 80
    const rows = term?.rows ?? 24
    const id = await qs.console.openSession({
      cwd: props.cwd ?? '.',
      shell: props.shell ?? null,
      cols,
      rows,
      owner: props.owner,
    })
    sessionId.value = id
    emit('opened', id)
    emit('state', { cwd: props.cwd ?? '.', exited: false })
  } catch (e) {
    emit('failed', e instanceof Error ? e.message : String(e))
  }
}

onMounted(() => {
  resizeObserver = new ResizeObserver(() => pushSize())
  if (host.value) resizeObserver.observe(host.value)
  void boot()
})

// V3 warm cache: detach while the module is hidden, re-attach on return —
// `qs-pty` buffers, and the replay makes the screen whole again.
onActivated(() => {
  if (detachTimer) {
    clearTimeout(detachTimer)
    detachTimer = null
  }
  const id = sessionId.value
  if (id && !exited.value) {
    void qs.console
      .attach(id)
      .then((missed) => {
        if (missed.raw) write(missed.raw)
        void nextTick(() => pushSize())
      })
      .catch(() => {})
  }
})

onDeactivated(() => {
  const id = sessionId.value
  if (!id || exited.value) return
  // A tick late, so a quick module round-trip does not thrash attach/detach.
  detachTimer = setTimeout(() => {
    void qs.console.detach(id).catch(() => {})
  }, 250)
})

onBeforeUnmount(() => {
  resizeObserver?.disconnect()
  if (detachTimer) clearTimeout(detachTimer)
  term?.dispose()
  term = null
})

// ── image paste (the one feature kept from the block era) ───────────────────

/**
 * A pasted image becomes a file plus its path in the terminal. Captured, and
 * only swallowed when an image was actually found — a text paste stays with
 * xterm exactly as it was.
 *
 * The path is wrapped in bracketed-paste markers when the running program
 * asked for them (`CSI ?2004h`, tracked by xterm since every byte now flows
 * through it) — that is what lets `claude` tell an attachment from typed text.
 */
async function onPastePane(event: ClipboardEvent) {
  const data = event.clipboardData
  if (!data) return
  const fromFiles = Array.from(data.files).find((file) => file.type.startsWith('image/'))
  const fromItems = fromFiles
    ? null
    : Array.from(data.items)
        .find((item) => item.kind === 'file' && item.type.startsWith('image/'))
        ?.getAsFile() ?? null
  const image = fromFiles ?? fromItems
  if (!image) return

  event.preventDefault()
  event.stopPropagation()

  try {
    const buffer = await image.arrayBuffer()
    // Chunked: `String.fromCharCode(...bytes)` on a megabyte of image blows
    // the argument limit, and a screenshot is exactly the normal case.
    const bytes = new Uint8Array(buffer)
    let binary = ''
    for (let i = 0; i < bytes.length; i += 0x8000) {
      binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000))
    }
    const extension = image.type.split('/')[1] ?? 'png'
    const path = await qs.console.savePastedImage(btoa(binary), extension)
    const id = sessionId.value
    if (!id || exited.value) return
    const bracketed = term?.modes.bracketedPasteMode ?? false
    const payload = bracketed ? `\x1b[200~"${path}"\x1b[201~` : `"${path}" `
    await qs.console.write(id, payload)
    term?.focus()
  } catch (e) {
    emit('failed', `The pasted image could not be saved: ${e instanceof Error ? e.message : String(e)}`)
  }
}

/** The paperclip: pick a file, its quoted path goes to the shell line. */
async function attachFile() {
  try {
    const dialog = await import('@tauri-apps/plugin-dialog')
    const picked = await dialog.open({ multiple: false, title: 'Attach a file' })
    const path = typeof picked === 'string' ? picked : null
    const id = sessionId.value
    if (path && id && !exited.value) {
      await qs.console.write(id, `"${path}" `)
      term?.focus()
    }
  } catch {
    // No dialog plugin in browser dev — nothing to attach.
  }
}

function focusTerm() {
  term?.focus()
}

/** Write text onto the shell's line without running it. */
function prefill(text: string) {
  const id = sessionId.value
  if (id && !exited.value) void qs.console.write(id, text).catch(() => {})
  term?.focus()
}

/** Run a command: the text plus the Enter the shell is waiting for. */
function run(text: string) {
  const id = sessionId.value
  if (id && !exited.value) void qs.console.write(id, `${text}\r`).catch(() => {})
  term?.focus()
}

defineExpose({ pushSize, prefill, run, focusTerm })
</script>

<template>
  <section
    ref="root"
    class="console-pane"
    :class="{ 'is-focused': active, 'is-exited': exited }"
    @pointerdown.capture="emit('focus')"
    @paste.capture="onPastePane"
  >
    <header v-if="chrome" class="console-pane-bar">
      <span class="console-pane-title" dir="rtl" :title="cwd">&lrm;{{ label ?? 'Shell' }} — {{ cwd }}</span>
      <span v-if="exited" class="console-pane-flag">exited</span>
      <button class="console-pane-btn" title="Attach a file — puts its quoted path on the shell line" @click.stop="attachFile">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M21 11.5 12.5 20a5.5 5.5 0 0 1-7.8-7.8L13 3.9a3.7 3.7 0 0 1 5.2 5.2L9.9 17.4a1.8 1.8 0 0 1-2.6-2.6L15 7.1" /></svg>
      </button>
      <button class="console-pane-btn" title="Pane menu" aria-haspopup="menu" @click.stop="paneMenu = !paneMenu">
        <svg viewBox="0 0 24 24" fill="currentColor" stroke="none"><circle cx="5" cy="12" r="1.6" /><circle cx="12" cy="12" r="1.6" /><circle cx="19" cy="12" r="1.6" /></svg>
      </button>
      <button class="console-pane-btn" :title="`Close pane${keys?.closePane ? ` (${keys.closePane})` : ''}`" @click.stop="emit('close')">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
      </button>

      <menu v-if="paneMenu" class="console-pane-menu" @click="paneMenu = false">
        <button class="console-pane-menu-item" @click="emit('split', 'right')">
          Split right<span class="console-pane-menu-key">{{ keys?.splitRight }}</span>
        </button>
        <button class="console-pane-menu-item" @click="emit('split', 'left')">
          Split left
        </button>
        <button class="console-pane-menu-item" @click="emit('split', 'down')">
          Split down<span class="console-pane-menu-key">{{ keys?.splitDown }}</span>
        </button>
        <button class="console-pane-menu-item" @click="emit('split', 'up')">
          Split up
        </button>
        <button class="console-pane-menu-item" @click="emit('zoom')">
          Zoom pane<span class="console-pane-menu-key">{{ keys?.zoomPane }}</span>
        </button>
        <button class="console-pane-menu-item" @click="emit('close')">
          Close pane<span class="console-pane-menu-key">{{ keys?.closePane }}</span>
        </button>
      </menu>
    </header>

    <div ref="host" class="console-term" @click="focusTerm" />
  </section>
</template>

<style scoped>
.console-pane {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  background: var(--qss-bg);
  /* The terminal type size; hosts may override on a wrapper. */
  --cpane-font-size: 13px;
}

.console-pane-bar {
  position: relative;
  z-index: 2;
  flex: none;
  display: flex;
  align-items: center;
  gap: 6px;
  height: 28px;
  padding: 0 6px;
  background: var(--qss-bg-raised);
  border-bottom: 1px solid var(--qss-border);
  color: var(--qss-text-muted);
  font: 500 11px/1 var(--qss-font-mono);
}

.console-pane.is-focused > .console-pane-bar {
  background: var(--qss-bg-card);
  color: var(--qss-text);
}

.console-pane-title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  text-align: left;
}

.console-pane-flag {
  flex: none;
  padding: 2px 6px;
  border-radius: 999px;
  background: var(--qss-bg-hover);
  font-size: 10px;
}

.console-pane-btn {
  flex: none;
  display: grid;
  place-items: center;
  width: 20px;
  height: 20px;
  border: 0;
  border-radius: 4px;
  background: transparent;
  color: inherit;
  cursor: pointer;
}
.console-pane-btn:hover { background: var(--qss-bg-hover); }
.console-pane-btn svg { width: 14px; height: 14px; }
.console-pane-btn:focus-visible {
  outline: 2px solid var(--qss-accent);
  outline-offset: -1px;
}

.console-pane-menu {
  position: absolute;
  top: 26px;
  right: 4px;
  z-index: 5;
  display: flex;
  flex-direction: column;
  min-width: 180px;
  margin: 0;
  padding: 4px;
  border: 1px solid var(--qss-border);
  border-radius: 6px;
  background: var(--qss-bg-overlay, var(--qss-bg-card));
  box-shadow: var(--qss-shadow-lg);
}

.console-pane-menu-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 5px 8px;
  border: 0;
  border-radius: 4px;
  background: transparent;
  color: var(--qss-text);
  font: 400 12px/1.2 var(--qss-font-sans);
  text-align: left;
  cursor: pointer;
}
.console-pane-menu-item:hover { background: var(--qss-bg-hover); }
.console-pane-menu-item:focus-visible {
  outline: 2px solid var(--qss-accent);
  outline-offset: -1px;
}
.console-pane-menu-key { color: var(--qss-text-muted); font-size: 11px; }

.console-term {
  flex: 1;
  min-height: 0;
  padding: 4px 0 0 6px;
}
.console-term :deep(.xterm) { height: 100%; }
/* The viewport fills the pane beyond the last whole terminal row. xterm's
   default black background otherwise shows there as a strip after fitting. */
.console-term :deep(.xterm-viewport) { background-color: var(--qss-bg); }
/* Keep a steady caret even if a TUI requests a blinking cursor via DECSCUSR
   or DECSET 12. The DOM renderer rebuilds its cursor span on redraw, which
   restarts its CSS animation and can flash it rapidly at the same position.
   Explicit application cursor hide/show and cursor shape still work. */
.console-term :deep(.xterm-cursor-blink) { animation: none !important; }
</style>
