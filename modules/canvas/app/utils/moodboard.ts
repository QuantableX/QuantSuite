/**
 * Moodboards (round 3, card 07): moving an image by drag, and which files a
 * drop or paste brings. The board itself lives in the crate
 * (crate/src/commands/moodboard.rs).
 */

/** `list` with the item at `from` moved to the gap `to` (0 = first … length = last). */
export function moveItem<T>(list: T[], from: number, to: number): T[] {
  if (from < 0 || from >= list.length) return list.slice()
  const out = list.slice()
  const [item] = out.splice(from, 1)
  const at = Math.max(0, Math.min(out.length, to > from ? to - 1 : to))
  out.splice(at, 0, item as T)
  return out
}

/** The gap a pointer at `x` over card `index` means: before it on its left half, after it on its right half. */
export function gapAt(index: number, rect: { left: number; width: number }, x: number): number {
  return x < rect.left + rect.width / 2 ? index : index + 1
}

const IMAGE_TYPES = ['image/png', 'image/jpeg', 'image/gif', 'image/webp']

/** The files of a drop or paste that can go on a board. */
export function imageFiles(files: ArrayLike<{ type: string; name: string }>): number[] {
  const out: number[] = []
  for (let i = 0; i < files.length; i++) if (IMAGE_TYPES.includes(files[i]!.type)) out.push(i)
  return out
}
