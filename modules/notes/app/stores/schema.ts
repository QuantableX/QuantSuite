import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import type {
  Property,
  PropertyConfig,
  PropertyKind,
  SelectOption,
  View,
  ViewConfig,
  ViewKind,
} from '#notes/types'

/**
 * The collection's schema: its properties and its views.
 *
 * Both are small, global and read by every view, so they live in one store —
 * a board needs the property to group by, a table needs every property, and
 * the view tabs need the views. Nothing here is per-note.
 */
export const useSchemaStore = defineStore('notes/schema', () => {
  const properties = ref<Property[]>([])
  const views = ref<View[]>([])
  const activeViewId = ref<string | null>(null)

  const propertyById = computed(() => {
    const map = new Map<string, Property>()
    for (const p of properties.value) map.set(p.id, p)
    return map
  })

  const activeView = computed(() => views.value.find((v) => v.id === activeViewId.value) ?? views.value[0] ?? null)

  /** The properties a view shows, in the view's own order. Anything the view
   * does not name is hidden, not appended — that is what makes `visible` an
   * ordering and not just a filter. */
  function visibleProperties(view: View | null): Property[] {
    if (!view) return []
    const ids = view.config.visible
    if (!ids || ids.length === 0) return properties.value
    return ids.map((id) => propertyById.value.get(id)).filter((p): p is Property => !!p)
  }

  function optionsOf(propertyId: string | undefined): SelectOption[] {
    if (!propertyId) return []
    return propertyById.value.get(propertyId)?.config.options ?? []
  }

  async function load() {
    const [props, vs] = await Promise.all([
      invoke<Property[]>('plugin:notes|list_properties'),
      invoke<View[]>('plugin:notes|list_views'),
    ])
    properties.value = props
    views.value = vs
    if (!activeViewId.value || !vs.some((v) => v.id === activeViewId.value)) {
      activeViewId.value = vs[0]?.id ?? null
    }
  }

  function setActiveView(id: string) {
    activeViewId.value = id
  }

  // ── Properties ────────────────────────────────────────────────────────

  async function createProperty(name: string, kind: PropertyKind, config?: PropertyConfig) {
    const property = await invoke<Property>('plugin:notes|create_property', {
      name,
      kind,
      config: config ?? null,
    })
    properties.value.push(property)
    return property
  }

  async function updateProperty(
    id: string,
    patch: { name?: string; config?: PropertyConfig; sortIndex?: number },
  ) {
    const updated = await invoke<Property>('plugin:notes|update_property', {
      id,
      name: patch.name ?? null,
      config: patch.config ?? null,
      sortIndex: patch.sortIndex ?? null,
    })
    const at = properties.value.findIndex((p) => p.id === id)
    if (at !== -1) properties.value[at] = updated
    return updated
  }

  /** Add an option to a select / multi-select. Returns the created option so
   * the caller can immediately assign it — the "create and pick" flow. */
  async function addOption(propertyId: string, name: string, color: SelectOption['color']) {
    const property = propertyById.value.get(propertyId)
    if (!property) throw new Error(`No property ${propertyId}`)
    const option: SelectOption = { id: crypto.randomUUID(), name, color }
    const options = [...(property.config.options ?? []), option]
    await updateProperty(propertyId, { config: { ...property.config, options } })
    return option
  }

  async function deleteProperty(id: string) {
    await invoke<boolean>('plugin:notes|delete_property', { id })
    properties.value = properties.value.filter((p) => p.id !== id)
    // A view that grouped or dated by it now points at nothing; drop the
    // reference so the view falls back instead of rendering an empty board.
    for (const view of views.value) {
      const config = { ...view.config }
      let touched = false
      if (config.groupBy === id) {
        delete config.groupBy
        touched = true
      }
      if (config.dateBy === id) {
        delete config.dateBy
        touched = true
      }
      if (config.visible?.includes(id)) {
        config.visible = config.visible.filter((v) => v !== id)
        touched = true
      }
      if (touched) await updateView(view.id, { config })
    }
  }

  // ── Views ─────────────────────────────────────────────────────────────

  async function createView(name: string, kind: ViewKind, config?: ViewConfig) {
    const view = await invoke<View>('plugin:notes|create_view', { name, kind, config: config ?? null })
    views.value.push(view)
    activeViewId.value = view.id
    return view
  }

  async function updateView(id: string, patch: { name?: string; config?: ViewConfig; sortIndex?: number }) {
    const updated = await invoke<View>('plugin:notes|update_view', {
      id,
      name: patch.name ?? null,
      config: patch.config ?? null,
      sortIndex: patch.sortIndex ?? null,
    })
    const at = views.value.findIndex((v) => v.id === id)
    if (at !== -1) views.value[at] = updated
    return updated
  }

  /** Merge one key into the active view's config — the shape every toolbar
   * control needs, so none of them has to rebuild the whole config object. */
  async function patchActiveConfig(patch: Partial<ViewConfig>) {
    const view = activeView.value
    if (!view) return
    await updateView(view.id, { config: { ...view.config, ...patch } })
  }

  async function deleteView(id: string) {
    await invoke<boolean>('plugin:notes|delete_view', { id })
    views.value = views.value.filter((v) => v.id !== id)
    if (activeViewId.value === id) activeViewId.value = views.value[0]?.id ?? null
  }

  function reset() {
    properties.value = []
    views.value = []
    activeViewId.value = null
  }

  return {
    properties,
    views,
    activeViewId,
    activeView,
    propertyById,
    visibleProperties,
    optionsOf,
    load,
    setActiveView,
    createProperty,
    updateProperty,
    addOption,
    deleteProperty,
    createView,
    updateView,
    patchActiveConfig,
    deleteView,
    reset,
  }
})
