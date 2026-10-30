<script setup lang="ts">
/**
 * Project actions (card 13): per project, commands that run in a Run
 * terminal of the thread's checkout — from the header, by their own key,
 * when a worktree is created (install dependencies) or when a thread
 * settles (`cargo clean`). Drag the grip to reorder; the first action is
 * the header's button.
 */
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { GripVertical, Plus, Trash2 } from 'lucide-vue-next'
import { usePilotStore } from '#pilot/stores/pilot'
import { chordOf, usableKey as usableChord } from '#pilot/utils/keybindings'
import type { ActionIcon, ProjectAction } from '#pilot/types'

const store = usePilotStore()
const ICONS: { id: ActionIcon; label: string }[] = [
  { id: 'play', label: 'Run' },
  { id: 'test', label: 'Test' },
  { id: 'lint', label: 'Lint' },
  { id: 'build', label: 'Build' },
  { id: 'configure', label: 'Configure' },
  { id: 'debug', label: 'Debug' },
]

const projects = computed(() => store.contexts.filter((c) => c.kind !== 'general'))
const project = ref('')
const draft = reactive<Record<string, ProjectAction[]>>({})
const notice = ref('')
const busy = ref(false)
const capturing = ref<number | null>(null)

function pull() {
  for (const k of Object.keys(draft)) delete draft[k]
  for (const [k, list] of Object.entries(store.settings?.actions ?? {})) draft[k] = list.map((a) => ({ ...a }))
}
onMounted(async () => {
  if (!store.settings) await store.loadSettings()
  pull()
  const active = store.sessions.find((s) => s.id === store.activeId)?.contextId
  project.value = projects.value.find((p) => p.id === active)?.id ?? projects.value[0]?.id ?? ''
})
watch(project, () => (capturing.value = null))

const list = computed<ProjectAction[]>(() => draft[project.value] ?? [])

function add() {
  if (!project.value) return
  draft[project.value] = [...list.value, { id: '', name: '', command: '', icon: 'play', key: '', runOnWorktreeCreate: false, runOnSettle: false }]
}
function remove(i: number) {
  draft[project.value] = list.value.filter((_, j) => j !== i)
}

function onKeyCapture(e: KeyboardEvent, i: number) {
  if (e.key === 'Tab') return
  e.preventDefault()
  e.stopPropagation()
  if (e.key === 'Escape') {
    capturing.value = null
    return
  }
  if (e.key === 'Backspace' || e.key === 'Delete') {
    list.value[i].key = ''
    capturing.value = null
    return
  }
  const chord = chordOf(e)
  if (!chord) return
  if (!usableChord(chord)) {
    notice.value = 'Use Ctrl or Alt with a key, or a function key.'
    return
  }
  list.value[i].key = chord
  capturing.value = null
  notice.value = ''
}

// ── Drag to reorder (pointer events; the grip starts it) ──
const dragging = ref<number | null>(null)
const rows = ref<HTMLElement | null>(null)
function onGripDown(e: PointerEvent, i: number) {
  if (e.button !== 0) return
  dragging.value = i
  ;(e.target as HTMLElement).setPointerCapture(e.pointerId)
  e.preventDefault()
}
function onGripMove(e: PointerEvent) {
  const from = dragging.value
  if (from === null || !rows.value) return
  const items = [...rows.value.querySelectorAll<HTMLElement>('.qp-act-row')]
  const to = items.findIndex((el) => {
    const r = el.getBoundingClientRect()
    return e.clientY >= r.top && e.clientY < r.bottom
  })
  if (to < 0 || to === from) return
  const next = [...list.value]
  const [moved] = next.splice(from, 1)
  next.splice(to, 0, moved)
  draft[project.value] = next
  dragging.value = to
}
function onGripUp() {
  dragging.value = null
}

async function save() {
  busy.value = true
  notice.value = ''
  try {
    const actions: Record<string, ProjectAction[]> = {}
    for (const [k, l] of Object.entries(draft)) actions[k] = l.map((a) => ({ ...a }))
    await store.saveSettings({ actions }, { quiet: true })
    pull()
    notice.value = 'Saved.'
  } catch (e) {
    notice.value = e instanceof Error ? e.message : String(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="qp-settings" data-module="pilot">
    <p class="qp-settings-desc">
      Commands for a project, run in a terminal of the thread's checkout — from the thread header, by their key, when a worktree is created
      (install dependencies) or when a thread settles (clean build output). The first one is the header's button.
    </p>
    <label class="qp-settings-field">
      <span>Project</span>
      <select v-model="project" class="qp-input">
        <option v-for="p in projects" :key="p.id" :value="p.id">{{ p.name }}</option>
      </select>
    </label>

    <div ref="rows" class="qp-act-list">
      <div v-for="(a, i) in list" :key="i" class="qp-act-row" :class="{ dragging: dragging === i }">
        <span class="qp-act-grip" title="Drag to reorder" @pointerdown="onGripDown($event, i)" @pointermove="onGripMove" @pointerup="onGripUp" @pointercancel="onGripUp">
          <GripVertical :size="14" />
        </span>
        <div class="qp-act-fields">
          <div class="qp-act-line">
            <input v-model="a.name" class="qp-input qp-act-name" placeholder="Name, e.g. Dev server" spellcheck="false" />
            <select v-model="a.icon" class="qp-input qp-act-icon" aria-label="Icon">
              <option v-for="o in ICONS" :key="o.id" :value="o.id">{{ o.label }}</option>
            </select>
            <button
              class="qp-input qp-act-key"
              :class="{ capturing: capturing === i }"
              :title="capturing === i ? 'Press the keys — Backspace clears, Escape cancels' : 'Click to set a shortcut'"
              @click="capturing = i"
              @keydown="capturing === i && onKeyCapture($event, i)"
              @blur="capturing === i && (capturing = null)"
            >
              {{ capturing === i ? 'Press keys…' : a.key || 'No shortcut' }}
            </button>
            <button class="qp-act-del" title="Remove the action" @click="remove(i)"><Trash2 :size="13" /></button>
          </div>
          <input v-model="a.command" class="qp-input qp-act-cmd" placeholder="Command, e.g. npm run dev" spellcheck="false" />
          <div class="qp-act-line">
            <label class="qp-settings-check"><input v-model="a.runOnWorktreeCreate" type="checkbox" /> <span>Run when a worktree is created</span></label>
            <label class="qp-settings-check"><input v-model="a.runOnSettle" type="checkbox" /> <span>Run when a thread settles</span></label>
          </div>
        </div>
      </div>
      <div v-if="!list.length" class="qp-act-empty">No actions for this project yet.</div>
    </div>

    <div class="qp-act-foot">
      <button class="qp-act-btn" :disabled="!project" @click="add"><Plus :size="12" /> Add action</button>
      <span class="qp-act-notice">{{ notice }}</span>
      <button class="qp-act-btn primary" :disabled="busy" @click="save">Save</button>
    </div>
  </div>
</template>

<style scoped>
.qp-settings { display: flex; flex-direction: column; gap: 10px; font-size: 12.5px; color: var(--qss-text, #d4d4d8); }
.qp-settings-title { margin: 6px 0 0; font-size: 13px; font-weight: 600; }
.qp-settings-desc { margin: 0; color: var(--qss-text-secondary, #9a9aa5); line-height: 1.5; }
.qp-settings-field { display: flex; flex-direction: column; gap: 4px; }
.qp-settings-field > span { font-size: 11.5px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-settings-check { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; color: var(--qss-text-secondary, #9a9aa5); cursor: pointer; }
.qp-input {
  padding: 5px 8px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: var(--qss-radius, 6px);
  background: var(--qss-bg, #18181e);
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 12.5px;
  outline: none;
}
.qp-input:focus { border-color: var(--qss-accent, #a0a0a8); }
.qp-act-list { display: flex; flex-direction: column; gap: 8px; }
.qp-act-row {
  display: flex;
  gap: 6px;
  padding: 8px;
  border: 1px solid var(--qss-border-subtle, #35353d);
  border-radius: 8px;
  background: var(--qss-bg-raised, #1f1f25);
}
.qp-act-row.dragging { border-color: var(--qss-accent, #7aa2f7); }
.qp-act-grip { display: inline-flex; align-items: flex-start; padding-top: 6px; color: var(--qss-text-muted, #6e6e7a); cursor: grab; touch-action: none; }
.qp-act-row.dragging .qp-act-grip { cursor: grabbing; }
.qp-act-fields { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 6px; }
.qp-act-line { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
.qp-act-name { flex: 1; min-width: 140px; }
.qp-act-icon { width: 110px; }
.qp-act-key { min-width: 120px; text-align: left; color: var(--qss-text-secondary, #9a9aa5); cursor: pointer; }
.qp-act-key.capturing { border-color: var(--qss-accent, #7aa2f7); color: var(--qss-text, #d4d4d8); }
.qp-act-cmd { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 12px; }
.qp-act-del { display: inline-flex; padding: 5px; border: none; border-radius: 5px; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; }
.qp-act-del:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-error, #f87171); }
.qp-act-empty { padding: 10px; font-size: 12px; color: var(--qss-text-muted, #6e6e7a); }
.qp-act-foot { display: flex; align-items: center; gap: 8px; }
.qp-act-btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 4px 12px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}
.qp-act-btn:hover:not(:disabled) { background: var(--qss-bg-hover, #313139); }
.qp-act-btn.primary { background: var(--qss-text, #d4d4d8); border-color: transparent; color: var(--qss-bg, #18181e); }
.qp-act-btn:disabled { opacity: 0.45; cursor: default; }
.qp-act-notice { flex: 1; font-size: 12px; color: var(--qss-text-secondary, #9a9aa5); }
</style>
