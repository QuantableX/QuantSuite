/**
 * The pilot mascot (round 2, card 05): the same blob, with more of
 * everything — moods for what the agent does, depth, movement, and the
 * user's own look (skin, eyes, an accessory, a name). This file holds what
 * is pure: which mood a state shows, the lines it says, the preferences
 * and their defaults. Pure, so the node tests import it.
 */
import type { LiveState } from '#pilot/types'

/** What the face shows. */
export type Mood =
  | 'sleep' // offline
  | 'calm' // idle
  | 'think' // thinking
  | 'focus' // working
  | 'juggle' // working while three or more agents work
  | 'tired' // working for a long time
  | 'call' // waiting on the user: looks at you, waves
  | 'worried' // error
  | 'cheer' // a turn finished
  | 'nod' // an approval was granted

export const MOODS: Mood[] = ['calm', 'think', 'focus', 'juggle', 'tired', 'call', 'worried', 'cheer', 'nod', 'sleep']

export const MOOD_LABEL: Record<Mood, string> = {
  sleep: 'Sleeping',
  calm: 'Idle',
  think: 'Thinking',
  focus: 'Working',
  juggle: 'Many agents busy',
  tired: 'Long run',
  call: 'Needs you',
  worried: 'Error',
  cheer: 'Turn done',
  nod: 'Approved',
}

/** A long run: the mascot gets tired after this. */
export const LONG_RUN_MS = 8 * 60_000
/** This many agents working at once and it juggles. */
export const JUGGLE_AT = 3

export interface MoodInput {
  state: LiveState
  /** Agents working anywhere right now. */
  busy?: number
  /** How long the current turn has run. */
  elapsedMs?: number
  /** A finish or an approval is being celebrated right now. */
  cheering?: boolean
  nodding?: boolean
}

/** The mood for a state and what happens around it; events win over states. */
export function moodOf(i: MoodInput): Mood {
  if (i.cheering && i.state !== 'error') return 'cheer'
  if (i.nodding && i.state !== 'error') return 'nod'
  switch (i.state) {
    case 'offline':
      return 'sleep'
    case 'thinking':
      return 'think'
    case 'waiting':
      return 'call'
    case 'error':
      return 'worried'
    case 'working':
      if ((i.elapsedMs ?? 0) >= LONG_RUN_MS) return 'tired'
      if ((i.busy ?? 0) >= JUGGLE_AT) return 'juggle'
      return 'focus'
    default:
      return 'calm'
  }
}

/** Short personality lines: tooltips and empty states only, never the transcript. */
export const LINES: Record<Mood, string[]> = {
  sleep: ['Zzz… wake me with a message.', 'Dreaming of green builds.'],
  calm: ['Ready when you are.', 'What are we building?', 'All quiet here.'],
  think: ['Hmm, let me think…', 'Turning it over.'],
  focus: ['Heads down.', 'On it.', 'Typing away.'],
  juggle: ['Keeping all the balls in the air.', 'Lots going on — all fine.'],
  tired: ['Still going — long one.', 'Not done yet, not giving up.'],
  call: ['Your turn — I need you.', 'Psst, a question for you.'],
  worried: ['Uh-oh. Something went wrong.', 'That did not go to plan.'],
  cheer: ['Done!', 'Nailed it.'],
  nod: ['Thanks — carrying on.', 'Got it.'],
}

/** One of the mood's lines, picked by `seed` (stable per render). */
export function lineFor(mood: Mood, name: string, seed = 0): string {
  const list = LINES[mood]
  const line = list[Math.abs(Math.floor(seed)) % list.length]!
  const n = name.trim()
  return n ? `${n}: ${line}` : line
}

// ── The user's look ──

export type Skin = 'night' | 'mint' | 'sky' | 'violet' | 'peach' | 'lemon'
export type EyeStyle = 'dots' | 'round' | 'sparkle'
export type Accessory = 'none' | 'cap' | 'glasses' | 'headset' | 'bow'
export type MascotSize = 'small' | 'medium' | 'large'
/** Lively: breathing and idle quirks. Still: expressions only. */
export type MascotMotion = 'lively' | 'still'

export interface MascotPrefs {
  on: boolean
  name: string
  skin: Skin
  eyes: EyeStyle
  accessory: Accessory
  size: MascotSize
  /** Tint the skin with the thread's agent colour. */
  agentTint: boolean
  /** A small mascot that can be dragged anywhere in QuantPilot. */
  companion: boolean
  /** Where the companion was left, as fractions of the window. */
  companionAt: { x: number; y: number }
  /**
   * Its own setting, not the system's: Windows reports reduced motion whenever
   * animation effects are off, which silenced the mascot entirely.
   */
  motion: MascotMotion
}

export const SKINS: { id: Skin; label: string; base: string; light: string; ink: string }[] = [
  // Night is the old face: a dark glyph, darker than the panel, with a light outline.
  { id: 'night', label: 'Night', base: '#141418', light: '#2c2c34', ink: '#e4e4e7' },
  { id: 'mint', label: 'Mint', base: '#3fbf95', light: '#9ff0cf', ink: '#13261f' },
  { id: 'sky', label: 'Sky', base: '#4c8fe6', light: '#a9cdfb', ink: '#0f1d33' },
  { id: 'violet', label: 'Violet', base: '#8b6ff0', light: '#cfc0ff', ink: '#1c1433' },
  { id: 'peach', label: 'Peach', base: '#f08e6a', light: '#ffd2bd', ink: '#33170d' },
  { id: 'lemon', label: 'Lemon', base: '#e8c547', light: '#fff0a8', ink: '#2e2608' },
]
export const EYES: { id: EyeStyle; label: string }[] = [
  { id: 'dots', label: 'Dots' },
  { id: 'round', label: 'Round' },
  { id: 'sparkle', label: 'Sparkle' },
]
export const ACCESSORIES: { id: Accessory; label: string }[] = [
  { id: 'none', label: 'None' },
  { id: 'cap', label: 'Cap' },
  { id: 'glasses', label: 'Glasses' },
  { id: 'headset', label: 'Headset' },
  { id: 'bow', label: 'Bow' },
]
export const MOTIONS: { id: MascotMotion; label: string }[] = [
  { id: 'lively', label: 'Lively' },
  { id: 'still', label: 'Still' },
]
export const SIZES: { id: MascotSize; label: string; px: number }[] = [
  { id: 'small', label: 'Small', px: 88 },
  { id: 'medium', label: 'Medium', px: 120 },
  { id: 'large', label: 'Large', px: 156 },
]

export const DEFAULT_PREFS: MascotPrefs = {
  on: true,
  name: 'Pip',
  skin: 'night',
  eyes: 'dots',
  accessory: 'none',
  size: 'medium',
  agentTint: false,
  companion: false,
  companionAt: { x: 0.92, y: 0.82 },
  motion: 'lively',
}

const oneOf = <T extends string>(v: unknown, list: { id: T }[], d: T): T => (list.some((x) => x.id === v) ? (v as T) : d)
const unit = (v: unknown, d: number) => (typeof v === 'number' && Number.isFinite(v) ? Math.min(1, Math.max(0, v)) : d)

/** Saved preferences, checked: unknown values fall back to the defaults. */
export function normalizePrefs(raw: unknown): MascotPrefs {
  const r = (raw && typeof raw === 'object' ? raw : {}) as Record<string, unknown>
  const at = (r.companionAt && typeof r.companionAt === 'object' ? r.companionAt : {}) as Record<string, unknown>
  return {
    on: typeof r.on === 'boolean' ? r.on : DEFAULT_PREFS.on,
    name: typeof r.name === 'string' ? r.name.trim().slice(0, 24) : DEFAULT_PREFS.name,
    skin: oneOf(r.skin, SKINS, DEFAULT_PREFS.skin),
    eyes: oneOf(r.eyes, EYES, DEFAULT_PREFS.eyes),
    accessory: oneOf(r.accessory, ACCESSORIES, DEFAULT_PREFS.accessory),
    size: oneOf(r.size, SIZES, DEFAULT_PREFS.size),
    agentTint: typeof r.agentTint === 'boolean' ? r.agentTint : DEFAULT_PREFS.agentTint,
    companion: typeof r.companion === 'boolean' ? r.companion : DEFAULT_PREFS.companion,
    companionAt: { x: unit(at.x, DEFAULT_PREFS.companionAt.x), y: unit(at.y, DEFAULT_PREFS.companionAt.y) },
    motion: oneOf(r.motion, MOTIONS, DEFAULT_PREFS.motion),
  }
}

/** An agent's colour for its outfit tint. */
const AGENT_TINT: Record<string, string> = {
  claude: '#d97757',
  codex: '#7a8794',
  gemini: '#4c8fe6',
  opencode: '#9a9aa5',
  pi: '#c084fc',
  omp: '#a78bfa',
  copilot: '#6e7bff',
  cursor: '#8b8b95',
  kilo: '#f5d04c',
  hermes: '#f59e0b',
  openclaw: '#ef4444',
  grok: '#9aa0a6',
}
export function agentTint(agent: string | null | undefined): string | null {
  return agent ? (AGENT_TINT[agent] ?? null) : null
}

/** Where the eyes look: toward a point, at most `max` (fractions of the face) from the center. */
export function gazeToward(dx: number, dy: number, max = 0.06): { x: number; y: number } {
  const d = Math.hypot(dx, dy)
  if (d < 1) return { x: 0, y: 0 }
  // Near the face the eyes turn a little; far away they turn fully.
  const reach = Math.min(1, d / 260)
  return { x: (dx / d) * max * reach, y: (dy / d) * max * reach * 0.8 }
}

// ── Idle quirks: what it does on its own while calm ──
// The face speaks with its eyes (no mouth), so the life is in the eyes and in
// the body: a new quirk every few seconds, never the same twice in a row.

export type Quirk = 'look' | 'roll' | 'tilt' | 'wink' | 'happy' | 'hop' | 'wiggle' | 'pop' | 'giggle' | 'stretch' | 'drowsy'

/** Each quirk: how long it runs (ms) and how often it comes up. */
export const QUIRKS: Record<Quirk, { ms: number; weight: number }> = {
  look: { ms: 1900, weight: 5 },
  roll: { ms: 1500, weight: 3 },
  tilt: { ms: 1900, weight: 4 },
  wink: { ms: 700, weight: 3 },
  happy: { ms: 1600, weight: 3 },
  hop: { ms: 900, weight: 3 },
  wiggle: { ms: 1100, weight: 2 },
  pop: { ms: 800, weight: 2 },
  giggle: { ms: 1300, weight: 2 },
  stretch: { ms: 1700, weight: 2 },
  drowsy: { ms: 3200, weight: 0 },
}

/** After this long without the user moving the pointer it starts nodding off now and then. */
export const DROWSY_AFTER_MS = 45_000

/** The next quirk: weighted, not the last one; drowsy only after a long quiet spell. */
export function pickQuirk(last: Quirk | null, quietMs: number, r: () => number = Math.random): Quirk {
  const pool = (Object.keys(QUIRKS) as Quirk[])
    .filter((q) => q !== last)
    .map((q) => [q, q === 'drowsy' ? (quietMs >= DROWSY_AFTER_MS ? 4 : 0) : QUIRKS[q].weight] as const)
    .filter(([, w]) => w > 0)
  const total = pool.reduce((n, [, w]) => n + w, 0)
  let x = r() * total
  for (const [q, w] of pool) {
    x -= w
    if (x < 0) return q
  }
  return pool[pool.length - 1]![0]
}

/** The pause before the next quirk (ms). */
export function quirkPause(r: () => number = Math.random): number {
  return 2800 + r() * 4200
}
