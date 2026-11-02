<script setup lang="ts">
/**
 * A diagram in a large view (parity 13): fit by default, + / − / Ctrl+wheel
 * to zoom, 0 to fit, Esc closes (focus goes back to the button that opened it).
 * The SVG is the cleaned, cached one (utils/mermaid.ts).
 */
import { computed, nextTick, onMounted, ref } from 'vue'
import { Maximize, X, ZoomIn, ZoomOut } from 'lucide-vue-next'
import { svgOf } from '#pilot/utils/mermaid'

const props = defineProps<{ hash: string }>()
const emit = defineEmits<{ close: [] }>()
const root = ref<HTMLElement | null>(null)
const zoom = ref<number | 'fit'>('fit')
// eslint-disable-next-line vue/no-v-html — cleaned in cleanSvg
const svg = computed(() => svgOf(props.hash) ?? '')
const scale = computed(() => (zoom.value === 'fit' ? 1 : zoom.value))
function by(f: number) {
  zoom.value = Math.min(8, Math.max(0.2, scale.value * f))
}
function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') emit('close')
  else if (e.key === '+' || e.key === '=') by(1.25)
  else if (e.key === '-') by(0.8)
  else if (e.key === '0') zoom.value = 'fit'
  else return
  e.preventDefault()
}
function onWheel(e: WheelEvent) {
  if (!e.ctrlKey) return
  e.preventDefault()
  by(e.deltaY < 0 ? 1.1 : 0.9)
}
onMounted(() => void nextTick(() => root.value?.focus()))
</script>

<template>
  <Teleport to="body">
    <div ref="root" class="qp-mmv" role="dialog" aria-modal="true" aria-label="Diagram" tabindex="-1" @keydown="onKey" @pointerdown.self="emit('close')">
      <header class="qp-mmv-bar">
        <span>Diagram</span>
        <div class="qp-mmv-spacer" />
        <button title="Zoom out (−)" @click="by(0.8)"><ZoomOut :size="14" /></button>
        <button class="qp-mmv-z" title="Fit (0)" @click="zoom = 'fit'">{{ zoom === 'fit' ? 'Fit' : `${Math.round(scale * 100)}%` }}</button>
        <button title="Zoom in (+)" @click="by(1.25)"><ZoomIn :size="14" /></button>
        <button title="Fit" @click="zoom = 'fit'"><Maximize :size="14" /></button>
        <button title="Close (Esc)" @click="emit('close')"><X :size="15" /></button>
      </header>
      <div class="qp-mmv-stage" :class="{ fit: zoom === 'fit' }" @wheel="onWheel">
        <div class="qp-mmv-svg" :style="zoom === 'fit' ? {} : { transform: `scale(${scale})`, transformOrigin: 'top left' }" v-html="svg" />
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.qp-mmv { position: fixed; inset: 0; z-index: 1200; display: flex; flex-direction: column; background: rgba(8, 8, 10, 0.94); color: var(--qss-text, #d4d4d8); outline: none; }
.qp-mmv-bar { display: flex; align-items: center; gap: 6px; padding: 8px 12px; font-size: 12.5px; }
.qp-mmv-bar button { border: none; background: transparent; color: var(--qss-text-secondary, #b0b0ba); cursor: pointer; display: inline-flex; padding: 4px; border-radius: 6px; font: inherit; }
.qp-mmv-bar button:hover { background: rgba(255, 255, 255, 0.08); color: #fff; }
.qp-mmv-z { min-width: 46px; justify-content: center; font-size: 12px; }
.qp-mmv-spacer { flex: 1; }
.qp-mmv-stage { flex: 1; min-height: 0; overflow: auto; padding: 16px; }
.qp-mmv-stage.fit { display: flex; align-items: center; justify-content: center; }
.qp-mmv-stage.fit .qp-mmv-svg :deep(svg) { max-width: calc(100vw - 64px); max-height: calc(100vh - 96px); width: auto; height: auto; }
</style>
