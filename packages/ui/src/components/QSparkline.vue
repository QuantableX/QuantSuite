<script setup lang="ts">
/**
 * SVG sparkline with an animated draw (DESIGN.md §6). Renders nothing until
 * it has at least three points — no fake curves.
 */
import { computed } from 'vue'

const props = withDefaults(
  defineProps<{
    points: number[]
    width?: number
    height?: number
  }>(),
  { width: 100, height: 26 }
)

const path = computed(() => {
  const pts = props.points
  if (pts.length < 3) return null
  const min = Math.min(...pts)
  const max = Math.max(...pts)
  const span = max - min || 1
  const stepX = props.width / (pts.length - 1)
  return pts
    .map((v, i) => {
      const x = (i * stepX).toFixed(2)
      const y = (props.height - 2 - ((v - min) / span) * (props.height - 4)).toFixed(2)
      return `${i === 0 ? 'M' : 'L'}${x} ${y}`
    })
    .join(' ')
})

// Generous overestimate of path length for the dash-draw animation.
const dashLen = computed(() => props.width * 2)
</script>

<template>
  <svg
    v-if="path"
    class="qsp"
    :viewBox="`0 0 ${width} ${height}`"
    preserveAspectRatio="none"
    aria-hidden="true"
  >
    <path :d="path" :style="{ '--len': dashLen }" />
  </svg>
</template>

<style scoped>
.qsp {
  display: block;
  width: 100%;
  height: 26px;
  overflow: visible;
}

.qsp path {
  fill: none;
  stroke: var(--qss-accent);
  stroke-width: 1.5;
  stroke-linecap: round;
  stroke-linejoin: round;
  stroke-dasharray: var(--len);
  stroke-dashoffset: var(--len);
  opacity: 0.85;
  animation: qsp-draw 620ms var(--qss-ease-out) forwards;
}

@keyframes qsp-draw {
  to {
    stroke-dashoffset: 0;
  }
}

@media (prefers-reduced-motion: reduce) {
  .qsp path {
    animation-duration: 1ms;
  }
}
</style>
