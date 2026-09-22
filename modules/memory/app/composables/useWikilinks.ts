/**
 * Wikilinks on the frontend: rendering them clickable in the preview, and
 * completing them in Monaco. The parsing mirrors the crate's rules
 * (modules/memory/crate/src/vault.rs): fenced code and inline code do not
 * link, `#heading` and `|alias` are understood.
 */
import type { useVaultStore } from '#memory/stores/vault'

const LINK_RE = /\[\[([^\[\]\n]+)\]\]/g

function splitLink(inner: string): { target: string; label: string } | null {
  const [targetPart, alias] = inner.split('|', 2) as [string, string | undefined]
  const target = (targetPart ?? '').split('#', 1)[0]!.trim()
  if (!target) return null
  return { target, label: (alias ?? targetPart ?? '').trim() || target }
}

/**
 * `[[Target|alias]]` → a markdown link whose href the page can intercept:
 * `#qm:<target>` when a memory answers to the target, `#qm-new:<target>`
 * when none does (click offers to create it — Obsidian's broken-link flow).
 */
export function renderWikilinks(body: string, resolves: (target: string) => boolean): string {
  let inFence = false
  return body
    .split('\n')
    .map((line) => {
      const trimmed = line.trimStart()
      if (trimmed.startsWith('```') || trimmed.startsWith('~~~')) {
        inFence = !inFence
        return line
      }
      if (inFence) return line
      // Inline code segments (odd indices after splitting on backticks) pass
      // through untouched.
      return line
        .split('`')
        .map((segment, i) => {
          if (i % 2 === 1) return segment
          return segment.replace(LINK_RE, (raw, inner: string) => {
            const link = splitLink(inner)
            if (!link) return raw
            const scheme = resolves(link.target) ? 'qm' : 'qm-new'
            return `[${link.label}](#${scheme}:${encodeURIComponent(link.target)})`
          })
        })
        .join('`')
    })
    .join('\n')
}

/** The `#qm:`/`#qm-new:` target of a click inside the preview, if any. */
export function wikilinkFromClick(event: MouseEvent): { target: string; missing: boolean } | null {
  const anchor = (event.target as HTMLElement | null)?.closest('a')
  const href = anchor?.getAttribute('href') ?? ''
  if (href.startsWith('#qm:')) {
    return { target: decodeURIComponent(href.slice(4)), missing: false }
  }
  if (href.startsWith('#qm-new:')) {
    return { target: decodeURIComponent(href.slice(8)), missing: true }
  }
  return null
}

let completionRegistered = false

/**
 * Register the `[[` completion once, process-wide — Monaco is a singleton, and
 * so is this provider. Titles are read through the store at completion time,
 * so the list is always current.
 */
export async function ensureWikilinkCompletion(vault: ReturnType<typeof useVaultStore>) {
  if (completionRegistered) return
  completionRegistered = true
  const monaco = await import('monaco-editor')
  monaco.languages.registerCompletionItemProvider('markdown', {
    triggerCharacters: ['['],
    provideCompletionItems(model, position) {
      const line = model.getLineContent(position.lineNumber).slice(0, position.column - 1)
      const open = line.lastIndexOf('[[')
      if (open === -1 || line.slice(open + 2).includes(']]')) return { suggestions: [] }
      const typed = line.slice(open + 2).toLowerCase()
      const range = new monaco.Range(
        position.lineNumber,
        open + 3, // 1-based, right after the "[["
        position.lineNumber,
        position.column
      )
      const suggestions = vault.titles
        .filter((title) => title.toLowerCase().includes(typed))
        .slice(0, 50)
        .map((title) => ({
          label: title,
          kind: monaco.languages.CompletionItemKind.Reference,
          insertText: `${title}]]`,
          range,
        }))
      return { suggestions }
    },
  })
}
