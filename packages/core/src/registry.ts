/**
 * The module registry. Generated from every `modules/<id>/module.json` by
 * `scripts/validate-modules.mjs`, so the manifests stay the single source of
 * truth and nothing is hand-maintained in two places.
 */

import generated from '../../../modules/registry.generated.json'
import type { AppInfo, ModuleInfo } from './types'

export const modules: ModuleInfo[] = (generated.modules as ModuleInfo[])
  .slice()
  .sort((a, b) => a.order - b.order)

/** Modules that have actually landed — the only ones with working routes. */
export const migratedModules = modules.filter((m) => m.status === 'migrated')

/** Modules the selector may navigate to: landed ones plus coming-soon stubs. */
export const routableModules = modules.filter((m) => m.status === 'migrated' || m.status === 'stub')

/** The suite apps — the rail's vertical entries, dashboard first (V3). */
export const apps: AppInfo[] = (generated.apps as AppInfo[]).slice().sort((a, b) => a.order - b.order)

export function moduleById(id: string): ModuleInfo | undefined {
  return modules.find((m) => m.id === (id === 'systems' ? 'algo' : id))
}

/** Which module a route belongs to, e.g. `/algo/backtests/1` → `algo`. */
export function moduleForRoute(path: string): ModuleInfo | undefined {
  const id = path.split('/').filter(Boolean)[0]
  return id ? moduleById(id) : undefined
}

export function appById(id: string): AppInfo | undefined {
  return apps.find((a) => a.id === id)
}

/** The app a module belongs to; undefined for own-window modules. */
export function appForModule(moduleId: string): AppInfo | undefined {
  return apps.find((a) => a.modules.includes(moduleId === 'systems' ? 'algo' : ['plan', 'habit'].includes(moduleId) ? 'flow' : moduleId))
}

/** An app's member modules, sorted by their position in the selector. */
export function modulesForApp(appId: string): ModuleInfo[] {
  const app = appById(appId)
  if (!app) return []
  return app.modules
    .map((id) => moduleById(id))
    .filter((m): m is ModuleInfo => m !== undefined)
}

/** Separate warm stages preserve both QuantAlgo interfaces and their jobs. */
export function stageForRoute(path: string): string | null {
  if (path === '/algo/manual' || path.startsWith('/algo/manual/')) return 'algo-manual'
  return moduleForRoute(path)?.id ?? null
}
