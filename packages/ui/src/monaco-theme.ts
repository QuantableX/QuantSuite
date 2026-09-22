/**
 * The suite's Monaco theme — one definition for every Monaco surface.
 *
 * Extracted from `QCodeEditor` on 2026-09-12 when QuantScript's version
 * compare needed a diff editor: the syntax palette and the chrome colours
 * are read off the host's CSS custom properties exactly as before, so an
 * editor and a diff editor side by side look like one component.
 *
 * Colours come from the host's `--qss-*` (or QuantCanvas' `--qc-*`) tokens,
 * read once per definition; Monaco takes hex only, so anything else falls
 * back to the known palette.
 */

export type MonacoTokens = 'qss' | 'qc'
export type MonacoTheme = 'dark' | 'light'

export const THEME_NAME = { dark: 'quantsuite-dark', light: 'quantsuite-light' } as const

/** Read a CSS custom property off the host, falling back to a known value. */
export function cssColor(host: HTMLElement | null, name: string, fallback: string): string {
  if (!host) return fallback
  const raw = getComputedStyle(host).getPropertyValue(name).trim()
  // Monaco accepts hex only. A token resolved to `color-mix(...)` or an empty
  // string (property not defined on this host) has to fall back.
  return /^#[0-9a-f]{3,8}$/i.test(raw) ? raw : fallback
}

/**
 * Syntax colours. These are QuantCanvas' palette, unchanged — they have been
 * in front of the user for months and are not a detail worth re-deciding here.
 */
const RULES_DARK = [
  { token: 'comment', foreground: 'a5a5a5', fontStyle: 'italic' },
  { token: 'keyword', foreground: '4472c4' },
  { token: 'keyword.operator', foreground: '4472c4' },
  { token: 'string', foreground: 'ed7d31' },
  { token: 'string.escape', foreground: 'f4b183' },
  { token: 'regexp', foreground: 'f4b183' },
  { token: 'number', foreground: 'ffd965' },
  { token: 'constant', foreground: 'ffd965' },
  { token: 'type', foreground: '8064a2' },
  { token: 'type.identifier', foreground: '70ad47' },
  { token: 'function', foreground: '5b9bd5' },
  { token: 'variable', foreground: 'ffffff' },
  { token: 'variable.parameter', foreground: 'b2a1c7' },
  { token: 'operator', foreground: '4472c4' },
  { token: 'delimiter', foreground: '4472c4' },
  { token: 'tag', foreground: '5b9bd5' },
  { token: 'attribute.name', foreground: '70ad47' },
  { token: 'attribute.value', foreground: 'ed7d31' },
  { token: 'metatag', foreground: 'a5a5a5' },
]

/** Two tokens need more contrast on a light ground; the rest carry over. */
const LIGHT_OVERRIDES: Record<string, string> = {
  variable: '000000',
  number: 'e0a903',
  constant: 'e0a903',
}
const RULES_LIGHT = RULES_DARK.map((r) =>
  LIGHT_OVERRIDES[r.token] ? { ...r, foreground: LIGHT_OVERRIDES[r.token]! } : r
)

/**
 * Define the theme for this host's tokens and make it the current one.
 * Returns the theme name, for editors created after the call.
 */
export function defineQuantsuiteTheme(
  monaco: any,
  host: HTMLElement | null,
  tokens: MonacoTokens,
  theme: MonacoTheme
): string {
  const p = tokens === 'qc' ? '--qc' : '--qss'
  const dark = theme === 'dark'

  const bg = cssColor(host, `${p}-bg`, dark ? '#212121' : '#d8d8de')
  const raised = cssColor(host, `${p}-bg-raised`, dark ? '#272727' : '#e0e0e5')
  const text = cssColor(host, `${p}-text`, dark ? '#e3e3e6' : '#24242c')
  const muted = cssColor(host, `${p}-text-muted`, dark ? '#67676f' : '#888892')
  const border = cssColor(host, `${p}-border`, dark ? '#333333' : '#babac2')

  monaco.editor.defineTheme(THEME_NAME[theme], {
    base: dark ? 'vs-dark' : 'vs',
    inherit: true,
    rules: dark ? RULES_DARK : RULES_LIGHT,
    colors: {
      'editor.background': bg,
      'editor.foreground': text,
      'editorGutter.background': bg,
      'editorLineNumber.foreground': muted,
      'editorLineNumber.activeForeground': text,
      'editor.lineHighlightBackground': raised,
      'editor.selectionBackground': dark ? '#4f4f4f' : '#a0a0a830',
      'editor.inactiveSelectionBackground': dark ? '#4f4f4f50' : '#a0a0a820',
      'editorCursor.foreground': '#f97316',
      'editorWidget.background': raised,
      'editorWidget.border': border,
      'editorSuggestWidget.background': raised,
      'editorSuggestWidget.border': border,
      'input.background': bg,
      'input.border': border,
      'input.foreground': text,
      'list.activeSelectionBackground': dark ? '#a0a0a830' : '#a0a0a840',
      'list.hoverBackground': raised,
      'scrollbarSlider.background': dark ? '#37373f80' : '#babac280',
      'scrollbarSlider.hoverBackground': dark ? '#4a4a5280' : '#a0a0a8a0',
      'editorOverviewRuler.border': border,
      // The diff editor's two grounds: a removed line and an inserted one,
      // faint enough to keep the syntax colours readable on top.
      'diffEditor.removedTextBackground': dark ? '#ff475722' : '#ff475726',
      'diffEditor.insertedTextBackground': dark ? '#2ed57322' : '#2ed57326',
      'diffEditor.removedLineBackground': dark ? '#ff475714' : '#ff475718',
      'diffEditor.insertedLineBackground': dark ? '#2ed57314' : '#2ed57318',
      'diffEditorGutter.removedLineBackground': dark ? '#ff475714' : '#ff475718',
      'diffEditorGutter.insertedLineBackground': dark ? '#2ed57314' : '#2ed57318',
    },
  })
  monaco.editor.setTheme(THEME_NAME[theme])
  return THEME_NAME[theme]
}

/** The editor font every Monaco surface in the suite uses. */
export const EDITOR_FONT = {
  fontSize: 13,
  fontFamily: '"CaskaydiaCove Nerd Font", "JetBrains Mono", "Fira Code", "Cascadia Code", monospace',
  fontLigatures: true,
} as const
