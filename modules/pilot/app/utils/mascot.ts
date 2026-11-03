/**
 * Bob The Blob — QuantPilot's mascot ("Bob" for short; the user named him,
 * 2026-11-03). A flat dark glyph that speaks with its eyes only: moods for
 * what the agent does, idle quirks, reactions to the user, and the user's own
 * look (skin, eyes, an accessory, a name). This file holds what is pure: which
 * mood a state shows, the lines Bob says, the preferences and their
 * defaults, the quirk picker and the small gesture detectors. Pure, so the
 * node tests import it.
 */
import type { LiveState } from '#pilot/types'

/** His name, and what the settings call him. */
export const MASCOT_NAME = 'Bob'
export const MASCOT_FULL_NAME = 'Bob The Blob'

/** What the face shows. */
export type Mood =
  | 'sleep' // offline, or napping after a long quiet spell
  | 'calm' // idle
  | 'think' // thinking
  | 'focus' // working: reads along
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

/** A long run: Bob gets tired after this. */
export const LONG_RUN_MS = 8 * 60_000
/** This many agents working at once and he juggles. */
export const JUGGLE_AT = 3
/** This long without the pointer while idle and he takes a nap. */
export const NAP_AFTER_MS = 3 * 60_000

export interface MoodInput {
  state: LiveState
  /** Agents working anywhere right now. */
  busy?: number
  /** How long the current turn has run. */
  elapsedMs?: number
  /** A finish or an approval is being celebrated right now. */
  cheering?: boolean
  nodding?: boolean
  /** Idle and nobody around for a while: he dozes off. */
  napping?: boolean
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
      return i.napping ? 'sleep' : 'calm'
  }
}

/** Bob's lines: tooltips and empty states only, never the transcript. */
export const LINES: Record<Mood, string[]> = {
  sleep: ['Zzz… wake me with a message.', 'Dreaming of green builds.', 'Just resting my eyes.'],
  calm: ['Ready when you are.', 'What are we building?', "I'm all eyes.", 'Point me at something.'],
  think: ['Hmm, let me think…', 'Turning it over.', 'Connecting the dots.'],
  focus: ['Heads down.', 'On it.', 'Reading along.'],
  juggle: ['Keeping all the balls in the air.', 'Lots going on — all fine.', 'Juggling, not dropping.'],
  tired: ['Still going — long one.', 'Not done yet, not giving up.', 'Phew. Almost there?'],
  call: ['Your turn — I need you.', 'Psst, a question for you.', 'Over here!'],
  worried: ['Uh-oh. Something went wrong.', 'That did not go to plan.', 'Hm. Want to take a look?'],
  cheer: ['Done!', 'Nailed it.', 'Ta-da!'],
  nod: ['Thanks — carrying on.', 'Got it.', 'Approved — back to work.'],
}

/** When he is calm, the time of day picks a greeting now and then. */
const GREETINGS: { from: number; to: number; line: string }[] = [
  { from: 5, to: 11, line: 'Morning! What are we building?' },
  { from: 11, to: 17, line: 'Afternoon — ready when you are.' },
  { from: 17, to: 22, line: 'Evening shift. What is next?' },
  { from: 22, to: 29, line: 'Late one, huh? I am here.' },
]

/**
 * One of the mood's lines, picked by `seed` (stable per render). With an
 * hour, a calm Bob greets by the time of day on every fourth seed.
 */
export function lineFor(mood: Mood, name: string, seed = 0, hour?: number): string {
  const list = LINES[mood]
  const s = Math.abs(Math.floor(seed))
  let line = list[s % list.length]!
  if (mood === 'calm' && hour !== undefined && s % 4 === 3) {
    const h = hour < 5 ? hour + 24 : hour
    line = GREETINGS.find((g) => h >= g.from && h < g.to)?.line ?? line
  }
  const n = name.trim()
  return n ? `${n}: ${line}` : line
}

// ── The user's look ──

export type Skin = 'night' | 'mint' | 'sky' | 'violet' | 'peach' | 'lemon' | 'ghost' | 'rose' | 'ocean'
export type EyeStyle = 'dots' | 'round' | 'sparkle'
export type Accessory = 'none' | 'cap' | 'glasses' | 'headset' | 'bow' | 'crown' | 'sprout' | 'party' | 'beanie'
export type MascotSize = 'small' | 'medium' | 'large'
/** Lively: breathing and frequent quirks. Calm: fewer, gentler ones. Still: expressions only. */
export type MascotMotion = 'still' | 'calm' | 'lively'

export interface MascotPrefs {
  on: boolean
  name: string
  skin: Skin
  eyes: EyeStyle
  accessory: Accessory
  size: MascotSize
  /** Tint the skin with the thread's agent colour. */
  agentTint: boolean
  /** A small Bob that can be dragged anywhere in QuantPilot. */
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
  // Night is the first face: a dark glyph, darker than the panel, with a light outline.
  { id: 'night', label: 'Night', base: '#141418', light: '#2c2c34', ink: '#e4e4e7' },
  { id: 'ghost', label: 'Ghost', base: '#d9dbe3', light: '#ffffff', ink: '#1b1b22' },
  { id: 'mint', label: 'Mint', base: '#3fbf95', light: '#9ff0cf', ink: '#13261f' },
  { id: 'sky', label: 'Sky', base: '#4c8fe6', light: '#a9cdfb', ink: '#0f1d33' },
  { id: 'ocean', label: 'Ocean', base: '#1f8a9e', light: '#8fe3ef', ink: '#06232a' },
  { id: 'violet', label: 'Violet', base: '#8b6ff0', light: '#cfc0ff', ink: '#1c1433' },
  { id: 'rose', label: 'Rose', base: '#e66a8e', light: '#ffc2d4', ink: '#2e0d18' },
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
  { id: 'beanie', label: 'Beanie' },
  { id: 'crown', label: 'Crown' },
  { id: 'party', label: 'Party hat' },
  { id: 'sprout', label: 'Sprout' },
  { id: 'bow', label: 'Bow' },
  { id: 'glasses', label: 'Glasses' },
  { id: 'headset', label: 'Headset' },
]
export const MOTIONS: { id: MascotMotion; label: string }[] = [
  { id: 'lively', label: 'Lively' },
  { id: 'calm', label: 'Calm' },
  { id: 'still', label: 'Still' },
]
export const SIZES: { id: MascotSize; label: string; px: number }[] = [
  { id: 'small', label: 'Small', px: 88 },
  { id: 'medium', label: 'Medium', px: 120 },
  { id: 'large', label: 'Large', px: 156 },
]

export const DEFAULT_PREFS: MascotPrefs = {
  on: true,
  name: MASCOT_NAME,
  skin: 'night',
  eyes: 'dots',
  accessory: 'none',
  size: 'medium',
  agentTint: false,
  companion: false,
  companionAt: { x: 0.92, y: 0.82 },
  motion: 'lively',
}

/** The name the first version gave him; saved preferences still carry it. */
const OLD_DEFAULT_NAME = 'Pip'

const oneOf = <T extends string>(v: unknown, list: { id: T }[], d: T): T => (list.some((x) => x.id === v) ? (v as T) : d)
const unit = (v: unknown, d: number) => (typeof v === 'number' && Number.isFinite(v) ? Math.min(1, Math.max(0, v)) : d)

/** Saved preferences, checked: unknown values fall back to the defaults; the old default name becomes Bob. */
export function normalizePrefs(raw: unknown): MascotPrefs {
  const r = (raw && typeof raw === 'object' ? raw : {}) as Record<string, unknown>
  const at = (r.companionAt && typeof r.companionAt === 'object' ? r.companionAt : {}) as Record<string, unknown>
  const name = typeof r.name === 'string' ? r.name.trim().slice(0, 24) : DEFAULT_PREFS.name
  return {
    on: typeof r.on === 'boolean' ? r.on : DEFAULT_PREFS.on,
    name: name === OLD_DEFAULT_NAME ? MASCOT_NAME : name,
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

// ── Idle quirks: what he does on his own while calm ──
// The face speaks with its eyes (no mouth), so the life is in the eyes and in
// the body: a new quirk every few seconds, never the same twice in a row.

export type Quirk =
  | 'look' // looks left, then right
  | 'roll' // rolls his eyes around
  | 'tilt' // tilts curiously
  | 'wink'
  | 'happy' // ^^ and a hop
  | 'hop'
  | 'wiggle'
  | 'pop' // eyes go wide ("oh, hi")
  | 'giggle' // > < and a shake
  | 'stretch' // stretches with closed eyes
  | 'drowsy' // nods off and startles awake
  | 'love' // a slow blink at you
  | 'spin' // a barrel roll
  | 'sidehop' // hops aside and back
  | 'shake' // shakes himself off
  | 'hiccup'
  | 'firefly' // a firefly visits; he follows it with his eyes

/** Each quirk: how long it runs (ms), how often it comes up, and whether it is a big one (not when Calm). */
export const QUIRKS: Record<Quirk, { ms: number; weight: number; big?: boolean }> = {
  look: { ms: 1900, weight: 5 },
  roll: { ms: 1500, weight: 3 },
  tilt: { ms: 1900, weight: 4 },
  wink: { ms: 700, weight: 3 },
  happy: { ms: 1600, weight: 3 },
  hop: { ms: 900, weight: 3, big: true },
  wiggle: { ms: 1100, weight: 2, big: true },
  pop: { ms: 800, weight: 2 },
  giggle: { ms: 1300, weight: 2, big: true },
  stretch: { ms: 1700, weight: 2 },
  drowsy: { ms: 3200, weight: 0 },
  love: { ms: 1600, weight: 3 },
  spin: { ms: 1100, weight: 1, big: true },
  sidehop: { ms: 1500, weight: 2, big: true },
  shake: { ms: 900, weight: 2, big: true },
  hiccup: { ms: 1100, weight: 2 },
  firefly: { ms: 4200, weight: 2 },
}

/** Every quirk as a trick in Settings (in this order). */
export const QUIRK_LABEL: Record<Quirk, string> = {
  look: 'Look around',
  roll: 'Eye roll',
  tilt: 'Tilt',
  wink: 'Wink',
  love: 'Slow blink',
  happy: 'Happy hop',
  hop: 'Hop',
  sidehop: 'Side hop',
  spin: 'Barrel roll',
  wiggle: 'Wiggle',
  shake: 'Shake off',
  hiccup: 'Hiccup',
  giggle: 'Giggle',
  pop: 'Oh, hi',
  stretch: 'Stretch',
  drowsy: 'Nod off',
  firefly: 'Firefly',
}

/** After this long without the user moving the pointer he starts nodding off now and then. */
export const DROWSY_AFTER_MS = 45_000

/** The next quirk: weighted, not the last one; drowsy only after a long quiet spell; no big ones when Calm. */
export function pickQuirk(last: Quirk | null, quietMs: number, r: () => number = Math.random, motion: MascotMotion = 'lively'): Quirk {
  const pool = (Object.keys(QUIRKS) as Quirk[])
    .filter((q) => q !== last && !(motion === 'calm' && QUIRKS[q].big))
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

/** The pause before the next quirk (ms): every few seconds when Lively, rarely when Calm. */
export function quirkPause(r: () => number = Math.random, motion: MascotMotion = 'lively'): number {
  return motion === 'calm' ? 9000 + r() * 9000 : 2800 + r() * 4200
}

// ── Gestures ──

/** Clicks this close together, this many of them, and Bob gets dizzy. */
export const DIZZY_CLICKS = 4
export const DIZZY_WINDOW_MS = 1600

/** Whether the latest clicks (timestamps, ms) make him dizzy. */
export function dizzyFrom(clicks: number[], now: number): boolean {
  return clicks.filter((t) => now - t <= DIZZY_WINDOW_MS).length >= DIZZY_CLICKS
}

/**
 * Petting: the pointer stroking over him, back and forth. `xs` are the
 * pointer's x positions over him (oldest first) within the last second; three
 * turns of direction, each after at least `minRun` px, count as petting.
 */
export function isPetting(xs: number[], minRun = 6): boolean {
  let dir = 0
  let run = 0
  let turns = 0
  for (let i = 1; i < xs.length; i++) {
    const dx = xs[i]! - xs[i - 1]!
    if (dx === 0) continue
    const d = Math.sign(dx)
    if (d === dir || dir === 0) {
      run += Math.abs(dx)
      dir = d
      continue
    }
    if (run >= minRun) turns++
    dir = d
    run = Math.abs(dx)
  }
  return turns >= 3
}
