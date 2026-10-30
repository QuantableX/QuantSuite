/**
 * Keybindings and the command palette (card 17). The user's rules live in
 * `keybindings.json` in the pilot data dir; invalid ones are skipped and
 * named in `problems`. The compiled table (defaults, then the user's rules)
 * is what the layout's dispatcher, the palette and the tooltips read. The
 * palette's frecency and picks are a per-device preference.
 */
import { defineStore } from 'pinia'
import { pilot } from '#pilot/utils/invoke'
import { compile, keysOf, parseUserRules, type KeyRule } from '#pilot/utils/keybindings'
import { normalizeQuery, PICK_BOOST_MS, type Frecency, type Picks } from '#pilot/utils/palette'

function load<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key)
    return raw === null ? fallback : (JSON.parse(raw) as T)
  } catch {
    return fallback
  }
}
function save(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value))
  } catch {
    /* storage unavailable: it lives for this session */
  }
}

export const useKeysStore = defineStore('pilot/keys', {
  state: () => ({
    user: [] as KeyRule[],
    /** Rules from the file that were skipped, and why; saving keeps them as they were. */
    problems: [] as string[],
    skipped: [] as unknown[],
    loaded: false,
    /** Settings → Keybindings is recording a shortcut: no command runs. */
    recording: false,
    paletteOpen: false,
    /** What the palette opens with (`>` for actions only). */
    paletteSeed: '',
    frecency: load<Frecency>('pilot.palette.frecency', {}),
    picks: load<Picks>('pilot.palette.picks', {}),
  }),
  getters: {
    rules: (s) => compile(s.user),
  },
  actions: {
    async load() {
      try {
        const text = await pilot<string | null>('plugin:pilot|pilot_keybindings_get')
        const { rules, problems, skipped } = parseUserRules(text ?? null)
        this.user = rules
        this.problems = problems
        this.skipped = skipped
      } catch (e) {
        this.problems = [`keybindings.json could not be read — the defaults apply (${e})`]
      }
      this.loaded = true
    },
    async save(user: KeyRule[]) {
      await pilot('plugin:pilot|pilot_keybindings_set', { rules: [...user, ...this.skipped].slice(0, 256) })
      this.user = user
    },
    /** The first key of a command, for tooltips and the palette ("" when it has none). */
    keyOf(command: string): string {
      return keysOf(this.rules, command)[0] ?? ''
    },
    /** A tooltip with the command's key now: "Model (Ctrl+Shift+M)". */
    hint(command: string, label: string): string {
      const key = this.keyOf(command)
      return key ? `${label} (${key})` : label
    },
    openPalette(seed = '') {
      this.paletteSeed = seed
      this.paletteOpen = true
    },
    closePalette() {
      this.paletteOpen = false
    },
    /** A thread was opened (from anywhere): it ranks higher in the palette. */
    noteOpen(id: string) {
      const f = this.frecency[id]
      const next: Frecency = { ...this.frecency, [id]: { n: Math.min((f?.n ?? 0) + 1, 1000), at: Date.now() } }
      // The 200 most recent are enough to rank by.
      const ids = Object.keys(next).sort((a, b) => next[b].at - next[a].at)
      this.frecency = Object.fromEntries(ids.slice(0, 200).map((k) => [k, next[k]]))
      save('pilot.palette.frecency', this.frecency)
    },
    /** The palette's pick for a query, boosted for three weeks. */
    notePick(query: string, id: string) {
      const q = normalizeQuery(query)
      if (!q) return
      const now = Date.now()
      const kept = Object.entries(this.picks).filter(([, p]) => now - p.at < PICK_BOOST_MS)
      this.picks = Object.fromEntries([...kept.slice(-199), [q, { id, at: now }]])
      save('pilot.palette.picks', this.picks)
    },
  },
})
