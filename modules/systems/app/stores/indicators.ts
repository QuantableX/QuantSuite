import { defineStore } from 'pinia'
import { loadSmitheryCatalog, type SmitheryCatalogEntry } from '@quantsuite/core'

export const useSystemsIndicatorsStore = defineStore('systems/indicators', () => {
  const indicators = ref<SmitheryCatalogEntry[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)

  async function load() {
    if (loading.value) return
    loading.value = true
    error.value = null
    try {
      indicators.value = (await loadSmitheryCatalog(true)).indicators
    } catch (err) {
      error.value = String(err)
    } finally {
      loading.value = false
    }
  }

  return { indicators, loading, error, load }
})
