/**
 * File helpers for the inspector (cards 11, 13): the Monaco language of a
 * path, how the Files tab can render a file, CSV/TSV parsing, and
 * `path:line(:col)` references in agent messages.
 */

const LANG: Record<string, string> = {
  ts: 'typescript', tsx: 'typescript', mts: 'typescript', cts: 'typescript', js: 'javascript', mjs: 'javascript', cjs: 'javascript', jsx: 'javascript',
  vue: 'html', html: 'html', htm: 'html', css: 'css', scss: 'scss', less: 'less', json: 'json', jsonc: 'json', md: 'markdown', markdown: 'markdown',
  rs: 'rust', py: 'python', toml: 'ini', ini: 'ini', yml: 'yaml', yaml: 'yaml', sh: 'shell', bash: 'shell', ps1: 'powershell', psm1: 'powershell',
  sql: 'sql', go: 'go', java: 'java', kt: 'kotlin', cs: 'csharp', cpp: 'cpp', cc: 'cpp', c: 'c', h: 'cpp', hpp: 'cpp', xml: 'xml', svg: 'xml',
  rb: 'ruby', php: 'php', swift: 'swift', lua: 'lua', dart: 'dart', r: 'r', bat: 'bat', cmd: 'bat', dockerfile: 'dockerfile', graphql: 'graphql',
}

/** Extensions that make a bare name a file reference (with the languages above). */
const FILE_EXT = new Set(['txt', 'log', 'lock', 'env', 'csv', 'tsv', 'png', 'jpg', 'jpeg', 'gif', 'webp', 'ico', 'pdf', 'cfg', 'conf', 'mdx', 'snap', 'patch', 'diff'])

export function extOf(path: string): string {
  const name = (path.split(/[\\/]/).pop() ?? '').toLowerCase()
  if (name === 'dockerfile') return 'dockerfile'
  const i = name.lastIndexOf('.')
  return i > 0 ? name.slice(i + 1) : ''
}

export function languageOf(path: string): string {
  return Object.hasOwn(LANG, extOf(path)) ? LANG[extOf(path)] : 'plaintext'
}

/** What the Files tab can show besides the code view. */
export type Rendered = 'markdown' | 'html' | 'svg' | 'csv' | null

export function renderedOf(path: string): Rendered {
  const e = extOf(path)
  if (e === 'md' || e === 'markdown') return 'markdown'
  if (e === 'html' || e === 'htm') return 'html'
  if (e === 'svg') return 'svg'
  if (e === 'csv' || e === 'tsv') return 'csv'
  return null
}

/** CSV or TSV rows (quoted fields, doubled quotes, newlines inside quotes); at most `max` rows. */
export function parseDelimited(text: string, sep: ',' | '\t', max = 2000): { rows: string[][]; more: boolean } {
  const rows: string[][] = []
  let row: string[] = []
  let field = ''
  let quoted = false
  for (let i = 0; i < text.length; i++) {
    const c = text[i]
    if (quoted) {
      if (c === '"' && text[i + 1] === '"') {
        field += '"'
        i++
      } else if (c === '"') quoted = false
      else field += c
      continue
    }
    if (c === '"' && field === '') quoted = true
    else if (c === sep) {
      row.push(field)
      field = ''
    } else if (c === '\n' || c === '\r') {
      if (c === '\r' && text[i + 1] === '\n') i++
      row.push(field)
      rows.push(row)
      row = []
      field = ''
      if (rows.length > max) return { rows: rows.slice(0, max), more: true }
    } else field += c
  }
  if (field !== '' || row.length) {
    row.push(field)
    rows.push(row)
  }
  return { rows: rows.slice(0, max), more: rows.length > max }
}

/** `src/a.ts:12`, `src/a.ts:12:5`, `src/a.ts#L12` → path and line; `null` for plain words and URLs. */
export function parseFileRef(raw: string): { path: string; line: number | null; column: number | null } | null {
  const text = raw.trim().replace(/^[`'"(]+|[`'",.;)]+$/g, '')
  if (!text || /^[a-z][a-z0-9+.-]*:\/\//i.test(text) || text.length > 400) return null
  let path = text
  let line: number | null = null
  let column: number | null = null
  const hash = /#L(\d+)(?:-L?\d+)?$/.exec(path)
  if (hash) {
    line = Number(hash[1])
    path = path.slice(0, hash.index)
  } else {
    const m = /:(\d+)(?::(\d+))?$/.exec(path)
    if (m) {
      line = Number(m[1])
      column = m[2] ? Number(m[2]) : null
      path = path.slice(0, m.index)
    }
  }
  // A file has a known extension, or a folder in its path; `store.load` is code, not a file.
  const e = extOf(path)
  const known = Object.hasOwn(LANG, e) || FILE_EXT.has(e)
  if (!known && !/[\\/]/.test(path)) return null
  if (/[()=<>{}]/.test(path)) return null
  if (/\s/.test(path) && !/^[a-z]:\\/i.test(path)) return null
  return { path, line, column }
}
