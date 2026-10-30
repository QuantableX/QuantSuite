/**
 * The composer's drafts (Composer.vue reads them under the same key): a
 * board card or a GitHub item prefills a new thread's box before the box
 * mounts. Call `useDrafts()` in a component's setup.
 */
const KEY = 'pilot.drafts.v1'

export function useDrafts() {
  return useState<Record<string, string>>('pilot-drafts', () => {
    try {
      return JSON.parse(localStorage.getItem(KEY) ?? '{}') as Record<string, string>
    } catch {
      return {}
    }
  })
}

export function saveDrafts(drafts: Record<string, string>) {
  try {
    localStorage.setItem(KEY, JSON.stringify(drafts))
  } catch {
    /* the draft lives for this session */
  }
}
