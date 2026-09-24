import { defineStore } from 'pinia'
import { useEngine } from '#systems/composables/useEngine'
import type { RunConfig, SystemMeta } from '#systems/types'

export const useSystemsStore = defineStore('systems/systems', () => {
  const engine = useEngine()
  const systems = ref<SystemMeta[]>([])
  const loaded = ref(false)
  const error = ref<string | null>(null)

  async function load() {
    error.value = null
    try {
      systems.value = await engine.listSystems()
      loaded.value = true
    } catch (e) {
      error.value = String(e)
    }
  }

  function byId(id: string): SystemMeta | undefined {
    return systems.value.find(s => s.id === id)
  }

  const readySystems = computed(() => systems.value.filter(s => s.status === 'ready'))

  async function create(name: string, short: string, description: string, config?: RunConfig) {
    const system = await engine.createSystem(name, short, description, config)
    systems.value.push(system)
    return system
  }

  function replace(system: SystemMeta) {
    systems.value = systems.value.map(s => s.id === system.id ? system : s)
  }

  async function remove(id: string) {
    await engine.deleteSystem(id)
    systems.value = systems.value.filter(s => s.id !== id)
  }

  return { systems, readySystems, loaded, error, load, byId, create, replace, remove }
})
