/**
 * Storage cleanup (parity 19) from the app's side: the paths the composer's
 * drafts still hold — kept in this webview's storage, so the backend cannot
 * see them — and the daily run when the user turned it on.
 */
import { pilot } from '#pilot/utils/invoke'
import type { StorageCleanResult, StoragePolicy } from '#pilot/types'

/** Draft attachments, pastes, chips, stashed and edited queue drafts. */
const DRAFT_KEYS = ['pilot.attach.v1', 'pilot.pasted.v1', 'pilot.chips.v1', 'pilot.stash.v1', 'pilot.queue-edits.v1']

export function draftPaths(): string[] {
  const out = new Set<string>()
  const walk = (v: unknown) => {
    if (Array.isArray(v)) v.forEach(walk)
    else if (v && typeof v === 'object') {
      for (const [k, x] of Object.entries(v as Record<string, unknown>)) {
        if (k === 'path' && typeof x === 'string' && x) out.add(x)
        else walk(x)
      }
    }
  }
  for (const key of DRAFT_KEYS) {
    try {
      walk(JSON.parse(localStorage.getItem(key) ?? 'null'))
    } catch {
      /* a damaged entry holds nothing */
    }
  }
  return [...out]
}

let ranToday = false
/** Once per app start at most; the backend skips it within 23 hours of the last run. */
export async function dailyStorageCleanup(policy: StoragePolicy | undefined): Promise<StorageCleanResult | null> {
  if (ranToday || !policy?.auto) return null
  ranToday = true
  // Not while the app starts up.
  await new Promise((r) => setTimeout(r, 60_000))
  return pilot<StorageCleanResult>('plugin:pilot|pilot_storage_clean', { policy, drafts: draftPaths(), shown: null, daily: true }).catch(() => null)
}
