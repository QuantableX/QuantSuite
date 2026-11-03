/**
 * Agent logos (round 2, card 01): which bundled mark identifies an agent.
 * The files are in `assets/agents/` (sources and terms in its NOTICE.md);
 * `AgentIcon.vue` turns file names into URLs. Pure, so the node tests can
 * import it.
 */

export interface AgentMark {
  file: string
  /** `mono` marks take the text color, so they read in both themes; `color` marks keep the brand colors (and their own tile where they have one). */
  tone: 'mono' | 'color'
}

/** Every agent QuantPilot knows or adds (cards 11 and 12), by adapter id. */
export const AGENT_MARKS: Record<string, AgentMark> = {
  claude: { file: 'claude.svg', tone: 'color' },
  codex: { file: 'openai.svg', tone: 'mono' },
  gemini: { file: 'gemini.svg', tone: 'color' },
  opencode: { file: 'opencode.svg', tone: 'mono' },
  copilot: { file: 'copilot.svg', tone: 'mono' },
  pi: { file: 'pi.svg', tone: 'mono' },
  omp: { file: 'omp.svg', tone: 'color' },
  cursor: { file: 'cursor.png', tone: 'color' },
  kilo: { file: 'kilo.svg', tone: 'color' },
  hermes: { file: 'hermes.png', tone: 'color' },
  openclaw: { file: 'openclaw.png', tone: 'color' },
  freebuff: { file: 'freebuff.png', tone: 'color' },
  grok: { file: 'grok.png', tone: 'color' },
  antigravity: { file: 'antigravity.png', tone: 'color' },
  kiro: { file: 'kiro.png', tone: 'color' },
  droid: { file: 'droid.svg', tone: 'color' },
  cline: { file: 'cline.png', tone: 'color' },
  devin: { file: 'devin.png', tone: 'color' },
}

/** Other names the same agents go by (their executables, older ids). */
const ALIASES: Record<string, string> = {
  'claude-code': 'claude',
  openai: 'codex',
  'gemini-cli': 'gemini',
  'github-copilot': 'copilot',
  'copilot-cli': 'copilot',
  'cursor-agent': 'cursor',
  kilocode: 'kilo',
  'kilo-code': 'kilo',
  factory: 'droid',
  'kiro-cli': 'kiro',
  agy: 'antigravity',
}

/** The bundled mark of an adapter id, or null (custom, unknown, not loaded). */
export function agentMark(id: string | null | undefined): AgentMark | null {
  const key = (id ?? '').trim().toLowerCase()
  if (!key) return null
  return AGENT_MARKS[key] ?? AGENT_MARKS[ALIASES[key] ?? ''] ?? null
}

/** A letter for an agent without a mark: the first letter or digit of its name. */
export function agentInitial(label: string | null | undefined): string {
  const ch = [...(label ?? '').trim()].find((c) => /[\p{L}\p{N}]/u.test(c))
  return ch ? ch.toLocaleUpperCase('en-US') : '?'
}

export type AgentGlyph =
  | { kind: 'mark'; mark: AgentMark }
  | { kind: 'image'; src: string }
  | { kind: 'letter'; letter: string }
  | { kind: 'terminal' }
  | { kind: 'unknown' }

/**
 * What identifies an agent. A custom agent shows its own image, else its
 * letter; a built-in its mark. An id that is still unknown (status not
 * loaded, nothing reported yet) shows a neutral glyph — never another
 * agent's logo. `''` is a V2 terminal row without an agent.
 */
export function agentGlyph(
  id: string | null | undefined,
  opts: { label?: string | null; image?: string | null; known?: boolean } = {},
): AgentGlyph {
  if (id === '') return { kind: 'terminal' }
  if (id == null) return { kind: 'unknown' }
  if (opts.image) return { kind: 'image', src: opts.image }
  const mark = agentMark(id)
  if (mark) return { kind: 'mark', mark }
  if (opts.known && opts.label) return { kind: 'letter', letter: agentInitial(opts.label) }
  return { kind: 'unknown' }
}

/** The id the settings will store for a typed custom id (settings.rs `slug`). */
export function agentSlug(id: string): string {
  return id
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]/gu, '-')
    .replace(/^-+|-+$/g, '')
}
