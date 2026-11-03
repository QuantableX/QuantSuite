/**
 * The agent menu (round 2, card 02): one menu wherever an agent is chosen —
 * which rows can be used and why not, the order of an agent's models
 * (recently used first), and where the menu and its submenu sit in the
 * window. Pure, so the node tests import it.
 */
import type { ModelRow } from './models'

/** What a pick in the menu starts or switches to. */
export interface AgentSetup {
  provider: string
  model: string
  effort: string
  fast: boolean
  /** An agent without a chat protocol: a terminal session that starts it (round 2, card 11). */
  terminal?: boolean
  /** The agent account (round 3, card 02); absent or "" = the default account. */
  instance?: string
}

/** A menu row's key: the agent, or `agent@account` for another account (round 3, card 02). */
export function accountKey(provider: string, instance?: string | null): string {
  return instance ? `${provider}@${instance}` : provider
}

export function splitAccountKey(key: string): { provider: string; instance: string } {
  const at = key.indexOf('@')
  return at < 0 ? { provider: key, instance: '' } : { provider: key.slice(0, at), instance: key.slice(at + 1) }
}

/**
 * A usage or limits key's label: `claude` → "Claude Code", `claude@work` →
 * "Claude Code · Work". A removed account keeps its id, so its past usage
 * never reads as the default account's.
 */
export function accountKeyLabel(key: string, agentLabel: (id: string) => string, instances: { provider: string; id: string; label: string }[]): string {
  const { provider, instance } = splitAccountKey(key)
  if (!instance) return agentLabel(provider)
  const acc = instances.find((i) => i.provider === provider && i.id === instance)
  return `${agentLabel(provider)} · ${acc?.label ?? instance}`
}

/** What the menu needs of an adapter (AdapterStatus). */
export interface MenuAdapter {
  id: string
  label: string
  found: boolean
  loggedIn: boolean | null
  /** Runs chat threads; absent on old status = Claude and Codex only. */
  chat?: boolean
}

export interface AgentRowState {
  usable: boolean
  /** Shown instead of the setup when the agent cannot be used. */
  reason: string | null
  /** A one-click fix: Settings → Providers on that agent. */
  fix: string | null
  /** Usable as a terminal session (no chat protocol). */
  terminal?: boolean
}

export function canChat(a: MenuAdapter): boolean {
  return a.chat ?? (a.id === 'claude' || a.id === 'codex')
}

/**
 * Why an agent cannot be picked here, in the order a user would fix it:
 * installed → chat → signed in → turned off → off in this project → busy.
 */
export function agentRowState(
  a: MenuAdapter,
  ctx: {
    /** Settings → Providers "Offered for threads" is off. */
    disabled?: boolean
    /** The project's own list of offered agents (null = every enabled one). */
    projectEnabled?: string[] | null
    /** The thread's agent is working: no other agent until it is done. */
    locked?: string | null
    current?: string | null
    /** New threads may start agents without a chat protocol as a terminal session (round 2, card 11). */
    allowTerminal?: boolean
  } = {},
): AgentRowState {
  if (!a.found) return { usable: false, reason: 'Not installed', fix: 'Set up…' }
  if (!canChat(a)) return ctx.allowTerminal ? { usable: true, reason: null, fix: null, terminal: true } : { usable: false, reason: 'Terminal session only', fix: null }
  if (a.loggedIn === false) return { usable: false, reason: 'Not signed in', fix: 'Sign in…' }
  if (ctx.disabled) return { usable: false, reason: 'Off in Settings', fix: 'Turn on…' }
  if (ctx.projectEnabled && !ctx.projectEnabled.includes(a.id)) return { usable: false, reason: 'Off in this project', fix: 'Project settings…' }
  if (ctx.locked && a.id !== ctx.current) return { usable: false, reason: ctx.locked, fix: null }
  return { usable: true, reason: null, fix: null }
}

/** Installed agents first, in their order; the ones not installed after them (round 2, card 11). */
export function installedFirst<T extends { found: boolean }>(list: T[]): T[] {
  return [...list.filter((a) => a.found), ...list.filter((a) => !a.found)]
}

/** Agents whose name or id has every typed word. */
export function filterAgents<T extends { id: string; label: string }>(list: T[], query: string): T[] {
  const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean)
  if (!words.length) return list
  return list.filter((a) => words.every((w) => `${a.id} ${a.label}`.toLowerCase().includes(w)))
}

// ── Recently used models ──

export const RECENT_MAX = 5
export type RecentModels = Record<string, string[]>

/** `model` first in the agent's list, once, at most RECENT_MAX. */
export function pushRecent(recent: RecentModels, provider: string, model: string): RecentModels {
  const list = (recent[provider] ?? []).filter((m) => m !== model)
  return { ...recent, [provider]: [model, ...list].slice(0, RECENT_MAX) }
}

/** The agent's recently used models first (in that order), then the rest in the catalog's order. Typed ids stay last. */
export function orderModels(rows: ModelRow[], recent: string[]): { recent: ModelRow[]; rest: ModelRow[] } {
  const known = rows.filter((r) => !r.custom)
  const first = recent.map((id) => known.find((r) => r.id === id)).filter((r): r is ModelRow => Boolean(r))
  const rest = rows.filter((r) => !first.includes(r))
  return { recent: first, rest }
}

/** "Haiku 5.5 · high" — the setup a row will really use. */
export function setupLabel(model: string, effort: string, names: Record<string, string> = {}): string {
  const m = model ? names[model] ?? model : 'Default'
  return effort ? `${m} · ${effort}` : m
}

// ── Placement ──

export interface Box {
  left: number
  top: number
  width: number
  height: number
}

const GAP = 6
const EDGE = 8

/**
 * The menu below its anchor, or above it when there is more room there;
 * never off screen. In a short window it may take the whole height.
 */
export function placeMenu(anchor: Box, size: { width: number; height: number }, view: { width: number; height: number }, prefer: 'below' | 'above' = 'below') {
  const below = view.height - (anchor.top + anchor.height) - GAP - EDGE
  const above = anchor.top - GAP - EDGE
  const fitsBelow = size.height <= below
  const fitsAbove = size.height <= above
  let side: 'below' | 'above' = prefer
  if (prefer === 'below' && !fitsBelow && (fitsAbove || above > below)) side = 'above'
  if (prefer === 'above' && !fitsAbove && (fitsBelow || below > above)) side = 'below'
  const room = side === 'below' ? below : above
  // Not enough room on either side: the whole window height.
  const full = view.height - 2 * EDGE
  const maxHeight = Math.max(120, Math.min(full, room >= Math.min(size.height, full) ? room : full))
  const height = Math.min(size.height, maxHeight)
  let top = side === 'below' ? anchor.top + anchor.height + GAP : anchor.top - GAP - height
  top = Math.max(EDGE, Math.min(top, view.height - EDGE - height))
  const left = Math.max(EDGE, Math.min(anchor.left, view.width - EDGE - size.width))
  return { left, top, maxHeight, side }
}

/** The submenu beside the menu (right, else left), its top at the row, kept inside the window. */
export function placeSubmenu(row: Box, menu: Box, size: { width: number; height: number }, view: { width: number; height: number }) {
  const right = menu.left + menu.width + 4
  const left = right + size.width <= view.width - EDGE ? right : Math.max(EDGE, menu.left - 4 - size.width)
  const maxHeight = view.height - 2 * EDGE
  const height = Math.min(size.height, maxHeight)
  const top = Math.max(EDGE, Math.min(row.top - 6, view.height - EDGE - height))
  return { left, top, maxHeight }
}
