import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import type { SearchHit } from '#notes/types'

export const useSearchStore = defineStore('notes/search', () => {
  const query = ref('')
  const results = ref<SearchHit[]>([])
  const loading = ref(false)

  // Searches run per keystroke and the command is async, so responses are not
  // order-guaranteed — only the newest request may touch results/loading.
  let requestSeq = 0

  async function run(text: string) {
    query.value = text
    const seq = ++requestSeq
    if (!text.trim()) {
      results.value = []
      loading.value = false
      return
    }
    loading.value = true
    try {
      const hits = await invoke<SearchHit[]>('plugin:notes|search', { query: text })
      if (seq === requestSeq) results.value = hits
    } finally {
      if (seq === requestSeq) loading.value = false
    }
  }

  function reset() {
    requestSeq++
    query.value = ''
    results.value = []
    loading.value = false
  }

  return { query, results, loading, run, reset }
})
