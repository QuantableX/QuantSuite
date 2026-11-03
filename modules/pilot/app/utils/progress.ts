/**
 * Visual progress (round 3, card 05): a thread's images as references first,
 * then turn by turn; two picks become before (the earlier) and after.
 */
import type { ProgressImage } from '#pilot/types'

export const SOURCE_LABEL: Record<string, string> = { you: 'you', browser: 'browser', page: 'page', tool: 'tool' }

/** "Turn 3 · browser" */
export function imageLabel(im: ProgressImage): string {
  return `${im.turn ? `Turn ${im.turn}` : 'Earlier'} · ${SOURCE_LABEL[im.source] ?? im.source}`
}

/** Pinned images in pin order, then the rest grouped by turn (in order). */
export function arrange(images: ProgressImage[], pins: string[]): { pinned: ProgressImage[]; turns: { turn: number; images: ProgressImage[] }[] } {
  const byKey = new Map(images.map((im) => [im.key, im]))
  const pinned = pins.map((k) => byKey.get(k)).filter((x): x is ProgressImage => !!x)
  const turns: { turn: number; images: ProgressImage[] }[] = []
  for (const im of images) {
    const last = turns[turns.length - 1]
    if (last && last.turn === im.turn) last.images.push(im)
    else turns.push({ turn: im.turn, images: [im] })
  }
  return { pinned, turns }
}

/** Two picked keys as before (earlier in the thread) and after. */
export function beforeAfter(images: ProgressImage[], picked: string[]): { before: ProgressImage; after: ProgressImage } | null {
  if (picked.length !== 2) return null
  const order = (k: string) => images.findIndex((im) => im.key === k)
  const [a, b] = [...picked].sort((x, y) => order(x) - order(y))
  const before = images.find((im) => im.key === a)
  const after = images.find((im) => im.key === b)
  return before && after ? { before, after } : null
}

/** Picking in compare mode: a third pick drops the oldest pick. */
export function togglePick(picked: string[], key: string): string[] {
  if (picked.includes(key)) return picked.filter((k) => k !== key)
  return [...picked, key].slice(-2)
}
