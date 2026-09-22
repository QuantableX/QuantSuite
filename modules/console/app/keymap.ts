/**
 * The console's action table — plain-terminal edition (2026-08-20 rollback).
 *
 * One list, three readers: the page binds the keys, the Shortcuts settings group
 * shows and rebinds them, and the canvas terminal window derives its pane subset
 * through `consolePaneKeys` below. It lives in none of those because one is a
 * page and the others are SFCs — and a second copy is worse than no copy at all:
 * a pane menu that advertises a chord the settings panel cannot show is a promise
 * the app does not keep. (The canvas *was* such a copy, and it drifted.)
 *
 * The defaults are the Windows/Linux terminal keymap people arrive with. The one
 * deviation states its reason in the row: splits are `Ctrl+Shift+D/E` because
 * `Ctrl+D` is end-of-input in every shell.
 *
 * In a plain terminal the keyboard belongs to the shell: every chord here either
 * carries Ctrl+Shift/Alt or acts on the *surface* (tabs, panes) rather than
 * inside it. Anything else goes to the PTY.
 */

import type { ShortcutBinding } from '@quantsuite/core'

/** The settings panel's headings, and the order it renders them in. */
export type ConsoleActionGroup = 'panes' | 'tabs'

export const CONSOLE_ACTION_GROUPS: readonly { id: ConsoleActionGroup; label: string }[] = [
  { id: 'panes', label: 'Panes' },
  { id: 'tabs', label: 'Tabs' },
]

export interface ConsoleActionSpec {
  id: string
  label: string
  group: ConsoleActionGroup
  /** As the settings panel spells it: `Ctrl+Shift+D`. Rebindable. */
  defaultBinding: string
  /**
   * Extra chords that also run the action and are **not** rebindable.
   *
   * Kept for the two conventions people arrive with — `Ctrl+Tab` for the next tab,
   * `Ctrl+F` for find — where dropping the familiar chord buys fidelity nobody
   * asked for. Shown in the settings row so they are never a secret.
   */
  also?: string[]
  /**
   * `event.key` values that are the same physical key in another shift state.
   *
   * `Ctrl+Shift+[` never arrives as `[`: with Shift down the browser reports the
   * shifted character, so the binding has to listen for `{` as well or it can
   * never fire. Only applied while the action still sits on its shipped chord —
   * once rebound, the alias describes a key that is no longer involved.
   */
  aliasKeys?: string[]
  /**
   * This action keeps its chord even while a full-screen program is running.
   *
   * The default is `false`, and that is the careful direction. A TUI owns the
   * keyboard: `Ctrl+C` interrupts it, `Ctrl+R` searches its history, `Ctrl+W`
   * deletes a word, and an application that helps itself to those is an
   * application people cannot use `vim` or `claude` inside of.
   *
   * The exceptions are the chords that act on the *surface* rather than within
   * it. No terminal program has ever used `Ctrl+Tab`, and being unable to leave
   * the pane you are in — with the mouse as the only way out — is its own kind of
   * trap. So: switching and creating groups, and nothing else — not `Ctrl+W`,
   * which closes a tab here and deletes a word there.
   *
   * xterm cancels every key it maps, and cancelling means `stopPropagation` —
   * which is why this needs a mechanism at all rather than just a binding: see
   * `attachCustomKeyEventHandler` in `Pane.vue`.
   */
  overridesTerminal?: boolean
  note?: string
}

/**
 * Direct tab access is eight rows plus "last", generated rather than typed out:
 * nine near-identical literals is nine chances for one of them to say `Ctrl+5`
 * twice.
 */
const TAB_SLOTS: ConsoleActionSpec[] = Array.from({ length: 8 }, (_, index) => ({
  id: `selectTab${index + 1}`,
  label: `Switch to tab ${index + 1}`,
  group: 'tabs' as const,
  defaultBinding: `Ctrl+${index + 1}`,
  overridesTerminal: true,
}))

export const CONSOLE_ACTIONS: readonly ConsoleActionSpec[] = [
  // ── panes ──────────────────────────────────────────────────────────────────
  {
    id: 'splitRight',
    label: 'Split right',
    group: 'panes',
    defaultBinding: 'Ctrl+Shift+D',
    note: 'Not Ctrl+D: that is end-of-input in every shell.',
  },
  { id: 'splitDown', label: 'Split down', group: 'panes', defaultBinding: 'Ctrl+Shift+E' },
  { id: 'closePane', label: 'Close pane', group: 'panes', defaultBinding: 'Ctrl+Shift+W' },
  {
    id: 'zoomPane',
    label: 'Maximize pane',
    group: 'panes',
    defaultBinding: 'Ctrl+Shift+Enter',
  },
  {
    id: 'focusPaneLeft',
    label: 'Focus pane left',
    group: 'panes',
    defaultBinding: 'Ctrl+Alt+ArrowLeft',
    note: 'By position in the split, not by order — the pane you can see to the left is the one you get.',
  },
  { id: 'focusPaneRight', label: 'Focus pane right', group: 'panes', defaultBinding: 'Ctrl+Alt+ArrowRight' },
  { id: 'focusPaneUp', label: 'Focus pane up', group: 'panes', defaultBinding: 'Ctrl+Alt+ArrowUp' },
  { id: 'focusPaneDown', label: 'Focus pane down', group: 'panes', defaultBinding: 'Ctrl+Alt+ArrowDown' },
  {
    id: 'prevPane',
    label: 'Previous console',
    group: 'panes',
    defaultBinding: 'Ctrl+Shift+[',
    aliasKeys: ['{'],
    also: ['Ctrl+Shift+Tab'],
    overridesTerminal: true,
    note:
      'Left to right, then the next row down — and never out of this group. ' +
      'Use the group chords to leave it.',
  },
  {
    id: 'nextPane',
    label: 'Next console',
    group: 'panes',
    defaultBinding: 'Ctrl+Shift+]',
    also: ['Ctrl+Tab'],
    overridesTerminal: true,
    aliasKeys: ['}'],
  },
  // ── tabs ───────────────────────────────────────────────────────────────────
  { id: 'newTab', label: 'New tab', group: 'tabs', defaultBinding: 'Ctrl+Shift+T', overridesTerminal: true },
  { id: 'closeTab', label: 'Close tab', group: 'tabs', defaultBinding: 'Ctrl+W' },
  {
    id: 'reopenTab',
    label: 'Reopen closed tab',
    group: 'tabs',
    defaultBinding: 'Ctrl+Alt+T',
    note:
      'Within a minute of closing it. The shell itself is new — it died with the ' +
      'tab — but it opens in the same directory with the same layout.',
  },
  {
    id: 'prevTab',
    label: 'Previous group',
    group: 'tabs',
    defaultBinding: 'Ctrl+PageUp',
    overridesTerminal: true,
    note: 'Whole groups, not the consoles inside one.',
  },
  {
    id: 'nextTab',
    label: 'Next group',
    group: 'tabs',
    defaultBinding: 'Ctrl+PageDown',
    overridesTerminal: true,
  },
  { id: 'moveTabLeft', label: 'Move tab left', group: 'tabs', defaultBinding: 'Ctrl+Shift+ArrowLeft' },
  { id: 'moveTabRight', label: 'Move tab right', group: 'tabs', defaultBinding: 'Ctrl+Shift+ArrowRight' },
  ...TAB_SLOTS,
  {
    id: 'selectLastTab',
    label: 'Switch to last tab',
    group: 'tabs',
    defaultBinding: 'Ctrl+9',
    overridesTerminal: true,
  },

]

/**
 * `Ctrl+Shift+D` → its modifiers and its key.
 *
 * The last separator is found from the second-to-last character, never by
 * splitting the whole string: `Ctrl++` is a chord a user can capture, and a plain
 * split on `+` turns its key into an empty string and its modifier into the key.
 */
function splitChord(spec: string): { modifiers: string; mods: string[]; key: string } {
  const at = spec.lastIndexOf('+', spec.length - 2)
  if (at === -1) return { modifiers: '', mods: [], key: spec }
  return {
    modifiers: spec.slice(0, at + 1),
    mods: spec.slice(0, at).split('+').map((part) => part.trim().toLowerCase()).filter(Boolean),
    key: spec.slice(at + 1),
  }
}

/** `Ctrl+Shift+D` → what `useShortcuts` compares a keydown against. */
export function parseBinding(spec: string): Omit<ShortcutBinding, 'handler'> | null {
  const { mods, key } = splitChord(spec.trim())
  if (!key) return null
  return {
    key,
    // Cmd folds into Ctrl because `useShortcuts` matches `ctrlKey || metaKey`;
    // storing them apart would produce a binding that can never fire.
    ctrl: mods.includes('ctrl') || mods.includes('cmd') || mods.includes('meta'),
    alt: mods.includes('alt'),
    shift: mods.includes('shift'),
  }
}

/**
 * Every chord that runs one action: its current binding, the fixed extras, and —
 * only while it is still on its shipped chord — the shift-state aliases.
 */
export function chordsFor(action: ConsoleActionSpec, binding: string): string[] {
  const chords = [binding, ...(action.also ?? [])]
  if (binding === action.defaultBinding && action.aliasKeys?.length) {
    const { modifiers } = splitChord(binding)
    for (const key of action.aliasKeys) chords.push(modifiers + key)
  }
  return chords
}

/**
 * Key names as a person writes them. `event.key` spells the arrows `ArrowLeft`,
 * which is right for matching and wrong on a keycap.
 */
const KEY_LABELS: Readonly<Record<string, string>> = {
  ArrowLeft: 'Left',
  ArrowRight: 'Right',
  ArrowUp: 'Up',
  ArrowDown: 'Down',
  ' ': 'Space',
}

export function prettyBinding(spec: string): string {
  const { modifiers, key } = splitChord(spec)
  return modifiers + (KEY_LABELS[key] ?? key)
}

/**
 * Whether a chord names a control character the shell cannot do without
 * (research: w2 §7.2).
 *
 * `Ctrl+<letter>` with no other modifier IS a control character: the PTY turns
 * `Ctrl+A` into 0x01. A keymap that intercepts it takes start-of-line away from
 * every readline; `Ctrl+R` would take reverse search, `Ctrl+L` the clear, and a
 * shell whose control characters never arrive is broken in a way no message
 * explains. The Shortcuts capture rejects rebinds of this shape, and this
 * predicate is the one place that decides which those are.
 *
 * Two exceptions: `Ctrl+C` and `Ctrl+V`. The console already mediates both —
 * `Ctrl+C` copies a selection and interrupts without one, `Ctrl+V` pastes
 * through the multiline guard — so binding them steals nothing new. (The
 * shipped `closeTab` default `Ctrl+W` predates this rule and stays: the
 * predicate gates what the capture UI will *accept*, not what the table
 * already ships.)
 */
export function isPtyReservedChord(chord: string): boolean {
  const spec = chord.trim()
  if (!/^Ctrl\+[A-Za-z]$/.test(spec)) return false
  const letter = spec.slice(-1).toUpperCase()
  return letter !== 'C' && letter !== 'V'
}

/**
 * The pane actions a `ConsolePane` host outside the console page answers for
 * itself — the canvas terminal window binds exactly these on its frame.
 */
const PANE_ACTION_IDS = ['splitRight', 'splitDown', 'closePane', 'zoomPane'] as const

export type ConsolePaneActionId = (typeof PANE_ACTION_IDS)[number]

export interface ConsolePaneKey {
  id: ConsolePaneActionId
  /** As the pane menus print it — the same spelling the console page hands `ConsolePane`. */
  label: string
  /** Every chord that runs the action, parsed for a keydown comparison. */
  chords: Omit<ShortcutBinding, 'handler'>[]
}

/**
 * The live pane keymap for a host outside the console page (q1 #22).
 *
 * The canvas terminal window used to carry its own copy of five chords, and the
 * copy drifted: `Alt+Z` for zoom where this table says `Ctrl+Shift+Enter`, and
 * no rebind ever reached it. This derives the subset from the one table,
 * resolved through the same override record the console page reads from the
 * `console` scope's `keymap` setting — so a rebind reaches every surface a pane
 * is hosted on, not just the page it was made on.
 */
export function consolePaneKeys(overrides: Record<string, unknown> = {}): ConsolePaneKey[] {
  const keys: ConsolePaneKey[] = []
  for (const action of CONSOLE_ACTIONS) {
    if (!(PANE_ACTION_IDS as readonly string[]).includes(action.id)) continue
    const stored = overrides[action.id]
    // Same fallback as the page: an override that is not a string, or does not
    // parse, falls back to the shipped chord rather than dropping the action.
    const spec = typeof stored === 'string' && parseBinding(stored) ? stored : action.defaultBinding
    keys.push({
      id: action.id as ConsolePaneActionId,
      label: prettyBinding(spec),
      chords: chordsFor(action, spec)
        .map((chord) => parseBinding(chord))
        .filter((parsed): parsed is Omit<ShortcutBinding, 'handler'> => parsed !== null),
    })
  }
  return keys
}
