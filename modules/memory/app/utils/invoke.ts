/**
 * The module's one lazy Tauri `invoke` (the control-module pattern).
 *
 * Import it explicitly (`#memory/utils/invoke`): auto-imports are global
 * across layers, and another module's helper of the same name could win.
 */

type InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<any>

let invoke: InvokeFn | null = null

export async function getInvoke(): Promise<InvokeFn> {
  if (typeof window === 'undefined' || !window.__TAURI_INTERNALS__) {
    return async () => { throw new Error('Open the desktop app to access your memory vault.') }
  }
  if (!invoke) {
    try {
      const tauri = await import('@tauri-apps/api/core')
      invoke = tauri.invoke as InvokeFn
    } catch {
      // Outside Tauri every command resolves to null — each caller already
      // handles an empty result.
      invoke = async () => null
    }
  }
  return invoke
}
