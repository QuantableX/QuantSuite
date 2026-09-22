/**
 * The suite's one theme composable (V3) — replaces five per-module copies.
 *
 * The suite is dark, monochrome, by decision (2026-08-14); there is no light
 * mode to toggle. The API keeps the shape the module copies exposed so their
 * call sites survive the import swap, and gives theming a single home if a
 * second theme ever returns.
 */

import { computed, ref } from 'vue'
import type { ComputedRef, Ref } from 'vue'

const theme: Ref<'dark'> = ref('dark')

export function useTheme(): { theme: Readonly<Ref<'dark'>>; isDark: ComputedRef<boolean> } {
  return {
    theme,
    isDark: computed(() => true),
  }
}
