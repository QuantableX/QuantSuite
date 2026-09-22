import { defineStore } from 'pinia'
import { useEngine } from '#systems/composables/useEngine'
import type { SystemMeta } from '#systems/types'

const fallbackSystems: SystemMeta[] = [
  {
    id: 'lces',
    name: 'Large-Cap Evaluation System',
    short: 'LCES',
    status: 'ready',
    description:
      'Survivorship-bias-free rotation across the top-N large-cap coins, ranked as they stood on each date.',
  },
  {
    id: 'sces',
    name: 'Small-Cap Evaluation System',
    short: 'SCES',
    status: 'ready',
    description:
      'Same engine, applied to a lower-rank small-cap cohort: excludes the top-ranked coins and rotates the slice beneath them.',
  },
]

export const useSystemsStore = defineStore('systems/systems', () => {
  const engine = useEngine()
  const systems = ref<SystemMeta[]>([...fallbackSystems])

  async function load() {
    try {
      const loaded = await engine.listSystems()
      if (loaded?.length) systems.value = loaded
    } catch {
      systems.value = [...fallbackSystems]
    }
  }

  function byId(id: string): SystemMeta | undefined {
    return systems.value.find(s => s.id === id)
  }

  const readySystems = computed(() => systems.value.filter(s => s.status === 'ready'))

  return { systems, readySystems, load, byId }
})
