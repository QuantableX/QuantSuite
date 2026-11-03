<script setup lang="ts">
/**
 * The pilot — still the blob from QuantControl's org chart, with more of
 * everything (round 2, card 05).
 *
 * Moods for what the agent does (utils/mascot.ts): sleeping, calm,
 * thinking, focused, juggling when several agents work, tired on a long
 * run, calling you (eyes on you, a wave), worried, cheering when a turn
 * finishes, nodding when an approval is granted. It speaks with its eyes
 * only — no mouth (the user found a smile on it uncanny) — on a flat dark
 * glyph with a soft outline, like the first face.
 * Movement: the eyes follow the pointer, otherwise glance about (each
 * glance a fresh spot and pause — a loop would read as a GIF); while calm it
 * does a quirk every few seconds (look around, roll its eyes, tilt, wink,
 * hop, wiggle, giggle > <, stretch, nod off after a long quiet spell, and an
 * "oh, hi" when the pointer comes back); hover perks it up, a click gives a
 * boing and a ^^ and it says something. The user's look: skin, eye style, an accessory,
 * a name, an agent tint.
 *
 * Built from plain elements: every motion is `transform`/`opacity`, so the
 * compositor runs it. Timers and the pointer listener live only while the
 * face is on screen (an active page, a visible document). Movement is the
 * mascot's own setting (Lively / Still), not the system's: Windows reports
 * reduced motion whenever animation effects are off, which silenced it on
 * the user's machine. Still keeps every expression and drops the continuous
 * motion and the quirks.
 */
import { computed, onActivated, onBeforeUnmount, onDeactivated, onMounted, ref, watch } from 'vue'
import { inActiveKeepAliveTree } from '@quantsuite/core'
import { useMascotStore } from '#pilot/stores/mascot'
import { agentTint, gazeToward, lineFor, moodOf, MOOD_LABEL, pickQuirk, QUIRKS, quirkPause, SKINS, type Mood, type Quirk } from '#pilot/utils/mascot'
import type { LiveState } from '#pilot/types'

const props = withDefaults(
  defineProps<{
    state: LiveState
    /** Bump whenever a turn finished: it cheers. */
    cheer?: number
    /** Bump when an approval was granted: it nods. */
    nod?: number
    /** Agents working anywhere: three or more and it juggles. */
    busy?: number
    /** When the running turn started: a long run tires it. */
    startedAt?: number | null
    /** The thread's agent, for the outfit tint. */
    agent?: string | null
    /** Show this mood regardless (Settings' gallery). */
    mood?: Mood | null
    /** Follow the pointer, react to hover and clicks. */
    interactive?: boolean
  }>(),
  { cheer: 0, nod: 0, busy: 0, startedAt: null, agent: null, mood: null, interactive: true }
)

const mascot = useMascotStore()
const prefs = computed(() => mascot.prefs)

// ── On screen? Timers and listeners only then ──
const root = ref<HTMLElement | null>(null)
const active = ref(false)
const pageVisible = ref(typeof document === 'undefined' ? true : !document.hidden)
const live = computed(() => active.value && pageVisible.value)
const onVisibility = () => (pageVisible.value = !document.hidden)
const reduced = computed(() => prefs.value.motion === 'still')

// ── Events: cheer, nod, click ──
const cheering = ref(false)
const nodding = ref(false)
/** ^^ for a moment after a click — the eyes only, no hop. */
const glee = ref(false)
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

const mood = computed<Mood>(
  () =>
    props.mood ??
    moodOf({
      state: props.state,
      busy: props.busy,
      elapsedMs: props.startedAt ? now.value - props.startedAt : 0,
      cheering: cheering.value,
      nodding: nodding.value,
    })
)

// ── Gaze: the pointer, else a glance now and then ──
const gaze = ref({ x: 0, y: 0 })
const blink = ref(false)
let followUntil = 0
let pendingPointer: { x: number; y: number } | null = null
let frame = 0
function onPointer(e: PointerEvent) {
  const t = Date.now()
  if (t - lastPointerAt > 15_000 && mood.value === 'calm' && !reduced.value) runQuirk('pop')
  lastPointerAt = t
  pendingPointer = { x: e.clientX, y: e.clientY }
  if (frame) return
  frame = requestAnimationFrame(() => {
    frame = 0
    const el = root.value
    const p = pendingPointer
    if (!el || !p || mood.value === 'sleep' || mood.value === 'cheer' || glee.value) return
    const r = el.getBoundingClientRect()
    gaze.value = gazeToward(p.x - (r.left + r.width / 2), p.y - (r.top + r.height / 2))
    followUntil = Date.now() + 2500
  })
}
const rand = (a: number, b: number) => a + Math.random() * (b - a)
function scheduleGlance() {
  const m = mood.value
  const delay = m === 'sleep' ? rand(6000, 11000) : m === 'focus' || m === 'juggle' ? rand(1400, 3000) : m === 'think' ? rand(1100, 2600) : m === 'call' ? rand(4000, 7000) : rand(2800, 6500)
  later(() => {
    if (Date.now() >= followUntil && !cheering.value) {
      const md = mood.value
      if (md === 'sleep') gaze.value = { x: rand(-0.01, 0.01), y: 0.02 }
      else if (md === 'call') gaze.value = { x: 0, y: 0 }
      else if (md === 'think') gaze.value = { x: rand(0.02, 0.05) * (Math.random() < 0.5 ? -1 : 1), y: -0.04 }
      // Working: round eyes (a slight squint read as suspicious), a look down at the work.
      else if (md === 'focus' || md === 'juggle') gaze.value = { x: rand(-0.03, 0.03), y: rand(0.015, 0.035) }
      else if (Math.random() < 0.3) gaze.value = { x: 0, y: 0 }
      else gaze.value = { x: rand(-0.05, 0.05), y: rand(-0.035, 0.03) }
    }
    scheduleGlance()
  }, delay)
}
function scheduleBlink() {
  later(() => {
    if (!cheering.value && !quirk.value && !glee.value && mood.value !== 'sleep') {
      blink.value = true
      later(() => {
        blink.value = false
        if (Math.random() < 0.18) later(() => {
          blink.value = true
          later(() => (blink.value = false), 110)
        }, 190)
      }, 130)
    }
    scheduleBlink()
  }, mood.value === 'sleep' ? rand(12000, 20000) : rand(3500, 8500))
}
// ── Idle quirks: while calm, a new one every few seconds (utils/mascot.ts) ──
const quirk = ref<Quirk | null>(null)
const side = ref<'l' | 'r'>('r')
let lastQuirk: Quirk | null = null
let lastPointerAt = Date.now()
function runQuirk(q: Quirk) {
  if (quirk.value || cheering.value) return
  lastQuirk = q
  const dir = Math.random() < 0.5 ? -1 : 1
  side.value = dir < 0 ? 'l' : 'r'
  // ^^ and a hop: the cheer face, without a word.
  if (q === 'happy') return cheerFor(QUIRKS.happy.ms)
  quirk.value = q
  if (Date.now() >= followUntil) {
    const look = (x: number, y: number, at: number) => later(() => (gaze.value = { x, y }), at)
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
    }
  }
  later(() => {
    if (quirk.value === q) quirk.value = null
  }, QUIRKS[q].ms)
}
function scheduleQuirk() {
  later(() => {
    if (!reduced.value && mood.value === 'calm' && !glee.value && !boing) runQuirk(pickQuirk(lastQuirk, Date.now() - lastPointerAt))
    scheduleQuirk()
  }, quirkPause())
}

function start() {
  if (!live.value) return
  clearTimers()
  scheduleGlance()
  scheduleBlink()
  scheduleQuirk()
  if (props.interactive) window.addEventListener('pointermove', onPointer, { passive: true })
  syncLong()
}
function stop() {
  clearTimers()
  quirk.value = null
  glee.value = false
  boing?.cancel()
  window.removeEventListener('pointermove', onPointer)
  if (frame) cancelAnimationFrame(frame)
  frame = 0
  if (longTimer) clearInterval(longTimer)
  longTimer = null
}
watch(live, (v) => (v ? start() : stop()))
watch(() => [props.state, props.startedAt], syncLong)
// Waking up: look around at once instead of waiting out a doze.
watch(mood, (m, was) => {
  if (was === 'sleep' && m !== 'sleep') gaze.value = { x: rand(-0.03, 0.03), y: -0.02 }
  if (m === 'call') gaze.value = { x: 0, y: 0 }
})

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

// ── Hover and click ──
const hover = ref(false)
const lineSeed = ref(Math.floor(Math.random() * 100))
/**
 * A click: one boing — squash, stretch, settle — that ends exactly where it
 * started, so nothing snaps back. Played with WAAPI on top of whatever the
 * body does (composite add: hover's scale stays), never twice at once. A
 * quirk in progress keeps going; otherwise the eyes go ^^ for a moment.
 */
let boing: Animation | null = null
function poke() {
  if (!props.interactive) return
  lineSeed.value++
  const body = root.value?.querySelector<HTMLElement>('.qp-m-body')
  if (body && !reduced.value && !boing) {
    boing = body.animate(
      [
        { transform: 'translateY(0) scale(1, 1)', easing: 'ease-out' },
        { transform: 'translateY(7%) scale(1.17, 0.83)', offset: 0.18, easing: 'ease-in-out' },
        { transform: 'translateY(-6%) scale(0.92, 1.1)', offset: 0.46, easing: 'ease-in-out' },
        { transform: 'translateY(1.5%) scale(1.04, 0.97)', offset: 0.72, easing: 'ease-out' },
        { transform: 'translateY(0) scale(1, 1)' },
      ],
      { duration: 720, composite: 'add' },
    )
    const done = () => (boing = null)
    boing.onfinish = done
    boing.oncancel = done
  }
  if (quirk.value || cheering.value || glee.value) return
  if (mood.value === 'calm' || mood.value === 'sleep') {
    glee.value = true
    later(() => (glee.value = false), 1200)
  }
}

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
const label = computed(() => lineFor(mood.value, prefs.value.name, lineSeed.value))
</script>

<template>
  <div
    v-if="prefs.on || $props.mood !== null"
    ref="root"
    class="qp-mascot"
    :class="[`m-${mood}`, `eyes-${prefs.eyes}`, `skin-${prefs.skin}`, quirk ? `q-${quirk} side-${side}` : '', { hover: hover && interactive, glee, blink, still: reduced }]"
    :data-own-motion="reduced ? undefined : ''"
    :style="vars"
    role="img"
    :aria-label="`${prefs.name || 'Mascot'} — ${MOOD_LABEL[mood]}`"
    :title="label"
    @pointerenter="hover = true"
    @pointerleave="hover = false"
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
      <div v-if="mood === 'juggle'" class="qp-m-juggle"><i /><i /><i /></div>
      <div v-if="mood === 'tired'" class="qp-m-sweat" />
      <div v-if="mood === 'sleep'" class="qp-m-zzz">z<span>z</span></div>
    </div>
  </div>
</template>

<style scoped>
.qp-mascot {
  --m-base: #2a2a31;
  --m-light: #4a4a55;
  --m-ink: #e4e4e7;
  --m-ring: color-mix(in srgb, var(--m-ink) 62%, transparent);
  --m-spring: cubic-bezier(0.34, 1.45, 0.64, 1);
  position: relative;
  width: 100%;
  aspect-ratio: 1;
  container-type: inline-size;
  user-select: none;
  cursor: default;
}
/* ── Bob, body: a flat glyph with a soft outline and a faint glow (the old face) ── */
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
.skin-night .qp-m-body { --m-ring: color-mix(in srgb, var(--m-ink) 66%, transparent); }
.skin-mint .qp-m-body,
.skin-sky .qp-m-body,
.skin-violet .qp-m-body,
.skin-peach .qp-m-body,
.skin-lemon .qp-m-body { --m-ring: color-mix(in srgb, var(--m-light) 55%, transparent); }
.qp-m-halo {
  position: absolute;
  left: 13%;
  top: 11%;
  width: 74%;
  height: 74%;
  border-radius: 50%;
  background: radial-gradient(circle, color-mix(in srgb, var(--m-ink) 22%, transparent) 55%, transparent 72%);
  opacity: 0.4;
  transition: opacity 0.5s ease;
}
/* ── Face parts (in the body's own percentages) ── */
.qp-m-eyes { position: absolute; inset: 0; transform: translate(var(--m-gx), var(--m-gy)); transition: transform 0.55s var(--m-spring); }
.qp-m-eye {
  position: absolute;
  top: 39%;
  width: 15%;
  height: 15%;
  border-radius: 50%;
  background: var(--m-ink);
  transform-origin: center;
  transition: transform 0.16s ease, border-radius 0.25s ease, opacity 0.2s ease, clip-path 0.3s ease;
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
.qp-m-acc { position: absolute; inset: 0; overflow: visible; pointer-events: none; }
.acc-fill { fill: #f87171; }
.acc-band { fill: none; stroke: #b91c1c; stroke-width: 5; stroke-linecap: round; }
.acc-line { fill: none; stroke: var(--m-ink); stroke-width: 3; stroke-linecap: round; }
.acc-line.thick { stroke-width: 6; stroke: #3f3f46; }
.acc-dark { fill: #3f3f46; }
.skin-night .acc-line.thick { stroke: #71717a; }
.skin-night .acc-dark { fill: #71717a; }

/* ── Moods ── */
.m-sleep .qp-m-eye { transform: scaleY(0.12); opacity: 0.7; }
.m-sleep .qp-m-body { transform: scaleY(0.95) translateY(2%); }
.m-sleep .qp-m-bob { animation-duration: 6.5s; }
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
.m-tired .qp-m-eye { clip-path: inset(48% 0 0 0); }
.m-tired .qp-m-brow { opacity: 0.6; }
.m-tired .qp-m-brow.l { transform: translateY(40%) rotate(-12deg); }
.m-tired .qp-m-brow.r { transform: translateY(40%) rotate(12deg); }
.m-tired .qp-m-body { transform: scaleY(0.96) translateY(2%); }
.m-call .qp-m-eye { transform: scale(1.18); }
.m-call .qp-m-brow { opacity: 1; background: var(--qss-warning, #d29a3f); }
.m-call .qp-m-brow.l,
.m-call .qp-m-brow.r { transform: translateY(-35%); }
.m-call .qp-m-body { box-shadow: 0 0 0 1.1cqw var(--qss-warning, #d29a3f); }
.m-call .qp-m-halo { opacity: 1; background: radial-gradient(circle, color-mix(in srgb, var(--qss-warning, #d29a3f) 35%, transparent) 55%, transparent 72%); animation: qp-m-pulse 1.6s ease-in-out infinite; }
.m-worried .qp-m-eye { transform: scale(0.9); }
.m-worried .qp-m-brow { opacity: 1; background: var(--qss-error, #f87171); }
.m-worried .qp-m-brow.l { transform: translateY(-20%) rotate(-16deg); }
.m-worried .qp-m-brow.r { transform: translateY(-20%) rotate(16deg); }
.m-worried .qp-m-body { box-shadow: 0 0 0 1.1cqw var(--qss-error, #f87171); animation: qp-m-shiver 0.5s ease-in-out 2; }
.m-cheer .qp-m-eye { background: transparent; box-shadow: none; border-top: 1.7cqw solid var(--m-ink); border-radius: 50%; transform: translateY(14%) scaleX(1.15); }
.m-cheer .qp-m-eye b,
.m-cheer .qp-m-eye s { opacity: 0; }
.m-cheer .qp-m-bob { animation: qp-m-jump 0.9s var(--m-spring) 1; }
.m-nod .qp-m-bob { animation: qp-m-nod 0.5s ease-in-out 2; }
.hover .qp-m-body { transform: scale(1.05); }
.hover .qp-m-eye { transform: scale(1.08); }
/* ^^ after a click: the cheer eyes without the hop. */
.qp-mascot.glee .qp-m-eye { background: transparent; box-shadow: none; border-top: 1.7cqw solid var(--m-ink); border-radius: 50%; transform: translateY(14%) scaleX(1.15); }
.qp-mascot.glee .qp-m-eye b,
.qp-mascot.glee .qp-m-eye s { opacity: 0; }

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
.qp-m-juggle { position: absolute; left: 30%; top: -6%; width: 40%; height: 40%; animation: qp-m-orbit 1.6s linear infinite; }
.qp-m-juggle i { position: absolute; width: 22%; height: 22%; border-radius: 50%; }
.qp-m-juggle i:nth-child(1) { left: 39%; top: 0; background: #34d399; }
.qp-m-juggle i:nth-child(2) { left: 0; top: 60%; background: #7aa2f7; }
.qp-m-juggle i:nth-child(3) { right: 0; top: 60%; background: #f472b6; }
.qp-m-sweat { position: absolute; right: 22%; top: 22%; width: 7%; height: 10%; border-radius: 50% 50% 50% 50% / 65% 65% 35% 35%; background: #7dd3fc; opacity: 0.9; animation: qp-m-drip 2.6s ease-in infinite; }
.qp-m-zzz { position: absolute; right: 8%; top: 4%; font-size: 13cqw; font-weight: 700; color: var(--qss-text-muted, #8a8a95); animation: qp-m-float 3.2s ease-in-out infinite; }
.qp-m-zzz span { font-size: 70%; margin-left: 2cqw; }

@keyframes qp-m-breathe { 0%, 100% { transform: scale(1, 1); } 50% { transform: scale(1.025, 0.975); } }
@keyframes qp-m-jump { 0% { transform: translateY(0) scale(1, 1); } 25% { transform: translateY(0) scale(1.1, 0.88); } 55% { transform: translateY(-12%) scale(0.94, 1.08); } 80% { transform: translateY(0) scale(1.06, 0.94); } 100% { transform: translateY(0) scale(1, 1); } }
@keyframes qp-m-nod { 0%, 100% { transform: translateY(0); } 50% { transform: translateY(5%) scale(1.02, 0.97); } }
@keyframes qp-m-pulse { 0%, 100% { transform: scale(1); opacity: 0.55; } 50% { transform: scale(1.07); opacity: 0.9; } }
@keyframes qp-m-shiver { 0%, 100% { transform: translateX(0); } 25% { transform: translateX(-2%); } 75% { transform: translateX(2%); } }
@keyframes qp-m-wave { 0%, 100% { transform: rotate(-12deg); } 50% { transform: rotate(22deg); } }
@keyframes qp-m-orbit { to { transform: rotate(360deg); } }
@keyframes qp-m-drip { 0%, 30% { transform: translateY(0); opacity: 0; } 40% { opacity: 0.9; } 100% { transform: translateY(160%); opacity: 0; } }
@keyframes qp-m-float { 0%, 100% { transform: translateY(0); opacity: 0.5; } 50% { transform: translateY(-14%); opacity: 1; } }

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
/* Giggle: > < eyes and a little shake. */
.qp-mascot.q-giggle .qp-m-eye { background: transparent; box-shadow: none; border-radius: 0.5cqw; }
.qp-mascot.q-giggle .qp-m-eye.l { border-top: 1.7cqw solid var(--m-ink); border-right: 1.7cqw solid var(--m-ink); transform: translateX(10%) rotate(45deg) scale(0.62); }
.qp-mascot.q-giggle .qp-m-eye.r { border-bottom: 1.7cqw solid var(--m-ink); border-left: 1.7cqw solid var(--m-ink); transform: translateX(-10%) rotate(45deg) scale(0.62); }
.qp-mascot.q-giggle .qp-m-eye b,
.qp-mascot.q-giggle .qp-m-eye s { opacity: 0; }
.q-giggle .qp-m-bob { animation: qp-q-giggle 1.3s ease-in-out both; }

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
/* One slow, complete close — never held half shut (that read as a squint). */
@keyframes qp-q-drowsy { 0% { transform: scaleY(1); } 50%, 68% { transform: scaleY(0.1); } 76% { transform: scale(1.3); } 88%, 100% { transform: scale(1); } }
@keyframes qp-q-nodoff { 0%, 100% { transform: translateY(0) rotate(0); } 60%, 70% { transform: translateY(5%) rotate(-4deg); } 76% { transform: translateY(-4%) rotate(0); } 88% { transform: translateY(0); } }
@keyframes qp-q-giggle { 0%, 100% { transform: rotate(0) translateY(0); } 15% { transform: rotate(-4deg) translateY(-2%); } 30% { transform: rotate(4deg); } 45% { transform: rotate(-4deg) translateY(-2%); } 60% { transform: rotate(4deg); } 75% { transform: rotate(-2deg); } }

/* A blink closes the eyes whatever the mood. */
.qp-mascot.blink .qp-m-eye { transform: scaleY(0.1); }
/* Closed round eyes are a line in the face's ink, not a white slit with a pupil. */
.eyes-round.m-sleep .qp-m-eye,
.eyes-round.blink .qp-m-eye { background: var(--m-ink); box-shadow: none; }
.eyes-round.m-sleep .qp-m-eye b,
.eyes-round.blink .qp-m-eye b { opacity: 0; }

/* Reduced motion: every expression, no continuous motion. */
.still .qp-m-bob,
.still .qp-m-eye,
.still .qp-m-halo,
.still .qp-m-hand,
.still .qp-m-juggle,
.still .qp-m-sweat,
.still .qp-m-zzz,
.still .qp-m-body { animation: none !important; }
</style>
