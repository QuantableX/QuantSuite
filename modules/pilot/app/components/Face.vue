<script setup lang="ts">
/**
 * The pilot — the orchestrator face from QuantControl's org chart, kept.
 *
 * Two dots that look around, blink, doze off when there is nothing to talk
 * to, and go ^^ when a turn finishes. Not a keyframe loop: a loop repeats,
 * and a face that repeats reads as a GIF. Each glance picks a fresh spot
 * and a fresh pause, with a bias back to center so it looks attentive
 * rather than shifty.
 *
 * The old version gated all of this on `prefers-reduced-motion`, which
 * Windows reports whenever "animation effects" is off — so on this machine
 * the eyes never moved at all. A glance and a blink are state changes, not
 * motion; they run regardless. Only the continuous halo pulse honours the
 * setting (CSS).
 */
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import type { LiveState } from '#pilot/types'

const props = withDefaults(
  defineProps<{
    state: LiveState
    /** Bump whenever a turn finished; the face goes ^^ for a moment. */
    cheer?: number
  }>(),
  { cheer: 0 }
)

const gaze = ref({ x: 0, y: 0 })
const blink = ref(false)
const happy = ref(false)

let gazeTimer: ReturnType<typeof setTimeout> | null = null
let blinkTimer: ReturnType<typeof setTimeout> | null = null
let happyTimer: ReturnType<typeof setTimeout> | null = null
let happyEnd: ReturnType<typeof setTimeout> | null = null

function rand(min: number, max: number): number {
  return min + Math.random() * (max - min)
}

const awake = () => props.state !== 'offline'

function scheduleGaze() {
  const s = props.state
  const delay =
    s === 'offline' ? rand(6000, 11000)
    : s === 'working' ? rand(700, 1600)
    : s === 'thinking' ? rand(1100, 2600)
    : s === 'waiting' ? rand(4000, 7000)
    : rand(2800, 6500)
  gazeTimer = setTimeout(() => {
    if (happy.value) {
      // ^^ holds still; a moving smile reads as a twitch.
    } else if (s === 'offline') {
      gaze.value = { x: rand(-1.5, 1.5), y: rand(0.5, 2) } // dozing: a slow drift down
    } else if (s === 'waiting') {
      gaze.value = { x: rand(-0.6, 0.6), y: rand(-0.6, 0.6) } // looking at you
    } else if (Math.random() < 0.3) {
      gaze.value = { x: 0, y: 0 }
    } else {
      gaze.value = { x: rand(-5, 5), y: rand(-3.5, 3) }
    }
    scheduleGaze()
  }, delay)
}

function scheduleBlink() {
  blinkTimer = setTimeout(() => {
    if (!happy.value) {
      blink.value = true
      setTimeout(() => {
        blink.value = false
        // A double blink now and then — the thing every real face does.
        if (Math.random() < 0.18) {
          setTimeout(() => {
            blink.value = true
            setTimeout(() => {
              blink.value = false
            }, 120)
          }, 200)
        }
      }, 140)
    }
    scheduleBlink()
  }, awake() ? rand(4000, 9000) : rand(12000, 20000))
}

/** ^^ for a moment: eyes to center, dots fade into arcs, then back. */
function beHappy(ms = 1800) {
  if (!awake()) return
  gaze.value = { x: 0, y: 0 }
  happy.value = true
  if (happyEnd) clearTimeout(happyEnd)
  happyEnd = setTimeout(() => {
    happy.value = false
  }, ms)
}

function scheduleHappy() {
  happyTimer = setTimeout(() => {
    if (props.state === 'idle') beHappy(rand(1500, 2400))
    scheduleHappy()
  }, rand(18000, 40000))
}

onMounted(() => {
  scheduleGaze()
  scheduleBlink()
  scheduleHappy()
})
onBeforeUnmount(() => {
  for (const t of [gazeTimer, blinkTimer, happyTimer, happyEnd]) if (t) clearTimeout(t)
})

watch(
  () => props.cheer,
  (now, before) => {
    if (before === undefined || (now ?? 0) <= (before ?? 0)) return
    beHappy(2200)
  }
)
// Waking up: look up and around at once instead of waiting out a doze.
watch(
  () => props.state,
  (s, was) => {
    if (was === 'offline' && s !== 'offline') gaze.value = { x: rand(-3, 3), y: -2 }
    if (s === 'waiting') gaze.value = { x: 0, y: 0 }
  }
)
</script>

<template>
  <svg viewBox="0 0 120 120" class="qp-face" :class="[`is-${state}`, { happy }]" aria-hidden="true">
    <defs>
      <filter id="qp-face-glow" x="-60%" y="-60%" width="220%" height="220%">
        <feGaussianBlur stdDeviation="5" result="b" />
        <feMerge><feMergeNode in="b" /><feMergeNode in="SourceGraphic" /></feMerge>
      </filter>
    </defs>
    <circle class="qp-face-halo" cx="60" cy="60" r="46" filter="url(#qp-face-glow)" />
    <circle class="qp-face-body" cx="60" cy="60" r="36" />
    <!-- brows: only the waiting face has them, raised -->
    <path class="qp-face-brow" d="M40.5 44 Q48 39 55.5 44" />
    <path class="qp-face-brow" d="M64.5 44 Q72 39 79.5 44" />
    <g class="qp-face-eyes" :style="{ transform: `translate(${gaze.x}px, ${gaze.y}px)` }">
      <circle class="qp-face-eye" :class="{ blink }" cx="48" cy="57" r="5.4" />
      <circle class="qp-face-eye" :class="{ blink }" cx="72" cy="57" r="5.4" />
      <!-- the happy face: dots give way to two little arcs -->
      <path class="qp-face-smile" d="M41.5 60 Q48 50 54.5 60" />
      <path class="qp-face-smile" d="M65.5 60 Q72 50 78.5 60" />
    </g>
  </svg>
</template>

<style scoped>
.qp-face {
  display: block;
  width: 100%;
  height: auto;
  overflow: visible;
}
.qp-face-halo {
  fill: var(--qss-text, #d4d4d8);
  opacity: 0.1;
  transform-box: fill-box;
  transform-origin: center;
  transition: opacity 0.6s ease;
}
.is-offline .qp-face-halo { opacity: 0; }
.is-thinking .qp-face-halo,
.is-working .qp-face-halo {
  opacity: 0.16;
  animation: qp-face-pulse 2.4s ease-in-out infinite;
}
.is-working .qp-face-halo { animation-duration: 1.3s; }
.is-waiting .qp-face-halo { fill: var(--qss-warning, #d29a3f); opacity: 0.22; }
.is-error .qp-face-halo { fill: var(--qss-error, #f87171); opacity: 0.18; }

.qp-face-body {
  fill: color-mix(in srgb, var(--qss-bg, #18181e) 40%, #000);
  stroke: var(--qss-text, #d4d4d8);
  stroke-width: 1.6;
  transition: stroke 0.4s ease;
}
.is-offline .qp-face-body { stroke: var(--qss-text-muted, #6e6e7a); }
.is-waiting .qp-face-body { stroke: var(--qss-warning, #d29a3f); }
.is-error .qp-face-body { stroke: var(--qss-error, #f87171); }

.qp-face-eyes { transition: transform 0.7s cubic-bezier(0.2, 0.8, 0.2, 1); }
.is-working .qp-face-eyes { transition-duration: 0.35s; }
.is-thinking .qp-face-eyes { transition-duration: 0.45s; }

.qp-face-eye {
  fill: var(--qss-text, #d4d4d8);
  transform-box: fill-box;
  transform-origin: center;
  transition: transform 0.12s ease-in, opacity 0.25s ease, fill 0.4s ease;
}
.qp-face-eye.blink { transform: scaleY(0.08); }
/* Dozing: half-lidded and dim. */
.is-offline .qp-face-eye { transform: scaleY(0.42); opacity: 0.55; fill: var(--qss-text-muted, #6e6e7a); }
.is-offline .qp-face-eye.blink { transform: scaleY(0.08); }
/* Waiting: wide open, straight at you. */
.is-waiting .qp-face-eye { transform: scale(1.18); }
.is-waiting .qp-face-eye.blink { transform: scale(1.18) scaleY(0.08); }
/* Working: a slight squint of concentration. */
.is-working .qp-face-eye { transform: scaleY(0.78); }
.is-working .qp-face-eye.blink { transform: scaleY(0.08); }
/* Error: flat. */
.is-error .qp-face-eye { transform: scaleY(0.22); fill: var(--qss-error, #f87171); }

.qp-face-brow {
  fill: none;
  stroke: var(--qss-warning, #d29a3f);
  stroke-width: 2;
  stroke-linecap: round;
  opacity: 0;
  transform-box: fill-box;
  transform-origin: center;
  transform: translateY(3px);
  transition: opacity 0.3s ease, transform 0.3s cubic-bezier(0.2, 0.8, 0.2, 1);
}
.is-waiting .qp-face-brow { opacity: 1; transform: translateY(0); }

/* ^^ */
.qp-face-smile {
  fill: none;
  stroke: var(--qss-text, #d4d4d8);
  stroke-width: 2.6;
  stroke-linecap: round;
  opacity: 0;
  transform-box: fill-box;
  transform-origin: center;
  transform: translateY(2px) scale(0.85);
  transition: opacity 0.25s ease, transform 0.3s cubic-bezier(0.2, 0.8, 0.2, 1);
}
.happy .qp-face-eye { opacity: 0; }
.happy .qp-face-smile { opacity: 1; transform: translateY(0) scale(1); }

@keyframes qp-face-pulse {
  0%, 100% { transform: scale(1); opacity: 0.12; }
  50% { transform: scale(1.06); opacity: 0.22; }
}
@media (prefers-reduced-motion: reduce) {
  .qp-face-halo { animation: none !important; }
}
</style>
