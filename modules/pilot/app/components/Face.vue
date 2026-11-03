<script setup lang="ts">
/**
 * Bob The Blob — QuantPilot's mascot ("Bob"; named by the user, 2026-11-03).
 *
 * A flat dark glyph with a soft outline that speaks with its eyes only — no
 * mouth, no cheeks (the user found a smile on him uncanny), and no half-shut
 * eyes held open (a slight squint read as suspicious). Every animation ends
 * where it started.
 *
 * Moods for what the agent does (utils/mascot.ts): sleeping (also a nap after
 * three quiet minutes), calm, thinking (a thought bubble), working (reads
 * along), juggling when several agents work, tired on a long run (droops,
 * sweats), calling you (a "!", a wave, attention hops), worried (nervous
 * glances), cheering when a turn finishes (a hop and a sparkle), nodding with
 * happy eyes when an approval is granted.
 * Life while calm: a quirk every few seconds (look around, roll the eyes,
 * tilt, wink, a slow "love" blink, hop, side hop, barrel roll, wiggle, shake
 * off, hiccup, giggle > <, stretch, a firefly he follows with his eyes, nod
 * off after a quiet spell) and an "oh, hi" when the pointer comes back.
 * Reactions: the eyes follow the pointer and watch the field you type in;
 * hover perks him up; a click is a boing and a ^^; rapid clicks make him
 * dizzy; stroking him with the pointer makes him happy; carried as the
 * companion he is excited (> < when swung fast) and lands with a squash.
 *
 * Built from plain elements: every motion is `transform`/`opacity` (CSS, or
 * WAAPI for one-off reactions), so the compositor runs it. Timers and
 * listeners live only while the face is on screen (an active page, a visible
 * document). Movement is his own setting (Lively / Calm / Still), not the
 * system's: Windows reports reduced motion whenever animation effects are
 * off, which silenced him on the user's machine — while not Still he carries
 * [data-own-motion], which the shell's global reduced-motion rule leaves
 * alone. Still keeps every expression and drops the motion and the quirks.
 */
import { computed, nextTick, onActivated, onBeforeUnmount, onDeactivated, onMounted, ref, watch } from 'vue'
import { inActiveKeepAliveTree } from '@quantsuite/core'
import { useMascotStore } from '#pilot/stores/mascot'
import {
  agentTint,
  dizzyFrom,
  gazeToward,
  isPetting,
  lineFor,
  MASCOT_NAME,
  moodOf,
  MOOD_LABEL,
  NAP_AFTER_MS,
  pickQuirk,
  QUIRKS,
  quirkPause,
  SKINS,
  type Mood,
  type Quirk,
} from '#pilot/utils/mascot'
import type { LiveState } from '#pilot/types'

const props = withDefaults(
  defineProps<{
    state: LiveState
    /** Bump whenever a turn finished: he cheers. */
    cheer?: number
    /** Bump when an approval was granted: he nods. */
    nod?: number
    /** Agents working anywhere: three or more and he juggles. */
    busy?: number
    /** When the running turn started: a long run tires him. */
    startedAt?: number | null
    /** The thread's agent, for the outfit tint. */
    agent?: string | null
    /** Show this mood regardless (Settings' gallery). */
    mood?: Mood | null
    /** Follow the pointer and typing, react to hover, clicks and petting, nap. */
    interactive?: boolean
    /** Carried as the companion: held, or swung fast. */
    carried?: 'no' | 'held' | 'fast'
  }>(),
  { cheer: 0, nod: 0, busy: 0, startedAt: null, agent: null, mood: null, interactive: true, carried: 'no' }
)

const mascot = useMascotStore()
const prefs = computed(() => mascot.prefs)
const motion = computed(() => prefs.value.motion)
const reduced = computed(() => motion.value === 'still')

// ── On screen? Timers and listeners only then ──
const root = ref<HTMLElement | null>(null)
const active = ref(false)
const pageVisible = ref(typeof document === 'undefined' ? true : !document.hidden)
const live = computed(() => active.value && pageVisible.value)
const onVisibility = () => (pageVisible.value = !document.hidden)

const timers = new Set<ReturnType<typeof setTimeout>>()
function later(fn: () => void, ms: number) {
  const t = setTimeout(() => {
    timers.delete(t)
    fn()
  }, ms)
  timers.add(t)
}
function clearTimers() {
  for (const t of timers) clearTimeout(t)
  timers.clear()
}
const rand = (a: number, b: number) => a + Math.random() * (b - a)

// ── Events: cheer, nod ──
const cheering = ref(false)
const nodding = ref(false)
let cheerEnd = 0
function cheerFor(ms: number) {
  cheering.value = true
  const until = Date.now() + ms
  cheerEnd = until
  later(() => {
    if (cheerEnd === until) cheering.value = false
  }, ms)
}
watch(
  () => props.cheer,
  (now, before) => {
    if (before !== undefined && now > before && props.state !== 'error') cheerFor(2200)
  }
)
watch(
  () => props.nod,
  (now, before) => {
    if (before === undefined || now <= before) return
    nodding.value = true
    later(() => (nodding.value = false), 1100)
  }
)

// A long run: checked twice a minute while working and on screen.
const now = ref(Date.now())
let longTimer: ReturnType<typeof setInterval> | null = null
function syncLong() {
  const want = live.value && props.state === 'working' && Boolean(props.startedAt)
  if (want && !longTimer) {
    now.value = Date.now()
    longTimer = setInterval(() => (now.value = Date.now()), 30_000)
  } else if (!want && longTimer) {
    clearInterval(longTimer)
    longTimer = null
  }
}

// A nap: idle and nobody around for a while.
const napping = ref(false)
let lastPointerAt = Date.now()
function scheduleNapCheck() {
  later(() => {
    if (props.interactive && !props.mood && props.state === 'idle' && Date.now() - lastPointerAt > NAP_AFTER_MS) napping.value = true
    scheduleNapCheck()
  }, 15_000)
}
function wake() {
  if (!napping.value) return
  napping.value = false
  later(() => runQuirk('pop'), 260)
}

const mood = computed<Mood>(
  () =>
    props.mood ??
    moodOf({
      state: props.state,
      busy: props.busy,
      elapsedMs: props.startedAt ? now.value - props.startedAt : 0,
      cheering: cheering.value,
      nodding: nodding.value,
      napping: napping.value,
    })
)

// ── Gaze: the pointer, the field being typed in, else a glance now and then ──
const gaze = ref({ x: 0, y: 0 })
const blink = ref(false)
let followUntil = 0
const following = () => Date.now() < followUntil
let pendingPointer: { x: number; y: number } | null = null
let frame = 0
function lookAt(x: number, y: number, reach = 0.06) {
  const el = root.value
  if (!el) return
  const r = el.getBoundingClientRect()
  gaze.value = gazeToward(x - (r.left + r.width / 2), y - (r.top + r.height / 2), reach)
}
function onPointer(e: PointerEvent) {
  const t = Date.now()
  if (napping.value) wake()
  else if (t - lastPointerAt > 15_000 && mood.value === 'calm' && !reduced.value) runQuirk('pop')
  lastPointerAt = t
  pendingPointer = { x: e.clientX, y: e.clientY }
  if (frame) return
  frame = requestAnimationFrame(() => {
    frame = 0
    const p = pendingPointer
    if (!p || mood.value === 'sleep' || mood.value === 'cheer' || glee.value || petted.value || dizzy.value) return
    lookAt(p.x, p.y)
    followUntil = Date.now() + 2500
  })
}
/** Typing somewhere: he watches the field. */
function onType(e: Event) {
  const t = e.target
  if (!(t instanceof HTMLElement) || !(t.tagName === 'TEXTAREA' || t.tagName === 'INPUT' || t.isContentEditable)) return
  lastPointerAt = Date.now()
  if (napping.value) wake()
  if (mood.value === 'sleep' || mood.value === 'cheer' || dizzy.value) return
  const a = t.getBoundingClientRect()
  lookAt(a.left + Math.min(a.width, 420) / 2, a.top + a.height / 2)
  followUntil = Date.now() + 1800
}

function scheduleGlance() {
  const m = mood.value
  let next = rand(2800, 6500)
  if (!following() && !cheering.value && !quirk.value) {
    if (m === 'sleep') {
      gaze.value = { x: rand(-0.01, 0.01), y: 0.02 }
      next = rand(6000, 11000)
    } else if (m === 'call') {
      gaze.value = { x: 0, y: 0 }
      next = rand(4000, 7000)
    } else if (m === 'think') {
      gaze.value = { x: rand(0.02, 0.05) * (Math.random() < 0.5 ? -1 : 1), y: -0.04 }
      next = rand(1100, 2600)
    } else if (m === 'focus' || m === 'juggle') {
      // Reading along: a line left to right in small jumps, then back.
      const y = rand(0.015, 0.03)
      const steps = [-0.04, -0.016, 0.008, 0.032]
      steps.forEach((x, i) => later(() => !following() && (gaze.value = { x, y }), i * 360))
      next = steps.length * 360 + rand(250, 900)
    } else if (m === 'worried') {
      // Nervous glances to and fro.
      gaze.value = { x: (gaze.value.x > 0 ? -1 : 1) * rand(0.025, 0.045), y: rand(-0.01, 0.015) }
      next = rand(450, 950)
    } else if (m === 'tired') {
      gaze.value = { x: rand(-0.02, 0.02), y: 0.02 }
      next = rand(4000, 8000)
    } else if (Math.random() < 0.3) {
      gaze.value = { x: 0, y: 0 }
    } else {
      gaze.value = { x: rand(-0.05, 0.05), y: rand(-0.035, 0.03) }
    }
  }
  later(scheduleGlance, next)
}
function scheduleBlink() {
  const tired = mood.value === 'tired'
  later(() => {
    if (!cheering.value && !quirk.value && !glee.value && !dizzy.value && !petted.value && mood.value !== 'sleep') {
      blink.value = true
      later(() => {
        blink.value = false
        if (!tired && Math.random() < 0.18) later(() => {
          blink.value = true
          later(() => (blink.value = false), 110)
        }, 190)
      }, tired ? 280 : 130)
    }
    scheduleBlink()
  }, mood.value === 'sleep' ? rand(12000, 20000) : tired ? rand(2600, 5000) : rand(3500, 8500))
}

// ── Idle quirks: while calm (utils/mascot.ts) ──
const quirk = ref<Quirk | null>(null)
const side = ref<'l' | 'r'>('r')
let lastQuirk: Quirk | null = null
let queued: Quirk | null = null
const flyEl = ref<HTMLElement | null>(null)
let flight: Animation | null = null
/** The firefly's way, in fractions of Bob's box: (x, y, at). It lands on his head for a moment. */
const FLIGHT: [number, number, number][] = [
  [-0.12, 0.3, 0],
  [0.14, 0.08, 0.16],
  [0.84, 0.18, 0.34],
  [0.96, 0.56, 0.48],
  [0.7, 0.04, 0.6],
  [0.55, 0.15, 0.7],
  [0.55, 0.15, 0.84],
  [1.15, -0.16, 1],
]
function runQuirk(q: Quirk, forced = false) {
  if (quirk.value) {
    if (forced) queued = q
    return
  }
  if (cheering.value || dizzy.value || petted.value || props.carried !== 'no') return
  lastQuirk = q
  const dir = Math.random() < 0.5 ? -1 : 1
  side.value = dir < 0 ? 'l' : 'r'
  // ^^ and a hop: the cheer face, without a word.
  if (q === 'happy') return cheerFor(QUIRKS.happy.ms)
  quirk.value = q
  const look = (x: number, y: number, at: number) => later(() => (forced || !following()) && (gaze.value = { x, y }), at)
  if (q === 'look') {
    look(dir * -0.055, 0.005, 0)
    look(dir * 0.055, -0.01, 700)
    look(0, 0, 1450)
  } else if (q === 'roll') {
    look(-0.045, -0.03, 0)
    look(0, -0.055, 320)
    look(0.045, -0.03, 640)
    look(0, 0, 1050)
  } else if (q === 'tilt') {
    look(dir * 0.045, -0.03, 0)
    look(0, 0, QUIRKS.tilt.ms - 350)
  } else if (q === 'drowsy') {
    look(0, 0.03, 0)
    look(0, 0, QUIRKS.drowsy.ms - 500)
  } else if (q === 'love') {
    look(0, 0, 0)
  } else if (q === 'sidehop') {
    look(dir * 0.05, 0, 0)
    look(-dir * 0.05, 0, 700)
    look(0, 0, 1300)
  } else if (q === 'firefly') {
    const ms = QUIRKS.firefly.ms
    for (const [x, y, at] of FLIGHT) {
      const gx = Math.max(-0.06, Math.min(0.06, (x - 0.5) * 0.14))
      const gy = Math.max(-0.06, Math.min(0.05, (y - 0.46) * 0.16))
      look(gx, gy, at * ms)
    }
    look(0, 0, ms + 200)
    void nextTick(() => {
      const el = flyEl.value
      const box = root.value?.getBoundingClientRect()
      if (!el || !box) return
      flight = el.animate(
        FLIGHT.map(([x, y, at]) => ({ transform: `translate(${x * box.width}px, ${y * box.height}px)`, offset: at })),
        { duration: ms, easing: 'ease-in-out', fill: 'both' },
      )
    })
  }
  later(() => {
    if (quirk.value !== q) return
    quirk.value = null
    flight = null
    if (queued) {
      const nextQ = queued
      queued = null
      later(() => runQuirk(nextQ, true), 120)
    }
  }, QUIRKS[q].ms)
}
function scheduleQuirk() {
  later(() => {
    if (!reduced.value && mood.value === 'calm' && !glee.value && !boing && props.carried === 'no') runQuirk(pickQuirk(lastQuirk, Date.now() - lastPointerAt, Math.random, motion.value))
    scheduleQuirk()
  }, quirkPause(Math.random, motion.value))
}
/** Play a quirk now (Settings' tricks), after the one in progress. */
function play(q: Quirk) {
  if (reduced.value) return
  runQuirk(q, true)
}
defineExpose({ play })

// ── Hover, click, petting, carrying ──
const hover = ref(false)
const lineSeed = ref(Math.floor(Math.random() * 100))
/** ^^ for a moment after a click — the eyes only, no hop. */
const glee = ref(false)
/** Clicked too often too fast: spinning eyes, a woozy sway. */
const dizzy = ref(false)
/** Stroked with the pointer: happy eyes and a contented sway. */
const petted = ref(false)
const sparkle = ref(false)
let clicks: number[] = []
let strokes: { t: number; x: number }[] = []
let boing: Animation | null = null

/** A squash-and-stretch on the body (WAAPI, composite add): ends exactly where it started. */
function squash(frames: Keyframe[], duration: number) {
  const body = root.value?.querySelector<HTMLElement>('.qp-m-body')
  if (!body || reduced.value || boing) return
  boing = body.animate(frames, { duration, composite: 'add' })
  const done = () => (boing = null)
  boing.onfinish = done
  boing.oncancel = done
}
const BOING: Keyframe[] = [
  { transform: 'translateY(0) scale(1, 1)', easing: 'ease-out' },
  { transform: 'translateY(7%) scale(1.17, 0.83)', offset: 0.18, easing: 'ease-in-out' },
  { transform: 'translateY(-6%) scale(0.92, 1.1)', offset: 0.46, easing: 'ease-in-out' },
  { transform: 'translateY(1.5%) scale(1.04, 0.97)', offset: 0.72, easing: 'ease-out' },
  { transform: 'translateY(0) scale(1, 1)' },
]
const LANDING: Keyframe[] = [
  { transform: 'translateY(0) scale(1, 1)', easing: 'ease-out' },
  { transform: 'translateY(9%) scale(1.22, 0.8)', offset: 0.22, easing: 'ease-in-out' },
  { transform: 'translateY(-3%) scale(0.96, 1.05)', offset: 0.55, easing: 'ease-out' },
  { transform: 'translateY(0) scale(1, 1)' },
]

/**
 * A click: one boing that ends where it started, never twice at once. Four
 * quick clicks make him dizzy. A quirk in progress keeps going; otherwise the
 * eyes go ^^ for a moment.
 */
function poke() {
  if (!props.interactive) return
  lineSeed.value++
  lastPointerAt = Date.now()
  if (napping.value) wake()
  const t = Date.now()
  clicks = [...clicks.filter((c) => t - c < 2000), t]
  if (!dizzy.value && dizzyFrom(clicks, t)) {
    clicks = []
    glee.value = false
    dizzy.value = true
    later(() => (dizzy.value = false), 1900)
    return
  }
  squash(BOING, 720)
  if (quirk.value || cheering.value || glee.value || dizzy.value) return
  if (mood.value === 'calm' || mood.value === 'sleep') {
    glee.value = true
    later(() => (glee.value = false), 1200)
  }
}

/** The pointer moving over him: back-and-forth strokes are petting. */
function onStroke(e: PointerEvent) {
  if (!props.interactive || petted.value || dizzy.value || props.carried !== 'no' || e.buttons) return
  const t = Date.now()
  strokes = [...strokes.filter((s) => t - s.t < 1000), { t, x: e.clientX }]
  if (strokes.length >= 6 && isPetting(strokes.map((s) => s.x))) {
    strokes = []
    glee.value = false
    petted.value = true
    later(() => (petted.value = false), 1700)
  }
}

// Carried as the companion: let go, he lands with a squash.
watch(
  () => props.carried,
  (now, was) => {
    if (now === 'no' && was !== 'no') {
      lastPointerAt = Date.now()
      squash(LANDING, 520)
    }
  }
)

// A cheer gets a small sparkle burst; a wake-up looks around.
watch(mood, (m, was) => {
  if (m === 'cheer' && was !== 'cheer' && !reduced.value) {
    sparkle.value = true
    later(() => (sparkle.value = false), 950)
  }
  if (was === 'sleep' && m !== 'sleep') gaze.value = { x: rand(-0.03, 0.03), y: -0.02 }
  if (m === 'call') gaze.value = { x: 0, y: 0 }
})

function start() {
  if (!live.value) return
  clearTimers()
  scheduleGlance()
  scheduleBlink()
  scheduleQuirk()
  scheduleNapCheck()
  if (props.interactive) {
    window.addEventListener('pointermove', onPointer, { passive: true })
    document.addEventListener('input', onType, true)
  }
  syncLong()
}
function stop() {
  clearTimers()
  quirk.value = null
  queued = null
  glee.value = false
  dizzy.value = false
  petted.value = false
  sparkle.value = false
  boing?.cancel()
  flight?.cancel()
  flight = null
  window.removeEventListener('pointermove', onPointer)
  document.removeEventListener('input', onType, true)
  if (frame) cancelAnimationFrame(frame)
  frame = 0
  if (longTimer) clearInterval(longTimer)
  longTimer = null
}
watch(live, (v) => (v ? start() : stop()))
watch(() => [props.state, props.startedAt], syncLong)
watch(
  () => props.state,
  (s) => {
    if (s !== 'idle') napping.value = false
  }
)

onMounted(() => {
  active.value = inActiveKeepAliveTree()
  document.addEventListener('visibilitychange', onVisibility)
  start()
})
onActivated(() => (active.value = true))
onDeactivated(() => (active.value = false))
onBeforeUnmount(() => {
  stop()
  document.removeEventListener('visibilitychange', onVisibility)
})

// ── Look ──
const skin = computed(() => SKINS.find((s) => s.id === prefs.value.skin) ?? SKINS[0]!)
const vars = computed(() => {
  const tint = prefs.value.agentTint ? agentTint(props.agent) : null
  const base = tint ? `color-mix(in srgb, ${tint} 70%, ${skin.value.base})` : skin.value.base
  return {
    '--m-base': base,
    '--m-light': tint ? `color-mix(in srgb, ${tint} 45%, ${skin.value.light})` : skin.value.light,
    '--m-ink': skin.value.ink,
    '--m-gx': `${gaze.value.x * 100}%`,
    '--m-gy': `${gaze.value.y * 100}%`,
  }
})
const classes = computed(() => [
  `m-${mood.value}`,
  `eyes-${prefs.value.eyes}`,
  `skin-${prefs.value.skin}`,
  quirk.value ? `q-${quirk.value} side-${side.value}` : '',
  props.carried !== 'no' ? `carried-${props.carried}` : '',
  { hover: hover.value && props.interactive, glee: glee.value || petted.value, dizzy: dizzy.value, petted: petted.value, blink: blink.value, still: reduced.value },
])
const name = computed(() => prefs.value.name || MASCOT_NAME)
const label = computed(() => lineFor(mood.value, prefs.value.name, lineSeed.value, new Date().getHours()))
</script>

<template>
  <div
    v-if="prefs.on || $props.mood !== null"
    ref="root"
    class="qp-mascot"
    :class="classes"
    :data-own-motion="reduced ? undefined : ''"
    :style="vars"
    role="img"
    :aria-label="`${name} — ${MOOD_LABEL[mood]}`"
    :title="label"
    @pointerenter="hover = true"
    @pointerleave="hover = false"
    @pointermove="onStroke"
    @click="poke"
  >
    <div class="qp-m-bob">
      <div class="qp-m-halo" />
      <div class="qp-m-body">
        <div class="qp-m-brow l" /><div class="qp-m-brow r" />
        <div class="qp-m-eyes">
          <i class="qp-m-eye l"><b /><s /></i>
          <i class="qp-m-eye r"><b /><s /></i>
        </div>
        <!-- accessories ride on the body -->
        <svg v-if="prefs.accessory !== 'none'" class="qp-m-acc" viewBox="0 0 100 100" aria-hidden="true">
          <g v-if="prefs.accessory === 'cap'">
            <path d="M14 18 Q50 -30 86 18 Q50 8 14 18 Z" class="acc-fill" />
            <path d="M12 18 Q50 6 88 18" class="acc-band" />
            <circle cx="50" cy="-13" r="6" class="acc-fill" />
          </g>
          <g v-else-if="prefs.accessory === 'beanie'">
            <path d="M9 24 Q50 -38 91 24 Z" class="acc-beanie" />
            <path d="M8 24 Q50 12 92 24" class="acc-beanie-band" />
            <circle cx="50" cy="-15" r="8" class="acc-beanie-pom" />
          </g>
          <g v-else-if="prefs.accessory === 'crown'">
            <path d="M24 14 L27 -14 L39 2 L50 -20 L61 2 L73 -14 L76 14 Q50 8 24 14 Z" class="acc-gold" />
            <circle cx="50" cy="5" r="3.4" class="acc-gem" />
            <circle cx="35" cy="8" r="2.4" class="acc-gem b" />
            <circle cx="65" cy="8" r="2.4" class="acc-gem b" />
          </g>
          <g v-else-if="prefs.accessory === 'party'">
            <path d="M40 9 L57 -38 L72 6 Q56 12 40 9 Z" class="acc-party" />
            <path d="M47 -10 L65 -14 M43 0 L69 -4" class="acc-party-stripe" />
            <circle cx="57" cy="-40" r="5" class="acc-party-pom" />
          </g>
          <g v-else-if="prefs.accessory === 'sprout'">
            <path d="M50 6 Q49 -8 52 -18" class="acc-stem" />
            <path d="M52 -17 Q64 -33 76 -17 Q63 -10 52 -17 Z" class="acc-leaf" />
            <path d="M50 -9 Q38 -24 27 -11 Q40 -3 50 -9 Z" class="acc-leaf b" />
          </g>
          <g v-else-if="prefs.accessory === 'glasses'" class="acc-line">
            <circle cx="33" cy="46" r="12" />
            <circle cx="67" cy="46" r="12" />
            <path d="M45 46 Q50 42 55 46 M21 44 L6 40 M79 44 L94 40" />
          </g>
          <g v-else-if="prefs.accessory === 'headset'">
            <path d="M6 54 Q4 -6 50 -2 Q96 -6 94 54" class="acc-line thick" />
            <rect x="-2" y="42" width="12" height="22" rx="5" class="acc-dark" />
            <rect x="90" y="42" width="12" height="22" rx="5" class="acc-dark" />
            <path d="M96 62 Q92 84 66 78" class="acc-line" />
            <circle cx="64" cy="78" r="4" class="acc-dark" />
          </g>
          <g v-else-if="prefs.accessory === 'bow'" class="acc-fill">
            <path d="M70 6 L56 -4 L58 16 Z M70 6 L84 -4 L82 16 Z" />
            <circle cx="70" cy="6" r="5" />
          </g>
        </svg>
      </div>
      <div v-if="mood === 'call'" class="qp-m-hand" />
      <div v-if="mood === 'call'" class="qp-m-alert">!</div>
      <div v-if="mood === 'think'" class="qp-m-thought"><i /><i /><i /></div>
      <div v-if="mood === 'juggle'" class="qp-m-juggle"><i /><i /><i /></div>
      <div v-if="mood === 'tired'" class="qp-m-sweat" />
      <div v-if="mood === 'sleep'" class="qp-m-zzz">z<span>z</span></div>
    </div>
    <div v-if="sparkle" class="qp-m-spark"><i v-for="n in 6" :key="n" :style="{ '--a': `${n * 60 - 30}deg` }" /></div>
    <i v-if="quirk === 'firefly'" ref="flyEl" class="qp-m-fly" />
  </div>
</template>

<style scoped>
.qp-mascot {
  --m-base: #2a2a31;
  --m-light: #4a4a55;
  --m-ink: #e4e4e7;
  --m-ring: color-mix(in srgb, var(--m-light) 55%, transparent);
  --m-glow: var(--m-light);
  --m-spring: cubic-bezier(0.34, 1.45, 0.64, 1);
  position: relative;
  width: 100%;
  aspect-ratio: 1;
  container-type: inline-size;
  user-select: none;
  cursor: default;
}
.skin-night { --m-ring: color-mix(in srgb, var(--m-ink) 66%, transparent); --m-glow: var(--m-ink); }
.skin-ghost { --m-ring: color-mix(in srgb, #ffffff 70%, transparent); --m-glow: #ffffff; }

/* ── Bob, body: a flat glyph with a soft outline and a faint glow ── */
.qp-m-bob { position: absolute; inset: 0; animation: qp-m-breathe 4.2s ease-in-out infinite; transform-origin: 50% 80%; }
.qp-m-body {
  position: absolute;
  left: 20%;
  top: 18%;
  width: 60%;
  height: 60%;
  border-radius: 50%;
  background: radial-gradient(circle at 50% 22%, color-mix(in srgb, var(--m-light) 30%, var(--m-base)), var(--m-base) 70%);
  box-shadow: 0 0 0 1.1cqw var(--m-ring);
  transition: transform 0.35s var(--m-spring), box-shadow 0.4s ease;
}
.qp-m-halo {
  position: absolute;
  left: 13%;
  top: 11%;
  width: 74%;
  height: 74%;
  border-radius: 50%;
  background: radial-gradient(circle, color-mix(in srgb, var(--m-glow) 22%, transparent) 55%, transparent 72%);
  opacity: 0.4;
  transition: opacity 0.5s ease;
}

/* ── Face parts (in the body's own percentages) ── */
.qp-m-eyes { position: absolute; inset: 0; transform: translate(var(--m-gx), var(--m-gy)); transition: transform 0.55s var(--m-spring), top 0.6s ease, bottom 0.6s ease; }
.qp-m-eye {
  position: absolute;
  top: 39%;
  width: 15%;
  height: 15%;
  border-radius: 50%;
  background: var(--m-ink);
  transform-origin: center;
  transition: transform 0.16s ease, border-radius 0.25s ease, opacity 0.2s ease;
}
.qp-m-eye.l { left: 25.5%; }
.qp-m-eye.r { left: 59.5%; }
.qp-m-eye b,
.qp-m-eye s { position: absolute; border-radius: 50%; background: #fff; opacity: 0; text-decoration: none; }
.qp-m-eye b { right: 16%; top: 14%; width: 32%; height: 32%; }
.qp-m-eye s { left: 22%; bottom: 18%; width: 18%; height: 18%; }
.eyes-round .qp-m-eye { top: 36%; width: 20%; height: 20%; background: #f4f4f5; box-shadow: inset 0 -0.6cqw 0 rgba(0, 0, 0, 0.12); }
.eyes-round .qp-m-eye.l { left: 23%; }
.eyes-round .qp-m-eye.r { left: 57%; }
.eyes-round .qp-m-eye b { right: 24%; top: 24%; width: 52%; height: 52%; background: #18181e; opacity: 1; }
.eyes-sparkle .qp-m-eye b,
.eyes-sparkle .qp-m-eye s { opacity: 0.95; }
.skin-night.eyes-sparkle .qp-m-eye b,
.skin-night.eyes-sparkle .qp-m-eye s { background: #18181e; opacity: 0.6; }
.qp-m-brow {
  position: absolute;
  top: 27%;
  width: 18%;
  height: 4%;
  border-radius: 2cqw;
  background: var(--m-ink);
  opacity: 0;
  transition: opacity 0.25s ease, transform 0.35s var(--m-spring);
}
.qp-m-brow.l { left: 24%; }
.qp-m-brow.r { left: 58%; }

/* ── Accessories ── */
.qp-m-acc { position: absolute; inset: 0; overflow: visible; pointer-events: none; }
.acc-fill { fill: #f87171; }
.acc-band { fill: none; stroke: #b91c1c; stroke-width: 5; stroke-linecap: round; }
.acc-line { fill: none; stroke: var(--m-ink); stroke-width: 3; stroke-linecap: round; }
.acc-line.thick { stroke-width: 6; stroke: #3f3f46; }
.acc-dark { fill: #3f3f46; }
.skin-night .acc-line.thick { stroke: #71717a; }
.skin-night .acc-dark { fill: #71717a; }
.acc-beanie { fill: #60a5fa; }
.acc-beanie-band { fill: none; stroke: #2563eb; stroke-width: 7; stroke-linecap: round; }
.acc-beanie-pom { fill: #f4f4f5; }
.acc-gold { fill: #f5c542; stroke: #b88a12; stroke-width: 1.6; stroke-linejoin: round; }
.acc-gem { fill: #ef4444; }
.acc-gem.b { fill: #38bdf8; }
.acc-party { fill: #a78bfa; }
.acc-party-stripe { fill: none; stroke: #fde047; stroke-width: 3.5; stroke-linecap: round; }
.acc-party-pom { fill: #fde047; }
.acc-stem { fill: none; stroke: #16a34a; stroke-width: 3.2; stroke-linecap: round; }
.acc-leaf { fill: #4ade80; }
.acc-leaf.b { fill: #22c55e; }

/* ── Moods ── */
.m-sleep .qp-m-eye { transform: scaleY(0.12); opacity: 0.7; }
.m-sleep .qp-m-body { transform: scaleY(0.95) translateY(2%); }
.m-sleep .qp-m-bob { animation: qp-m-sleepsway 6.5s ease-in-out infinite; }
.m-think .qp-m-brow { opacity: 0.8; }
.m-think .qp-m-brow.l { transform: translateY(-20%) rotate(-8deg); }
.m-think .qp-m-brow.r { transform: translateY(-45%) rotate(10deg); }
.m-think .qp-m-halo,
.m-focus .qp-m-halo,
.m-juggle .qp-m-halo { opacity: 0.6; }
.m-focus .qp-m-brow,
.m-juggle .qp-m-brow { opacity: 0.55; }
.m-focus .qp-m-brow.l { transform: translateY(30%) rotate(4deg); }
.m-focus .qp-m-brow.r { transform: translateY(30%) rotate(-4deg); }
.m-focus .qp-m-bob,
.m-juggle .qp-m-bob { animation-duration: 2.2s; }
/* Tired: the eyes sit lower and blink slowly — never half shut. */
.m-tired .qp-m-eyes { top: 6%; bottom: -6%; }
.m-tired .qp-m-brow { opacity: 0.6; }
.m-tired .qp-m-brow.l { transform: translateY(70%) rotate(-12deg); }
.m-tired .qp-m-brow.r { transform: translateY(70%) rotate(12deg); }
.m-tired .qp-m-body { transform: scaleY(0.96) translateY(2%); }
.m-tired .qp-m-bob { animation-duration: 6s; }
.m-call .qp-m-eye { transform: scale(1.18); }
.m-call .qp-m-brow { opacity: 1; background: var(--qss-warning, #d29a3f); }
.m-call .qp-m-brow.l,
.m-call .qp-m-brow.r { transform: translateY(-35%); }
.m-call .qp-m-body { box-shadow: 0 0 0 1.1cqw var(--qss-warning, #d29a3f); }
.m-call .qp-m-halo { opacity: 1; background: radial-gradient(circle, color-mix(in srgb, var(--qss-warning, #d29a3f) 35%, transparent) 55%, transparent 72%); animation: qp-m-pulse 1.6s ease-in-out infinite; }
.m-call .qp-m-bob { animation: qp-m-attention 2.6s ease-in-out infinite; }
.m-worried .qp-m-eye { transform: scale(0.9); }
.m-worried .qp-m-brow { opacity: 1; background: var(--qss-error, #f87171); }
.m-worried .qp-m-brow.l { transform: translateY(-20%) rotate(-16deg); }
.m-worried .qp-m-brow.r { transform: translateY(-20%) rotate(16deg); }
.m-worried .qp-m-body { box-shadow: 0 0 0 1.1cqw var(--qss-error, #f87171); animation: qp-m-shiver 0.5s ease-in-out 2; }
/* ^^ — cheering, nodding to an approval, a click, petting. */
.m-cheer .qp-m-eye,
.m-nod .qp-m-eye,
.qp-mascot.glee .qp-m-eye { background: transparent; box-shadow: none; border-top: 1.7cqw solid var(--m-ink); border-radius: 50%; transform: translateY(14%) scaleX(1.15); }
.m-cheer .qp-m-eye b,
.m-cheer .qp-m-eye s,
.m-nod .qp-m-eye b,
.m-nod .qp-m-eye s,
.qp-mascot.glee .qp-m-eye b,
.qp-mascot.glee .qp-m-eye s { opacity: 0; }
/* One hop per cheer: the cheer always outlasts it, so it never stops mid-air. */
.m-cheer .qp-m-bob { animation: qp-m-jump 0.9s var(--m-spring) 1; }
.m-nod .qp-m-bob { animation: qp-m-nod 0.5s ease-in-out 2; }
.hover .qp-m-body { transform: scale(1.05); }
.hover .qp-m-eye { transform: scale(1.08); }

/* ── Extras ── */
.qp-m-hand {
  position: absolute;
  right: 8%;
  top: 46%;
  width: 14%;
  height: 14%;
  border-radius: 50%;
  background: var(--m-base);
  box-shadow: 0 0 0 0.9cqw var(--m-ring);
  transform-origin: -60% 60%;
  animation: qp-m-wave 0.9s ease-in-out infinite;
}
.qp-m-alert {
  position: absolute;
  right: 12%;
  top: 2%;
  width: 17%;
  height: 17%;
  border-radius: 50%;
  display: grid;
  place-items: center;
  font: 800 11cqw/1 system-ui, sans-serif;
  color: #1a1206;
  background: var(--qss-warning, #d29a3f);
  animation: qp-m-popin 0.45s var(--m-spring) both, qp-m-alert 1.3s ease-in-out 0.45s infinite;
}
.qp-m-thought { position: absolute; right: 9%; top: 3%; width: 26%; height: 22%; }
.qp-m-thought i { position: absolute; border-radius: 50%; background: color-mix(in srgb, var(--m-glow) 75%, transparent); opacity: 0.25; animation: qp-m-think 1.5s ease-in-out infinite; }
.qp-m-thought i:nth-child(1) { left: 0; bottom: 0; width: 18%; height: 22%; }
.qp-m-thought i:nth-child(2) { left: 30%; bottom: 30%; width: 26%; height: 30%; animation-delay: 0.2s; }
.qp-m-thought i:nth-child(3) { right: 0; top: 0; width: 38%; height: 44%; animation-delay: 0.4s; }
.qp-m-juggle { position: absolute; left: 30%; top: -6%; width: 40%; height: 40%; animation: qp-m-orbit 1.6s linear infinite; }
.qp-m-juggle i { position: absolute; width: 22%; height: 22%; border-radius: 50%; }
.qp-m-juggle i:nth-child(1) { left: 39%; top: 0; background: #34d399; }
.qp-m-juggle i:nth-child(2) { left: 0; top: 60%; background: #7aa2f7; }
.qp-m-juggle i:nth-child(3) { right: 0; top: 60%; background: #f472b6; }
.qp-m-sweat { position: absolute; right: 22%; top: 22%; width: 7%; height: 10%; border-radius: 50% 50% 50% 50% / 65% 65% 35% 35%; background: #7dd3fc; opacity: 0.9; animation: qp-m-drip 2.6s ease-in infinite; }
.qp-m-zzz { position: absolute; right: 8%; top: 4%; font-size: 13cqw; font-weight: 700; color: var(--qss-text-muted, #8a8a95); animation: qp-m-float 3.2s ease-in-out infinite; }
.qp-m-zzz span { font-size: 70%; margin-left: 2cqw; }
.qp-m-spark { position: absolute; left: 50%; top: 48%; width: 0; height: 0; pointer-events: none; }
.qp-m-spark i {
  position: absolute;
  left: -4cqw;
  top: -4cqw;
  width: 8cqw;
  height: 8cqw;
  background: var(--m-glow);
  clip-path: polygon(50% 0, 62% 38%, 100% 50%, 62% 62%, 50% 100%, 38% 62%, 0 50%, 38% 38%);
  opacity: 0;
  animation: qp-m-spark 0.9s ease-out both;
}
.qp-m-fly {
  position: absolute;
  left: -2cqw;
  top: -2cqw;
  width: 4cqw;
  height: 4cqw;
  border-radius: 50%;
  background: #f4f7a1;
  box-shadow: 0 0 3cqw 1cqw rgba(236, 252, 120, 0.55);
  pointer-events: none;
  animation: qp-m-glowfly 0.7s ease-in-out infinite alternate;
}

/* ── Idle quirks ── */
.side-l { --q-dir: -1; }
.side-r { --q-dir: 1; }
.q-tilt .qp-m-bob { animation: qp-q-tilt 1.9s var(--m-spring) both; }
.q-tilt .qp-m-eye { animation: qp-q-wide 1.9s ease both; }
.q-wink.side-l .qp-m-eye.l,
.q-wink.side-r .qp-m-eye.r { animation: qp-q-wink 0.7s ease both; }
.q-wink .qp-m-bob { animation: qp-q-lean 0.7s ease both; }
.q-hop .qp-m-bob { animation: qp-m-jump 0.9s var(--m-spring) 1; }
.q-wiggle .qp-m-bob { animation: qp-q-wiggle 1.1s ease-in-out both; }
.q-pop .qp-m-eye { animation: qp-q-pop 0.8s var(--m-spring) both; }
.q-pop .qp-m-bob { animation: qp-q-perk 0.8s var(--m-spring) both; }
.q-look .qp-m-bob { animation: qp-q-sway 1.9s ease-in-out both; }
.q-roll .qp-m-bob { animation: qp-q-roll 1.5s ease-in-out both; }
.q-stretch .qp-m-bob { animation: qp-q-stretch 1.7s ease-in-out both; }
.q-stretch .qp-m-eye { animation: qp-q-shut 1.7s ease both; }
.q-drowsy .qp-m-eye { animation: qp-q-drowsy 3.2s ease both; }
.q-drowsy .qp-m-bob { animation: qp-q-nodoff 3.2s ease both; }
.q-love .qp-m-eye { animation: qp-q-love 1.6s ease-in-out both; }
.q-love .qp-m-bob { animation: qp-q-lean 1.6s ease-in-out both; }
.q-spin .qp-m-bob { animation: qp-q-spin 1.1s cubic-bezier(0.45, 0, 0.25, 1) both; }
.q-sidehop .qp-m-bob { animation: qp-q-sidehop 1.5s ease-in-out both; }
.q-shake .qp-m-bob { animation: qp-q-shake 0.9s ease-in-out both; }
.q-hiccup .qp-m-bob { animation: qp-q-hiccup 1.1s ease-out both; }
.q-hiccup .qp-m-eye { animation: qp-q-hiccupeyes 1.1s ease-out both; }
.q-firefly .qp-m-bob { animation: qp-q-firefly 4.2s ease-in-out both; }
/* Giggle, and carried fast: > < eyes. */
.qp-mascot.q-giggle .qp-m-eye,
.qp-mascot.carried-fast .qp-m-eye { background: transparent; box-shadow: none; border-radius: 0.5cqw; }
.qp-mascot.q-giggle .qp-m-eye.l,
.qp-mascot.carried-fast .qp-m-eye.l { border-top: 1.7cqw solid var(--m-ink); border-right: 1.7cqw solid var(--m-ink); transform: translateX(10%) rotate(45deg) scale(0.62); }
.qp-mascot.q-giggle .qp-m-eye.r,
.qp-mascot.carried-fast .qp-m-eye.r { border-bottom: 1.7cqw solid var(--m-ink); border-left: 1.7cqw solid var(--m-ink); transform: translateX(-10%) rotate(45deg) scale(0.62); }
.qp-mascot.q-giggle .qp-m-eye b,
.qp-mascot.q-giggle .qp-m-eye s,
.qp-mascot.carried-fast .qp-m-eye b,
.qp-mascot.carried-fast .qp-m-eye s { opacity: 0; }
.q-giggle .qp-m-bob { animation: qp-q-giggle 1.3s ease-in-out both; }
/* Carried: wide-eyed. */
.carried-held .qp-m-eye { transform: scale(1.2); }
/* Dizzy: spinning eyes, a woozy sway. */
.qp-mascot.dizzy .qp-m-eye { background: transparent; box-shadow: none; border: 1.3cqw solid transparent; border-top-color: var(--m-ink); border-right-color: var(--m-ink); border-radius: 50%; animation: qp-q-whirl 0.55s linear infinite; }
.qp-mascot.dizzy .qp-m-eye.r { animation-direction: reverse; }
.qp-mascot.dizzy .qp-m-eye b,
.qp-mascot.dizzy .qp-m-eye s { opacity: 0; }
.dizzy .qp-m-bob { animation: qp-q-woozy 1.9s ease-in-out both; }
/* Petted: happy eyes and a contented sway. */
.petted .qp-m-bob { animation: qp-q-purr 1.7s ease-in-out both; }

@keyframes qp-m-breathe { 0%, 100% { transform: scale(1, 1); } 50% { transform: scale(1.025, 0.975); } }
@keyframes qp-m-sleepsway { 0%, 100% { transform: rotate(-2deg) scale(1, 1); } 50% { transform: rotate(2deg) scale(1.03, 0.97); } }
@keyframes qp-m-attention { 0%, 40%, 100% { transform: translateY(0) scale(1, 1); } 8% { transform: translateY(0) scale(1.06, 0.94); } 18% { transform: translateY(-9%) scale(0.96, 1.05); } 30% { transform: translateY(0) scale(1.04, 0.96); } }
@keyframes qp-m-jump { 0% { transform: translateY(0) scale(1, 1); } 25% { transform: translateY(0) scale(1.1, 0.88); } 55% { transform: translateY(-12%) scale(0.94, 1.08); } 80% { transform: translateY(0) scale(1.06, 0.94); } 100% { transform: translateY(0) scale(1, 1); } }
@keyframes qp-m-nod { 0%, 100% { transform: translateY(0); } 50% { transform: translateY(5%) scale(1.02, 0.97); } }
@keyframes qp-m-pulse { 0%, 100% { transform: scale(1); opacity: 0.55; } 50% { transform: scale(1.07); opacity: 0.9; } }
@keyframes qp-m-shiver { 0%, 100% { transform: translateX(0); } 25% { transform: translateX(-2%); } 75% { transform: translateX(2%); } }
@keyframes qp-m-wave { 0%, 100% { transform: rotate(-12deg); } 50% { transform: rotate(22deg); } }
@keyframes qp-m-orbit { to { transform: rotate(360deg); } }
@keyframes qp-m-drip { 0%, 30% { transform: translateY(0); opacity: 0; } 40% { opacity: 0.9; } 100% { transform: translateY(160%); opacity: 0; } }
@keyframes qp-m-float { 0%, 100% { transform: translateY(0); opacity: 0.5; } 50% { transform: translateY(-14%); opacity: 1; } }
@keyframes qp-m-popin { from { transform: scale(0); } to { transform: scale(1); } }
@keyframes qp-m-alert { 0%, 60%, 100% { transform: translateY(0) rotate(0); } 20% { transform: translateY(-14%) rotate(-8deg); } 40% { transform: translateY(0) rotate(6deg); } }
@keyframes qp-m-think { 0%, 100% { opacity: 0.25; transform: scale(0.85); } 40% { opacity: 0.85; transform: scale(1); } }
@keyframes qp-m-spark { 0% { opacity: 0; transform: rotate(var(--a)) translateY(0) scale(0.4); } 25% { opacity: 1; } 100% { opacity: 0; transform: rotate(var(--a)) translateY(-34cqw) scale(1); } }
@keyframes qp-m-glowfly { from { opacity: 0.55; } to { opacity: 1; } }

@keyframes qp-q-tilt { 0% { transform: rotate(0); } 25%, 75% { transform: rotate(calc(var(--q-dir, 1) * 13deg)) translateY(-2%); } 100% { transform: rotate(0); } }
@keyframes qp-q-wide { 0%, 100% { transform: scale(1); } 25%, 75% { transform: scale(1.14); } }
@keyframes qp-q-wink { 0%, 100% { transform: scaleY(1); } 25%, 70% { transform: scaleY(0.1); } }
@keyframes qp-q-lean { 0%, 100% { transform: rotate(0); } 30%, 70% { transform: rotate(calc(var(--q-dir, 1) * 7deg)); } }
@keyframes qp-q-wiggle { 0%, 100% { transform: rotate(0); } 15% { transform: rotate(-9deg); } 35% { transform: rotate(9deg); } 55% { transform: rotate(-6deg); } 75% { transform: rotate(4deg); } }
@keyframes qp-q-pop { 0%, 100% { transform: scale(1); } 30% { transform: scale(1.42); } 60% { transform: scale(1.22); } }
@keyframes qp-q-perk { 0%, 100% { transform: translateY(0) scale(1, 1); } 30% { transform: translateY(-5%) scale(0.96, 1.05); } }
@keyframes qp-q-sway { 0%, 100% { transform: rotate(0); } 20%, 35% { transform: rotate(calc(var(--q-dir, 1) * -5deg)); } 55%, 72% { transform: rotate(calc(var(--q-dir, 1) * 5deg)); } }
@keyframes qp-q-roll { 0%, 100% { transform: rotate(0); } 25% { transform: rotate(-5deg); } 50% { transform: rotate(0) translateY(-3%); } 75% { transform: rotate(5deg); } }
@keyframes qp-q-stretch { 0%, 100% { transform: scale(1, 1); } 35%, 55% { transform: translateY(-5%) scale(0.92, 1.12); } 75% { transform: scale(1.07, 0.93); } 88% { transform: scale(0.98, 1.02); } }
@keyframes qp-q-shut { 0%, 100% { transform: scaleY(1); } 22%, 66% { transform: scaleY(0.1); } }
/* One slow, complete close — never held half shut. */
@keyframes qp-q-drowsy { 0% { transform: scaleY(1); } 50%, 68% { transform: scaleY(0.1); } 76% { transform: scale(1.3); } 88%, 100% { transform: scale(1); } }
@keyframes qp-q-nodoff { 0%, 100% { transform: translateY(0) rotate(0); } 60%, 70% { transform: translateY(5%) rotate(-4deg); } 76% { transform: translateY(-4%) rotate(0); } 88% { transform: translateY(0); } }
@keyframes qp-q-love { 0%, 100% { transform: scaleY(1); } 32%, 58% { transform: scaleY(0.1); } }
@keyframes qp-q-spin { 0% { transform: translateY(0) rotate(0); } 20% { transform: translateY(0) scale(1.06, 0.94); } 50% { transform: translateY(-12%) rotate(calc(var(--q-dir, 1) * 180deg)); } 80% { transform: translateY(0) rotate(calc(var(--q-dir, 1) * 360deg)) scale(1.05, 0.95); } 100% { transform: translateY(0) rotate(calc(var(--q-dir, 1) * 360deg)); } }
@keyframes qp-q-sidehop {
  0%, 100% { transform: translate(0, 0) scale(1, 1); }
  10% { transform: translate(0, 0) scale(1.06, 0.94); }
  25% { transform: translate(calc(var(--q-dir, 1) * 9%), -9%) scale(0.96, 1.05); }
  40% { transform: translate(calc(var(--q-dir, 1) * 18%), 0) scale(1.06, 0.94); }
  52% { transform: translate(calc(var(--q-dir, 1) * 18%), 0) scale(1, 1); }
  70% { transform: translate(calc(var(--q-dir, 1) * 9%), -9%) scale(0.96, 1.05); }
  85% { transform: translate(0, 0) scale(1.06, 0.94); }
}
@keyframes qp-q-shake { 0%, 100% { transform: rotate(0) scale(1, 1); } 10% { transform: rotate(-11deg) scale(1.05, 0.96); } 25% { transform: rotate(11deg) scale(1.05, 0.96); } 40% { transform: rotate(-9deg); } 55% { transform: rotate(8deg); } 70% { transform: rotate(-4deg); } 85% { transform: rotate(2deg); } }
@keyframes qp-q-hiccup { 0%, 100% { transform: translateY(0) scale(1, 1); } 12% { transform: translateY(-7%) scale(0.97, 1.03); } 24% { transform: translateY(0) scale(1.03, 0.97); } 56% { transform: translateY(0); } 66% { transform: translateY(-6%) scale(0.97, 1.03); } 78% { transform: translateY(0) scale(1.02, 0.98); } }
@keyframes qp-q-hiccupeyes { 0%, 100% { transform: scale(1); } 12% { transform: scale(1.25); } 30% { transform: scale(1); } 66% { transform: scale(1.22); } 82% { transform: scale(1); } }
@keyframes qp-q-firefly { 0%, 100% { transform: translateY(0); } 70%, 84% { transform: translateY(-2%) scale(1.02, 0.98); } }
@keyframes qp-q-giggle { 0%, 100% { transform: rotate(0) translateY(0); } 15% { transform: rotate(-4deg) translateY(-2%); } 30% { transform: rotate(4deg); } 45% { transform: rotate(-4deg) translateY(-2%); } 60% { transform: rotate(4deg); } 75% { transform: rotate(-2deg); } }
@keyframes qp-q-whirl { to { transform: rotate(360deg); } }
@keyframes qp-q-woozy { 0%, 100% { transform: rotate(0) translateX(0); } 20% { transform: rotate(-8deg) translateX(-3%); } 45% { transform: rotate(7deg) translateX(3%); } 70% { transform: rotate(-5deg) translateX(-2%); } 88% { transform: rotate(2deg); } }
@keyframes qp-q-purr { 0%, 100% { transform: rotate(0) scale(1, 1); } 25% { transform: rotate(-5deg) scale(1.03, 0.97); } 50% { transform: rotate(4deg) scale(1.03, 0.97); } 75% { transform: rotate(-3deg) scale(1.02, 0.98); } }

/* A blink closes the eyes whatever the mood. */
.qp-mascot.blink .qp-m-eye { transform: scaleY(0.1); }
.m-tired.blink .qp-m-eye { transition-duration: 0.26s; }
/* Closed round eyes are a line in the face's ink, not a white slit with a pupil. */
.eyes-round.m-sleep .qp-m-eye,
.eyes-round.blink .qp-m-eye { background: var(--m-ink); box-shadow: none; }
.eyes-round.m-sleep .qp-m-eye b,
.eyes-round.blink .qp-m-eye b { opacity: 0; }

/* Still: every expression, no motion and no quirks. */
.still .qp-m-bob,
.still .qp-m-eye,
.still .qp-m-halo,
.still .qp-m-hand,
.still .qp-m-alert,
.still .qp-m-thought i,
.still .qp-m-juggle,
.still .qp-m-sweat,
.still .qp-m-zzz,
.still .qp-m-fly,
.still .qp-m-body { animation: none !important; }
</style>
