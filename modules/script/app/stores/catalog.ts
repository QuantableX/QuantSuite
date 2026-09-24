import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { computed, ref } from 'vue'
import type {
  CollectionCatalog,
  CollectionInstallOutcome,
  CollectionItemDetail,
  CollectionPlan,
  CollectionRemoved,
  CollectionSource,
  CollectionSourceInput,
} from '#script/types'
import { DEFAULT_COLLECTION_FILTERS, filterItems, type CollectionFilters } from '#script/utils/catalog'

const SOURCE_KEY = 'quantsuite.script.collection.source'

function rememberedSource(): string | null {
  try {
    return localStorage.getItem(SOURCE_KEY)
  } catch {
    return null
  }
}

/**
 * The QuantScript Collection (plugin:script|collection_*): the catalog of one source
 * at a pinned commit, the item on screen, installs and removals. Tokens go
 * straight to the backend's credential store and are never kept here.
 */
export const useCatalogStore = defineStore('script/catalog', () => {
  const sources = ref<CollectionSource[]>([])
  const sourcesLoading = ref(false)
  const sourcesError = ref<string | null>(null)
  const sourceId = ref<string | null>(rememberedSource())

  const catalog = ref<CollectionCatalog | null>(null)
  const catalogLoading = ref(false)
  const catalogError = ref<string | null>(null)

  const selectedKey = ref<string | null>(null)
  const detail = ref<CollectionItemDetail | null>(null)
  const detailLoading = ref(false)
  const detailError = ref<string | null>(null)

  const filters = ref<CollectionFilters>({ ...DEFAULT_COLLECTION_FILTERS })
  /** `install` / `remove` while one runs. */
  const busy = ref<string | null>(null)

  const source = computed(() => sources.value.find((s) => s.id === sourceId.value) ?? null)
  const items = computed(() => catalog.value?.items ?? [])
  const visible = computed(() => filterItems(items.value, filters.value))
  const byKey = computed(() => new Map(items.value.map((i) => [i.key, i])))
  const installedCount = computed(() => items.value.filter((i) => i.local?.installed_version).length)
  const updateCount = computed(() => items.value.filter((i) => i.local?.update).length)

  async function loadSources() {
    sourcesLoading.value = true
    sourcesError.value = null
    try {
      sources.value = await invoke<CollectionSource[]>('plugin:script|collection_sources')
      if (!sources.value.some((s) => s.id === sourceId.value)) {
        sourceId.value = sources.value.find((s) => s.enabled)?.id ?? sources.value[0]?.id ?? null
      }
    } catch (err) {
      sourcesError.value = String(err)
    } finally {
      sourcesLoading.value = false
    }
  }

  function selectSource(id: string) {
    if (id === sourceId.value) return
    sourceId.value = id
    try {
      localStorage.setItem(SOURCE_KEY, id)
    } catch {
      /* per-viewer convenience only */
    }
    catalog.value = null
    detail.value = null
    selectedKey.value = null
    void loadCatalog(false)
  }

  /** `refresh` resolves the branch to its newest commit. */
  async function loadCatalog(refresh = false) {
    if (!sourceId.value) return
    catalogLoading.value = true
    catalogError.value = null
    try {
      catalog.value = await invoke<CollectionCatalog>('plugin:script|collection_catalog', { sourceId: sourceId.value, refresh })
      const updated = catalog.value.source
      sources.value = sources.value.map((s) => (s.id === updated.id ? updated : s))
      if (selectedKey.value && !byKey.value.has(selectedKey.value)) selectedKey.value = null
      if (selectedKey.value) void openItem(selectedKey.value)
    } catch (err) {
      catalogError.value = String(err)
      catalog.value = null
    } finally {
      catalogLoading.value = false
    }
  }

  async function openItem(key: string) {
    if (!sourceId.value) return
    selectedKey.value = key
    detailLoading.value = true
    detailError.value = null
    try {
      const doc = await invoke<CollectionItemDetail>('plugin:script|collection_item', { sourceId: sourceId.value, key })
      if (selectedKey.value === key) detail.value = doc
    } catch (err) {
      if (selectedKey.value === key) {
        detailError.value = String(err)
        detail.value = null
      }
    } finally {
      if (selectedKey.value === key) detailLoading.value = false
    }
  }

  function plan(keys: string[]): Promise<CollectionPlan> {
    return invoke<CollectionPlan>('plugin:script|collection_plan', { sourceId: sourceId.value, keys })
  }

  async function install(keys: string[], overwrite: boolean): Promise<CollectionInstallOutcome> {
    busy.value = 'install'
    try {
      return await invoke<CollectionInstallOutcome>('plugin:script|collection_install', { sourceId: sourceId.value, keys, overwrite })
    } finally {
      busy.value = null
      await loadCatalog(false)
    }
  }

  async function remove(key: string): Promise<CollectionRemoved> {
    busy.value = 'remove'
    try {
      return await invoke<CollectionRemoved>('plugin:script|collection_remove', { key })
    } finally {
      busy.value = null
      await loadCatalog(false)
    }
  }

  // ── Settings ──
  async function saveSource(input: CollectionSourceInput): Promise<CollectionSource> {
    const saved = await invoke<CollectionSource>('plugin:script|collection_source_save', { source: input })
    await loadSources()
    return saved
  }

  async function deleteSource(id: string) {
    await invoke('plugin:script|collection_source_delete', { id })
    if (sourceId.value === id) catalog.value = null
    await loadSources()
  }

  async function setToken(id: string, token: string) {
    const updated = await invoke<CollectionSource>('plugin:script|collection_token_set', { sourceId: id, token })
    sources.value = sources.value.map((s) => (s.id === updated.id ? updated : s))
  }

  async function clearToken(id: string) {
    const updated = await invoke<CollectionSource>('plugin:script|collection_token_clear', { sourceId: id })
    sources.value = sources.value.map((s) => (s.id === updated.id ? updated : s))
  }

  /** Test connection: resolve the branch and read the catalog. */
  async function testSource(id: string): Promise<{ ok: boolean; message: string }> {
    try {
      const doc = await invoke<CollectionCatalog>('plugin:script|collection_catalog', { sourceId: id, refresh: true })
      sources.value = sources.value.map((s) => (s.id === doc.source.id ? doc.source : s))
      if (id === sourceId.value) catalog.value = doc
      return { ok: true, message: `${doc.items.length} packages at ${doc.commit.startsWith('local-') ? doc.commit : doc.commit.slice(0, 7)}` }
    } catch (err) {
      return { ok: false, message: String(err) }
    }
  }

  return {
    sources,
    sourcesLoading,
    sourcesError,
    sourceId,
    source,
    catalog,
    catalogLoading,
    catalogError,
    items,
    visible,
    byKey,
    installedCount,
    updateCount,
    selectedKey,
    detail,
    detailLoading,
    detailError,
    filters,
    busy,
    loadSources,
    selectSource,
    loadCatalog,
    openItem,
    plan,
    install,
    remove,
    saveSource,
    deleteSource,
    setToken,
    clearToken,
    testSource,
  }
})
