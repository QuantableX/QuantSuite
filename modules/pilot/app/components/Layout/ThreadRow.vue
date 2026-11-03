<script setup lang="ts">
/**
 * One sidebar row: the agent's logo (a status ring around it while it works,
 * waits, hit a limit or failed), the title
 * (an input while renaming), project · time, and what needs you — pending
 * requests, an error, a limit, an unread finished turn. The hover action
 * settles, un-settles or wakes; drag it across rows to do several at once.
 * Files dropped from Explorer open the thread with them attached.
 */
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import { Archive, ArchiveRestore, Ellipsis, Pin, Sunrise } from 'lucide-vue-next'
import { timeAgo } from '#pilot/utils/format'
import { elapsedLabel, wakeLabel, type RowStatus, type Section } from '#pilot/utils/sidebar'
import { useGitStore } from '#pilot/stores/git'
import { useCrewStore } from '#pilot/stores/crew'
import { countLabel, pendingGates } from '#pilot/utils/crew'
import { openPr } from '#pilot/utils/gitActions'
import type { SessionMeta } from '#pilot/types'

const props = defineProps<{
  thread: SessionMeta
  section: Section
  status: RowStatus
  pending: number
  unread: boolean
  active: boolean
  focused: boolean
  renaming: boolean
  regenerating: boolean
  /** Shown with every project in view. */
  project: string | null
  providerLabel: string
  /** The row being dragged: its slot stays, empty, until the others close the gap. */
  source: boolean
  inRange: boolean
  now: number
  /** The working turn's start (round 2, card 12: elapsed time on the row). */
  startedAt?: number | null
}>()

const git = useGitStore()
/** The thread's open pull request, else its latest one. */
const pr = computed(() => openPr(git.links[props.thread.id]) ?? (git.links[props.thread.id] ?? []).filter((l) => l.kind === 'pr').at(-1) ?? null)
/** The crew this thread leads (round 2, card 07): done of all, and a decision waiting. */
const crews = useCrewStore()
const team = computed(() => {
  const s = crews.ofThread(props.thread.id)
  if (!s || s.crew.orchestratorId !== props.thread.id) return null
  return { label: countLabel(s.tasks), gates: pendingGates(s).length }
})

const emit = defineEmits<{
  open: []
  menu: [x: number, y: number]
  rename: [title: string]
  renameCancel: []
  dragStart: [e: PointerEvent]
  actionStart: [e: PointerEvent]
  files: [files: File[]]
  focus: []
}>()

const title = computed(() => props.thread.title || 'New thread')
const isDraft = computed(() => props.thread.kind === 'chat' && !props.thread.providerSessionId && !props.thread.title)
/** A V2 terminal row may have no agent yet (''); a chat thread without one is not known yet. */
const agent = computed(() => (props.thread.kind === 'terminal' ? props.thread.provider : props.thread.provider || null))

// A one-second clock only while this row works (round 2, card 12).
const tick = ref(Date.now())
let ticker: ReturnType<typeof setInterval> | null = null
watch(
  () => props.status === 'working' && Boolean(props.startedAt),
  (on) => {
    if (ticker) clearInterval(ticker)
    ticker = on ? setInterval(() => (tick.value = Date.now()), 1000) : null
    tick.value = Date.now()
  },
  { immediate: true },
)
onBeforeUnmount(() => ticker && clearInterval(ticker))
const elapsed = computed(() => (props.status === 'working' && props.startedAt ? elapsedLabel(tick.value - props.startedAt) : ''))

const sub = computed(() => {
  const parts: string[] = []
  if (props.project) parts.push(props.project)
  if (elapsed.value) {
    parts.push(`working ${elapsed.value}`)
    return parts.join(' · ')
  }
  if (props.section === 'snoozed' && props.thread.snoozedUntil) parts.push(`until ${wakeLabel(props.thread.snoozedUntil, props.now)}`)
  else if (isDraft.value) parts.push('Draft')
  else parts.push(timeAgo(props.thread.lastActivityAt || props.thread.updatedAt))
  return parts.join(' · ')
})

const action = computed(() => {
  if (props.section === 'settled') return { icon: ArchiveRestore, label: 'Un-settle (drag across rows for several)' }
  if (props.section === 'snoozed') return { icon: Sunrise, label: 'Wake (drag across rows for several)' }
  return { icon: Archive, label: 'Settle (drag across rows for several)' }
})

// ── Rename ──
const input = ref<HTMLInputElement | null>(null)
const draft = ref('')
watch(
  () => props.renaming,
  async (on) => {
    if (!on) return
    draft.value = props.thread.title
    await nextTick()
    input.value?.focus()
    input.value?.select()
  },
  { immediate: true }
)
let cancelled = false
function onRenameKey(e: KeyboardEvent) {
  e.stopPropagation()
  if (e.key === 'Enter') {
    e.preventDefault()
    input.value?.blur()
  } else if (e.key === 'Escape') {
    e.preventDefault()
    cancelled = true
    emit('renameCancel')
  }
}
function onRenameBlur() {
  if (cancelled) {
    cancelled = false
    return
  }
  emit('rename', draft.value)
}

// ── Files from Explorer (HTML5 is the only way in for OS files; in-app dragging is pointer events) ──
const fileOver = ref(false)
function hasFiles(e: DragEvent) {
  return Array.from(e.dataTransfer?.types ?? []).includes('Files')
}
function onDragOver(e: DragEvent) {
  if (!hasFiles(e)) return
  e.preventDefault()
  if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy'
  fileOver.value = true
}
function onDrop(e: DragEvent) {
  fileOver.value = false
  if (!hasFiles(e)) return
  e.preventDefault()
  // The row takes them: the module's own drop target must not as well.
  e.stopPropagation()
  const files = Array.from(e.dataTransfer?.files ?? [])
  if (files.length) emit('files', files)
}

function onPointerDown(e: PointerEvent) {
  if (e.button !== 0 || props.renaming) return
  emit('dragStart', e)
}
function onContextMenu(e: MouseEvent) {
  e.preventDefault()
  emit('menu', e.clientX, e.clientY)
}
function openMenuFromButton(e: MouseEvent) {
  const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
  emit('menu', r.left, r.bottom + 2)
}
</script>

<template>
  <div
    class="qp-row"
    role="option"
    :data-thread="thread.id"
    data-nav
    :aria-selected="active"
    :tabindex="focused ? 0 : -1"
    :class="{ active, source, 'in-range': inRange, 'file-over': fileOver }"
    @click="!renaming && emit('open')"
    @pointerdown="onPointerDown"
    @contextmenu="onContextMenu"
    @focus="emit('focus')"
    @dragover="onDragOver"
    @dragleave="fileOver = false"
    @drop="onDrop"
  >
    <span class="qp-row-mark" :data-status="status" :data-provider="thread.provider">
      <PilotAgentIcon :agent="agent" :size="14" :status="status" :label="providerLabel" />
    </span>
    <span class="qp-row-text">
      <input
        v-if="renaming"
        ref="input"
        v-model="draft"
        class="qp-row-input"
        aria-label="Thread title"
        @keydown="onRenameKey"
        @blur="onRenameBlur"
        @click.stop
        @pointerdown.stop
      />
      <span v-else class="qp-row-title" :class="{ draft: isDraft, regenerating }">{{ regenerating ? 'Writing a title…' : title }}</span>
      <span class="qp-row-sub">
        <Pin v-if="section === 'pinned'" class="qp-row-pin" :size="10" />
        <span class="qp-row-sub-text">{{ sub }}</span>
        <span v-for="tag in (thread.tags ?? []).slice(0, 3)" :key="tag" class="qp-row-tag">#{{ tag }}</span>
        <PilotGitPrChip v-if="pr" :link="pr" compact />
        <span v-if="team" class="qp-row-crew" :title="`Leads a crew: ${team.label} tasks done`">crew <span class="mono">{{ team.label }}</span></span>
        <span v-if="team?.gates" class="qp-row-flag warn">Decision</span>
        <span v-if="status === 'limited'" class="qp-row-flag warn">Limited</span>
        <span v-else-if="status === 'error'" class="qp-row-flag err">Failed</span>
      </span>
    </span>
    <span class="qp-row-end">
      <span v-if="pending" class="qp-row-pill" :title="`${pending} waiting for you`">{{ pending }}</span>
      <span v-else-if="status === 'waiting'" class="qp-row-dot needs" title="Needs you" />
      <span v-else-if="unread" class="qp-row-dot unread" title="Finished while you were away" />
      <span class="qp-row-actions">
        <button
          class="qp-row-act"
          :title="action.label"
          :aria-label="action.label"
          :tabindex="focused ? 0 : -1"
          @pointerdown.stop="emit('actionStart', $event)"
          @click.stop
        >
          <component :is="action.icon" :size="13" />
        </button>
        <button class="qp-row-act" title="More (Shift+F10)" aria-label="Thread menu" :tabindex="focused ? 0 : -1" @pointerdown.stop @click.stop="openMenuFromButton">
          <Ellipsis :size="13" />
        </button>
      </span>
    </span>
  </div>
</template>

<style scoped>
.qp-row {
  position: relative;
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0 6px;
  padding: 5px 6px;
  border-radius: 6px;
  color: var(--qss-text, #d4d4d8);
  cursor: pointer;
  user-select: none;
  outline: none;
  transition: transform 140ms ease;
}
.qp-row:hover { background: var(--qss-bg-hover, #313139); }
.qp-row.active { background: var(--qss-bg-card, #292930); }
.qp-row:focus-visible { box-shadow: inset 0 0 0 1px var(--qss-accent, #7c8cff); }
.qp-row.source { visibility: hidden; }
.qp-row.in-range { background: color-mix(in srgb, var(--qss-accent, #7c8cff) 18%, transparent); }
.qp-row.file-over { box-shadow: inset 0 0 0 1px var(--qss-accent, #7c8cff); background: color-mix(in srgb, var(--qss-accent, #7c8cff) 12%, transparent); }
@media (prefers-reduced-motion: reduce) {
  .qp-row { transition: none; }
}
.qp-row-mark {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  flex-shrink: 0;
  color: var(--qss-text, #d4d4d8);
}
.qp-row-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
.qp-row-title { font-size: 12.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-row-title.draft,
.qp-row-title.regenerating { color: var(--qss-text-secondary, #9a9aa5); }
.qp-row-input {
  width: 100%;
  min-width: 0;
  padding: 1px 4px;
  margin: -2px -4px;
  background: var(--qss-bg, #18181e);
  border: 1px solid var(--qss-accent, #7c8cff);
  border-radius: 4px;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 12.5px;
  outline: none;
}
.qp-row-sub { display: flex; align-items: center; gap: 4px; min-width: 0; font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-row-sub-text { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-row-tag { flex-shrink: 0; font-size: 10.5px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-row-crew { flex-shrink: 0; font-size: 10.5px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-row-pin { flex-shrink: 0; color: var(--qss-text-secondary, #9a9aa5); }
.qp-row-flag { flex-shrink: 0; font-size: 10.5px; }
.qp-row-flag.warn { color: var(--qss-warning, #d29a3f); }
.qp-row-flag.err { color: var(--qss-error, #f87171); }
.qp-row-end { display: flex; align-items: center; gap: 4px; flex-shrink: 0; }
.qp-row-pill {
  min-width: 16px;
  height: 16px;
  padding: 0 5px;
  border-radius: 8px;
  background: var(--qss-warning, #d29a3f);
  color: var(--qss-bg, #18181e);
  font-family: var(--qss-font-mono, ui-monospace, monospace);
  font-size: 10px;
  font-weight: 600;
  line-height: 16px;
  text-align: center;
}
.qp-row-dot { width: 7px; height: 7px; border-radius: 50%; }
.qp-row-dot.needs { background: var(--qss-warning, #d29a3f); box-shadow: 0 0 5px var(--qss-warning, #d29a3f); }
.qp-row-dot.unread { background: var(--qss-accent, #7c8cff); }
.qp-row-actions { display: none; align-items: center; gap: 1px; }
.qp-row:hover .qp-row-actions,
.qp-row:focus-within .qp-row-actions { display: flex; }
.qp-row:hover .qp-row-dot,
.qp-row:hover .qp-row-pill { display: none; }
.qp-row-act {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  padding: 0;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--qss-text-muted, #6e6e7a);
  cursor: pointer;
}
.qp-row-act:hover { background: var(--qss-bg-card, #292930); color: var(--qss-text, #d4d4d8); }
</style>
