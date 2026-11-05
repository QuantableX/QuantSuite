/**
 * The client-mode seam (PLAN-SERVER-CLIENT §6.1, card Server 5/7).
 *
 * Every `import … from '@tauri-apps/api/core'` in the app resolves here (a
 * Vite plugin in nuxt.config.ts); this module imports the real one. The
 * runtime cannot be wrapped instead: Tauri defines `window.__TAURI_INTERNALS__`
 * and its `invoke` non-writable and non-configurable (§6.3.4 risk 10).
 *
 * On a PC without a server nothing changes. When this PC uses a server,
 * `invoke` routes by the remote manifest (`routing.mjs`): server commands go
 * through `plugin:qs|gateway_remote_call`, which holds the gateway connection
 * in Rust; an `external` call the server answers with `step_up_required`
 * asks for the server PIN once and is sent again. Events need no seam: the
 * Rust side re-emits the server's events here under their own names.
 */
import * as tauri from '@tauri-apps/api/core'
import type { InvokeArgs, InvokeOptions } from '@tauri-apps/api/core'
import manifest from '../../../../crates/qs-gateway/remote-manifest.json'
import { mergeSearch, route } from './routing.mjs'

export * from '@tauri-apps/api/core'

interface LinkMode {
  mode: 'local' | 'client'
}

let mode: Promise<LinkMode> | null = null

/** Local or client, asked once per window (switching restarts the app). */
function linkMode(): Promise<LinkMode> {
  mode ??= tauri.invoke<LinkMode>('plugin:qs|gateway_link_status').catch(() => ({ mode: 'local' as const }))
  return mode
}

interface GatewayError {
  code?: string
  message?: string
}

/** Asks the shell's PIN prompt (ServerPinPrompt); `null` when cancelled or
 * when no prompt exists in this window. */
function askPin(message: string): Promise<string | null> {
  return new Promise((resolve) => {
    const ev = new CustomEvent('qs:server-pin', { detail: { message, resolve }, cancelable: true })
    // A mounted prompt claims the request with preventDefault().
    if (window.dispatchEvent(ev)) resolve(null)
  })
}

async function remote<T>(cmd: string, args: InvokeArgs | undefined): Promise<T> {
  const body = { cmd, args: args ?? {} }
  for (let attempt = 0; ; attempt++) {
    try {
      return await tauri.invoke<T>('plugin:qs|gateway_remote_call', body)
    } catch (e) {
      const err: GatewayError = e && typeof e === 'object' ? (e as GatewayError) : { message: String(e) }
      if (err.code === 'step_up_required' && attempt === 0) {
        const pin = await askPin(err.message ?? '')
        if (!pin) throw 'This needs the server PIN.'
        await tauri.invoke('plugin:qs|gateway_step_up', { pin })
        continue
      }
      // Module code shows errors as text, as it does for local commands.
      throw err.message ?? String(e)
    }
  }
}

export async function invoke<T>(cmd: string, args?: InvokeArgs, options?: InvokeOptions): Promise<T> {
  if (cmd.startsWith('plugin:qs|gateway_')) return tauri.invoke<T>(cmd, args, options)
  if ((await linkMode()).mode !== 'client') return tauri.invoke<T>(cmd, args, options)
  const a = (args && typeof args === 'object' && !Array.isArray(args) ? args : {}) as Record<string, unknown>
  switch (route(manifest, cmd, a)) {
    case 'server':
      return remote<T>(cmd, args)
    case 'deny':
      throw `${cmd} is not available while this PC uses a server.`
    case 'both': {
      const [mine, far] = await Promise.allSettled([tauri.invoke<unknown[]>(cmd, args, options), remote<unknown[]>(cmd, args)])
      if (mine.status === 'rejected' && far.status === 'rejected') throw far.reason
      const limit = typeof a.limit === 'number' ? a.limit : undefined
      return mergeSearch(manifest, mine.status === 'fulfilled' ? mine.value : [], far.status === 'fulfilled' ? far.value : [], limit) as T
    }
    default:
      return tauri.invoke<T>(cmd, args, options)
  }
}
