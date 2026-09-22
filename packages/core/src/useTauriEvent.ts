/**
 * The suite's one Tauri-event composable (V3) — replaces three per-module
 * copies (notes, systems, algo) that had drifted apart.
 *
 * `listen()` failing is the normal browser-development case (no Tauri bridge),
 * so the rejection is swallowed rather than logged — the algo copy logged it
 * and printed one error per subscription on every `npm run dev` page load.
 *
 * Deliberately NOT keep-alive gated, unlike `useShortcuts`: every call site
 * (systems' `eval:progress` and `engine:status`, the cache panel's
 * `cache:updated`, QuantNotes' `workspace:active-changed` and `tool:item-*`) only
 * writes state the view must already be correct on the way back in, and the
 * backend keeps emitting while the module is hidden. Gating would trade a few
 * cheap store writes for a silently stale view, since none of these call sites
 * has an activation refresh to catch up with. Gate at the call site — never
 * here — and only where you also wire that refresh.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { onMounted, onUnmounted } from 'vue'

export function useTauriEvent<T = Record<string, unknown>>(eventName: string, handler: (payload: T) => void) {
  let unlisten: UnlistenFn | null = null
  let disposed = false

  onMounted(async () => {
    try {
      const stop = await listen<T>(eventName, event => handler(event.payload))
      // The component can unmount inside the listen() round-trip — the unmount
      // hook then ran with unlisten still null, so drop the listener here.
      if (disposed) stop()
      else unlisten = stop
    } catch {
      unlisten = null
    }
  })

  onUnmounted(() => {
    disposed = true
    unlisten?.()
    unlisten = null
  })
}
