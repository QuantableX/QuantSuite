/**
 * The unified settings modal's extension point (V3).
 *
 * `QSettingsModal` is one surface for the whole suite: a "Suite" group plus a
 * group for the active module. A module contributes sections by registering
 * Vue components from a client plugin in its layer, conventionally
 * `modules/<id>/app/plugins/<id>-settings.client.ts`:
 *
 *   registerSettingsSections('canvas', [
 *     { id: 'general', label: 'General', component: markRaw(CanvasSettingsGeneral) },
 *   ])
 *
 * The shell registers its own suite-level sections under the reserved id
 * `suite`. Registration is idempotent per (moduleId, section id) so HMR
 * re-runs don't duplicate entries.
 */

import { shallowRef } from 'vue'
import { isModuleEnabled } from './app-availability'
import type { Component } from 'vue'

export interface SettingsSection {
  id: string
  label: string
  /** Wrap in `markRaw()` at the call site — sections are static components. */
  component: Component
  order?: number
}

/**
 * A `shallowRef` over a replaced Map, not a plain Map: `settingsSectionsFor` is
 * read from computeds in QSettingsModal, so a module registering sections after
 * the modal opened (the HMR case) has to invalidate them. A mutated Map is not
 * a reactive dependency and those computeds would never recompute.
 */
const registry = shallowRef(new Map<string, SettingsSection[]>())

export function registerSettingsSections(moduleId: string, sections: SettingsSection[]): void {
  const existing = registry.value.get(moduleId) ?? []
  const merged = [...existing]
  for (const section of sections) {
    const i = merged.findIndex((s) => s.id === section.id)
    if (i >= 0) merged[i] = section
    else merged.push(section)
  }
  merged.sort((a, b) => (a.order ?? 0) - (b.order ?? 0))
  registry.value = new Map(registry.value).set(moduleId, merged)
}

export function settingsSectionsFor(moduleId: string): SettingsSection[] {
  return isModuleEnabled(moduleId) ? registry.value.get(moduleId) ?? [] : []
}
