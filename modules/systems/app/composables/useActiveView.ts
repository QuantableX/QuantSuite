import type { EngineView } from '#systems/types'

const VIEWS: EngineView[] = ['settings', 'live', 'backtest']

/** Derive the active system id + view from the current route. */
export function useActiveView() {
  const route = useRoute()
  const systemId = computed(() => (route.params.id as string) || 'lces')
  const view = computed<EngineView>(() => {
    const last = route.path.split('/').filter(Boolean).pop() as EngineView
    return VIEWS.includes(last) ? last : 'live'
  })
  return { systemId, view }
}
