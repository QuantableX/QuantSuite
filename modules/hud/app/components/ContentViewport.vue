<script setup lang="ts">
import { horizontalWheel } from '#hud/utils/horizontalScroll'

const props = defineProps<{ horizontal: boolean; section: string }>()
const viewport = ref<HTMLElement | null>(null)
const content = ref<HTMLElement | null>(null)
const atStart = ref(true)
const atEnd = ref(true)
let resize: ResizeObserver | undefined
let mutation: MutationObserver | undefined
let measurementFrame = 0
function measure() {
  const el = viewport.value
  if (!el) return
  atStart.value = el.scrollLeft <= 1
  atEnd.value = el.scrollLeft + el.clientWidth >= el.scrollWidth - 2
}
function scheduleMeasure() {
  if (measurementFrame) return
  // Batch DOM changes and measurements outside the ResizeObserver pass.
  measurementFrame = requestAnimationFrame(() => {
    measurementFrame = 0
    measure()
  })
}
function wheel(event: WheelEvent) {
  if (props.horizontal && viewport.value) horizontalWheel(event, viewport.value)
}
function step(direction: number) {
  const el = viewport.value
  if (!el) return
  el.scrollBy({ left: direction * Math.max(260, el.clientWidth * .75),
    behavior: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'instant' : 'smooth' })
}
function keydown(event: KeyboardEvent) {
  if (!props.horizontal || event.target !== viewport.value) return
  if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
    event.preventDefault()
    step(event.key === 'ArrowRight' ? 1 : -1)
  }
}
watch(() => [props.section, props.horizontal], async () => {
  await nextTick()
  viewport.value?.scrollTo({ left: 0, top: 0, behavior: 'instant' })
  measure()
})
onMounted(() => {
  resize = new ResizeObserver(scheduleMeasure)
  if (viewport.value) resize.observe(viewport.value)
  if (content.value) {
    resize.observe(content.value)
    mutation = new MutationObserver(scheduleMeasure)
    mutation.observe(content.value, { childList: true, subtree: true })
  }
  measure()
})
onUnmounted(() => {
  resize?.disconnect()
  mutation?.disconnect()
  cancelAnimationFrame(measurementFrame)
})
</script>

<template>
  <div class="content-viewport" :class="{ horizontal }">
    <div ref="viewport" class="scroll-content" :tabindex="horizontal ? 0 : undefined"
      :aria-label="horizontal ? 'HUD content — scroll horizontally' : undefined"
      @wheel="wheel" @scroll.passive="measure" @keydown="keydown">
      <div ref="content" class="viewport-content"><slot /></div>
    </div>
    <div v-if="horizontal && (!atStart || !atEnd)" class="scroll-navigation">
      <button class="scroll-arrow" :disabled="atStart" aria-label="Scroll left" @click="step(-1)">‹</button>
      <button class="scroll-arrow" :disabled="atEnd" aria-label="Scroll right" @click="step(1)">›</button>
    </div>
  </div>
</template>

<style scoped>
.content-viewport { position: relative; flex: 1; min-height: 0; min-width: 0; display: flex; flex-direction: column; }
.scroll-content { overflow-y: auto; min-height: 0; flex: 1; padding: 2px 12px 8px; }
.viewport-content { display: flex; flex-direction: column; min-height: 100%; }
.horizontal .scroll-content { overflow-x: auto; overflow-y: hidden; padding: 12px 20px 12px; scroll-snap-type: x proximity; scroll-padding-inline: 20px; overscroll-behavior-x: contain; }
.horizontal .scroll-content::-webkit-scrollbar { height: 3px; }
.horizontal .scroll-content::-webkit-scrollbar-thumb { background: var(--border-color); border-radius: 3px; }
.horizontal .viewport-content { flex-direction: row; height: 100%; min-height: 0; width: max-content; min-width: 100%; gap: 16px; }
.horizontal .scroll-content:focus-visible { outline: 1px solid var(--accent-green); outline-offset: -2px; }
.scroll-navigation { position: absolute; inset: 0 3px; display: flex; align-items: center; justify-content: space-between; pointer-events: none; }
.scroll-arrow { pointer-events: auto; opacity: 0; border: 1px solid var(--border-color); background: var(--bg-primary); color: var(--text-primary); font: inherit; font-size: 22px; line-height: 24px; width: 26px; height: 30px; border-radius: 8px; cursor: pointer; }
.content-viewport:hover .scroll-arrow, .content-viewport:focus-within .scroll-arrow { opacity: .95; }
.scroll-arrow:hover { background: var(--bg-secondary); }
.scroll-arrow:disabled { visibility: hidden; }
</style>
