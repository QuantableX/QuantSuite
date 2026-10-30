/**
 * QuantPilot's keybindings (card 17; docs/PLAN-QUANTPILOT.md §7), after T3
 * Code's: every shortcut is a rule `{ key, command, when? }`. The defaults
 * come first, the user's rules (keybindings.json in the pilot data dir) after
 * them; the last rule whose key and condition match wins, whatever its
 * command. A rule whose command starts with `-` removes the earlier rules of
 * that command on that key. Pure functions: the layout's dispatcher, the
 * palette and Settings → Keybindings use them.
 */

export interface KeyRule {
  key: string
  command: string
  when?: string
}

export interface CommandInfo {
  id: string
  title: string
  /** Settings groups and palette sections. */
  group: 'General' | 'Threads' | 'Panels' | 'Composer'
  /** Offered in the command palette (the jumps and focus moves are keys only). */
  palette: boolean
}

const C = (id: string, title: string, group: CommandInfo['group'], palette = true): CommandInfo => ({ id, title, group, palette })

/** Every command a key can run. */
export const COMMANDS: CommandInfo[] = [
  C('commandPalette.toggle', 'Command palette', 'General', false),
  C('navigation.back', 'Go back', 'General'),
  C('navigation.forward', 'Go forward', 'General'),
  C('workspace.focusNext', 'Focus the next area', 'General'),
  C('workspace.focusPrevious', 'Focus the previous area', 'General'),
  C('settings.open', 'Open settings', 'General'),
  C('chat.new', 'New thread', 'Threads'),
  C('chat.newWithoutProject', 'New thread without a project', 'Threads'),
  C('chat.find', 'Find in thread', 'Threads'),
  C('thread.previous', 'Previous thread', 'Threads'),
  C('thread.next', 'Next thread', 'Threads'),
  ...Array.from({ length: 9 }, (_, i) => C(`thread.jump.${i + 1}`, `Open thread ${i + 1}`, 'Threads', false)),
  C('thread.stop', 'Stop the agent', 'Threads'),
  C('thread.settle', 'Settle or unsettle the thread', 'Threads'),
  C('thread.pin', 'Pin or unpin the thread', 'Threads'),
  C('thread.copyReference', 'Copy a reference to the thread', 'Threads'),
  C('thread.rename', 'Rename the thread', 'Threads'),
  C('thread.undo', 'Undo the last sidebar action', 'Threads'),
  C('thread.restart', 'Restart the agent session', 'Threads'),
  C('thread.openTui', "Open in the agent's terminal UI", 'Threads'),
  C('sidebar.toggle', 'Toggle the threads sidebar', 'Panels'),
  C('threadPanel.toggle', 'Toggle the thread panel', 'Panels'),
  C('rightPanel.toggle', 'Toggle the inspector', 'Panels'),
  C('rightPanel.close', 'Close the inspector tab', 'Panels'),
  C('view.reopenClosed', 'Reopen the closed tab', 'Panels'),
  C('diff.toggle', 'Diff', 'Panels'),
  C('filePicker.toggle', 'Go to file', 'Panels'),
  C('projectSearch.toggle', 'Search in the project', 'Panels'),
  C('preview.toggle', 'Browser', 'Panels'),
  C('terminal.toggle', 'Toggle the terminal', 'Panels'),
  C('composer.stash', 'Stash the draft', 'Composer'),
  C('modelPicker.toggle', 'Choose the model', 'Composer'),
  C('composer.effort', 'Choose the effort', 'Composer'),
  C('composer.mode', 'Choose the permission mode', 'Composer'),
  C('composer.workspace', 'Choose Local or Worktree', 'Composer'),
  C('composer.branch', 'Choose the branch', 'Composer'),
  C('composer.previousWorktree', 'Reuse the previous worktree', 'Composer'),
]

const NOT_TERMINAL = '!terminalFocus'

/** T3 Code's table, as the card adjusted it. */
export const DEFAULT_RULES: KeyRule[] = [
  { key: 'Ctrl+K', command: 'commandPalette.toggle', when: NOT_TERMINAL },
  { key: 'Ctrl+[', command: 'navigation.back', when: NOT_TERMINAL },
  { key: 'Ctrl+]', command: 'navigation.forward', when: NOT_TERMINAL },
  { key: 'F6', command: 'workspace.focusNext' },
  { key: 'Shift+F6', command: 'workspace.focusPrevious' },
  { key: 'Ctrl+,', command: 'settings.open' },
  { key: 'Ctrl+N', command: 'chat.new', when: NOT_TERMINAL },
  { key: 'Ctrl+Alt+N', command: 'chat.newWithoutProject', when: NOT_TERMINAL },
  { key: 'Ctrl+F', command: 'chat.find', when: '!terminalFocus && !previewFocus && !inspectorFocus' },
  { key: 'Ctrl+Shift+[', command: 'thread.previous' },
  { key: 'Ctrl+Shift+]', command: 'thread.next' },
  ...Array.from({ length: 9 }, (_, i) => ({ key: `Ctrl+${i + 1}`, command: `thread.jump.${i + 1}`, when: '!modelPickerOpen && !paletteOpen' })),
  { key: 'Ctrl+Shift+S', command: 'thread.settle', when: NOT_TERMINAL },
  { key: 'Ctrl+Shift+P', command: 'thread.pin', when: NOT_TERMINAL },
  { key: 'Ctrl+Shift+C', command: 'thread.copyReference', when: NOT_TERMINAL },
  { key: 'F2', command: 'thread.rename', when: '!editableFocus' },
  { key: 'Ctrl+Z', command: 'thread.undo', when: '!terminalFocus && !editableFocus' },
  { key: 'Ctrl+B', command: 'sidebar.toggle' },
  { key: 'Ctrl+Shift+B', command: 'threadPanel.toggle', when: NOT_TERMINAL },
  { key: 'Ctrl+Alt+B', command: 'rightPanel.toggle' },
  { key: 'Ctrl+W', command: 'rightPanel.close', when: '!terminalFocus && inspectorOpen' },
  { key: 'Ctrl+Shift+T', command: 'view.reopenClosed' },
  { key: 'Ctrl+D', command: 'diff.toggle', when: NOT_TERMINAL },
  { key: 'Ctrl+P', command: 'filePicker.toggle', when: NOT_TERMINAL },
  { key: 'Ctrl+Shift+F', command: 'projectSearch.toggle', when: NOT_TERMINAL },
  { key: 'Ctrl+Shift+J', command: 'preview.toggle' },
  { key: 'Ctrl+J', command: 'terminal.toggle' },
  { key: 'Ctrl+S', command: 'composer.stash', when: '!terminalFocus && !inspectorFocus' },
  { key: 'Ctrl+Shift+M', command: 'modelPicker.toggle', when: NOT_TERMINAL },
  { key: 'Ctrl+Shift+E', command: 'composer.effort', when: NOT_TERMINAL },
  { key: 'Ctrl+Shift+A', command: 'composer.mode', when: NOT_TERMINAL },
  { key: 'Ctrl+Shift+X', command: 'composer.workspace', when: NOT_TERMINAL },
  { key: 'Ctrl+Shift+G', command: 'composer.branch', when: NOT_TERMINAL },
  { key: 'Ctrl+Shift+L', command: 'composer.previousWorktree', when: NOT_TERMINAL },
]

/** Keys the suite itself answers anywhere; binding them here takes them from it inside QuantPilot. */
export const SUITE_KEYS: Record<string, string> = {
  'Ctrl+K': 'the QuantSuite command palette',
  'Ctrl+Shift+N': 'QuantSuite quick note',
}

/** Context keys a `when` may name; unknown ones are false. */
export const CONTEXT_KEYS = [
  'terminalFocus',
  'terminalOpen',
  'previewFocus',
  'previewOpen',
  'inspectorFocus',
  'inspectorOpen',
  'composerFocus',
  'editableFocus',
  'turnRunning',
  'modelPickerOpen',
  'paletteOpen',
  'usagePageOpen',
] as const

export type KeyContext = Partial<Record<string, boolean>>

// ── Keys ──

const MODIFIER_KEYS = new Set(['Control', 'Shift', 'Alt', 'Meta', 'AltGraph', 'OS'])

/** Punctuation by its physical key, as an unshifted US keyboard prints it. */
const CODE_CHARS: Record<string, string> = {
  BracketLeft: '[',
  BracketRight: ']',
  Comma: ',',
  Period: '.',
  Slash: '/',
  Backslash: '\\',
  Semicolon: ';',
  Quote: "'",
  Backquote: '`',
  Minus: '-',
  Equal: '=',
}

const NAMED: Record<string, string> = {
  ' ': 'Space',
  spacebar: 'Space',
  space: 'Space',
  esc: 'Escape',
  escape: 'Escape',
  enter: 'Enter',
  return: 'Enter',
  tab: 'Tab',
  backspace: 'Backspace',
  delete: 'Delete',
  del: 'Delete',
  insert: 'Insert',
  home: 'Home',
  end: 'End',
  pageup: 'PageUp',
  pagedown: 'PageDown',
  arrowup: 'ArrowUp',
  up: 'ArrowUp',
  arrowdown: 'ArrowDown',
  down: 'ArrowDown',
  arrowleft: 'ArrowLeft',
  left: 'ArrowLeft',
  arrowright: 'ArrowRight',
  right: 'ArrowRight',
  plus: '+',
}

function keyName(raw: string): string | null {
  if (!raw) return null
  const lower = raw.toLowerCase()
  if (NAMED[lower] || NAMED[raw]) return NAMED[lower] ?? NAMED[raw]
  if (/^f([1-9]|1\d|2[0-4])$/.test(lower)) return lower.toUpperCase()
  if (raw.length === 1) return raw.toUpperCase()
  return null
}

/**
 * The key of a keydown as a chord (`Ctrl+Shift+M`), or `null` while only
 * modifiers are held. Letters follow the layout (Ctrl+Z is the key labelled
 * Z on a German keyboard too); digits and punctuation follow the physical key,
 * so Shift does not turn Ctrl+Shift+1 into Ctrl+Shift+!.
 */
export function chordOf(e: Pick<KeyboardEvent, 'key' | 'code' | 'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey'>): string | null {
  if (MODIFIER_KEYS.has(e.key)) return null
  let key: string | null = null
  if (/^Key[A-Z]$/.test(e.code)) key = /^[a-z]$/i.test(e.key) ? e.key.toUpperCase() : e.code.slice(3)
  else if (/^Digit\d$/.test(e.code)) key = e.code.slice(5)
  else if (/^Numpad\d$/.test(e.code)) key = e.code.slice(6)
  else if (CODE_CHARS[e.code]) key = !e.shiftKey && e.key.length === 1 ? e.key : CODE_CHARS[e.code]
  else key = keyName(e.key)
  if (!key) return null
  const parts: string[] = []
  if (e.ctrlKey || e.metaKey) parts.push('Ctrl')
  if (e.altKey) parts.push('Alt')
  if (e.shiftKey) parts.push('Shift')
  parts.push(key)
  return parts.join('+')
}

/**
 * A written shortcut in the canonical form (`ctrl+shift+m`, `mod+[`,
 * `Alt+F6` → `Ctrl+Shift+M`, `Ctrl+[`, `Alt+F6`); `null` when it is not one.
 */
export function normalizeKey(text: string): string | null {
  const raw = text.trim()
  if (!raw) return null
  // "mod++": the last part is the plus key itself.
  const plus = raw.endsWith('++') || raw === '+'
  const parts = (plus ? raw.slice(0, raw.length - 1) : raw)
    .split('+')
    .map((p) => p.trim())
    .filter(Boolean)
  let ctrl = false
  let alt = false
  let shift = false
  let key: string | null = plus ? '+' : null
  const rest = plus ? parts : parts.slice(0, -1)
  for (const p of rest) {
    const m = p.toLowerCase()
    if (m === 'ctrl' || m === 'control' || m === 'mod' || m === 'cmd' || m === 'meta' || m === 'command') ctrl = true
    else if (m === 'alt' || m === 'option') alt = true
    else if (m === 'shift') shift = true
    else return null
  }
  if (!plus) key = keyName(parts.at(-1) ?? '')
  if (!key || MODIFIER_KEYS.has(key)) return null
  return [ctrl && 'Ctrl', alt && 'Alt', shift && 'Shift', key].filter(Boolean).join('+')
}

/** A chord the app may take: a modifier with a key, or a function key alone. */
export function usableKey(chord: string): boolean {
  const parts = chord.split('+')
  const key = parts.at(-1) ?? ''
  return /^F\d{1,2}$/.test(key) || (parts.length > 1 && (parts.includes('Ctrl') || parts.includes('Alt')))
}

// ── When expressions ──

export type WhenNode =
  | { type: 'id'; name: string }
  | { type: 'not'; node: WhenNode }
  | { type: 'and' | 'or'; left: WhenNode; right: WhenNode }

const MAX_WHEN = 256
const MAX_DEPTH = 64

/** `!a && (b || c)` → a tree; throws on anything else. `!` binds tightest, then `&&`, then `||`. */
export function parseWhen(text: string): WhenNode {
  if (text.length > MAX_WHEN) throw new Error('the condition is too long')
  const tokens = text.match(/\s*(&&|\|\||!|\(|\)|[A-Za-z_][\w.]*|\S)/g)?.map((t) => t.trim()) ?? []
  let i = 0
  const peek = () => tokens[i]
  const fail = (what: string): never => {
    throw new Error(what)
  }
  const or = (depth: number): WhenNode => {
    if (depth > MAX_DEPTH) fail('the condition nests too deep')
    let left = and(depth)
    while (peek() === '||') {
      i++
      left = { type: 'or', left, right: and(depth) }
    }
    return left
  }
  const and = (depth: number): WhenNode => {
    let left = unary(depth)
    while (peek() === '&&') {
      i++
      left = { type: 'and', left, right: unary(depth) }
    }
    return left
  }
  const unary = (depth: number): WhenNode => {
    const t = peek()
    if (t === '!') {
      i++
      return { type: 'not', node: unary(depth + 1) }
    }
    if (t === '(') {
      i++
      const node = or(depth + 1)
      if (peek() !== ')') fail('a parenthesis is not closed')
      i++
      return node
    }
    if (t && /^[A-Za-z_][\w.]*$/.test(t)) {
      i++
      return { type: 'id', name: t }
    }
    return fail(t ? `unexpected "${t}"` : 'the condition ends too early')
  }
  if (!tokens.length) fail('the condition is empty')
  const node = or(0)
  if (i < tokens.length) fail(`unexpected "${tokens[i]}"`)
  return node
}

export function evalWhen(node: WhenNode, ctx: KeyContext): boolean {
  switch (node.type) {
    case 'id':
      return Boolean(ctx[node.name])
    case 'not':
      return !evalWhen(node.node, ctx)
    case 'and':
      return evalWhen(node.left, ctx) && evalWhen(node.right, ctx)
    case 'or':
      return evalWhen(node.left, ctx) || evalWhen(node.right, ctx)
  }
}

// ── Rules ──

export interface CompiledRule {
  key: string
  command: string
  when?: string
  whenAst?: WhenNode
  /** Where it came from: the defaults or the user's file. */
  source: 'default' | 'user'
}

const KNOWN = new Set(COMMANDS.map((c) => c.id))

/** A rule from the user's file, checked; the reason when it cannot be used. */
export function checkRule(raw: unknown): { rule: KeyRule } | { error: string } {
  if (!raw || typeof raw !== 'object') return { error: 'not a rule' }
  const r = raw as Record<string, unknown>
  if (typeof r.command !== 'string' || !r.command.trim()) return { error: 'no command' }
  const command = r.command.trim()
  if (!KNOWN.has(command.replace(/^-/, ''))) return { error: `unknown command "${command}"` }
  if (typeof r.key !== 'string') return { error: `${command}: no key` }
  const key = normalizeKey(r.key)
  if (!key) return { error: `${command}: "${r.key}" is not a key` }
  if (r.when !== undefined) {
    if (typeof r.when !== 'string') return { error: `${command}: the condition is not text` }
    try {
      parseWhen(r.when)
    } catch (e) {
      return { error: `${command}: ${(e as Error).message}` }
    }
  }
  return { rule: { key, command, ...(typeof r.when === 'string' && r.when.trim() ? { when: r.when.trim() } : {}) } }
}

/**
 * The user's file (its text) as rules, what was skipped and why, and the
 * skipped entries as they are — saving keeps them for the user to fix.
 * Unreadable text means the defaults alone.
 */
export function parseUserRules(text: string | null): { rules: KeyRule[]; problems: string[]; skipped: unknown[] } {
  if (!text?.trim()) return { rules: [], problems: [], skipped: [] }
  let data: unknown
  try {
    data = JSON.parse(text)
  } catch {
    return { rules: [], problems: ['keybindings.json is not valid JSON — the defaults apply'], skipped: [] }
  }
  if (!Array.isArray(data)) return { rules: [], problems: ['keybindings.json is not a list of rules — the defaults apply'], skipped: [] }
  const rules: KeyRule[] = []
  const problems: string[] = []
  const skipped: unknown[] = []
  for (const raw of data.slice(0, 256)) {
    const r = checkRule(raw)
    if ('rule' in r) rules.push(r.rule)
    else {
      problems.push(r.error)
      skipped.push(raw)
    }
  }
  if (data.length > 256) problems.push(`only the first 256 rules apply`)
  return { rules, problems, skipped }
}

/** Defaults, then the user's rules; a `-command` rule removes that command's earlier rules on its key. */
export function compile(user: KeyRule[], defaults: KeyRule[] = DEFAULT_RULES): CompiledRule[] {
  const out: CompiledRule[] = []
  const add = (r: KeyRule, source: CompiledRule['source']) => {
    const key = normalizeKey(r.key)
    if (!key) return
    if (r.command.startsWith('-')) {
      const id = r.command.slice(1)
      for (let i = out.length - 1; i >= 0; i--) if (out[i].command === id && out[i].key === key) out.splice(i, 1)
      return
    }
    let whenAst: WhenNode | undefined
    if (r.when) {
      try {
        whenAst = parseWhen(r.when)
      } catch {
        return
      }
    }
    out.push({ key, command: r.command, when: r.when, whenAst, source })
  }
  defaults.forEach((r) => add(r, 'default'))
  user.forEach((r) => add(r, 'user'))
  return out
}

/** The command a chord runs here: the last rule whose key and condition match. */
export function resolve(rules: CompiledRule[], chord: string, ctx: KeyContext): string | null {
  for (let i = rules.length - 1; i >= 0; i--) {
    const r = rules[i]
    if (r.key === chord && (!r.whenAst || evalWhen(r.whenAst, ctx))) return r.command
  }
  return null
}

/** The keys a command has now, first one first. */
export function keysOf(rules: CompiledRule[], command: string): string[] {
  const keys: string[] = []
  for (const r of rules) if (r.command === command && !keys.includes(r.key)) keys.push(r.key)
  return keys
}

/**
 * Settings → Keybindings: `command` on `key` instead of what it had. The
 * user's rules keep everything else; the old defaults of the command are
 * removed with `-command` rules.
 */
export function rebind(user: KeyRule[], command: string, key: string, when?: string): KeyRule[] {
  const kept = user.filter((r) => r.command !== command && r.command !== `-${command}`)
  const removals = DEFAULT_RULES.filter((r) => r.command === command).map((r) => ({ key: r.key, command: `-${command}` }))
  const def = DEFAULT_RULES.find((r) => r.command === command)
  const condition = when ?? def?.when
  return [...kept, ...removals, { key, command, ...(condition ? { when: condition } : {}) }]
}

/** Back to the default keys of a command. */
export function resetBinding(user: KeyRule[], command: string): KeyRule[] {
  return user.filter((r) => r.command !== command && r.command !== `-${command}`)
}

/** No key for a command (its defaults removed). */
export function unbind(user: KeyRule[], command: string): KeyRule[] {
  const kept = resetBinding(user, command)
  return [...kept, ...DEFAULT_RULES.filter((r) => r.command === command).map((r) => ({ key: r.key, command: `-${command}` }))]
}

/** Two conditions that can hold at once: unless one is exactly the other's negation. */
function overlap(a?: string, b?: string): boolean {
  if (!a || !b) return true
  const n = (s: string) => s.replace(/\s+/g, '')
  const x = n(a)
  const y = n(b)
  return !(x === `!${y}` || y === `!${x}`)
}

/** What else answers `key` where `command` would: other commands and the suite's own keys. */
export function conflicts(rules: CompiledRule[], command: string, key: string, when?: string, extra: Record<string, string> = {}): string[] {
  const out: string[] = []
  for (const r of rules) {
    if (r.key !== key || r.command === command || !overlap(when, r.when)) continue
    const title = COMMANDS.find((c) => c.id === r.command)?.title ?? r.command
    if (!out.includes(title)) out.push(title)
  }
  if (SUITE_KEYS[key] && command !== 'commandPalette.toggle') out.push(SUITE_KEYS[key])
  if (extra[key]) out.push(extra[key])
  return out
}
