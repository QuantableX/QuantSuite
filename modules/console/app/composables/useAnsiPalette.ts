/**
 * The configurable ANSI palette (docs/PLAN-CONSOLE.md §1, phase P7).
 *
 * The suite theme itself is not configurable (PLAN-V3 §3) — these sixteen colours
 * are the one exception, because "red" in a compiler's output is the program's
 * choice of channel, not the app's design.
 *
 * Module-level state on purpose: the block renderer and the alternate-screen
 * pane must agree on what red is, and two independent loads could disagree for a
 * frame. One load, one map, both read it, and a change repaints both.
 */

import { ref } from 'vue'
import { bus, qs } from '@quantsuite/core'

/**
 * The built-in sixteen. Kept as `var(…)` where the suite has a token, so a theme
 * change repaints without re-rendering: only the four channels the suite has an
 * opinion about are tokens, the rest are the terminal colours xterm gets.
 *
 * The **one** source of the defaults (q1 #38). `Pane.vue`'s xterm theme resolves
 * these through `colour()` — xterm needs resolved colours, not CSS variables —
 * and the settings panel imports this table for its swatches. Neither keeps a
 * copy; if this table is wrong, everything is wrong *together*, which is the
 * only failure mode that gets noticed.
 */
export const ANSI_BUILTIN: readonly string[] = [
  'var(--qss-bg-chrome)',
  'var(--qss-error)',
  'var(--qss-success)',
  'var(--qss-warning)',
  '#5b9cff',
  '#c084fc',
  '#22d3ee',
  'var(--qss-text)',
  'var(--qss-text-muted)',
  '#ff6b7a',
  '#4ade80',
  '#fbbf24',
  '#7fb2ff',
  '#d8b4fe',
  '#67e8f9',
  '#ffffff',
]

/**
 * The sixteen names, index-aligned with `ANSI_BUILTIN` — the vocabulary of the
 * settings panel's rows. Beside the colours so a seventeenth entry (or a
 * renumbering) cannot happen to one table without the other.
 */
export const ANSI_LABELS: readonly string[] = [
  'Black',
  'Red',
  'Green',
  'Yellow',
  'Blue',
  'Magenta',
  'Cyan',
  'White',
  'Bright black',
  'Bright red',
  'Bright green',
  'Bright yellow',
  'Bright blue',
  'Bright magenta',
  'Bright cyan',
  'Bright white',
]

/** Overrides for 0–15, keyed `"0"`…`"15"`; anything else is ignored. */
const overrides = ref<Record<string, string>>({})
/** Bumped on every change, so consumers can invalidate their own memos. */
const revision = ref(0)
let loaded = false
let offSetting: (() => void) | undefined

function sanitize(value: unknown): Record<string, string> {
  if (!value || typeof value !== 'object') return {}
  const out: Record<string, string> = {}
  for (const [key, colour] of Object.entries(value as Record<string, unknown>)) {
    const index = Number(key)
    // A key outside 0–15 or a non-string colour is a database someone edited by
    // hand; dropping it beats painting `undefined` into a style attribute.
    if (!Number.isInteger(index) || index < 0 || index > 15) continue
    if (typeof colour !== 'string' || !colour.trim()) continue
    out[String(index)] = colour.trim()
  }
  return out
}

async function load() {
  try {
    overrides.value = sanitize(await qs.core.getSetting('console', 'ansi.palette'))
  } catch {
    // Browser development, or the setting was never written: the built-ins are
    // the answer, and a failed read must not blank the terminal's colours.
    overrides.value = {}
  }
  revision.value += 1
}

/**
 * The palette, loaded once per page and kept current.
 *
 * `revision` is what a memoising renderer watches: `BlockOutput` caches a CSS
 * string per colour, and without an invalidation signal a palette change would
 * only show up on output printed afterwards.
 *
 * The `bus.on` below is subscribed once and — deliberately — never unsubscribed
 * in the app. The state above is module-level and shared by every consumer
 * (Pane, Editor, Groups, BlockOutput); tying the subscription to any one
 * component's lifetime would let that component's unmount go deaf for all the
 * others, and refcounting four permanent consumers buys nothing. Under the V3
 * warm cache the console page lives as long as the window does, so "module
 * lifetime" and "page lifetime" are the same thing. `dispose()` exists for
 * tests, which do create and tear down repeatedly.
 */
export function useAnsiPalette() {
  if (!loaded) {
    loaded = true
    void load()
    // The settings panel writes the value; this is how it arrives without a
    // restart (P7's "every setting takes effect without one").
    offSetting = bus.on<{ scope: string; key: string }>('core.setting.changed', (event) => {
      if (event.payload?.scope === 'console' && event.payload?.key === 'ansi.palette') void load()
    })
  }

  return {
    overrides,
    revision,
    /** Colour for an ANSI index 0–15: the override if there is one. */
    colour: (index: number) => overrides.value[String(index)] ?? ANSI_BUILTIN[index] ?? 'inherit',
    /** For tests only (see above) — the app never calls it, and must not. */
    dispose: () => {
      offSetting?.()
      offSetting = undefined
      loaded = false
    },
  }
}
