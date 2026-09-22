import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { ref, computed } from 'vue'
import { bus } from '@quantsuite/core'
import type { IndicatorInfo, IndicatorRegistry, Strategy } from '#algo/types'

/**
 * The Indicator Smithery's registry, as the forge reports it
 * (`python -m smithery.registry --json`, PLAN-QUANTALGO §6): every regime
 * indicator with its hypothesis, parameters, warm-up and certification.
 */
export const useIndicatorsStore = defineStore('algo/indicators', () => {
  const registry = ref<IndicatorRegistry | null>(null)
  const isLoading = ref(false)
  const error = ref<string | null>(null)

  const indicators = computed<IndicatorInfo[]>(() => registry.value?.indicators ?? [])
  const certified = computed(() => indicators.value.filter((i) => i.certification?.certified))

  async function load(refresh = false) {
    isLoading.value = true
    error.value = null
    try {
      registry.value = await invoke<IndicatorRegistry>('plugin:algo|list_indicators', { refresh })
    } catch (err) {
      error.value = String(err)
      console.error('[indicators store] Failed to load the registry:', err)
    } finally {
      isLoading.value = false
    }
  }

  // A script saved or created in QuantScript changes the registry this
  // store mirrors (docs/PLAN-QUANTSCRIPT.md): re-read it, bypassing the
  // crate's cache, once the registry has been read at all.
  for (const topic of ['script.file.saved', 'script.file.created']) {
    bus.on(topic, () => {
      if (registry.value) void load(true)
    })
  }

  async function createStrategy(indicator: string, name?: string): Promise<Strategy> {
    try {
      return await invoke<Strategy>('plugin:algo|create_strategy_from_indicator', {
        indicator,
        name: name ?? null,
      })
    } catch (err) {
      console.error('[indicators store] Failed to create a strategy:', err)
      throw err
    }
  }

  return { registry, indicators, certified, isLoading, error, load, createStrategy }
})
