<script setup lang="ts">
/**
 * A message's images at full size (parity 12): the original file, not the
 * thumbnail, loaded only when shown. Fit or 1:1, zoom (+ / − / Ctrl+wheel),
 * ← / → between the message's images, Save a copy (a native dialog), Copy
 * path, Esc closes and focus goes back to the thumbnail that opened it. A
 * moved or deleted file says so instead of showing nothing.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { ChevronLeft, ChevronRight, Copy, Download, Maximize, X, ZoomIn, ZoomOut } from 'lucide-vue-next'
import { pilot } from '#pilot/utils/invoke'

const props = defineProps<{ threadId: string; images: { path: string; name?: string }[]; start: number }>()
const emit = defineEmits<{ close: [] }>()
const index = ref(props.start)
const src = ref('')
const meta = ref<{ width: number | null; height: number | null; size: number; mime: string } | null>(null)
const error = ref('')
const notice = ref('')
const zoom = ref<number | 'fit'>('fit')
const root = ref<HTMLElement | null>(null)
const current = computed(() => props.images[index.value] ?? null)

async function load() {
  src.value = ''
  meta.value = null
  error.value = ''
  notice.value = ''
  zoom.value = 'fit'
  const im = current.value
  if (!im) return
  try {
    const m = await pilot<{ mime: string; data: string; size: number; width: number | null; height: number | null }>('plugin:pilot|pilot_attachment_media', { id: props.threadId, path: im.path })
    if (!m || current.value !== im) return
    src.value = `data:${m.mime};base64,${m.data}`
    meta.value = { width: m.width, height: m.height, size: m.size, mime: m.mime }
  } catch (e) {
    error.value = String(e)
  }
}
watch(index, () => void load())
onMounted(() => {
  void load()
  void nextTick(() => root.value?.focus())
})
// The large data URL goes with the viewer.
onBeforeUnmount(() => {
  src.value = ''
})

function step(d: number) {
  const n = props.images.length
  if (n > 1) index.value = (index.value + d + n) % n
}
function setZoom(z: number | 'fit') {
  zoom.value = z === 'fit' ? 'fit' : Math.min(8, Math.max(0.1, z))
}
const zoomNumber = computed(() => (zoom.value === 'fit' ? 1 : zoom.value))
function zoomBy(f: number) {
  setZoom((zoom.value === 'fit' ? 1 : zoom.value) * f)
}
function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') emit('close')
  else if (e.key === 'ArrowRight') step(1)
  else if (e.key === 'ArrowLeft') step(-1)
  else if (e.key === '+' || e.key === '=') zoomBy(1.25)
  else if (e.key === '-') zoomBy(0.8)
  else if (e.key === '0') setZoom('fit')
  else if (e.key === '1') setZoom(1)
  else return
  e.preventDefault()
}
function onWheel(e: WheelEvent) {
  if (!e.ctrlKey) return
  e.preventDefault()
  zoomBy(e.deltaY < 0 ? 1.1 : 0.9)
}

async function saveCopy() {
  const im = current.value
  if (!im) return
  try {
    const { save } = await import('@tauri-apps/plugin-dialog')
    const name = im.path.split(/[\\/]/).pop() ?? 'image.png'
    const target = await save({ defaultPath: name, title: 'Save a copy of the image' })
    if (!target) return
    await pilot('plugin:pilot|pilot_file_save_as', { id: props.threadId, path: im.path, target, content: null })
    notice.value = `Saved to ${target}.`
  } catch (e) {
    error.value = String(e)
  }
}
async function copyPath() {
  const im = current.value
  if (!im) return
  await navigator.clipboard.writeText(im.path).then(
    () => (notice.value = 'Path copied.'),
    () => (error.value = 'The clipboard is not available.')
  )
}
const kb = (n: number) => (n > 1024 * 1024 ? `${(n / 1024 / 1024).toFixed(1)} MB` : `${Math.round(n / 1024)} KB`)
</script>

<template>
  <Teleport to="body">
    <div ref="root" class="qp-mv" role="dialog" aria-modal="true" :aria-label="`Image ${index + 1} of ${images.length}`" tabindex="-1" @keydown="onKey" @pointerdown.self="emit('close')">
      <header class="qp-mv-bar">
        <span class="qp-mv-name" :title="current?.path">{{ current?.name || current?.path.split(/[\\/]/).pop() }}</span>
        <span v-if="meta" class="qp-mv-dim">{{ meta.width && meta.height ? `${meta.width}×${meta.height}` : '' }} · {{ kb(meta.size) }}</span>
        <span v-if="images.length > 1" class="qp-mv-dim">{{ index + 1 }} / {{ images.length }}</span>
        <div class="qp-mv-spacer" />
        <button class="qp-mv-btn" title="Zoom out (−)" @click="zoomBy(0.8)"><ZoomOut :size="14" /></button>
        <button class="qp-mv-btn qp-mv-z" title="Fit (0) / actual size (1)" @click="setZoom(zoom === 'fit' ? 1 : 'fit')">{{ zoom === 'fit' ? 'Fit' : `${Math.round(zoomNumber * 100)}%` }}</button>
        <button class="qp-mv-btn" title="Zoom in (+)" @click="zoomBy(1.25)"><ZoomIn :size="14" /></button>
        <button class="qp-mv-btn" title="Fit" @click="setZoom('fit')"><Maximize :size="14" /></button>
        <button class="qp-mv-btn" title="Copy path" @click="copyPath"><Copy :size="14" /></button>
        <button class="qp-mv-btn" title="Save a copy" :disabled="!!error" @click="saveCopy"><Download :size="14" /></button>
        <button class="qp-mv-btn" title="Close (Esc)" @click="emit('close')"><X :size="15" /></button>
      </header>
      <div class="qp-mv-stage" @wheel="onWheel">
        <button v-if="images.length > 1" class="qp-mv-nav prev" aria-label="Previous image" @click="step(-1)"><ChevronLeft :size="22" /></button>
        <div v-if="error" class="qp-mv-err" role="alert">{{ error }}</div>
        <div v-else-if="!src" class="qp-mv-dim">Loading…</div>
        <div v-else class="qp-mv-scroll" :class="{ fit: zoom === 'fit' }">
          <img :src="src" :alt="current?.name ?? 'image'" :style="zoom === 'fit' ? {} : { width: meta?.width ? `${meta.width * zoomNumber}px` : `${zoomNumber * 100}%` }" />
        </div>
        <button v-if="images.length > 1" class="qp-mv-nav next" aria-label="Next image" @click="step(1)"><ChevronRight :size="22" /></button>
      </div>
      <div v-if="notice" class="qp-mv-note" role="status">{{ notice }}</div>
    </div>
  </Teleport>
</template>

<style scoped>
.qp-mv { position: fixed; inset: 0; z-index: 1200; display: flex; flex-direction: column; background: rgba(8, 8, 10, 0.92); color: var(--qss-text, #d4d4d8); outline: none; }
.qp-mv-bar { display: flex; align-items: center; gap: 8px; padding: 8px 12px; font-size: 12.5px; }
.qp-mv-name { font-weight: 500; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-mv-dim { color: var(--qss-text-muted, #8a8a95); font-size: 11.5px; }
.qp-mv-spacer { flex: 1; }
.qp-mv-btn { border: none; background: transparent; color: var(--qss-text-secondary, #b0b0ba); cursor: pointer; display: inline-flex; align-items: center; padding: 4px; border-radius: 6px; font: inherit; }
.qp-mv-btn:hover { background: rgba(255, 255, 255, 0.08); color: #fff; }
.qp-mv-z { min-width: 46px; justify-content: center; font-size: 12px; }
.qp-mv-stage { flex: 1; min-height: 0; position: relative; display: flex; align-items: center; justify-content: center; }
.qp-mv-scroll { max-width: 100%; max-height: 100%; overflow: auto; }
.qp-mv-scroll.fit { display: flex; align-items: center; justify-content: center; width: 100%; height: 100%; }
.qp-mv-scroll.fit img { max-width: calc(100% - 96px); max-height: calc(100% - 24px); object-fit: contain; }
.qp-mv-scroll img { display: block; image-rendering: auto; }
.qp-mv-nav { position: absolute; top: 50%; transform: translateY(-50%); border: none; background: rgba(0, 0, 0, 0.4); color: #fff; border-radius: 50%; width: 40px; height: 40px; display: flex; align-items: center; justify-content: center; cursor: pointer; z-index: 1; }
.qp-mv-nav.prev { left: 12px; }
.qp-mv-nav.next { right: 12px; }
.qp-mv-err { color: var(--qss-error, #e06c75); font-size: 13px; padding: 20px; }
.qp-mv-note { position: absolute; bottom: 14px; left: 50%; transform: translateX(-50%); padding: 4px 10px; border-radius: 8px; background: rgba(0, 0, 0, 0.6); font-size: 12px; }
</style>
