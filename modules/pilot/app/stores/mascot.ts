/**
 * The mascot's look (round 2, card 05): Settings → Appearance → Mascot,
 * kept on this machine. Faces also hear small events (an approval granted
 * nods) through `poke`.
 */
import { defineStore } from 'pinia'
import { normalizePrefs, type MascotPrefs } from '#pilot/utils/mascot'

const KEY = 'pilot.mascot.v1'

function load(): MascotPrefs {
  try {
    return normalizePrefs(JSON.parse(localStorage.getItem(KEY) ?? '{}'))
  } catch {
    return normalizePrefs({})
  }
}

export const useMascotStore = defineStore('pilot/mascot', {
  state: () => ({
    prefs: load(),
    /** Per thread: bumped when an approval there was granted (the face nods). */
    nods: {} as Record<string, number>,
  }),
  actions: {
    set(patch: Partial<MascotPrefs>) {
      this.prefs = normalizePrefs({ ...this.prefs, ...patch })
      try {
        localStorage.setItem(KEY, JSON.stringify(this.prefs))
      } catch {
        /* this session only */
      }
    },
    nod(threadId: string) {
      this.nods[threadId] = (this.nods[threadId] ?? 0) + 1
    },
  },
})
