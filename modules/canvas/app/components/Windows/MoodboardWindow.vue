<script setup lang="ts">
/**
 * A moodboard on the canvas (round 3, card 07; PandaCode's moodboards):
 * reference images you drop or paste (Ctrl+V) — screenshots of other apps,
 * colours, sketches — with captions, in the order you drag them to (pointer
 * events). Agents see them through quantsuite.canvas.moodboard_list /
 * moodboard_read; a chat connected to the board by an edge can take its
 * images as attachments for the next message. Images are copied into the
 * workspace's private QuantSuite folder; nothing is downloaded.
 */
import { computed, onMounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { ImagePlus, Paperclip, Plus, X } from 'lucide-vue-next'
import type { CanvasWindow } from '../../../shared/types'
import { useCanvasStore } from '../../../stores/canvas'
import { useWorkspacesStore } from '../../../stores/workspaces'
import { useThreadStore } from '#pilot/stores/thread'
import { gapAt, imageFiles, moveItem } from '../../utils/moodboard'

interface Summary {
  id: string
  name: string
  images: number
}
interface Image {
  id: string
  path: string
  caption: string
  source: string
}
interface Board {
  id: string
  name: string
  images: Image[]
}

const props = defineProps<{ window: CanvasWindow }>()
const canvasStore = useCanvasStore()
const workspaces = useWorkspacesStore()
const threads = useThreadStore()

const wsId = computed(() => workspaces.contentWorkspaceId!)
const folder = computed(() => workspaces.contentWorkspace?.path ?? null)
const boardId = computed(() => props.window.moodboardConfig?.board ?? null)
function setBoard(board: string | null) {
  canvasStore.updateWindow(wsId.value, props.window.id, { moodboardConfig: { board } })
}

const list = ref<Summary[]>([])
const board = ref<Board | null>(null)
const src = ref<Record<string, string>>({})
const error = ref('')
const notice = ref('')
const busy = ref(false)
function say(text: string) {
  notice.value = text
  setTimeout(() => (notice.value === text ? (notice.value = '') : undefined), 3000)
}

async function loadList() {
  if (!folder.value) return
  try {
    list.value = await invoke<Summary[]>('plugin:canvas|moodboard_list', { workspace: folder.value })
  } catch (e) {
    error.value = String(e)
  }
}
async function loadBoard() {
  if (!folder.value || !boardId.value) {
    board.value = null
    return
  }
  try {
    board.value = await invoke<Board>('plugin:canvas|moodboard_read', { workspace: folder.value, board: boardId.value })
    error.value = ''
    for (const im of board.value.images) {
      if (src.value[im.id]) continue
      invoke<string>('plugin:canvas|moodboard_image', { workspace: folder.value, board: boardId.value, item: im.id })
        .then((url) => (src.value = { ...src.value, [im.id]: url }))
        .catch(() => undefined)
    }
  } catch (e) {
    board.value = null
    error.value = String(e)
  }
}
onMounted(() => {
  void loadList()
  void loadBoard()
})
watch(() => [folder.value, boardId.value], () => void loadBoard())
watch(
  () => board.value?.name,
  (n) => {
    const title = n ? `Moodboard · ${n}` : 'Moodboard'
    if (props.window.title !== title) canvasStore.updateWindow(wsId.value, props.window.id, { title })
  },
  { immediate: true },
)

// ── A new board ──
const naming = ref(false)
const newName = ref('')
async function create() {
  if (!newName.value.trim() || !folder.value) return
  try {
    const b = await invoke<Summary>('plugin:canvas|moodboard_create', { workspace: folder.value, name: newName.value })
    naming.value = false
    newName.value = ''
    setBoard(b.id)
    await loadList()
  } catch (e) {
    error.value = String(e)
  }
}

// ── Adding images: drop, paste, the file dialog ──
async function addData(name: string, data: string) {
  if (!folder.value || !boardId.value) return
  await invoke('plugin:canvas|moodboard_add', { workspace: folder.value, board: boardId.value, name, data, path: null })
}
function readFile(f: File): Promise<string> {
  return new Promise((res, rej) => {
    const r = new FileReader()
    r.onload = () => res(String(r.result))
    r.onerror = () => rej(r.error)
    r.readAsDataURL(f)
  })
}
async function addFiles(files: File[]) {
  if (!boardId.value) {
    error.value = 'Pick or make a moodboard first.'
    return
  }
  const picked = imageFiles(files).map((i) => files[i]!)
  if (!picked.length) {
    error.value = 'Only PNG, JPEG, GIF and WebP images go on a moodboard.'
    return
  }
  busy.value = true
  error.value = ''
  try {
    for (const f of picked) await addData(f.name || 'pasted image', await readFile(f))
    say(`${picked.length} image${picked.length === 1 ? '' : 's'} added.`)
    await loadBoard()
    await loadList()
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}
const dropping = ref(false)
function onDragOver(e: DragEvent) {
  if (!e.dataTransfer?.types.includes('Files')) return
  e.preventDefault()
  e.stopPropagation()
  dropping.value = true
}
function onDrop(e: DragEvent) {
  dropping.value = false
  if (!e.dataTransfer?.files.length) return
  e.preventDefault()
  e.stopPropagation()
  void addFiles(Array.from(e.dataTransfer.files))
}
function onPaste(e: ClipboardEvent) {
  const files = Array.from(e.clipboardData?.items ?? [])
    .filter((i) => i.kind === 'file')
    .map((i) => i.getAsFile())
    .filter((f): f is File => !!f)
  if (!files.length) return
  e.preventDefault()
  e.stopPropagation()
  void addFiles(files)
}
async function browse() {
  if (!boardId.value || !folder.value) return
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const picked = await open({ multiple: true, title: 'Images for the moodboard', filters: [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp'] }] })
    const paths = Array.isArray(picked) ? picked : typeof picked === 'string' ? [picked] : []
    for (const p of paths) await invoke('plugin:canvas|moodboard_add', { workspace: folder.value, board: boardId.value, name: p.split(/[\\/]/).pop() ?? 'image', data: null, path: p })
    if (paths.length) {
      say(`${paths.length} image${paths.length === 1 ? '' : 's'} added.`)
      await loadBoard()
      await loadList()
    }
  } catch (e) {
    error.value = String(e)
  }
}

// ── Captions and removing ──
async function saveCaption(im: Image, value: string) {
  if (!folder.value || !boardId.value || value.trim() === im.caption) return
  im.caption = value.trim()
  await invoke('plugin:canvas|moodboard_caption', { workspace: folder.value, board: boardId.value, item: im.id, caption: im.caption }).catch((e) => (error.value = String(e)))
}
async function remove(im: Image) {
  if (!folder.value || !boardId.value) return
  await invoke('plugin:canvas|moodboard_remove', { workspace: folder.value, board: boardId.value, item: im.id }).catch((e) => (error.value = String(e)))
  await loadBoard()
  await loadList()
}

// ── Reorder by drag (pointer events) ──
const grid = ref<HTMLElement | null>(null)
const drag = ref<{ id: string; x0: number; y0: number; active: boolean } | null>(null)
const gap = ref<number | null>(null)
function onDown(e: PointerEvent, im: Image) {
  if (e.button !== 0 || (e.target as HTMLElement).closest('input, button')) return
  drag.value = { id: im.id, x0: e.clientX, y0: e.clientY, active: false }
  ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
}
function onMove(e: PointerEvent) {
  const d = drag.value
  if (!d) return
  if (!d.active && Math.hypot(e.clientX - d.x0, e.clientY - d.y0) > 5) d.active = true
  if (!d.active) return
  const cards = Array.from(grid.value?.querySelectorAll<HTMLElement>('[data-mood-item]') ?? [])
  const i = cards.findIndex((c) => {
    const r = c.getBoundingClientRect()
    return e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom
  })
  if (i >= 0) gap.value = gapAt(i, cards[i]!.getBoundingClientRect(), e.clientX)
}
async function onUp() {
  const d = drag.value
  const to = gap.value
  drag.value = null
  gap.value = null
  if (!d?.active || to === null || !board.value || !folder.value) return
  const ids = board.value.images.map((x) => x.id)
  const from = ids.indexOf(d.id)
  const next = moveItem(board.value.images, from, to)
  if (next.map((x) => x.id).join() === ids.join()) return
  board.value = { ...board.value, images: next }
  await invoke('plugin:canvas|moodboard_reorder', { workspace: folder.value, board: board.value.id, order: next.map((x) => x.id) }).catch((e) => (error.value = String(e)))
}
/** Where the drop line shows: before card `i` (left edge) or after the last. */
const lineBefore = (i: number) => drag.value?.active && gap.value === i
const lineAfterLast = (i: number) => drag.value?.active && gap.value === i + 1 && i === (board.value?.images.length ?? 0) - 1

// ── A connected chat takes images as attachments ──
const chatThread = computed(() => {
  const st = canvasStore.activeCanvasState
  const ids = new Set((st?.edges ?? []).filter((e) => e.from === props.window.id || e.to === props.window.id).flatMap((e) => [e.from, e.to]))
  return st?.windows.find((w) => w.type === 'chat' && ids.has(w.id))?.chatConfig?.threadId ?? null
})
async function attach(images: Image[]) {
  const id = chatThread.value
  if (!id) return
  busy.value = true
  try {
    for (const im of images) {
      const url = src.value[im.id] ?? (await invoke<string>('plugin:canvas|moodboard_image', { workspace: folder.value, board: boardId.value, item: im.id }))
      const saved = await threads.saveImage(id, im.caption || im.source || 'moodboard', url.split(',')[1] ?? '')
      if (saved) window.dispatchEvent(new CustomEvent('qp-add-attachment', { detail: { threadId: id, attachment: saved } }))
    }
    say(`${images.length} image${images.length === 1 ? '' : 's'} added to the chat's next message.`)
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}

/** Keys meant for the board stay there: Space does not pan the canvas, Ctrl+V pastes here. */
function keepKeys(e: KeyboardEvent) {
  if (e.code === 'Space' || (e.ctrlKey && e.key.toLowerCase() === 'v')) e.stopPropagation()
}
</script>

<template>
  <div class="qc-mood" tabindex="0" :class="{ dropping }" @keydown="keepKeys" @paste="onPaste" @dragover="onDragOver" @dragleave="dropping = false" @drop="onDrop">
    <div v-if="!folder" class="qc-mood-empty">Moodboards live in a workspace — open one to collect references here.</div>
    <template v-else>
      <div class="qc-mood-bar">
        <select class="qc-mood-pick" :value="boardId ?? ''" aria-label="Moodboard" @change="setBoard(($event.target as HTMLSelectElement).value || null)">
          <option value="">{{ list.length ? 'Pick a moodboard…' : 'No moodboards yet' }}</option>
          <option v-for="b in list" :key="b.id" :value="b.id">{{ b.name }} ({{ b.images }})</option>
        </select>
        <button class="qc-mood-btn" title="A new moodboard" @click="naming = !naming"><Plus :size="13" /></button>
        <span class="qc-mood-spacer" />
        <template v-if="board">
          <button class="qc-mood-btn" :disabled="busy" title="Add images from files" @click="browse"><ImagePlus :size="13" /> Add</button>
          <button v-if="chatThread && board.images.length" class="qc-mood-btn" :disabled="busy" title="Add every image to the connected chat's next message" @click="attach(board.images)">
            <Paperclip :size="13" /> All to chat
          </button>
        </template>
      </div>
      <form v-if="naming" class="qc-mood-new" @submit.prevent="create">
        <input v-model="newName" class="qc-mood-input" placeholder="Name, e.g. Calm dashboards" aria-label="Moodboard name" spellcheck="false" autofocus />
        <button class="qc-mood-btn primary" type="submit" :disabled="!newName.trim()">Create</button>
        <button class="qc-mood-btn" type="button" @click="naming = false">Cancel</button>
      </form>
      <p v-if="error" class="qc-mood-err" role="alert">{{ error }}</p>
      <p v-if="notice" class="qc-mood-note" role="status">{{ notice }}</p>

      <div class="qc-mood-body">
        <div v-if="!board" class="qc-mood-empty">
          {{ list.length ? 'Pick a moodboard above, or make a new one with +.' : 'Make a moodboard with +, then drop or paste reference images into it.' }}
        </div>
        <div v-else-if="!board.images.length" class="qc-mood-empty">
          Drop images here, paste one (Ctrl+V), or Add them from files. Agents see this board through quantsuite.canvas.moodboard_read.
        </div>
        <div v-else ref="grid" class="qc-mood-grid">
          <figure
            v-for="(im, i) in board.images"
            :key="im.id"
            class="qc-mood-card"
            :class="{ dragging: drag?.active && drag.id === im.id, before: lineBefore(i), after: lineAfterLast(i) }"
            :data-mood-item="im.id"
            @pointerdown.stop="onDown($event, im)"
            @pointermove="onMove"
            @pointerup="onUp"
            @pointercancel="onUp"
          >
            <div class="qc-mood-img">
              <img v-if="src[im.id]" :src="src[im.id]" :alt="im.caption || im.source" draggable="false" />
              <span v-else class="qc-mood-wait" />
              <button class="qc-mood-x" title="Remove from the board" aria-label="Remove" @click="remove(im)"><X :size="12" /></button>
              <button v-if="chatThread" class="qc-mood-att" title="Add to the connected chat's next message" aria-label="Add to the chat" @click="attach([im])"><Paperclip :size="12" /></button>
            </div>
            <input class="qc-mood-cap" :value="im.caption" placeholder="Caption" aria-label="Caption" spellcheck="false" @change="saveCaption(im, ($event.target as HTMLInputElement).value)" @keydown.enter="($event.target as HTMLInputElement).blur()" />
          </figure>
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.qc-mood { width: 100%; height: 100%; min-height: 0; display: flex; flex-direction: column; background: var(--qss-bg, #18181e); color: var(--qss-text, #d4d4d8); font-size: 12px; outline: none; }
.qc-mood.dropping { box-shadow: inset 0 0 0 2px var(--qss-accent, #7aa2f7); }
.qc-mood-bar { display: flex; align-items: center; gap: 6px; padding: 6px 8px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); }
.qc-mood-pick { max-width: 240px; height: 26px; padding: 0 6px; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; background: var(--qss-bg-raised, #1f1f25); color: inherit; font: inherit; }
.qc-mood-spacer { flex: 1; }
.qc-mood-btn { display: inline-flex; align-items: center; gap: 5px; height: 26px; padding: 0 8px; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; background: transparent; color: var(--qss-text, #d4d4d8); font: inherit; cursor: pointer; white-space: nowrap; }
.qc-mood-btn:hover:not(:disabled) { background: var(--qss-bg-hover, #313139); }
.qc-mood-btn.primary { border-color: var(--qss-text, #d4d4d8); background: var(--qss-text, #d4d4d8); color: var(--qss-bg, #18181e); font-weight: 600; }
.qc-mood-btn:disabled { opacity: 0.5; cursor: default; }
.qc-mood-new { display: flex; gap: 6px; padding: 6px 8px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); }
.qc-mood-input { flex: 1; min-width: 0; padding: 5px 8px; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; background: var(--qss-bg, #18181e); color: inherit; font: inherit; outline: none; }
.qc-mood-err { margin: 0; padding: 4px 10px; color: var(--qss-error, #f87171); }
.qc-mood-note { margin: 0; padding: 4px 10px; color: var(--qss-text-secondary, #9a9aa5); }
.qc-mood-body { flex: 1; min-height: 0; overflow-y: auto; padding: 10px; }
.qc-mood-empty { margin: 40px auto; max-width: 380px; text-align: center; line-height: 1.55; color: var(--qss-text-muted, #6e6e7a); }
.qc-mood-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(170px, 1fr)); gap: 12px; }
.qc-mood-card { position: relative; margin: 0; display: flex; flex-direction: column; gap: 5px; min-width: 0; cursor: grab; touch-action: none; user-select: none; }
.qc-mood-card.dragging { opacity: 0.4; cursor: grabbing; }
.qc-mood-card.before::before,
.qc-mood-card.after::after { content: ''; position: absolute; top: 0; bottom: 26px; width: 3px; border-radius: 2px; background: var(--qss-accent, #7aa2f7); }
.qc-mood-card.before::before { left: -8px; }
.qc-mood-card.after::after { right: -8px; }
.qc-mood-img { position: relative; aspect-ratio: 4 / 3; display: flex; align-items: center; justify-content: center; overflow: hidden; border: 1px solid var(--qss-border-subtle, #35353d); border-radius: 8px; background: repeating-conic-gradient(#232329 0% 25%, #1c1c21 0% 50%) 50% / 16px 16px; }
.qc-mood-img img { width: 100%; height: 100%; object-fit: cover; pointer-events: none; }
.qc-mood-wait { width: 40%; height: 3px; border-radius: 2px; background: var(--qss-border, #47474f); }
.qc-mood-x,
.qc-mood-att { position: absolute; top: 5px; display: none; padding: 3px; border: none; border-radius: 5px; background: rgba(0, 0, 0, 0.6); color: #fff; cursor: pointer; }
.qc-mood-x { right: 5px; }
.qc-mood-att { right: 30px; }
.qc-mood-card:hover .qc-mood-x,
.qc-mood-card:hover .qc-mood-att { display: inline-flex; }
.qc-mood-cap { min-width: 0; padding: 3px 6px; border: 1px solid transparent; border-radius: 5px; background: transparent; color: var(--qss-text-secondary, #9a9aa5); font: inherit; outline: none; }
.qc-mood-cap:hover { border-color: var(--qss-border-subtle, #35353d); }
.qc-mood-cap:focus { border-color: var(--qss-accent, #a0a0a8); color: var(--qss-text, #d4d4d8); }
</style>
