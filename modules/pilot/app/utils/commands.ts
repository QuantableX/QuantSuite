/**
 * What each command id does (card 17). Components register the handlers
 * they own (`useCommand('chat.find', …)` in the timeline); the keys, the
 * palette and anything else run them by id. Warm cache: a cached component
 * stands down while its module stage is in the background, and the newest
 * active registration answers. A handler returns `false` when it did not
 * apply (no open thread), so the key goes on to whoever else wants it.
 */
import { onActivated, onDeactivated, onMounted, onUnmounted } from 'vue'
import { inActiveKeepAliveTree } from '@quantsuite/core'

type Handler = () => unknown

const handlers = new Map<string, Handler[]>()

export function registerCommand(id: string, fn: Handler): () => void {
  const list = (handlers.get(id) ?? []).filter((f) => f !== fn)
  list.push(fn)
  handlers.set(id, list)
  return () => handlers.set(id, (handlers.get(id) ?? []).filter((f) => f !== fn))
}

/** Runs the command, newest registration first; `false` when nothing answers it. */
export function runCommand(id: string): boolean {
  const list = handlers.get(id) ?? []
  for (let i = list.length - 1; i >= 0; i--) {
    const r = list[i]()
    if (r instanceof Promise) r.catch((e) => console.error(`[pilot] ${id} failed`, e))
    if (r !== false) return true
  }
  return false
}

export function hasCommand(id: string): boolean {
  return Boolean(handlers.get(id)?.length)
}

/** Several commands of one component, armed and disarmed with it. */
export function useCommands(map: Record<string, Handler>) {
  let off: (() => void)[] = []
  const arm = () => {
    disarm()
    off = Object.entries(map).map(([id, fn]) => registerCommand(id, fn))
  }
  const disarm = () => {
    off.forEach((f) => f())
    off = []
  }
  onMounted(() => inActiveKeepAliveTree() && arm())
  onActivated(arm)
  onDeactivated(disarm)
  onUnmounted(disarm)
}
