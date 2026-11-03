<script setup lang="ts">
/**
 * The Progress tab (round 3, card 05; PandaCode's visual progress): every
 * image of the thread — what you attached, Browser captures, images in pages
 * the agent rendered, image files its tools read or wrote — turn by turn,
 * labelled with turn and source. Pinned references come first and stay with
 * the thread. A click opens the media viewer; Compare picks two for a
 * before/after. Nothing is copied: gone files show as gone.
 */
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { Pin, PinOff, RefreshCw, SplitSquareHorizontal } from 'lucide-vue-next'
import { pilot } from '#pilot/utils/invoke'
import { useThreadStore } from '#pilot/stores/thread'
import { arrange, beforeAfter, imageLabel, togglePick } from '#pilot/utils/progress'
import type { Progress, ProgressImage } from '#pilot/types'

const props = defineProps<{ threadId: string }>()
const threads = useThreadStore()

const progress = ref<Progress | null>(null)
const thumbs = ref<Record<string, string | null>>({})
const loading = ref(false)
const error = ref('')

async function load() {
  loading.value = true
  error.value = ''
  try {
    progress.value = await pilot<Progress>('plugin:pilot|pilot_progress', { id: props.threadId })
    void loadThumbs()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    loading.value = false
  }
}
async function loadThumbs() {
  const want = (progress.value?.images ?? []).filter((im) => !im.gone && !(im.key in thumbs.value)).map((im) => im.key)
  for (let i = 0; i < want.length; i += 24) {
    const chunk = want.slice(i, i + 24)
    try {
      const got = (await pilot<Record<string, string | null>>('plugin:pilot|pilot_progress_thumbs', { id: props.threadId, keys: chunk })) ?? {}
      thumbs.value = { ...thumbs.value, ...got }
    } catch {
      for (const k of chunk) thumbs.value[k] = null
    }
  }
}
watch(
  () => props.threadId,
  () => {
    thumbs.value = {}
    picking.value = false
    picked.value = []
    void load()
  },
  { immediate: true },
)
// New images arrive with new items: look again a moment after the timeline grows.
let again: ReturnType<typeof setTimeout> | null = null
watch(
  () => threads.view(props.threadId)?.items.length ?? 0,
  () => {
    if (again) clearTimeout(again)
    again = setTimeout(() => void load(), 900)
  },
)
onBeforeUnmount(() => again && clearTimeout(again))

const images = computed(() => progress.value?.images ?? [])
const layout = computed(() => arrange(images.value, progress.value?.pins ?? []))
const pinned = computed(() => new Set(progress.value?.pins ?? []))

async function setPin(im: ProgressImage, on: boolean) {
  try {
    const pins = await pilot<string[]>('plugin:pilot|pilot_progress_pin', { id: props.threadId, key: im.key, pinned: on })
    if (progress.value && pins) progress.value = { ...progress.value, pins }
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
}

// ── Viewer and compare ──
const viewing = ref<number | null>(null)
const viewerImages = computed(() => images.value.map((im) => ({ key: im.key, path: im.path ?? undefined, name: im.name })))
const picking = ref(false)
const picked = ref<string[]>([])
const pair = computed(() => beforeAfter(images.value, picked.value))
const comparing = ref(false)
function onTile(im: ProgressImage) {
  if (picking.value) {
    if (!im.gone) picked.value = togglePick(picked.value, im.key)
    return
  }
  viewing.value = images.value.findIndex((x) => x.key === im.key)
}
function pickRole(im: ProgressImage): string {
  if (!picked.value.includes(im.key)) return ''
  if (!pair.value) return 'Picked'
  return pair.value.before.key === im.key ? 'Before' : 'After'
}
function toggleCompare() {
  picking.value = !picking.value
  picked.value = []
}
</script>

<template>
  <div class="qp-prog">
    <div class="qp-prog-bar">
      <span class="qp-prog-count">{{ images.length }} image{{ images.length === 1 ? '' : 's' }}</span>
      <span class="qp-prog-spacer" />
      <button class="qp-prog-btn" :class="{ on: picking }" :disabled="images.filter((i) => !i.gone).length < 2" :aria-pressed="picking" title="Pick two images to compare" @click="toggleCompare">
        <SplitSquareHorizontal :size="13" /> {{ picking ? 'Cancel' : 'Compare' }}
      </button>
      <button class="qp-prog-icon" title="Refresh" :disabled="loading" @click="load"><RefreshCw :size="13" /></button>
    </div>
    <div v-if="picking" class="qp-prog-pick" role="status">
      <span>{{ picked.length < 2 ? `Pick ${2 - picked.length} more image${picked.length === 1 ? '' : 's'} — the earlier one is "before".` : 'Ready.' }}</span>
      <button class="qp-prog-btn primary" :disabled="!pair" @click="comparing = true">Compare</button>
    </div>
    <p v-if="error" class="qp-prog-err" role="alert">{{ error }}</p>

    <div class="qp-prog-body">
      <div v-if="progress && !images.length" class="qp-prog-empty">
        <p>No images in this thread yet.</p>
        <p class="qp-prog-dim">Attach screenshots, capture the Browser tab, or let the agent render pages and read images — they line up here turn by turn.</p>
      </div>

      <section v-if="layout.pinned.length" class="qp-prog-sec">
        <h4>References</h4>
        <div class="qp-prog-grid">
          <div v-for="im in layout.pinned" :key="`pin-${im.key}`" class="qp-prog-tile" :class="{ gone: im.gone, picked: picked.includes(im.key) }">
            <button class="qp-prog-img" :title="im.name" :disabled="im.gone && !picking" @click="onTile(im)">
              <img v-if="thumbs[im.key]" :src="thumbs[im.key]!" alt="" />
              <span v-else-if="im.gone" class="qp-prog-gone">Gone</span>
              <span v-else class="qp-prog-wait" />
              <span v-if="pickRole(im)" class="qp-prog-badge">{{ pickRole(im) }}</span>
            </button>
            <div class="qp-prog-cap">
              <span class="qp-prog-label">{{ imageLabel(im) }}</span>
              <button class="qp-prog-pin on" title="Unpin" aria-label="Unpin" @click="setPin(im, false)"><PinOff :size="12" /></button>
            </div>
          </div>
        </div>
      </section>

      <section v-for="t in layout.turns" :key="`turn-${t.turn}-${t.images[0]?.key}`" class="qp-prog-sec">
        <h4>{{ t.turn ? `Turn ${t.turn}` : 'Earlier' }}</h4>
        <div class="qp-prog-grid">
          <div v-for="im in t.images" :key="im.key" class="qp-prog-tile" :class="{ gone: im.gone, picked: picked.includes(im.key) }" :data-progress="im.key">
            <button class="qp-prog-img" :title="im.name" :disabled="im.gone && !picking" @click="onTile(im)">
              <img v-if="thumbs[im.key]" :src="thumbs[im.key]!" alt="" />
              <span v-else-if="im.gone" class="qp-prog-gone">Gone</span>
              <span v-else-if="im.key in thumbs" class="qp-prog-gone">Not shown</span>
              <span v-else class="qp-prog-wait" />
              <span v-if="pickRole(im)" class="qp-prog-badge">{{ pickRole(im) }}</span>
            </button>
            <div class="qp-prog-cap">
              <span class="qp-prog-label">{{ imageLabel(im) }}</span>
              <button
                class="qp-prog-pin"
                :class="{ on: pinned.has(im.key) }"
                :title="pinned.has(im.key) ? 'Unpin' : 'Pin as a reference'"
                :aria-pressed="pinned.has(im.key)"
                :aria-label="pinned.has(im.key) ? 'Unpin' : 'Pin as a reference'"
                @click="setPin(im, !pinned.has(im.key))"
              >
                <Pin :size="12" />
              </button>
            </div>
          </div>
        </div>
      </section>
    </div>

    <PilotThreadMediaViewer v-if="viewing !== null && viewing >= 0" :thread-id="threadId" :images="viewerImages" :start="viewing" @close="viewing = null" />
    <PilotInspectorCompare v-if="comparing && pair" :thread-id="threadId" :before="pair.before" :after="pair.after" @close="comparing = false" />
  </div>
</template>

<style scoped>
.qp-prog { flex: 1; min-height: 0; display: flex; flex-direction: column; font-size: 12px; color: var(--qss-text, #d4d4d8); }
.qp-prog-bar { display: flex; align-items: center; gap: 6px; padding: 6px 8px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); }
.qp-prog-count { color: var(--qss-text-secondary, #9a9aa5); }
.qp-prog-spacer { flex: 1; }
.qp-prog-btn { display: inline-flex; align-items: center; gap: 5px; height: 24px; padding: 0 9px; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; background: transparent; color: var(--qss-text, #d4d4d8); font: inherit; font-size: 11.5px; cursor: pointer; }
.qp-prog-btn:hover:not(:disabled) { background: var(--qss-bg-hover, #313139); }
.qp-prog-btn.on { background: var(--qss-bg-card, #292930); }
.qp-prog-btn.primary { border-color: var(--qss-text, #d4d4d8); background: var(--qss-text, #d4d4d8); color: var(--qss-bg, #18181e); font-weight: 600; }
.qp-prog-btn:disabled { opacity: 0.45; cursor: default; }
.qp-prog-icon { display: inline-flex; padding: 4px; border: none; border-radius: 5px; background: transparent; color: var(--qss-text-secondary, #9a9aa5); cursor: pointer; }
.qp-prog-icon:hover { background: var(--qss-bg-hover, #313139); }
.qp-prog-pick { display: flex; align-items: center; gap: 8px; padding: 6px 10px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); color: var(--qss-text-secondary, #9a9aa5); }
.qp-prog-pick span { flex: 1; min-width: 0; }
.qp-prog-err { margin: 6px 10px 0; color: var(--qss-error, #f87171); }
.qp-prog-body { flex: 1; min-height: 0; overflow-y: auto; padding: 4px 10px 12px; }
.qp-prog-empty { padding: 24px 8px; text-align: center; }
.qp-prog-empty p { margin: 0 0 6px; }
.qp-prog-dim { color: var(--qss-text-muted, #6e6e7a); line-height: 1.5; }
.qp-prog-sec h4 { margin: 10px 0 6px; font-size: 11.5px; font-weight: 600; color: var(--qss-text-secondary, #9a9aa5); }
.qp-prog-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(138px, 1fr)); gap: 8px; }
.qp-prog-tile { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
.qp-prog-img {
  position: relative;
  aspect-ratio: 16 / 10;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  padding: 0;
  border: 1px solid var(--qss-border-subtle, #35353d);
  border-radius: 7px;
  background: repeating-conic-gradient(#232329 0% 25%, #1c1c21 0% 50%) 50% / 16px 16px;
  cursor: pointer;
}
.qp-prog-img:hover:not(:disabled) { border-color: var(--qss-border, #47474f); }
.qp-prog-tile.picked .qp-prog-img { border-color: var(--qss-text, #d4d4d8); box-shadow: 0 0 0 1px var(--qss-text, #d4d4d8); }
.qp-prog-img img { width: 100%; height: 100%; object-fit: cover; }
.qp-prog-gone { color: var(--qss-text-muted, #6e6e7a); font-size: 11.5px; }
.qp-prog-tile.gone .qp-prog-img { cursor: default; }
.qp-prog-wait { width: 40%; height: 3px; border-radius: 2px; background: var(--qss-border, #47474f); }
.qp-prog-badge { position: absolute; top: 5px; left: 5px; padding: 1px 7px; border-radius: 999px; background: var(--qss-text, #d4d4d8); color: var(--qss-bg, #18181e); font-size: 10.5px; font-weight: 600; }
.qp-prog-cap { display: flex; align-items: center; gap: 4px; min-width: 0; }
.qp-prog-label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--qss-text-secondary, #9a9aa5); font-size: 11px; }
.qp-prog-pin { display: inline-flex; padding: 3px; border: none; border-radius: 5px; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; }
.qp-prog-pin:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-prog-pin.on { color: var(--qss-text, #d4d4d8); }
</style>
