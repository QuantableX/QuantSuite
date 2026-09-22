<script setup lang="ts">
/**
 * QuantConsole's groups in the unified settings modal — `ConsoleSettingsPanel`
 * (docs/PLAN-CONSOLE.md §5, phase P7).
 *
 * One component, four groups: `console-settings.client.ts` registers this
 * once per `section`, so the modal's left column carries "Shell",
 * "ANSI colours", "Shortcuts" and "Agent access" instead of one scroll with
 * sixteen colour rows and the whole keymap in it.
 *
 * The shortcuts themselves come from `../keymap`, which the page binds from as
 * well: a panel with its own copy of the table can only ever show a keymap the
 * app does not have.
 *
 * Three rules shape everything below:
 *
 *   - **There is no Save button** (`QSettingsModal`). A discrete choice writes on
 *     the spot; anything typed is debounced ~300ms, and a pending write is
 *     flushed on unmount — the modal closes on Escape, which is exactly inside
 *     that window, and a silently dropped retention cap is worse than a slow one.
 *   - **Nothing here is retroactive that cannot be.** A retention cap governs
 *     future pruning, the default shell governs the *next* session. Each row says
 *     so where it is true.
 *   - **A missing backend is a state, not an empty panel.** In a plain browser
 *     (`npm run dev`) there is no `set_setting`; the controls still render with
 *     their real defaults and say plainly that nothing is being stored.
 *
 * Styling is local: the modal is teleported to `<body>`, outside
 * `[data-module="console"]`, so the module stylesheet cannot reach it. The
 * shared `.qsu-*` row language from shell.css is global and is used for
 * everything it covers.
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { qs, type ConsoleShell } from '@quantsuite/core'
import {
  CONSOLE_ACTIONS,
  CONSOLE_ACTION_GROUPS,
  isPtyReservedChord,
  parseBinding,
  prettyBinding,
} from '../keymap'
import { ANSI_BUILTIN, ANSI_LABELS } from '../composables/useAnsiPalette'

/** Which group this instance renders — the plugin registers one section per id. */
type SectionId = 'shell' | 'palette' | 'keymap' | 'agent'

const props = defineProps<{ section: SectionId }>()

const SCOPE = 'console'

const KEY = {
  shellDefault: 'shell.default',
  palette: 'ansi.palette',
  keymap: 'keymap',
  agentExec: 'agent.exec.enabled',
} as const

// ── backend plumbing ─────────────────────────────────────────────────────

/** Values are visible before the first read resolves; `ready` gates the flash. */
const ready = ref(false)
const backendMissing = ref(false)
const readError = ref<string | null>(null)
const writeError = ref<string | null>(null)

function inTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI__' in window
}

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e)
}

/**
 * Read one key, or `null` for "use the default".
 *
 * Never throws and never leaves the caller without a value: a rejected
 * `get_setting` (no backend, locked db) has to end in defaults on screen with
 * the reason said out loud, not in a section that renders nothing.
 */
async function read<T>(key: string): Promise<T | null> {
  if (!inTauri()) {
    backendMissing.value = true
    return null
  }
  try {
    return await qs.core.getSetting<T>(SCOPE, key)
  } catch (e) {
    readError.value = message(e)
    return null
  }
}

async function write(key: string, value: unknown): Promise<void> {
  if (!inTauri()) {
    backendMissing.value = true
    return
  }
  try {
    await qs.core.setSetting(SCOPE, key, value)
    writeError.value = null
  } catch (e) {
    writeError.value = `Saving ${key} failed — the value on screen is not stored: ${message(e)}`
  }
}

/**
 * Pending debounced writes by key, value included.
 *
 * The value is kept next to the timer so `onBeforeUnmount` can still land it:
 * typing `2000` and hitting Escape unmounts this component ~300ms after the last
 * keystroke, and a settings modal without a Save button must not lose the edit
 * the user just watched themselves make.
 */
interface PendingWrite {
  timer: ReturnType<typeof setTimeout>
  value: unknown
}
const pending = new Map<string, PendingWrite>()
const WRITE_DEBOUNCE_MS = 300

function writeSoon(key: string, value: unknown) {
  const existing = pending.get(key)
  if (existing) clearTimeout(existing.timer)
  const timer = setTimeout(() => {
    pending.delete(key)
    void write(key, value)
  }, WRITE_DEBOUNCE_MS)
  pending.set(key, { timer, value })
}

const notice = computed(() => {
  if (backendMissing.value) {
    return 'No console backend in this window: every control below shows its real default, and nothing you change here is stored. Open the desktop app to set these.'
  }
  if (readError.value) {
    return `Reading the stored console settings failed, so the controls show defaults: ${readError.value}`
  }
  return null
})

// ── shell ────────────────────────────────────────────────────────────────

const shells = ref<ConsoleShell[]>([])
const shellsError = ref<string | null>(null)
/** `null` = whatever the platform reports as its default. */
const shellDefault = ref<string | null>(null)

const platformDefault = computed(() => shells.value.find((s) => s.isDefault) ?? null)

const platformDefaultLabel = computed(() =>
  platformDefault.value ? `Platform default (${platformDefault.value.label})` : 'Platform default'
)

/**
 * A stored id no longer on this machine — a shell that was uninstalled, or a
 * profile carried over from another box. Shown as its own option rather than
 * quietly snapping the select to "platform default", which would disagree with
 * what is actually saved.
 */
const missingShell = computed(() => {
  const id = shellDefault.value
  if (!id) return null
  return shells.value.some((s) => s.id === id) ? null : id
})

function onShellChange(event: Event) {
  const raw = (event.target as HTMLSelectElement).value
  shellDefault.value = raw === '' ? null : raw
  // A discrete choice, so no debounce — there is nothing to coalesce.
  void write(KEY.shellDefault, shellDefault.value)
}

// ── ANSI palette ─────────────────────────────────────────────────────────

/**
 * One source (q1 #38): `useAnsiPalette` owns the sixteen colours and their
 * names, and the block renderer, the xterm theme and these swatches all read
 * that same table — a drift between what the panel shows and what the
 * terminal paints is no longer possible. The two aliases keep the
 * row-building code below reading as it always has.
 */
const ANSI_NAMES = ANSI_LABELS
const BUILTIN_CSS = ANSI_BUILTIN

const palette = ref<Record<string, string>>({})
const paletteErrors = ref<Record<string, string | null>>({})

/**
 * A hidden element the browser resolves colours through.
 *
 * `<input type="color">` accepts nothing but `#rrggbb`, and half the built-in
 * palette is `var(--qss-*)` — handing those to the input makes every unset
 * swatch black, which reads as "the default is black". The same path resolves a
 * user's `tomato` or `rgb(…)` override, so the swatch never disagrees with the
 * text field beside it.
 */
const probe = ref<HTMLElement | null>(null)
const probeReady = ref(false)
const hexMemo = new Map<string, string>()
const FALLBACK_HEX = '#000000'

function toHex(computedColor: string): string {
  const m = computedColor.match(/^rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)/)
  if (!m) return FALLBACK_HEX
  const part = (v: string) =>
    Math.max(0, Math.min(255, Math.round(Number(v))))
      .toString(16)
      .padStart(2, '0')
  return `#${part(m[1])}${part(m[2])}${part(m[3])}`
}

function hexOf(css: string): string {
  if (!probeReady.value) return FALLBACK_HEX
  const memo = hexMemo.get(css)
  if (memo) return memo
  const el = probe.value
  if (!el) return FALLBACK_HEX
  // Cleared first: an invalid value is a no-op for `style.color`, and without
  // this the previous colour would be reported as if it had been accepted.
  el.style.color = ''
  el.style.color = css
  const hex = toHex(getComputedStyle(el).color)
  hexMemo.set(css, hex)
  return hex
}

function isColor(value: string): boolean {
  if (typeof CSS === 'undefined' || typeof CSS.supports !== 'function') return true
  return CSS.supports('color', value)
}

const paletteRows = computed(() =>
  ANSI_NAMES.map((name, index) => {
    const key = String(index)
    const override = palette.value[key]
    const builtin = BUILTIN_CSS[index] ?? ''
    return {
      key,
      index,
      name,
      override: override ?? '',
      custom: override !== undefined,
      swatch: hexOf(override ?? builtin),
      builtinHex: hexOf(builtin),
      error: paletteErrors.value[key] ?? null,
    }
  })
)

const paletteCount = computed(() => Object.keys(palette.value).length)

function setPaletteEntry(key: string, value: string | null) {
  const next = { ...palette.value }
  if (value === null) delete next[key]
  else next[key] = value
  palette.value = next
  // Only the differences are stored; an empty object means "the built-in palette",
  // which is what a fresh install reads.
  writeSoon(KEY.palette, next)
}

function onPaletteText(key: string, event: Event) {
  const raw = (event.target as HTMLInputElement).value.trim()
  if (raw === '') {
    paletteErrors.value = { ...paletteErrors.value, [key]: null }
    setPaletteEntry(key, null)
    return
  }
  if (!isColor(raw)) {
    // Reported, not written: a stored `#12345` would render as no colour at all
    // and look like the override was ignored.
    paletteErrors.value = { ...paletteErrors.value, [key]: 'Not a CSS colour.' }
    palette.value = { ...palette.value, [key]: raw }
    return
  }
  paletteErrors.value = { ...paletteErrors.value, [key]: null }
  setPaletteEntry(key, raw)
}

function onPaletteSwatch(key: string, event: Event) {
  const raw = (event.target as HTMLInputElement).value
  paletteErrors.value = { ...paletteErrors.value, [key]: null }
  // Debounced by `setPaletteEntry`: dragging inside the picker fires `input`
  // continuously, and every one of those would otherwise be a database write.
  setPaletteEntry(key, raw)
}

function resetPaletteEntry(key: string) {
  paletteErrors.value = { ...paletteErrors.value, [key]: null }
  setPaletteEntry(key, null)
}

function resetPalette() {
  paletteErrors.value = {}
  palette.value = {}
  writeSoon(KEY.palette, {})
}

// ── keymap ───────────────────────────────────────────────────────────────

/**
 * Chords the console must not take. `Ctrl+K` is the suite's (PLAN-V3 §3); the
 * other three are the shell's own keys, and rebinding one of them to a tab
 * action would take away the interrupt, the end-of-input or the paste of every
 * session at once — with nothing left in the UI to explain why.
 */
const RESERVED: ReadonlyMap<string, string> = new Map([
  ['Ctrl+K', 'Ctrl+K opens the suite command palette. The console never intercepts it.'],
  ['Ctrl+C', 'Ctrl+C copies the selection, or sends SIGINT when there is none.'],
  ['Ctrl+D', 'Ctrl+D is end-of-input in every shell.'],
  ['Ctrl+V', 'Ctrl+V pastes into the command editor.'],
])

const MODIFIER_KEYS: ReadonlySet<string> = new Set([
  'Control',
  'Shift',
  'Alt',
  'Meta',
  'AltGraph',
  'CapsLock',
  'OS',
])

const keymap = ref<Record<string, string>>({})
/** The action whose next keystroke is being captured, or `null`. */
const capturing = ref<string | null>(null)
const keymapError = ref<{ action: string; reason: string } | null>(null)
/** A chord that was accepted with a caveat — shown, never blocking. */
const keymapWarning = ref<{ action: string; note: string } | null>(null)

const keymapRows = computed(() =>
  CONSOLE_ACTIONS.map((action) => {
    const override = keymap.value[action.id]
    const current = override ?? action.defaultBinding
    return {
      id: action.id,
      label: action.label,
      group: action.group,
      note: action.note,
      current,
      display: prettyBinding(current),
      /** The fixed extras, shown so a second working chord is never a secret.
       * Raw, not prettified: they are compared against `current` for conflicts. */
      also: action.also ?? [],
      custom: override !== undefined,
    }
  })
)

/**
 * The same rows under the headings the page groups them by.
 *
 * Thirty-odd shortcuts in one column is a list nobody reads to the end of, and the
 * one being looked for is always in the third of it that scrolled past.
 */
const keymapGroups = computed(() =>
  CONSOLE_ACTION_GROUPS.map((group) => ({
    ...group,
    rows: keymapRows.value.filter((row) => row.group === group.id),
  })).filter((group) => group.rows.length > 0)
)

const keymapCount = computed(() => Object.keys(keymap.value).length)

/**
 * Actions sharing a binding, by action id.
 *
 * A conflict is shown rather than prevented: the user may well be mid-swap
 * (moving `Ctrl+Shift+F` onto another action before rebinding this one), and
 * refusing the first half of that would make the swap impossible.
 *
 * The fixed extra chords count too. Rebinding an action onto `Ctrl+F` collides
 * with "Find in pane" just as squarely as onto its primary chord, and a conflict
 * the panel cannot see is one the user meets at the keyboard instead.
 */
const conflicts = computed<Record<string, string[]>>(() => {
  const byBinding = new Map<string, string[]>()
  for (const row of keymapRows.value) {
    for (const chord of [row.current, ...(row.also ?? [])]) {
      const list = byBinding.get(chord) ?? []
      list.push(row.label)
      byBinding.set(chord, list)
    }
  }
  const out: Record<string, string[]> = {}
  for (const row of keymapRows.value) {
    const others = (byBinding.get(row.current) ?? []).filter((label) => label !== row.label)
    if (others.length > 0) out[row.id] = others
  }
  return out
})

/**
 * A captured key, spelled the way it is stored — which is the way `event.key`
 * reports it, because that is what the shortcut layer matches against. A
 * friendlier spelling here (`Space` for `' '`) would read well in the row and
 * produce a binding that can never fire; `prettyBinding` does the friendly
 * spelling at the last moment instead. Letters are upper-cased only because the
 * comparison is case-insensitive and a keycap reading `Ctrl+t` looks like a typo.
 */
function keyName(key: string): string {
  return key.length === 1 ? key.toUpperCase() : key
}

function bindingFrom(event: KeyboardEvent): string {
  const parts: string[] = []
  // Cmd folds into Ctrl because `useShortcuts` matches `ctrlKey || metaKey`:
  // storing "Meta+T" would produce a binding the shortcut layer can never fire.
  if (event.ctrlKey || event.metaKey) parts.push('Ctrl')
  if (event.altKey) parts.push('Alt')
  if (event.shiftKey) parts.push('Shift')
  parts.push(keyName(event.key))
  return parts.join('+')
}

/**
 * Keys that carry no character, so they cannot fire mid-word.
 *
 * The rule below asks for Ctrl or Alt because the console's shortcuts also reach
 * the command editor, and a bare letter there would type instead of act. A
 * function key or a paging key types nothing in any field, which is exactly why
 * `F3` and `PageUp` are shipped bare — and a rule that refuses to let the user
 * reproduce a shipped default is a rule that is wrong.
 */
function isCharacterlessKey(key: string): boolean {
  return /^F([1-9]|1\d|2[0-4])$/.test(key) || key === 'PageUp' || key === 'PageDown'
}

function rejectionFor(binding: string): string | null {
  const reserved = RESERVED.get(binding)
  if (reserved) return reserved
  // The PTY-compliance rule (w2 §7.2), decided in one place: `keymap.ts`'
  // predicate. A bare Ctrl+letter *is* a control character, and binding one
  // takes it away from every readline in every session at once.
  if (isPtyReservedChord(binding)) {
    return (
      'Ctrl+<letter> chords belong to the shell: the PTY turns this one into a ' +
      'control character (Ctrl+R is its reverse search, Ctrl+A its start-of-line). ' +
      'Add Shift or Alt to it.'
    )
  }
  // Modifier order is fixed (Ctrl, Alt, Shift), so a chord with either of them
  // starts with it.
  if (binding.startsWith('Ctrl+') || binding.startsWith('Alt+')) return null
  // Shift alone does not qualify a letter, but it does qualify Shift+F3.
  if (isCharacterlessKey(binding.replace(/^Shift\+/, ''))) return null
  return 'Needs Ctrl or Alt — an unmodified key would fire while you are working in a session.'
}

/**
 * A chord the capture accepts but the text-input guard drops while typing:
 * `useShortcuts` ignores a one-character key without Ctrl whenever an input,
 * textarea or select holds the focus — and a pane's resting focus is its
 * editor. `Alt+<letter>` is the shape that reaches here (bare letters are
 * refused outright above). Warned rather than refused, because the chord
 * still fires with the block list focused — which is where the shipped
 * `Alt+Shift+F` filter chord lives too.
 */
function guardedWhileTyping(binding: string): boolean {
  const parsed = parseBinding(binding)
  return parsed !== null && parsed.key.length === 1 && !parsed.ctrl
}

function onCaptureKey(event: KeyboardEvent) {
  // Still assembling the chord: a bare Shift press is not a binding.
  if (MODIFIER_KEYS.has(event.key)) return

  // Both, in the capture phase, before anything else sees the key: the page
  // behind this modal listens for these very chords on `window`, and the modal
  // itself closes on Escape — so cancelling a capture with Escape would
  // otherwise also close the settings the user is still editing.
  event.preventDefault()
  event.stopPropagation()

  const action = capturing.value
  endCapture()
  if (!action || event.key === 'Escape') return

  const binding = bindingFrom(event)
  const reason = rejectionFor(binding)
  if (reason) {
    keymapError.value = { action, reason }
    return
  }
  keymapWarning.value = guardedWhileTyping(binding)
    ? {
        action,
        note:
          `${prettyBinding(binding)} is bound, but it cannot fire while the caret is in ` +
          'a text field — the input guard drops Alt+letter chords there, and a pane ' +
          'rests with its editor focused. It works with the block list focused.',
      }
    : null
  setBinding(action, binding)
}

function beginCapture(id: string) {
  keymapError.value = null
  keymapWarning.value = null
  if (capturing.value) endCapture()
  capturing.value = id
  window.addEventListener('keydown', onCaptureKey, true)
}

function endCapture() {
  capturing.value = null
  window.removeEventListener('keydown', onCaptureKey, true)
}

function setBinding(id: string, binding: string) {
  const action = CONSOLE_ACTIONS.find((a) => a.id === id)
  if (!action) return
  const next = { ...keymap.value }
  // Only differences are stored, so a later change to a shipped default reaches
  // everyone who never touched that row.
  if (binding === action.defaultBinding) delete next[id]
  else next[id] = binding
  keymap.value = next
  void write(KEY.keymap, next)
}

function resetBinding(id: string) {
  keymapError.value = null
  if (keymapWarning.value?.action === id) keymapWarning.value = null
  const next = { ...keymap.value }
  delete next[id]
  keymap.value = next
  void write(KEY.keymap, next)
}

function resetKeymap() {
  keymapError.value = null
  keymapWarning.value = null
  keymap.value = {}
  void write(KEY.keymap, {})
}

// ── agent access ─────────────────────────────────────────────────────────

const agentExec = ref(false)

function toggleAgentExec() {
  agentExec.value = !agentExec.value
  void write(KEY.agentExec, agentExec.value)
}

// ── load ─────────────────────────────────────────────────────────────────

/**
 * Only the keys this section shows.
 *
 * Each section is its own component instance, so a full read on every mount
 * would be five reads to render one group — and `get_setting` is a database
 * round trip per key.
 */
async function load() {
  try {
    if (props.section === 'shell') {
      const stored = await read<string>(KEY.shellDefault)
      shellDefault.value = typeof stored === 'string' && stored !== '' ? stored : null
      if (inTauri()) {
        try {
          shells.value = await qs.console.listShells()
        } catch (e) {
          // Distinct from a settings failure: the picker has no options, but the
          // stored value is still known and must not be presented as unset.
          shellsError.value = message(e)
        }
      }
      return
    }

    if (props.section === 'palette') {
      const stored = await read<Record<string, unknown>>(KEY.palette)
      palette.value = sanitisePalette(stored)
      return
    }

    if (props.section === 'keymap') {
      const stored = await read<Record<string, unknown>>(KEY.keymap)
      keymap.value = sanitiseKeymap(stored)
      return
    }

    const stored = await read<boolean>(KEY.agentExec)
    // Anything other than a literal `true` is off. The default for arbitrary
    // shell execution is refusal (§11.4), so a corrupt value must not enable it.
    agentExec.value = stored === true
  } finally {
    ready.value = true
  }
}

/** Keys outside `"0"`…`"15"` and non-string values are dropped, not rendered. */
function sanitisePalette(stored: Record<string, unknown> | null): Record<string, string> {
  const out: Record<string, string> = {}
  if (!stored || typeof stored !== 'object') return out
  for (let i = 0; i < 16; i++) {
    const value = stored[String(i)]
    if (typeof value === 'string' && value.trim() !== '') out[String(i)] = value.trim()
  }
  return out
}

/** Same for the keymap: only known action ids, only strings. */
function sanitiseKeymap(stored: Record<string, unknown> | null): Record<string, string> {
  const out: Record<string, string> = {}
  if (!stored || typeof stored !== 'object') return out
  for (const action of CONSOLE_ACTIONS) {
    const value = stored[action.id]
    if (typeof value === 'string' && value.trim() !== '') out[action.id] = value.trim()
  }
  return out
}

onMounted(() => {
  probeReady.value = probe.value !== null
  void load()
})

onBeforeUnmount(() => {
  endCapture()
  for (const [key, entry] of pending) {
    clearTimeout(entry.timer)
    void write(key, entry.value)
  }
  pending.clear()
})
</script>

<template>
  <div class="csp">
    <p v-if="notice" class="csp-banner">{{ notice }}</p>
    <p v-if="writeError" class="csp-banner is-error" role="status">{{ writeError }}</p>

    <!-- ── Shell ────────────────────────────────────────────────────── -->
    <template v-if="section === 'shell'">
      <div class="qsu-row">
        <div class="qsu-row-text">
          <p class="qsu-label">Shell for new sessions</p>
          <p class="qsu-hint">
            Applies to the next session you open. Sessions already running keep the
            shell they started with.
          </p>
        </div>
        <select
          class="qsu-select"
          :value="shellDefault ?? ''"
          :disabled="!ready"
          @change="onShellChange"
        >
          <option value="">{{ platformDefaultLabel }}</option>
          <option v-for="shell in shells" :key="shell.id" :value="shell.id">
            {{ shell.label }}
          </option>
          <option v-if="missingShell" :value="missingShell">
            {{ missingShell }} (not on this machine)
          </option>
        </select>
      </div>

      <p v-if="missingShell" class="csp-note is-warn">
        The stored default <code>{{ missingShell }}</code> is not on this machine.
        Until you pick another, new sessions fall back to the platform default.
      </p>

      <p v-if="shellsError" class="csp-note is-error">
        Listing the shells failed, so only the stored value is shown: {{ shellsError }}
      </p>

      <div v-if="shells.length" class="csp-shells">
        <p class="csp-head">Found on this machine</p>
        <div v-for="shell in shells" :key="shell.id" class="csp-shell">
          <div class="csp-shell-top">
            <span class="csp-shell-name">{{ shell.label }}</span>
            <span v-if="shell.isDefault" class="csp-tag">platform default</span>
          </div>
          <code class="csp-path">{{ shell.path }}</code>
        </div>
      </div>
      <p v-else-if="ready && !shellsError" class="csp-note">
        No shells reported. The list comes from the console backend, which this
        window does not have.
      </p>
    </template>

    <!-- ── ANSI palette ─────────────────────────────────────────────── -->
    <template v-else-if="section === 'palette'">
      <p class="csp-note">
        Overrides for the sixteen ANSI colours used by block output and by the
        alternate-screen panes. An empty row uses the built-in palette. The suite
        theme itself is not configurable — these sixteen are.
      </p>

      <div class="csp-palette-head">
        <p class="csp-head">
          {{ paletteCount === 0 ? 'No overrides' : paletteCount + ' of 16 overridden' }}
        </p>
        <button class="qsu-btn csp-btn" :disabled="paletteCount === 0" @click="resetPalette">
          Reset all
        </button>
      </div>

      <div class="csp-colors">
        <div v-for="row in paletteRows" :key="row.key" class="csp-color" :class="{ 'is-custom': row.custom }">
          <span class="csp-index">{{ row.index }}</span>
          <span class="csp-color-name">{{ row.name }}</span>
          <!-- The only place in this module with literal colour values in the
               markup: here the user's colours are the data, not the theme. -->
          <input
            class="csp-swatch"
            type="color"
            :value="row.swatch"
            :disabled="!ready"
            :aria-label="'Colour ' + row.index + ', ' + row.name"
            @input="onPaletteSwatch(row.key, $event)"
          >
          <input
            class="csp-input csp-input--css"
            type="text"
            spellcheck="false"
            autocomplete="off"
            :value="row.override"
            :placeholder="row.builtinHex"
            :disabled="!ready"
            :aria-label="'CSS colour for ANSI ' + row.index"
            :aria-invalid="row.error ? 'true' : 'false'"
            @input="onPaletteText(row.key, $event)"
          >
          <button
            class="csp-mini"
            :disabled="!row.custom"
            title="Back to the built-in colour"
            @click="resetPaletteEntry(row.key)"
          >
            Reset
          </button>
          <p v-if="row.error" class="csp-inline is-error">{{ row.error }}</p>
        </div>
      </div>

      <!-- Resolves `var(--qss-*)` and free-form CSS colours to the hex the
           swatch inputs need; never visible, never in the layout. -->
      <span ref="probe" class="csp-probe" aria-hidden="true" />
    </template>

    <!-- ── Shortcuts ────────────────────────────────────────────────── -->
    <template v-else-if="section === 'keymap'">
      <p class="csp-note">
        The console's own shortcuts. Only rows you change are stored, so shipped
        defaults keep moving with the app. Ctrl+K belongs to the suite command
        palette and is never taken here.
      </p>

      <div class="csp-palette-head">
        <p class="csp-head">
          {{ keymapCount === 0 ? 'All defaults' : keymapCount + ' changed' }}
        </p>
        <button class="qsu-btn csp-btn" :disabled="keymapCount === 0" @click="resetKeymap">
          Reset all
        </button>
      </div>

      <div v-for="group in keymapGroups" :key="group.id" class="csp-keys">
        <p class="csp-head csp-keys-head">{{ group.label }}</p>
        <div v-for="row in group.rows" :key="row.id" class="csp-key">
          <div class="csp-key-text">
            <p class="qsu-label">{{ row.label }}</p>
            <p v-if="row.note" class="qsu-hint">{{ row.note }}</p>
            <p v-if="row.also.length" class="qsu-hint">
              Also on {{ row.also.map(prettyBinding).join(', ') }}, which cannot be changed.
            </p>
            <p v-if="conflicts[row.id]" class="csp-inline is-warn">
              Also bound to {{ conflicts[row.id].join(', ') }} — the console fires
              the first match in this list and the rest never run.
            </p>
            <p v-if="keymapError && keymapError.action === row.id" class="csp-inline is-error">
              {{ keymapError.reason }}
            </p>
            <p v-if="keymapWarning && keymapWarning.action === row.id" class="csp-inline is-warn">
              {{ keymapWarning.note }}
            </p>
          </div>

          <div class="csp-key-controls">
            <span v-if="capturing === row.id" class="csp-capture" role="status">
              Press a key, Escape to cancel
            </span>
            <kbd v-else class="csp-kbd" :class="{ 'is-custom': row.custom }">{{ row.display }}</kbd>
            <button class="csp-mini" :disabled="!ready" @click="beginCapture(row.id)">
              {{ capturing === row.id ? 'Listening' : 'Change' }}
            </button>
            <button class="csp-mini" :disabled="!row.custom" @click="resetBinding(row.id)">
              Reset
            </button>
          </div>
        </div>
      </div>
    </template>

    <!-- ── Agent access ─────────────────────────────────────────────── -->
    <template v-else>
      <div class="qsu-row">
        <div class="qsu-row-text">
          <p class="qsu-label">Let an agent run commands</p>
          <p class="qsu-hint">
            Off by default. On, an agent may execute arbitrary shell commands through
            the console — the same power you have at the prompt, including commands
            that delete files or reach the network.
          </p>
        </div>
        <button
          class="qsu-toggle"
          :class="{ 'is-on': agentExec }"
          role="switch"
          :aria-checked="agentExec"
          :disabled="!ready"
          @click="toggleAgentExec"
        />
      </div>

      <p class="csp-note">
        This switch is one of two locks. Every individual call is still put to
        QuantMCP's approval mode, and the console's <code>run_command</code>
        capability stays outside the plugin's default permission set. Turning this
        on does not approve anything by itself; turning it off stops the calls from
        being offered at all.
      </p>
    </template>
  </div>
</template>

<style scoped>
.csp {
  min-width: 0;
}

/* ── notes and banners ── */

.csp-banner {
  margin: 0 0 12px;
  padding: 8px 10px;
  border: 1px solid color-mix(in srgb, var(--qss-warning) 40%, transparent);
  border-left-width: 3px;
  border-radius: 8px;
  background: color-mix(in srgb, var(--qss-warning) 12%, transparent);
  font-size: 11.5px;
  line-height: 1.5;
  color: var(--qss-text);
}
.csp-banner.is-error {
  border-color: color-mix(in srgb, var(--qss-error) 45%, transparent);
  background: color-mix(in srgb, var(--qss-error) 12%, transparent);
}

.csp-note {
  margin: 10px 0 0;
  font-size: 11.5px;
  line-height: 1.55;
  color: var(--qss-text-muted);
}
.csp-note.is-warn {
  color: var(--qss-warning);
}
.csp-note.is-error {
  color: var(--qss-error);
}
.csp-note code {
  font-family: var(--qss-font-mono);
  font-size: 11px;
  color: var(--qss-text-secondary);
}

.csp-inline {
  margin: 3px 0 0;
  font-size: 11px;
  line-height: 1.45;
}
.csp-inline.is-error {
  color: var(--qss-error);
}
.csp-inline.is-warn {
  color: var(--qss-warning);
}

.csp-head {
  margin: 0;
  font-size: 9.5px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--qss-text-muted);
}

/* ── inputs ── */

.csp-input {
  padding: 6px 8px;
  border: 1px solid var(--qss-border);
  border-radius: 7px;
  background: var(--qss-bg-card);
  color: var(--qss-text);
  font-family: inherit;
  font-size: 12px;
}
.csp-input:focus-visible {
  outline: 1px solid var(--qss-accent);
  outline-offset: -1px;
}
.csp-input[aria-invalid='true'] {
  border-color: var(--qss-error);
}
.csp-input:disabled {
  opacity: 0.5;
}

.csp-input--num {
  width: 112px;
  text-align: right;
  font-variant-numeric: tabular-nums;
}
.csp-input--css {
  min-width: 0;
  flex: 1;
  font-family: var(--qss-font-mono);
  font-size: 11px;
}

.csp-number {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 3px;
  flex: none;
}

/* The font-size row: − field + on one line, the field just wide enough for
   two digits. The buttons are the keymap rows' `.csp-mini`. */
.csp-stepper {
  flex: none;
  display: flex;
  align-items: center;
  gap: 6px;
}
.csp-input--font {
  width: 64px;
}

/* Segmented pairs inside a settings row: sized by their labels, not stretched
   across the row the way the global `.qsu-seg` grows in a column layout. */
.csp-seg {
  flex: none;
}
.csp-seg .qsu-seg-btn {
  flex: none;
}
.csp-unit {
  font-size: 10.5px;
  font-variant-numeric: tabular-nums;
  color: var(--qss-text-muted);
}

/* ── shells ── */

.csp-shells {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 14px;
  padding-top: 12px;
  border-top: 1px solid var(--qss-border-subtle);
}

.csp-shell {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.csp-shell-top {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 6px;
}
.csp-shell-name {
  font-size: 12px;
  color: var(--qss-text);
}
.csp-shell-why {
  margin: 2px 0 0;
  font-size: 10.5px;
  line-height: 1.5;
  color: var(--qss-text-muted);
}

.csp-path {
  font-family: var(--qss-font-mono);
  font-size: 10.5px;
  color: var(--qss-text-secondary);
  overflow-wrap: anywhere;
}

.csp-tag {
  flex: none;
  padding: 1px 6px;
  border: 1px solid var(--qss-border);
  border-radius: 999px;
  font-size: 9.5px;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--qss-text-muted);
}
.csp-tag.is-warn {
  color: var(--qss-warning);
  border-color: color-mix(in srgb, var(--qss-warning) 40%, transparent);
}

/* ── palette ── */

.csp-palette-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-top: 14px;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--qss-border-subtle);
}

.csp-btn {
  padding: 5px 10px;
  font-size: 11.5px;
}
.csp-btn:disabled {
  opacity: 0.45;
  cursor: default;
}

.csp-colors {
  display: flex;
  flex-direction: column;
}

/* Grid rather than flex: the sixteen rows have to line up their swatches, and a
   name column that shifts per row is what makes a colour table unreadable. */
.csp-color {
  display: grid;
  grid-template-columns: 20px minmax(0, 1fr) 30px minmax(80px, 1.1fr) auto;
  align-items: center;
  gap: 8px;
  padding: 5px 0;
}
.csp-color + .csp-color {
  border-top: 1px solid var(--qss-border-subtle);
}
.csp-color .csp-inline {
  grid-column: 2 / -1;
}

.csp-index {
  font-family: var(--qss-font-mono);
  font-size: 11px;
  text-align: right;
  font-variant-numeric: tabular-nums;
  color: var(--qss-text-muted);
}
.csp-color-name {
  font-size: 11.5px;
  color: var(--qss-text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.csp-color.is-custom .csp-color-name {
  color: var(--qss-text);
}

.csp-swatch {
  width: 30px;
  height: 22px;
  padding: 0;
  border: 1px solid var(--qss-border);
  border-radius: 5px;
  background: var(--qss-bg-card);
  cursor: pointer;
}
.csp-swatch:disabled {
  opacity: 0.5;
  cursor: default;
}
/* The native swatch keeps its own inset padding, which at 22px leaves a frame
   thicker than the colour it is showing. */
.csp-swatch::-webkit-color-swatch-wrapper {
  padding: 2px;
}
.csp-swatch::-webkit-color-swatch {
  border: none;
  border-radius: 3px;
}

/* Never rendered — a resolver for `var(--qss-*)` and free-form CSS colours.
   Absolute and zero-sized rather than `display: none`, so it stays a styled
   element the layout cannot feel. */
.csp-probe {
  position: absolute;
  width: 0;
  height: 0;
  overflow: hidden;
  visibility: hidden;
}

/* ── keymap ── */

.csp-keys {
  display: flex;
  flex-direction: column;
}

.csp-keys-head {
  margin-top: 14px;
  padding-bottom: 4px;
}

.csp-key {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  padding: 9px 0;
}
.csp-key + .csp-key {
  border-top: 1px solid var(--qss-border-subtle);
}

.csp-key-text {
  min-width: 0;
}

.csp-key-controls {
  flex: none;
  display: flex;
  align-items: center;
  gap: 6px;
}

.csp-kbd {
  min-width: 84px;
  padding: 3px 7px;
  border: 1px solid var(--qss-border);
  border-radius: 6px;
  background: var(--qss-bg-card);
  font-family: var(--qss-font-mono);
  font-size: 11px;
  text-align: center;
  color: var(--qss-text-secondary);
}
.csp-kbd.is-custom {
  border-color: color-mix(in srgb, var(--qss-accent) 55%, var(--qss-border));
  color: var(--qss-text);
}

.csp-capture {
  min-width: 84px;
  padding: 3px 7px;
  border: 1px dashed var(--qss-accent);
  border-radius: 6px;
  font-size: 10.5px;
  text-align: center;
  color: var(--qss-text);
}

.csp-mini {
  flex: none;
  padding: 4px 8px;
  border: 1px solid var(--qss-border);
  border-radius: 6px;
  background: var(--qss-bg-card);
  color: var(--qss-text-secondary);
  font-family: inherit;
  font-size: 11px;
  cursor: pointer;
  transition: background var(--qss-dur-instant) var(--qss-ease-out),
    color var(--qss-dur-instant) var(--qss-ease-out);
}
.csp-mini:hover:not(:disabled) {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}
.csp-mini:disabled {
  opacity: 0.4;
  cursor: default;
}
</style>
