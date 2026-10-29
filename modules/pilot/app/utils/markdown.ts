/**
 * Markdown for the conversation: agent replies and user messages
 * (docs/PLAN-QUANTPILOT.md, card 3).
 *
 * - `marked` with GFM (tables, task lists, strikethrough).
 * - Code blocks get a header with the language and a copy button; once a
 *   message stops streaming, `highlightCode` colours them with Monaco's
 *   tokenizer (`colorize`) — the suite's one highlighter, loaded lazily.
 * - Math: `$…$` inline and `$$…$$` blocks through KaTeX, outside code.
 *   Dollar amounts stay text (`$5 and $10`): an inline formula cannot start
 *   or end with a space or be followed by a digit.
 * - GitHub alerts: `> [!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]`, `[!CAUTION]`.
 * - Mermaid blocks render as code (mermaid is not installed in the suite).
 * - The result always goes through the suite's sanitiser.
 */
import { Marked, type Tokens } from 'marked'
import { sanitizeHtml } from '@quantsuite/ui'
import { parseFileRef } from './files'

let katex: { renderToString: (tex: string, opts: Record<string, unknown>) => string } | null = null
let katexLoading: Promise<void> | null = null

/** Load KaTeX (and its stylesheet) once; resolves when formulas can render. */
export function ensureKatex(): Promise<void> {
  if (katex) return Promise.resolve()
  if (!katexLoading) {
    katexLoading = Promise.all([import('katex'), import('katex/dist/katex.min.css')])
      .then(([mod]) => {
        katex = (mod as any).default ?? mod
      })
      .catch(() => {
        katexLoading = null
      })
  }
  return katexLoading ?? Promise.resolve()
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')
}

function renderTex(tex: string, display: boolean): string {
  if (!katex) return display ? `<pre class="qp-md-math-src">${escapeHtml(tex)}</pre>` : `<code>${escapeHtml(tex)}</code>`
  try {
    return katex.renderToString(tex, { displayMode: display, throwOnError: false, output: 'html' })
  } catch {
    return `<code>${escapeHtml(tex)}</code>`
  }
}

/** Whether a text needs KaTeX at all — cheap enough to run on every render. */
export function hasMath(src: string): boolean {
  return /\$\$[\s\S]+?\$\$/.test(src) || /(^|[^\\$])\$(?!\s)[^$\n]+?(?<!\s)\$(?!\d)/.test(src)
}

const LANG_LABEL: Record<string, string> = {
  ts: 'TypeScript',
  typescript: 'TypeScript',
  js: 'JavaScript',
  javascript: 'JavaScript',
  tsx: 'TSX',
  jsx: 'JSX',
  py: 'Python',
  python: 'Python',
  rs: 'Rust',
  rust: 'Rust',
  sh: 'Shell',
  bash: 'Bash',
  shell: 'Shell',
  zsh: 'Shell',
  ps1: 'PowerShell',
  powershell: 'PowerShell',
  pwsh: 'PowerShell',
  json: 'JSON',
  yaml: 'YAML',
  yml: 'YAML',
  toml: 'TOML',
  html: 'HTML',
  vue: 'Vue',
  css: 'CSS',
  scss: 'SCSS',
  md: 'Markdown',
  markdown: 'Markdown',
  sql: 'SQL',
  go: 'Go',
  java: 'Java',
  c: 'C',
  cpp: 'C++',
  cs: 'C#',
  csharp: 'C#',
  diff: 'Diff',
  mermaid: 'Mermaid',
  text: 'Text',
  txt: 'Text',
}

/** Monaco language ids for the fence names agents use. */
export const MONACO_LANG: Record<string, string> = {
  ts: 'typescript',
  typescript: 'typescript',
  tsx: 'typescript',
  js: 'javascript',
  javascript: 'javascript',
  jsx: 'javascript',
  py: 'python',
  python: 'python',
  rs: 'rust',
  rust: 'rust',
  sh: 'shell',
  bash: 'shell',
  shell: 'shell',
  zsh: 'shell',
  ps1: 'powershell',
  powershell: 'powershell',
  pwsh: 'powershell',
  json: 'json',
  yaml: 'yaml',
  yml: 'yaml',
  html: 'html',
  vue: 'html',
  css: 'css',
  scss: 'scss',
  md: 'markdown',
  markdown: 'markdown',
  sql: 'sql',
  go: 'go',
  java: 'java',
  c: 'cpp',
  cpp: 'cpp',
  cs: 'csharp',
  csharp: 'csharp',
}

const ALERTS: Record<string, string> = {
  NOTE: 'Note',
  TIP: 'Tip',
  IMPORTANT: 'Important',
  WARNING: 'Warning',
  CAUTION: 'Caution',
}

const marked = new Marked({ gfm: true, breaks: false })

marked.use({
  extensions: [
    {
      name: 'mathBlock',
      level: 'block',
      start(src: string) {
        const i = src.indexOf('$$')
        return i < 0 ? undefined : i
      },
      tokenizer(src: string) {
        const m = /^\$\$([\s\S]+?)\$\$(?:\n|$)/.exec(src)
        if (m) return { type: 'mathBlock', raw: m[0], text: m[1].trim() }
        return undefined
      },
      renderer(token: Tokens.Generic) {
        return `<div class="qp-md-math">${renderTex(token.text as string, true)}</div>`
      },
    },
    {
      name: 'mathInline',
      level: 'inline',
      start(src: string) {
        const i = src.indexOf('$')
        return i < 0 ? undefined : i
      },
      tokenizer(src: string) {
        const m = /^\$(?!\s)([^$\n]+?)(?<!\s)\$(?!\d)/.exec(src)
        if (m) return { type: 'mathInline', raw: m[0], text: m[1] }
        return undefined
      },
      renderer(token: Tokens.Generic) {
        return renderTex(token.text as string, false)
      },
    },
  ],
  renderer: {
    code(token: Tokens.Code) {
      const lang = (token.lang ?? '').trim().split(/\s+/)[0].toLowerCase()
      const label = LANG_LABEL[lang] ?? (lang || 'Code')
      const code = escapeHtml(token.text)
      return (
        `<div class="qp-md-code" data-lang="${escapeHtml(lang)}">` +
        `<div class="qp-md-code-head"><span class="qp-md-code-lang">${escapeHtml(label)}</span>` +
        `<button type="button" class="qp-md-copy" data-copy>Copy</button></div>` +
        `<pre><code data-lang="${escapeHtml(lang)}">${code}</code></pre></div>`
      )
    },
    /** `src/a.ts:12` in backticks opens in the Files tab (card 13). */
    codespan(token: Tokens.Codespan) {
      const ref = parseFileRef(token.text)
      const text = escapeHtml(token.text)
      if (!ref) return `<code>${text}</code>`
      return `<code class="qp-md-file" data-file="${escapeHtml(ref.path)}" data-line="${ref.line ?? ''}" title="Open in the Files tab">${text}</code>`
    },
    link(token: Tokens.Link) {
      const text = this.parser.parseInline(token.tokens)
      const href = escapeHtml(token.href ?? '')
      const title = token.title ? ` title="${escapeHtml(token.title)}"` : ''
      return `<a href="${href}"${title} data-link>${text}</a>`
    },
  },
})

/** Turn `> [!NOTE]` blockquotes into alert boxes, after parsing. */
function alerts(html: string): string {
  return html.replace(/<blockquote>\s*<p>\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]\s*(?:<br>\s*)?/g, (_m, kind: string) => {
    return `<blockquote class="qp-md-alert is-${kind.toLowerCase()}"><p class="qp-md-alert-title">${ALERTS[kind]}</p><p>`
  })
}

export function renderMarkdown(src: string): string {
  const html = marked.parse(src, { async: false }) as string
  return sanitizeHtml(alerts(html))
}

let monaco: any = null
let monacoLoading: Promise<any> | null = null

async function loadMonaco(host: HTMLElement) {
  if (monaco) return monaco
  if (!monacoLoading) {
    monacoLoading = (async () => {
      const ui = await import('@quantsuite/ui')
      ui.installMonacoEnvironment()
      const m = await import('monaco-editor')
      ui.defineQuantsuiteTheme(m, host, 'qss', 'dark')
      monaco = m
      return m
    })().catch(() => {
      monacoLoading = null
      return null
    })
  }
  return monacoLoading
}

/** Colour every not-yet-highlighted code block inside `root`. */
export async function highlightCode(root: HTMLElement) {
  const blocks = [...root.querySelectorAll<HTMLElement>('pre > code[data-lang]:not([data-hl])')].filter(
    (el) => MONACO_LANG[el.dataset.lang ?? ''] && (el.textContent?.length ?? 0) < 60_000
  )
  if (!blocks.length) return
  const m = await loadMonaco(root)
  if (!m) return
  for (const el of blocks) {
    if (!el.isConnected || el.dataset.hl) continue
    const lang = MONACO_LANG[el.dataset.lang ?? '']
    try {
      const html: string = await m.editor.colorize(el.textContent ?? '', lang, { tabSize: 2 })
      if (!el.isConnected) continue
      el.innerHTML = html
      el.dataset.hl = '1'
    } catch {
      el.dataset.hl = '0'
    }
  }
}
