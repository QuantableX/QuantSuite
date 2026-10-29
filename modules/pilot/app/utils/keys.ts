/** Key chords as text (`Ctrl+Shift+R`), for user-set shortcuts (card 13). */

const MODIFIERS = new Set(['Control', 'Shift', 'Alt', 'Meta', 'AltGraph'])

/** The chord of a keydown, or `null` while only modifiers are held. */
export function chordOf(e: KeyboardEvent): string | null {
  if (MODIFIERS.has(e.key)) return null
  const parts: string[] = []
  if (e.ctrlKey || e.metaKey) parts.push('Ctrl')
  if (e.altKey) parts.push('Alt')
  if (e.shiftKey) parts.push('Shift')
  const key = e.code.startsWith('Key') ? e.code.slice(3) : e.code.startsWith('Digit') ? e.code.slice(5) : e.key.length === 1 ? e.key.toUpperCase() : e.key
  parts.push(key)
  return parts.join('+')
}

/** A chord the app may give to an action: a modifier with a key, or a function key. */
export function usableChord(chord: string): boolean {
  const parts = chord.split('+')
  const key = parts.at(-1) ?? ''
  return /^F\d{1,2}$/.test(key) || (parts.length > 1 && (parts.includes('Ctrl') || parts.includes('Alt')))
}

export function normalizeChord(chord: string): string {
  const parts = chord
    .split('+')
    .map((p) => p.trim())
    .filter(Boolean)
  const mods = ['Ctrl', 'Alt', 'Shift'].filter((m) => parts.some((p) => p.toLowerCase() === m.toLowerCase()))
  const key = parts.filter((p) => !['ctrl', 'alt', 'shift', 'control', 'cmd', 'meta'].includes(p.toLowerCase())).at(-1) ?? ''
  return [...mods, key.length === 1 ? key.toUpperCase() : key].filter(Boolean).join('+')
}
