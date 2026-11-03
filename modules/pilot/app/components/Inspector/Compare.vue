<script setup lang="ts">
/**
 * Before / after (round 3, card 05): two of a thread's images, the earlier
 * as "before" — side by side, or one over the other with a divider you drag
 * (pointer events; ← / → move it). Esc closes and focus goes back.
 */
import { nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import { Columns2, SplitSquareHorizontal, X } from 'lucide-vue-next'
import { pilot } from '#pilot/utils/invoke'
import { imageLabel } from '#pilot/utils/progress'
import type { ProgressImage } from '#pilot/types'

const props = defineProps<{ threadId: string; before: ProgressImage; after: ProgressImage }>()
const emit = defineEmits<{ close: [] }>()

const mode = ref<'side' | 'slider'>('slider')
const src = ref<{ before: string | null; after: string | null }>({ before: null, after: null })
const error = ref<{ before: string; after: string }>({ before: '', after: '' })
const split = ref(50)
const root = ref<HTMLElement | null>(null)
const stage = ref<HTMLElement | null>(null)
let opener: Element | null = null

async function fetchOne(which: 'before' | 'after') {
  try {
    const m = await pilot<{ mime: string; data: string }>('plugin:pilot|pilot_progress_image', { id: props.threadId, key: props[which].key })
    src.value[which] = m ? `data:${m.mime};base64,${m.data}` : null
  } catch (e) {
    error.value[which] = e instanceof Error ? e.message : String(e)
  }
}
onMounted(async () => {
  opener = document.activeElement
  void fetchOne('before')
  void fetchOne('after')
  await nextTick()
  root.value?.focus()
})
onBeforeUnmount(() => (opener as HTMLElement | null)?.focus?.())

// ── The divider ──
let dragging = false
function moveTo(e: PointerEvent) {
  const r = stage.value?.getBoundingClientRect()
  if (!r || !r.width) return
  split.value = Math.min(100, Math.max(0, ((e.clientX - r.left) / r.width) * 100))
}
function onDown(e: PointerEvent) {
  dragging = true
  ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
  moveTo(e)
}
function onMove(e: PointerEvent) {
  if (dragging) moveTo(e)
}
function onUp() {
  dragging = false
}
function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.stopPropagation()
    emit('close')
  } else if (mode.value === 'slider' && (e.key === 'ArrowLeft' || e.key === 'ArrowRight')) {
    e.preventDefault()
    split.value = Math.min(100, Math.max(0, split.value + (e.key === 'ArrowLeft' ? -5 : 5)))
  }
}
</script>

<template>
  <Teleport to="body">
    <div ref="root" class="qp-cmp" role="dialog" aria-modal="true" aria-label="Before and after" tabindex="-1" @keydown="onKey">
      <header class="qp-cmp-bar">
        <span class="qp-cmp-title">Before and after</span>
        <span class="qp-cmp-dim">{{ imageLabel(before) }} → {{ imageLabel(after) }}</span>
        <span class="qp-cmp-spacer" />
        <span class="qp-cmp-modes" role="radiogroup" aria-label="Compare">
          <button role="radio" :aria-checked="mode === 'slider'" :class="{ on: mode === 'slider' }" @click="mode = 'slider'"><SplitSquareHorizontal :size="13" /> Slider</button>
          <button role="radio" :aria-checked="mode === 'side'" :class="{ on: mode === 'side' }" @click="mode = 'side'"><Columns2 :size="13" /> Side by side</button>
        </span>
        <button class="qp-cmp-x" title="Close (Esc)" aria-label="Close" @click="emit('close')"><X :size="15" /></button>
      </header>

      <div v-if="mode === 'side'" class="qp-cmp-side">
        <figure v-for="w in (['before', 'after'] as const)" :key="w" class="qp-cmp-pane">
          <figcaption><b>{{ w === 'before' ? 'Before' : 'After' }}</b> · {{ imageLabel(props[w]) }} · {{ props[w].name }}</figcaption>
          <div class="qp-cmp-img">
            <img v-if="src[w]" :src="src[w]!" :alt="w" />
            <span v-else class="qp-cmp-msg">{{ error[w] || 'Loading…' }}</span>
          </div>
        </figure>
      </div>

      <div v-else ref="stage" class="qp-cmp-stage" @pointerdown="onDown" @pointermove="onMove" @pointerup="onUp" @pointercancel="onUp">
        <img v-if="src.after" class="qp-cmp-layer" :src="src.after" alt="after" draggable="false" />
        <img v-if="src.before" class="qp-cmp-layer" :src="src.before" alt="before" draggable="false" :style="{ clipPath: `inset(0 ${100 - split}% 0 0)` }" />
        <span v-if="!src.before || !src.after" class="qp-cmp-msg">{{ error.before || error.after || 'Loading…' }}</span>
        <div class="qp-cmp-divider" :style="{ left: `${split}%` }" role="slider" aria-label="Divider" :aria-valuenow="Math.round(split)" aria-valuemin="0" aria-valuemax="100">
          <span class="qp-cmp-handle" />
        </div>
        <span class="qp-cmp-tag left">Before</span>
        <span class="qp-cmp-tag right">After</span>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.qp-cmp { position: fixed; inset: 0; z-index: 1200; display: flex; flex-direction: column; background: rgba(8, 8, 10, 0.94); color: var(--qss-text, #d4d4d8); outline: none; font-size: 12.5px; }
.qp-cmp-bar { display: flex; align-items: center; gap: 10px; padding: 8px 12px; }
.qp-cmp-title { font-weight: 600; }
.qp-cmp-dim { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--qss-text-muted, #8a8a95); }
.qp-cmp-spacer { flex: 1; }
.qp-cmp-modes { display: inline-flex; border: 1px solid rgba(255, 255, 255, 0.14); border-radius: 7px; overflow: hidden; }
.qp-cmp-modes button { display: inline-flex; align-items: center; gap: 5px; padding: 4px 10px; border: none; background: transparent; color: var(--qss-text-secondary, #b0b0ba); font: inherit; cursor: pointer; }
.qp-cmp-modes button + button { border-left: 1px solid rgba(255, 255, 255, 0.14); }
.qp-cmp-modes button.on { background: rgba(255, 255, 255, 0.1); color: #fff; }
.qp-cmp-x { display: inline-flex; padding: 4px; border: none; border-radius: 6px; background: transparent; color: var(--qss-text-secondary, #b0b0ba); cursor: pointer; }
.qp-cmp-x:hover { background: rgba(255, 255, 255, 0.08); color: #fff; }
.qp-cmp-side { flex: 1; min-height: 0; display: grid; grid-template-columns: 1fr 1fr; gap: 12px; padding: 0 12px 12px; }
.qp-cmp-pane { min-width: 0; min-height: 0; margin: 0; display: flex; flex-direction: column; gap: 6px; }
.qp-cmp-pane figcaption { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--qss-text-secondary, #b0b0ba); }
.qp-cmp-img { flex: 1; min-height: 0; display: flex; align-items: center; justify-content: center; border-radius: 8px; background: rgba(255, 255, 255, 0.03); }
.qp-cmp-img img { max-width: 100%; max-height: 100%; object-fit: contain; }
.qp-cmp-stage { position: relative; flex: 1; min-height: 0; margin: 0 12px 12px; border-radius: 8px; overflow: hidden; background: rgba(255, 255, 255, 0.03); cursor: ew-resize; touch-action: none; user-select: none; }
.qp-cmp-layer { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: contain; pointer-events: none; }
.qp-cmp-msg { position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; color: var(--qss-text-muted, #8a8a95); }
.qp-cmp-divider { position: absolute; top: 0; bottom: 0; width: 2px; margin-left: -1px; background: #fff; box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.4); pointer-events: none; }
.qp-cmp-handle { position: absolute; top: 50%; left: 50%; width: 22px; height: 22px; margin: -11px 0 0 -11px; border-radius: 50%; background: #fff; box-shadow: 0 1px 6px rgba(0, 0, 0, 0.5); }
.qp-cmp-tag { position: absolute; top: 10px; padding: 2px 8px; border-radius: 999px; background: rgba(0, 0, 0, 0.6); color: #fff; font-size: 11.5px; pointer-events: none; }
.qp-cmp-tag.left { left: 10px; }
.qp-cmp-tag.right { right: 10px; }
</style>
