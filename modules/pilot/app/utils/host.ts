/**
 * Where a QuantPilot chat runs (pre-1.1.0 embed). On the /pilot page there
 * is no host and the chat uses the page (the active thread, the inspector).
 * A chat embedded in another module (QuantCanvas's chat window) provides a
 * host: the thread it shows, and the module's own way to open a file —
 * QuantCanvas opens a File window instead of navigating away.
 */
import { inject, type InjectionKey } from 'vue'

export interface PilotHost {
  /** The module that embeds the chat, e.g. `canvas`. */
  kind: string
  /** The thread this chat shows. */
  threadId: string
  /** Open a file of the thread (absolute, or relative to its folder); false = not handled. */
  openFile?: (path: string, line: number | null) => boolean
  /** How the host names its open action, e.g. "Open in a File window". */
  openLabel?: string
}

export const PILOT_HOST: InjectionKey<PilotHost> = Symbol('pilot-host')

export function useHost(): PilotHost | null {
  return inject(PILOT_HOST, null)
}
