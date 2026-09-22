import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { ref, computed } from 'vue'
import type { Strategy } from '#algo/types'

export const useStrategiesStore = defineStore('algo/strategies', () => {
  // ── State ──

  const strategies = ref<Strategy[]>([])
  const activeStrategyId = ref<string | null>(null)
  const editorContent = ref<string>('')
  const savedEditorContent = ref<string>('')
  const paramsContent = ref<string>('{}')
  const savedParamsContent = ref<string>('{}')
  const savedStrategyId = ref<string | null>(null)
  const isLoading = ref(false)

  // ── Getters ──

  const activeStrategy = computed(() => {
    if (!activeStrategyId.value) return null
    return strategies.value.find((s) => s.id === activeStrategyId.value) ?? null
  })

  const isDirty = computed(() =>
    !!activeStrategyId.value
    && savedStrategyId.value === activeStrategyId.value
    && (
      editorContent.value !== savedEditorContent.value
      || paramsContent.value !== savedParamsContent.value
    ),
  )

  // ── Actions ──

  async function load() {
    isLoading.value = true
    try {
      strategies.value = await invoke<Strategy[]>('plugin:algo|list_strategies')
    } catch (err) {
      console.error('[strategies store] Failed to load strategies:', err)
    } finally {
      isLoading.value = false
    }
  }

  async function create(name: string, description: string): Promise<Strategy> {
    try {
      const strategy = await invoke<Strategy>('plugin:algo|create_strategy', { name, description })
      await load()
      return strategy
    } catch (err) {
      console.error('[strategies store] Failed to create strategy:', err)
      throw err
    }
  }

  async function save(id: string, code: string, params?: string | null) {
    try {
      const normalizedParams = normalizeParams(params)
      await invoke('plugin:algo|save_strategy', {
        id,
        code,
        params: normalizedParams ? JSON.parse(normalizedParams) : null,
      })
      if (activeStrategyId.value === id) {
        savedStrategyId.value = id
        savedEditorContent.value = code
        paramsContent.value = normalizedParams ?? '{}'
        savedParamsContent.value = paramsContent.value
      }
      // Refresh the strategy list to pick up updated_at changes
      await load()
    } catch (err) {
      console.error('[strategies store] Failed to save strategy:', err)
      throw err
    }
  }

  async function updateMeta(id: string, payload: { name?: string; description?: string }) {
    try {
      await invoke('plugin:algo|update_strategy_meta', {
        id,
        name: payload.name ?? null,
        description: payload.description ?? null,
      })
      await load()
    } catch (err) {
      console.error('[strategies store] Failed to update strategy meta:', err)
      throw err
    }
  }

  async function remove(id: string) {
    try {
      await invoke('plugin:algo|delete_strategy', { id })
      if (activeStrategyId.value === id) {
        activeStrategyId.value = null
        editorContent.value = ''
        savedEditorContent.value = ''
        paramsContent.value = '{}'
        savedParamsContent.value = '{}'
        savedStrategyId.value = null
      }
      await load()
    } catch (err) {
      console.error('[strategies store] Failed to delete strategy:', err)
      throw err
    }
  }

  async function select(id: string) {
    activeStrategyId.value = id
    try {
      const content = await readFile(id)
      // Two quick clicks fire concurrent reads — drop a result that lost the
      // race, or its code would end up saved under the newer strategy's id.
      if (activeStrategyId.value !== id) return
      editorContent.value = content
      savedEditorContent.value = editorContent.value
      paramsContent.value = formatParams(strategies.value.find((s) => s.id === id)?.params_json)
      savedParamsContent.value = paramsContent.value
      savedStrategyId.value = id
    } catch (err) {
      console.error('[strategies store] Failed to read strategy file:', err)
      if (activeStrategyId.value !== id) return
      editorContent.value = ''
      savedEditorContent.value = ''
      paramsContent.value = formatParams(strategies.value.find((s) => s.id === id)?.params_json)
      savedParamsContent.value = paramsContent.value
      savedStrategyId.value = id
    }
  }

  function formatParams(paramsJson?: string | null): string {
    if (!paramsJson) return '{}'
    try {
      return JSON.stringify(JSON.parse(paramsJson), null, 2)
    } catch {
      return paramsJson
    }
  }

  function normalizeParams(params?: string | null): string | null {
    const trimmed = (params ?? '').trim()
    if (!trimmed) return null
    return JSON.stringify(JSON.parse(trimmed), null, 2)
  }

  async function readFile(id: string): Promise<string> {
    try {
      return await invoke<string>('plugin:algo|read_strategy_file', { id })
    } catch (err) {
      console.error('[strategies store] Failed to read strategy file:', err)
      throw err
    }
  }

  return {
    // State
    strategies,
    activeStrategyId,
    editorContent,
    savedEditorContent,
    paramsContent,
    savedParamsContent,
    savedStrategyId,
    isLoading,
    // Getters
    activeStrategy,
    isDirty,
    // Actions
    load,
    create,
    save,
    updateMeta,
    delete: remove,
    select,
    readFile,
  }
})
