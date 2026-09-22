/**
 * The module's one lazy Tauri `invoke` (the suite-wide pattern).
 *
 * Import it explicitly (`#pilot/utils/invoke`): auto-imports are global
 * across layers, and another module's helper of the same name could win.
 */

type InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<any>

let invoke: InvokeFn | null = null

export async function getInvoke(): Promise<InvokeFn> {
  if (!invoke) {
    const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
    if (!inTauri) {
      // Outside Tauri every command resolves to null — callers treat null as
      // "offline" and the page still renders.
      invoke = async () => null
      return invoke
    }
    try {
      const tauri = await import('@tauri-apps/api/core')
      invoke = tauri.invoke as InvokeFn
    } catch {
      invoke = async () => null
    }
  }
  return invoke
}

/** One plugin call, by its full `plugin:pilot|…` name (the isolation guard
 *  wants the namespace visible at the call site). */
export async function pilot<T>(cmd: string, args?: Record<string, unknown>): Promise<T | null> {
  const fn = await getInvoke()
  return (await fn(cmd, args)) as T | null
}
