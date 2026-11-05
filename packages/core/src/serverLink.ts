/**
 * The server this PC uses, if any (PLAN-SERVER-CLIENT §5, card Server 5/7):
 * its connection state for the title bar and the offline banner, which
 * modules' pages come from it, a tick that makes those pages reload their
 * data after a reconnect, and what this device's role allows.
 */
import { computed, readonly, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

export type ServerDeviceRole = 'full' | 'no_trading' | 'read_only'

export interface ServerLinkStatus {
  mode: 'local' | 'client'
  connection: { id: string; name: string; address: string; role: ServerDeviceRole } | null
  state: 'local' | 'connecting' | 'online' | 'offline' | 'refused'
  since: string | null
  message: string | null
  server_app: string | null
  role: ServerDeviceRole | null
  server_modules: string[]
}

const status = ref<ServerLinkStatus | null>(null)
const refreshTick = ref(0)
let started = false

function start() {
  if (started || typeof window === 'undefined' || !('__TAURI__' in window)) return
  started = true
  invoke<ServerLinkStatus>('plugin:qs|gateway_link_status')
    .then((s) => {
      status.value = s
    })
    .catch(() => undefined)
  void listen<ServerLinkStatus>('server:connection', (e) => {
    status.value = e.payload
  })
  void listen('server:refresh', () => {
    refreshTick.value++
  })
}

const isClient = computed(() => status.value?.mode === 'client')

export function useServerLink() {
  start()
  return {
    status: readonly(status),
    isClient,
    /** Local, or connected to the server. */
    online: computed(() => !isClient.value || status.value?.state === 'online'),
    /** Bumped when the server's pages must load their data again. */
    refreshTick: readonly(refreshTick),
    /** This module's pages come from the server. */
    isServerModule: (moduleId: string | null | undefined) =>
      isClient.value && !!moduleId && (status.value?.server_modules ?? []).includes(moduleId),
    /** Live trading and keys: local, or a Full device (the PIN is asked when it matters). */
    canTrade: computed(() => !isClient.value || status.value?.role === 'full'),
    /** Changing data at all. */
    canWrite: computed(() => !isClient.value || status.value?.role !== 'read_only'),
    retry: () => invoke('plugin:qs|gateway_link_retry').catch(() => undefined),
  }
}
