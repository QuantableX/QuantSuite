/**
 * The Diff tab's all-files view (card 24; T3 Code's diff panel), free of
 * Vue for node tests: a unified patch as numbered lines, and find across
 * every file — folded ones and ones not on screen yet included.
 */

export interface PatchLine {
  kind: 'hunk' | 'add' | 'del' | 'ctx' | 'meta'
  text: string
  /** Line number on the old side, the new side. */
  old: number | null
  new: number | null
}

/** `git diff` output as lines to show: the file header goes, hunks are numbered. */
export function parsePatch(patch: string): PatchLine[] {
  const out: PatchLine[] = []
  let oldNo = 0
  let newNo = 0
  let inHunk = false
  for (const raw of patch.replace(/\r\n/g, '\n').split('\n')) {
    const hunk = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@(.*)$/.exec(raw)
    if (hunk) {
      oldNo = Number(hunk[1])
      newNo = Number(hunk[2])
      inHunk = true
      out.push({ kind: 'hunk', text: raw, old: null, new: null })
      continue
    }
    if (!inHunk) {
      // diff --git, index, ---, +++, mode lines; "Binary files … differ" stays as a note.
      if (/^Binary files /.test(raw) || raw.startsWith('… (cut at')) out.push({ kind: 'meta', text: raw, old: null, new: null })
      continue
    }
    if (raw.startsWith('+')) out.push({ kind: 'add', text: raw.slice(1), old: null, new: newNo++ })
    else if (raw.startsWith('-')) out.push({ kind: 'del', text: raw.slice(1), old: oldNo++, new: null })
    else if (raw.startsWith(' ')) out.push({ kind: 'ctx', text: raw.slice(1), old: oldNo++, new: newNo++ })
    else if (raw.startsWith('\\')) out.push({ kind: 'meta', text: raw, old: null, new: null })
    else if (raw.startsWith('… (cut at')) out.push({ kind: 'meta', text: raw, old: null, new: null })
  }
  return out
}

export interface Match {
  file: number
  line: number
  start: number
  end: number
}

/** Every occurrence of `query` in the files' lines, in order; hunk headers are not searched. */
export function findMatches(files: PatchLine[][], query: string, caseSensitive = false): Match[] {
  if (!query) return []
  const q = caseSensitive ? query : query.toLowerCase()
  const out: Match[] = []
  files.forEach((lines, file) => {
    lines.forEach((l, line) => {
      if (l.kind === 'hunk') return
      const text = caseSensitive ? l.text : l.text.toLowerCase()
      let at = text.indexOf(q)
      while (at >= 0) {
        out.push({ file, line, start: at, end: at + q.length })
        at = text.indexOf(q, at + Math.max(1, q.length))
      }
    })
  })
  return out
}

/** A line cut at its matches: `mark` segments are found text, `current` the active one. */
export function markSegments(text: string, ranges: { start: number; end: number; current: boolean }[]): { text: string; mark: boolean; current: boolean }[] {
  const out: { text: string; mark: boolean; current: boolean }[] = []
  let at = 0
  for (const r of [...ranges].sort((a, b) => a.start - b.start)) {
    if (r.start > at) out.push({ text: text.slice(at, r.start), mark: false, current: false })
    out.push({ text: text.slice(r.start, r.end), mark: true, current: r.current })
    at = r.end
  }
  if (at < text.length || !out.length) out.push({ text: text.slice(at), mark: false, current: false })
  return out
}

/** The next match index after `from` in a direction, wrapping. */
export function stepMatch(count: number, from: number, dir: 1 | -1): number {
  if (!count) return -1
  return (((from + dir) % count) + count) % count
}
