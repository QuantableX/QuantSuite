<script setup lang="ts">
import { computed } from 'vue'
import geometry from '#hud/assets/trigger-tab.json'

const props = defineProps<{ side: 'left' | 'right' | 'top'; tucked: boolean; pinned: boolean }>()
defineEmits<{ activate: []; hover: [] }>()

// Shared with trigger_region.rs: one contour for paint and the native hit area.
const size = computed(() => props.side === 'top'
  ? { width: geometry.height, height: geometry.width }
  : { width: geometry.width, height: geometry.height })
const outline = computed(() => 'M 0 0 ' + geometry.curves.map(curve => {
  const values = curve.slice(2)
  if (props.side === 'top') for (let i = 0; i < values.length; i += 2) {
    [values[i], values[i + 1]] = [values[i + 1]!, values[i]!]
  }
  return `C ${values.join(' ')}`
}).join(' '))
const clip = computed(() => `path('${outline.value} Z')`)
const arrowPoints = computed(() => {
  if (props.side === 'top') return props.tucked ? '39 8 44 12 49 8' : '39 12 44 8 49 12'
  const pointsRight = (props.side === 'right') === props.tucked
  return pointsRight ? '8 39 12 44 8 49' : '12 39 8 44 12 49'
})
</script>

<template>
  <button
    class="trigger-tab"
    :class="{ mirrored: side === 'left', top: side === 'top' }"
    :style="{ width: `${size.width}px`, height: `${size.height}px`, clipPath: clip }"
    :aria-label="tucked ? 'Open QuantHUD' : 'Close QuantHUD'"
    :aria-expanded="!tucked"
    :aria-disabled="!tucked && pinned"
    @click="$emit('activate')"
    @mouseenter="$emit('hover')"
  >
    <svg :viewBox="`0 0 ${size.width} ${size.height}`" aria-hidden="true">
      <path class="tab-fill" :d="`${outline} Z`" />
      <path class="tab-outline" :d="outline" />
      <polyline class="tab-arrow" :points="arrowPoints" :transform="side === 'left' ? `translate(${geometry.width} 0) scale(-1 1)` : undefined" />
    </svg>
  </button>
</template>

<style scoped>
.trigger-tab {
  position: absolute;
  top: 50%;
  left: 0;
  transform: translateY(-50%);
  padding: 0;
  border: 0;
  background: transparent;
  cursor: pointer;
  pointer-events: auto;
}
.trigger-tab.mirrored { transform: translateY(-50%) scaleX(-1); }
.trigger-tab.top { top: 0; left: 50%; transform: translateX(-50%); }
.trigger-tab svg { display: block; width: 100%; height: 100%; overflow: visible; }
.tab-fill { fill: var(--bg-primary); transition: fill 150ms ease; }
.tab-outline { fill: none; stroke: var(--border-color); stroke-width: 1; }
.tab-arrow { fill: none; stroke: var(--text-secondary); stroke-width: 1.25; stroke-linecap: round; stroke-linejoin: round; }
.trigger-tab:hover .tab-fill, .trigger-tab:focus-visible .tab-fill { fill: var(--bg-secondary); }
.trigger-tab:hover .tab-arrow, .trigger-tab:focus-visible .tab-arrow { stroke: var(--text-primary); }
.trigger-tab:focus-visible .tab-outline { stroke: var(--text-primary); stroke-width: 2; }
@media (prefers-reduced-motion: reduce) { .tab-fill { transition: none; } }
</style>
