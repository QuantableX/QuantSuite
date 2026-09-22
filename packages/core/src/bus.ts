/**
 * Frontend client for the Rust event bus (ARCHITECTURE.md §3).
 *
 * Every webview listens on one Tauri channel and fans out locally by topic, so
 * a hundred subscriptions still cost one listener.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import type { QsEvent } from './types'

export const CHANNEL = 'qs://event'

type Handler = (event: QsEvent<any>) => void

const handlers = new Map<string, Set<Handler>>()
let unlisten: UnlistenFn | null = null
let starting: Promise<void> | null = null

async function ensureListening() {
  if (unlisten) return
  if (starting) return starting

  starting = listen<QsEvent>(CHANNEL, (msg) => {
    const event = msg.payload
    handlers.get(event.topic)?.forEach((h) => {
      try {
        h(event)
      } catch (err) {
        // One bad subscriber must not stop delivery to the others.
        console.error(`[bus] handler for "${event.topic}" threw`, err)
      }
    })
    handlers.get('*')?.forEach((h) => {
      try {
        h(event)
      } catch (err) {
        // Same contract for the wildcard fan-out.
        console.error('[bus] wildcard handler threw', err)
      }
    })
  })
    .then((fn) => {
      unlisten = fn
    })
    .catch(() => {
      // No Tauri backend — running the shell in a plain browser during
      // development. Subscriptions stay registered and simply never fire.
      starting = null
    })

  return starting
}

/**
 * Subscribe to a topic. Pass `'*'` to observe everything - useful for a debug
 * view, not for wiring.
 */
export function on<T = unknown>(topic: string, handler: (event: QsEvent<T>) => void): () => void {
  void ensureListening()

  let set = handlers.get(topic)
  if (!set) {
    set = new Set()
    handlers.set(topic, set)
  }
  set.add(handler as Handler)

  return () => {
    set!.delete(handler as Handler)
    if (set!.size === 0) handlers.delete(topic)
  }
}

/** Publish an event. Rejects if the topic is malformed - the Rust side validates. */
export function emit(topic: string, payload?: unknown, source = 'shell'): Promise<void> {
  return invoke('plugin:qs|emit_event', { topic, source, payload: payload ?? null })
}

export function recentEvents(limit = 100): Promise<QsEvent[]> {
  return invoke('plugin:qs|recent_events', { limit })
}

/** Tear down the single Tauri listener. Only useful in tests. */
export async function dispose() {
  handlers.clear()
  if (unlisten) {
    unlisten()
    unlisten = null
    starting = null
  }
}
