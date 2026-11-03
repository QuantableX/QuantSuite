<script setup lang="ts">
/**
 * A QuantPilot chat on the canvas (pre-1.1.0). The window keeps only the
 * thread id in the canvas layout; the thread is a normal QuantPilot thread —
 * in its sidebar, in sync with it, and left alone when the window closes.
 * Without a thread the window offers a new one in the canvas's workspace or
 * an existing one. File references in the chat open as File windows beside
 * it instead of leaving the canvas.
 */
import { computed, onMounted, ref, watch } from 'vue'
import { ChevronDown } from 'lucide-vue-next'
import { v4 as uuidv4 } from 'uuid'
import { invoke } from '@tauri-apps/api/core'
import type { CanvasWindow, FilePreviewKind } from '../../../shared/types'
import { useCanvasStore } from '../../../stores/canvas'
import { useWorkspacesStore } from '../../../stores/workspaces'
import { usePilotStore } from '#pilot/stores/pilot'
import type { AgentSetup } from '#pilot/utils/agentMenu'

const props = defineProps<{ window: CanvasWindow }>()
const canvasStore = useCanvasStore()
const workspaces = useWorkspacesStore()
const pilot = usePilotStore()

const workspaceId = computed(() => workspaces.contentWorkspaceId!)
const threadId = computed(() => props.window.chatConfig?.threadId ?? null)
/** The canvas's folder as a QuantPilot project; General for the general canvas. */
const context = computed(() => (workspaces.contentWorkspaceId === workspaces.GENERAL_CONTENT ? 'general' : workspaces.contentWorkspaceId ?? 'general'))
const busy = ref(false)
const error = ref('')
const filter = ref('')

onMounted(() => {
  if (!pilot.loaded) void pilot.load()
})

const thread = computed(() => (threadId.value ? pilot.sessions.find((s) => s.id === threadId.value) ?? null : null))
const missing = computed(() => Boolean(threadId.value) && pilot.loaded && !thread.value)
// The window is titled after its thread.
watch(
  () => thread.value?.title,
  (t) => {
    const title = t?.trim() ? `Chat · ${t.trim()}` : 'Chat'
    if (props.window.title !== title) canvasStore.updateWindow(workspaceId.value, props.window.id, { title })
  },
  { immediate: true }
)

const candidates = computed(() => {
  const q = filter.value.trim().toLowerCase()
  return pilot.sessions
    .filter((s) => s.kind !== 'terminal' && (!q || s.title.toLowerCase().includes(q)))
    .sort((a, b) => Number(b.contextId === context.value) - Number(a.contextId === context.value) || (b.lastActivityAt ?? 0) - (a.lastActivityAt ?? 0))
    .slice(0, 40)
})

function attach(id: string | null) {
  canvasStore.updateWindow(workspaceId.value, props.window.id, { chatConfig: { threadId: id } })
}

/** ▾ beside "New thread": QuantPilot's agent menu (round 2, card 02). */
const newMore = ref<HTMLElement | null>(null)
const menuOpen = ref(false)
async function newThread(setup?: AgentSetup) {
  menuOpen.value = false
  busy.value = true
  error.value = ''
  try {
    const t = await invoke<{ id: string }>('plugin:pilot|pilot_thread_create', { req: { contextId: context.value, ...(setup ?? {}) } })
    await pilot.refreshSessions()
    attach(t.id)
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}

// ── Opening a file from the chat: a File window beside this one ──
const IMAGE = /\.(png|jpe?g|gif|webp|bmp|svg|ico)$/i
const MARKDOWN = /\.(md|markdown|mdx)$/i
const CODE: Record<string, string> = { ts: 'typescript', tsx: 'typescript', js: 'javascript', jsx: 'javascript', vue: 'html', rs: 'rust', py: 'python', json: 'json', css: 'css', html: 'html', toml: 'ini', yaml: 'yaml', yml: 'yaml', sh: 'shell', ps1: 'powershell', go: 'go', java: 'java', c: 'c', cpp: 'cpp', cs: 'csharp', sql: 'sql' }
function previewOf(path: string): { kind: FilePreviewKind; language?: string } {
  if (IMAGE.test(path)) return { kind: 'image' }
  if (MARKDOWN.test(path)) return { kind: 'markdown', language: 'markdown' }
  const ext = path.split('.').pop()?.toLowerCase() ?? ''
  return CODE[ext] ? { kind: 'code', language: CODE[ext] } : { kind: 'text' }
}
function openFile(path: string, _line: number | null): boolean {
  const cwd = thread.value?.cwd
  const absolute = /^([a-zA-Z]:[\\/]|[\\/]{2}|\/)/.test(path) || !cwd ? path : `${cwd.replace(/[\\/]+$/, '')}/${path}`
  const p = previewOf(absolute)
  const pos = props.window.position
  canvasStore.addWindow(workspaceId.value, {
    id: uuidv4(),
    type: 'file',
    title: absolute.split(/[/\\]/).pop() ?? absolute,
    position: { x: pos.x + pos.width + 24, y: pos.y, width: 520, height: Math.max(320, Math.min(pos.height, 560)) },
    status: 'idle',
    minimized: false,
    zIndex: 0,
    fileConfig: { filePath: absolute, previewKind: p.kind, language: p.language },
  })
  return true
}
const host = { kind: 'canvas', openFile, openLabel: 'Open in a File window' }

/** Keys meant for the chat stay there: Space does not arm canvas panning, Ctrl+V does not paste a window. */
function keepKeys(e: KeyboardEvent) {
  if (e.code === 'Space' || (e.ctrlKey && e.key.toLowerCase() === 'v')) e.stopPropagation()
}
</script>

<template>
  <div class="canvas-chat" @keydown="keepKeys">
    <PilotEmbedChat v-if="threadId && !missing" :thread-id="threadId" :host="host" />
    <div v-else class="canvas-chat__pick">
      <p v-if="missing" class="canvas-chat__note">The thread of this window no longer exists. Start another or pick one.</p>
      <span class="canvas-chat__newgroup">
        <button class="canvas-chat__new" :disabled="busy" @click="newThread()">{{ busy ? 'Starting…' : 'New thread in this workspace' }}</button>
        <button ref="newMore" class="canvas-chat__new canvas-chat__more" :disabled="busy" title="With another agent or model" aria-haspopup="menu" :aria-expanded="menuOpen" @click.stop="menuOpen = !menuOpen"><ChevronDown :size="13" /></button>
      </span>
      <PilotAgentMenu v-if="menuOpen" :anchor="newMore" title="New thread with" :project="context" @pick="newThread" @close="menuOpen = false" />
      <p v-if="error" class="canvas-chat__err">{{ error }}</p>
      <input v-model="filter" class="canvas-chat__filter" placeholder="Or pick a QuantPilot thread…" aria-label="Find a thread" />
      <div class="canvas-chat__list" role="listbox">
        <button v-for="s in candidates" :key="s.id" class="canvas-chat__row" role="option" @click="attach(s.id)">
          <span class="canvas-chat__title">{{ s.title || 'New thread' }}</span>
          <span class="canvas-chat__ctx">{{ s.contextId === context ? 'this workspace' : s.contextName }}</span>
        </button>
        <p v-if="pilot.loaded && !candidates.length" class="canvas-chat__note">No threads yet.</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.canvas-chat { width: 100%; height: 100%; min-height: 0; display: flex; flex-direction: column; background: var(--qss-bg, #18181e); }
.canvas-chat__pick { flex: 1; min-height: 0; display: flex; flex-direction: column; gap: 8px; padding: 14px; font-size: 12.5px; color: var(--qss-text, #d4d4d8); }
.canvas-chat__new { align-self: flex-start; padding: 6px 12px; border: 1px solid var(--qss-accent, #7aa2f7); border-radius: 7px; background: transparent; color: var(--qss-text, #d4d4d8); font: inherit; cursor: pointer; }
.canvas-chat__new:disabled { opacity: 0.5; cursor: default; }
.canvas-chat__newgroup { display: inline-flex; align-self: flex-start; }
.canvas-chat__newgroup .canvas-chat__new:first-child { border-radius: 7px 0 0 7px; }
.canvas-chat__more { display: inline-flex; align-items: center; padding: 6px 8px; border-left: none; border-radius: 0 7px 7px 0; }
.canvas-chat__filter { height: 28px; padding: 0 8px; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; background: var(--qss-bg, #18181e); color: inherit; font: inherit; }
.canvas-chat__list { flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 1px; }
.canvas-chat__row { display: flex; justify-content: space-between; gap: 10px; padding: 6px 8px; border: none; border-radius: 6px; background: transparent; color: inherit; font: inherit; text-align: left; cursor: pointer; }
.canvas-chat__row:hover { background: var(--qss-bg-hover, #313139); }
.canvas-chat__title { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.canvas-chat__ctx { flex-shrink: 0; color: var(--qss-text-muted, #6e6e7a); font-size: 11.5px; }
.canvas-chat__note { margin: 0; color: var(--qss-text-muted, #6e6e7a); }
.canvas-chat__err { margin: 0; color: var(--qss-error, #f87171); }
</style>
