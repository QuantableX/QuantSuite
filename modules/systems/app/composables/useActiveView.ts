import type { EngineView } from '#systems/types'

const VIEWS: EngineView[] = ['settings', 'live', 'backtest']

/** Derive the active system id + view from the current route. */
export function useActiveView() {
  // Layout guards need the destination immediately; Nuxt's useRoute waits
  // for the destination page to render, which a loading guard can prevent.
  const router = useRouter()
  const systemId = computed(() => (router.currentRoute.value.params.id as string) || '')
  const view = computed<EngineView>(() => {
    const last = router.currentRoute.value.path.split('/').filter(Boolean).pop() as EngineView
    return VIEWS.includes(last) ? last : 'live'
  })
  return { systemId, view }
}
