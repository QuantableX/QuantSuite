<script setup lang="ts">
/**
 * A design artboard on the canvas (round 3, card 06; PandaCode's design
 * canvas): an HTML mockup — written by an agent through
 * quantsuite.canvas.design_write, or started here — in a sandboxed frame at
 * desktop, tablet or phone width. "Comment" picks an element and pins a
 * comment to it (a CSS selector); "Ask the agent" sends the comment to the
 * chat this window is connected to, or to a new chat beside it. The mockup
 * reloads when it changes. Stored in the workspace's private QuantSuite
 * folder; Export copies it into a folder you pick.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useTauriEvent } from '@quantsuite/core'
import { Check, Download, MessageSquarePlus, MessagesSquare, Plus, RefreshCw, RotateCcw, Send, Trash2 } from 'lucide-vue-next'
import { v4 as uuidv4 } from 'uuid'
import type { ArtboardDevice, CanvasWindow } from '../../../shared/types'
import { useCanvasStore } from '../../../stores/canvas'
import { useWorkspacesStore } from '../../../stores/workspaces'
import { usePilotStore } from '#pilot/stores/pilot'
import { useThreadStore } from '#pilot/stores/thread'
import { DEVICES, askText, frameDoc, starterHtml } from '../../utils/artboard'

interface Comment {
  id: string
  selector: string
  quote: string
  text: string
  status: string
  createdAt: number
  resolvedAt: number | null
  note: string
}
interface Artboard {
  id: string
  name: string
  updatedAt: number
  openComments: number
  resolvedComments: number
}
interface Full extends Artboard {
  html: string
  comments: Comment[]
}

const props = defineProps<{ window: CanvasWindow }>()
const canvasStore = useCanvasStore()
const workspaces = useWorkspacesStore()
const pilot = usePilotStore()
const threads = useThreadStore()

const wsId = computed(() => workspaces.contentWorkspaceId!)
const folder = computed(() => workspaces.contentWorkspace?.path ?? null)
const cfg = computed(() => props.window.artboardConfig ?? { artboard: null, device: 'desktop' as ArtboardDevice })
function setCfg(patch: Partial<{ artboard: string | null; device: ArtboardDevice }>) {
  canvasStore.updateWindow(wsId.value, props.window.id, { artboardConfig: { ...cfg.value, ...patch } })
}

const list = ref<Artboard[]>([])
const full = ref<Full | null>(null)
const error = ref('')
const busy = ref(false)

async function loadList() {
  if (!folder.value) return
  try {
    list.value = await invoke<Artboard[]>('plugin:canvas|design_list', { workspace: folder.value })
  } catch (e) {
    error.value = String(e)
  }
}
async function loadOne() {
  const id = cfg.value.artboard
  if (!folder.value || !id) {
    full.value = null
    return
  }
  try {
    full.value = await invoke<Full>('plugin:canvas|design_read', { workspace: folder.value, artboard: id })
    error.value = ''
  } catch (e) {
    full.value = null
    error.value = String(e)
  }
}
onMounted(() => {
  void loadList()
  void loadOne()
})
watch(() => [folder.value, cfg.value.artboard], () => void loadOne())
// The mockup reloads when it (or its comments) change — an agent's design_write included.
useTauriEvent<{ workspace: string; id: string }>('canvas-design-changed', (p) => {
  if (!folder.value || p.workspace !== folder.value) return
  void loadList()
  if (p.id === cfg.value.artboard) void loadOne()
})

// The window is titled after its artboard.
watch(
  () => full.value?.name,
  (n) => {
    const title = n ? `Artboard · ${n}` : 'Artboard'
    if (props.window.title !== title) canvasStore.updateWindow(wsId.value, props.window.id, { title })
  },
  { immediate: true },
)

// ── New artboard ──
const naming = ref(false)
const newName = ref('')
async function create() {
  const name = newName.value.trim()
  if (!name || !folder.value) return
  busy.value = true
  try {
    const a = await invoke<Artboard>('plugin:canvas|design_write', { workspace: folder.value, name, html: starterHtml(name), artboard: null })
    naming.value = false
    newName.value = ''
    setCfg({ artboard: a.id })
    await loadList()
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}

// ── The frame ──
const frame = ref<HTMLIFrameElement | null>(null)
const stage = ref<HTMLElement | null>(null)
const stageSize = ref({ w: 800, h: 500 })
let ro: ResizeObserver | null = null
onMounted(() => {
  ro = new ResizeObserver(([e]) => (stageSize.value = { w: e!.contentRect.width, h: e!.contentRect.height }))
  if (stage.value) ro.observe(stage.value)
})
watch(stage, (el) => el && ro?.observe(el))
const device = computed(() => DEVICES.find((d) => d.id === cfg.value.device) ?? DEVICES[0]!)
const scale = computed(() => Math.min(1, Math.max(0.1, (stageSize.value.w - 24) / device.value.width)))
const frameH = computed(() => Math.max(200, (stageSize.value.h - 24) / scale.value))
const srcdoc = computed(() => (full.value ? frameDoc(full.value.html) : ''))
const nonce = ref(0)
function reload() {
  nonce.value++
  void loadOne()
}

function post(msg: Record<string, unknown>) {
  frame.value?.contentWindow?.postMessage({ qcArt: 1, ...msg }, '*')
}
const shownComments = computed(() => (full.value?.comments ?? []).filter((c) => showResolved.value || c.status !== 'resolved'))
function watchPins() {
  post({ type: 'watch', selectors: shownComments.value.map((c) => c.selector) })
}
watch(shownComments, () => watchPins())
const rects = ref<({ x: number; y: number; w: number; h: number } | null)[]>([])
const picking = ref(false)
const draft = ref<{ selector: string; quote: string; text: string } | null>(null)
function onMessage(e: MessageEvent) {
  if (!frame.value || e.source !== frame.value.contentWindow) return
  const d = e.data ?? {}
  if (d.qcArt !== 1) return
  if (d.type === 'ready') watchPins()
  else if (d.type === 'rects') rects.value = Array.isArray(d.rects) ? d.rects : []
  else if (d.type === 'picked') {
    picking.value = false
    panelOpen.value = true
    draft.value = { selector: String(d.selector ?? ''), quote: String(d.quote ?? ''), text: '' }
    void nextTick(() => document.querySelector<HTMLTextAreaElement>(`[data-art-draft="${props.window.id}"]`)?.focus())
  } else if (d.type === 'cancel') picking.value = false
}
onMounted(() => window.addEventListener('message', onMessage))
onBeforeUnmount(() => {
  window.removeEventListener('message', onMessage)
  ro?.disconnect()
})
function togglePick() {
  picking.value = !picking.value
  post({ type: 'pick', on: picking.value })
}

// ── Comments ──
const panelOpen = ref(true)
const showResolved = ref(false)
const active = ref<string | null>(null)
async function saveDraft() {
  const d = draft.value
  if (!d || !d.text.trim() || !full.value || !folder.value) return
  busy.value = true
  try {
    const c = await invoke<Comment>('plugin:canvas|design_comment_add', { workspace: folder.value, artboard: full.value.id, selector: d.selector, text: d.text, quote: d.quote })
    draft.value = null
    active.value = c.id
    await loadOne()
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}
async function setStatus(c: Comment, status: 'open' | 'resolved') {
  if (!full.value || !folder.value) return
  await invoke('plugin:canvas|design_comment_status', { workspace: folder.value, artboard: full.value.id, comment: c.id, status }).catch((e) => (error.value = String(e)))
  await loadOne()
}
const deleting = ref<string | null>(null)
async function remove(c: Comment) {
  if (!full.value || !folder.value) return
  await invoke('plugin:canvas|design_comment_delete', { workspace: folder.value, artboard: full.value.id, comment: c.id }).catch((e) => (error.value = String(e)))
  deleting.value = null
  await loadOne()
}

/** The chat window this artboard is connected to (a canvas edge). */
function linkedChat(): CanvasWindow | null {
  const st = canvasStore.activeCanvasState
  const ids = new Set((st?.edges ?? []).filter((e) => e.from === props.window.id || e.to === props.window.id).flatMap((e) => [e.from, e.to]))
  return st?.windows.find((w) => w.type === 'chat' && ids.has(w.id)) ?? null
}
const notice = ref('')
async function ask(c: Comment) {
  if (!full.value) return
  busy.value = true
  error.value = ''
  try {
    const chat = linkedChat()
    let threadId = chat?.chatConfig?.threadId ?? null
    if (!threadId) {
      const t = await pilot.createThread({ contextId: wsId.value }, false)
      if (!t) throw new Error(pilot.lastError || 'The chat could not be started.')
      threadId = t.id
      if (chat) canvasStore.updateWindow(wsId.value, chat.id, { chatConfig: { threadId } })
      else {
        const pos = props.window.position
        const id = uuidv4()
        canvasStore.addWindow(wsId.value, { id, type: 'chat', title: 'Chat', position: { x: pos.x + pos.width + 32, y: pos.y, width: 520, height: Math.max(480, pos.height) }, status: 'idle', minimized: false, zIndex: 0, chatConfig: { threadId } })
        canvasStore.addEdge(wsId.value, props.window.id, id, 'link')
      }
    }
    const ok = await threads.send(threadId, askText(full.value, c))
    if (!ok) throw new Error(threads.view(threadId)?.error || 'The message was not sent.')
    notice.value = chat ? 'Sent to the connected chat.' : 'Sent to a new chat beside the artboard.'
    setTimeout(() => (notice.value = ''), 2600)
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    busy.value = false
  }
}

// ── Export ──
async function exportTo() {
  if (!full.value || !folder.value) return
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const picked = await open({ directory: true, multiple: false, title: 'Export the mockup into this folder', defaultPath: folder.value })
    if (typeof picked !== 'string') return
    const path = await invoke<string>('plugin:canvas|design_export', { workspace: folder.value, artboard: full.value.id, folder: picked })
    notice.value = `Exported to ${path}`
    setTimeout(() => (notice.value = ''), 4000)
  } catch (e) {
    error.value = String(e)
  }
}

/** Keys meant for the artboard stay there (Space does not pan the canvas). */
function keepKeys(e: KeyboardEvent) {
  if (e.code === 'Space' || (e.ctrlKey && e.key.toLowerCase() === 'v')) e.stopPropagation()
}
const pinNo = (c: Comment) => shownComments.value.findIndex((x) => x.id === c.id) + 1
</script>

<template>
  <div class="qc-art" @keydown="keepKeys">
    <div v-if="!folder" class="qc-art-empty">Artboards live in a workspace — open one to design here.</div>
    <template v-else>
      <div class="qc-art-bar">
        <select class="qc-art-pick" :value="cfg.artboard ?? ''" aria-label="Artboard" @change="setCfg({ artboard: ($event.target as HTMLSelectElement).value || null })">
          <option value="">{{ list.length ? 'Pick an artboard…' : 'No artboards yet' }}</option>
          <option v-for="a in list" :key="a.id" :value="a.id">{{ a.name }}{{ a.openComments ? ` (${a.openComments})` : '' }}</option>
        </select>
        <button class="qc-art-btn" title="A new artboard" @click="naming = !naming"><Plus :size="13" /></button>
        <span class="qc-art-seg" role="radiogroup" aria-label="Device">
          <button v-for="d in DEVICES" :key="d.id" role="radio" :aria-checked="cfg.device === d.id" :class="{ on: cfg.device === d.id }" :title="`${d.label} · ${d.width}px`" @click="setCfg({ device: d.id })">{{ d.label }}</button>
        </span>
        <span class="qc-art-spacer" />
        <template v-if="full">
          <button class="qc-art-btn" :class="{ on: picking }" :aria-pressed="picking" title="Pick an element and comment on it (Esc cancels)" @click="togglePick"><MessageSquarePlus :size="13" /> {{ picking ? 'Pick an element…' : 'Comment' }}</button>
          <button class="qc-art-btn" title="Reload the mockup" @click="reload"><RefreshCw :size="13" /></button>
          <button class="qc-art-btn" title="Export a copy into a folder" @click="exportTo"><Download :size="13" /></button>
          <button class="qc-art-btn" :class="{ on: panelOpen }" :title="panelOpen ? 'Hide the comments' : 'Show the comments'" @click="panelOpen = !panelOpen"><MessagesSquare :size="13" /> {{ full.openComments }}</button>
        </template>
      </div>
      <form v-if="naming" class="qc-art-new" @submit.prevent="create">
        <input v-model="newName" class="qc-art-input" placeholder="Name, e.g. Landing page" aria-label="Artboard name" spellcheck="false" autofocus />
        <button class="qc-art-btn primary" type="submit" :disabled="busy || !newName.trim()">Create</button>
        <button class="qc-art-btn" type="button" @click="naming = false">Cancel</button>
      </form>
      <p v-if="error" class="qc-art-err" role="alert">{{ error }}</p>
      <p v-if="notice" class="qc-art-note" role="status">{{ notice }}</p>

      <div class="qc-art-body">
        <div ref="stage" class="qc-art-stage">
          <div v-if="!full" class="qc-art-empty">
            <template v-if="list.length">Pick an artboard above, or make a new one.</template>
            <template v-else>No artboards in this workspace yet. Make one with +, or ask an agent to design one — it writes it with quantsuite.canvas.design_write and it opens here.</template>
          </div>
          <div v-else class="qc-art-device" :style="{ width: `${device.width * scale}px`, height: `${frameH * scale}px` }">
            <iframe
              :key="`${full.id}-${nonce}-${full.updatedAt}`"
              ref="frame"
              class="qc-art-frame"
              sandbox="allow-scripts"
              :srcdoc="srcdoc"
              :title="full.name"
              :style="{ width: `${device.width}px`, height: `${frameH}px`, transform: `scale(${scale})` }"
            />
            <button
              v-for="(c, i) in shownComments"
              v-show="rects[i]"
              :key="c.id"
              class="qc-art-pin"
              :class="{ resolved: c.status === 'resolved', on: active === c.id }"
              :style="rects[i] ? { left: `${(rects[i]!.x + rects[i]!.w) * scale - 9}px`, top: `${rects[i]!.y * scale - 9}px` } : {}"
              :title="c.text"
              @click="active = c.id; panelOpen = true"
            >
              {{ i + 1 }}
            </button>
          </div>
        </div>

        <aside v-if="full && panelOpen" class="qc-art-panel" aria-label="Comments">
          <div class="qc-art-panel-head">
            <span>Comments</span>
            <label class="qc-art-check"><input v-model="showResolved" type="checkbox" /> Resolved</label>
          </div>
          <div v-if="draft" class="qc-art-draft">
            <div class="qc-art-el" :title="draft.selector">{{ draft.quote || draft.selector }}</div>
            <textarea v-model="draft.text" :data-art-draft="window.id" class="qc-art-input" rows="3" placeholder="What should change here?" @keydown.enter.ctrl.prevent="saveDraft" @keydown.esc.stop="draft = null" />
            <div class="qc-art-row">
              <button class="qc-art-btn" @click="draft = null">Cancel</button>
              <button class="qc-art-btn primary" :disabled="busy || !draft.text.trim()" @click="saveDraft">Pin comment</button>
            </div>
          </div>
          <div class="qc-art-list">
            <div v-for="c in shownComments" :key="c.id" class="qc-art-c" :class="{ on: active === c.id, resolved: c.status === 'resolved' }" :data-comment="c.id" @click="active = c.id">
              <div class="qc-art-c-head">
                <span class="qc-art-no">{{ pinNo(c) }}</span>
                <span class="qc-art-el" :title="c.selector">{{ c.quote || c.selector }}</span>
              </div>
              <div class="qc-art-c-text">{{ c.text }}</div>
              <div v-if="c.status === 'resolved' && c.note" class="qc-art-c-note">Resolved: {{ c.note }}</div>
              <div class="qc-art-row">
                <button v-if="c.status !== 'resolved'" class="qc-art-btn" :disabled="busy" title="Send it to the connected chat (or a new one)" @click.stop="ask(c)"><Send :size="12" /> Ask the agent</button>
                <button v-if="c.status !== 'resolved'" class="qc-art-btn" title="Resolve" @click.stop="setStatus(c, 'resolved')"><Check :size="12" /></button>
                <button v-else class="qc-art-btn" title="Reopen" @click.stop="setStatus(c, 'open')"><RotateCcw :size="12" /></button>
                <button v-if="deleting !== c.id" class="qc-art-btn" title="Delete" @click.stop="deleting = c.id"><Trash2 :size="12" /></button>
                <template v-else>
                  <button class="qc-art-btn danger" @click.stop="remove(c)">Delete</button>
                  <button class="qc-art-btn" @click.stop="deleting = null">Keep</button>
                </template>
              </div>
            </div>
            <p v-if="!shownComments.length && !draft" class="qc-art-hint">No {{ showResolved ? '' : 'open ' }}comments. Click Comment, then an element of the mockup.</p>
          </div>
        </aside>
      </div>
    </template>
  </div>
</template>

<style scoped>
.qc-art { width: 100%; height: 100%; min-height: 0; display: flex; flex-direction: column; background: var(--qss-bg, #18181e); color: var(--qss-text, #d4d4d8); font-size: 12px; }
.qc-art-bar { display: flex; align-items: center; gap: 6px; padding: 6px 8px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); flex-wrap: wrap; }
.qc-art-pick { max-width: 220px; height: 26px; padding: 0 6px; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; background: var(--qss-bg-raised, #1f1f25); color: inherit; font: inherit; }
.qc-art-seg { display: inline-flex; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; overflow: hidden; }
.qc-art-seg button { padding: 3px 9px; border: none; background: transparent; color: var(--qss-text-secondary, #9a9aa5); font: inherit; cursor: pointer; }
.qc-art-seg button + button { border-left: 1px solid var(--qss-border-subtle, #35353d); }
.qc-art-seg button.on { background: var(--qss-bg-card, #292930); color: var(--qss-text, #d4d4d8); }
.qc-art-spacer { flex: 1; }
.qc-art-btn { display: inline-flex; align-items: center; gap: 5px; height: 26px; padding: 0 8px; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; background: transparent; color: var(--qss-text, #d4d4d8); font: inherit; cursor: pointer; white-space: nowrap; }
.qc-art-btn:hover:not(:disabled) { background: var(--qss-bg-hover, #313139); }
.qc-art-btn.on { background: var(--qss-bg-card, #292930); border-color: var(--qss-text-secondary, #9a9aa5); }
.qc-art-btn.primary { border-color: var(--qss-text, #d4d4d8); background: var(--qss-text, #d4d4d8); color: var(--qss-bg, #18181e); font-weight: 600; }
.qc-art-btn.danger { border-color: color-mix(in srgb, var(--qss-error, #f87171) 55%, transparent); color: var(--qss-error, #f87171); }
.qc-art-btn:disabled { opacity: 0.5; cursor: default; }
.qc-art-new { display: flex; gap: 6px; padding: 6px 8px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); }
.qc-art-input { flex: 1; min-width: 0; padding: 5px 8px; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; background: var(--qss-bg, #18181e); color: inherit; font: inherit; outline: none; resize: vertical; }
.qc-art-input:focus { border-color: var(--qss-accent, #a0a0a8); }
.qc-art-err { margin: 0; padding: 4px 10px; color: var(--qss-error, #f87171); }
.qc-art-note { margin: 0; padding: 4px 10px; color: var(--qss-text-secondary, #9a9aa5); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qc-art-body { flex: 1; min-height: 0; display: flex; }
.qc-art-stage { position: relative; flex: 1; min-width: 0; min-height: 0; overflow: hidden; display: flex; justify-content: center; padding: 12px; background: repeating-conic-gradient(#1c1c21 0% 25%, #19191d 0% 50%) 50% / 20px 20px; }
.qc-art-empty { margin: auto; max-width: 420px; padding: 24px; text-align: center; line-height: 1.55; color: var(--qss-text-muted, #6e6e7a); }
.qc-art-device { position: relative; flex-shrink: 0; box-shadow: 0 6px 30px rgba(0, 0, 0, 0.45); border-radius: 4px; overflow: hidden; background: #fff; }
.qc-art-frame { position: absolute; top: 0; left: 0; border: none; background: #fff; transform-origin: 0 0; }
.qc-art-pin { position: absolute; z-index: 2; width: 18px; height: 18px; padding: 0; border: 2px solid #fff; border-radius: 50% 50% 50% 0; background: #e5484d; color: #fff; font: inherit; font-size: 10px; font-weight: 700; line-height: 14px; text-align: center; cursor: pointer; box-shadow: 0 1px 4px rgba(0, 0, 0, 0.4); }
.qc-art-pin.resolved { background: #46a758; }
.qc-art-pin.on { outline: 2px solid var(--qss-accent, #7aa2f7); outline-offset: 1px; }
.qc-art-panel { width: 260px; flex-shrink: 0; display: flex; flex-direction: column; min-height: 0; border-left: 1px solid var(--qss-border-subtle, #35353d); }
.qc-art-panel-head { display: flex; align-items: center; justify-content: space-between; padding: 8px 10px; font-weight: 600; }
.qc-art-check { display: inline-flex; align-items: center; gap: 4px; font-weight: 400; color: var(--qss-text-secondary, #9a9aa5); cursor: pointer; }
.qc-art-draft { display: flex; flex-direction: column; gap: 6px; margin: 0 8px 8px; padding: 8px; border: 1px solid var(--qss-accent, #7aa2f7); border-radius: 8px; }
.qc-art-list { flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 6px; padding: 0 8px 10px; }
.qc-art-c { display: flex; flex-direction: column; gap: 5px; padding: 8px; border: 1px solid var(--qss-border-subtle, #35353d); border-radius: 8px; cursor: pointer; }
.qc-art-c.on { border-color: var(--qss-text-secondary, #9a9aa5); }
.qc-art-c.resolved { opacity: 0.7; }
.qc-art-c-head { display: flex; align-items: center; gap: 6px; min-width: 0; }
.qc-art-no { flex-shrink: 0; width: 18px; height: 18px; border-radius: 50%; background: #e5484d; color: #fff; font-size: 10px; font-weight: 700; display: inline-flex; align-items: center; justify-content: center; }
.qc-art-c.resolved .qc-art-no { background: #46a758; }
.qc-art-el { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--qss-text-muted, #6e6e7a); font-size: 11.5px; }
.qc-art-c-text { line-height: 1.45; white-space: pre-wrap; overflow-wrap: anywhere; }
.qc-art-c-note { color: var(--qss-text-secondary, #9a9aa5); font-size: 11.5px; line-height: 1.45; }
.qc-art-row { display: flex; flex-wrap: wrap; gap: 4px; }
.qc-art-hint { margin: 0; padding: 6px 2px; color: var(--qss-text-muted, #6e6e7a); line-height: 1.5; }
</style>
