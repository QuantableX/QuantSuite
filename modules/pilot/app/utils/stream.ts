/**
 * Where a streaming message's unfinished tail starts (parity 21): after its
 * last blank line outside a code fence. Everything before is finished blocks.
 */
export function streamBoundary(src: string): number {
  let fence = ''
  let boundary = 0
  let pos = 0
  const lines = src.split('\n')
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!
    pos += line.length + 1
    const m = /^ {0,3}(`{3,}|~{3,})/.exec(line)
    if (m) {
      if (!fence) fence = m[1]!
      else if (m[1]![0] === fence[0] && m[1]!.length >= fence.length && !line.trim().slice(m[1]!.length).trim()) fence = ''
      continue
    }
    if (!fence && !line.trim() && i < lines.length - 1) boundary = pos
  }
  return Math.min(boundary, src.length)
}
