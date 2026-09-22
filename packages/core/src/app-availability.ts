import { readonly, ref } from 'vue'
import { apps, appForModule } from './registry'
import { qs } from './commands'
import * as bus from './bus'

export const APP_ENABLED_PREFIX = 'apps.enabled.'
const enabled = ref<Record<string, boolean>>({})
const ready = ref(false)
const error = ref<string | null>(null)
let loading: Promise<void> | null = null
let subscribed = false
let revision = 0
const changedAt = new Map<string, number>()

export const appAvailability = { ready: readonly(ready), error: readonly(error) }

export function isAppEnabled(id: string): boolean {
  return id === 'dashboard' || (ready.value && enabled.value[id] !== false)
}

export function isModuleEnabled(id: string): boolean {
  const owner = appForModule(id)
  return !owner || isAppEnabled(owner.id)
}

export function isRouteEnabled(path: string): boolean {
  return isModuleEnabled(path.split(/[/?#]/).filter(Boolean)[0] ?? '')
}

function applySetting(key: string, value: unknown) {
  if (!key.startsWith(APP_ENABLED_PREFIX) || typeof value !== 'boolean') return
  const id = key.slice(APP_ENABLED_PREFIX.length)
  if (!apps.some((a) => a.id === id && id !== 'dashboard')) return
  revision++
  changedAt.set(id, revision)
  enabled.value = { ...enabled.value, [id]: value }
}

/** Read before routing or rendering, so disabled apps never flash at startup. */
export function loadAppAvailability(): Promise<void> {
  if (loading) return loading
  if (!subscribed) {
    subscribed = true
    bus.on<{ scope: string; key: string; value: unknown }>('core.setting.changed', ({ payload }) => {
      if (payload.scope === 'core') applySetting(payload.key, payload.value)
    })
  }
  const startedAt = revision
  loading = (async () => {
    try {
      const settings = await qs.core.getSettings('core')
      // A setting event arriving during the read is newer than its snapshot.
      const next: Record<string, boolean> = {}
      for (const app of apps) {
        const saved = settings[APP_ENABLED_PREFIX + app.id]
        next[app.id] = (changedAt.get(app.id) ?? 0) > startedAt
          ? enabled.value[app.id] !== false
          : typeof saved === 'boolean' ? saved : app.defaultEnabled
      }
      enabled.value = next
      ready.value = true
      error.value = null
    } catch (e) {
      error.value = String(e)
    } finally {
      loading = null
    }
  })()
  return loading
}

export async function setAppEnabled(id: string, value: boolean): Promise<void> {
  if (!ready.value) throw new Error('App settings are not loaded. Try again.')
  if (!apps.some((a) => a.id === id && id !== 'dashboard')) throw new Error('Unknown app')
  const key = APP_ENABLED_PREFIX + id
  const startedAt = changedAt.get(id)
  await qs.core.setSetting('core', key, value)
  if (changedAt.get(id) === startedAt) applySetting(key, value)
}
