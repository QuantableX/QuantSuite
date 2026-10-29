<script setup lang="ts">
/**
 * A thread's menu — right-click, Shift+F10 / the context-menu key, or the
 * row's "…" button. Up/Down move, Right/Enter open a submenu, Left/Escape
 * go back. Snooze → Custom… and Delete… switch the panel to a small form
 * instead of a dialog.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, type Component } from 'vue'
import {
  AlarmClock,
  Archive,
  ArchiveRestore,
  Check,
  ChevronRight,
  Copy,
  FolderGit2,
  Pencil,
  Pin,
  PinOff,
  RefreshCw,
  SquareTerminal,
  Sunrise,
  Trash2,
} from 'lucide-vue-next'
import { usePilotStore } from '#pilot/stores/pilot'
import { useSidebarStore } from '#pilot/stores/sidebar'
import { inHours, parseDuration, tomorrowAt, wakeLabel } from '#pilot/utils/sidebar'
import { threadReference } from '#pilot/utils/format'
import type { SessionMeta } from '#pilot/types'

const props = defineProps<{ thread: SessionMeta; x: number; y: number }>()
const emit = defineEmits<{ close: []; rename: [] }>()

const store = usePilotStore()
const sidebar = useSidebarStore()

interface Item {
  id: string
  label: string
  hint?: string
  icon?: Component
  disabled?: boolean
  danger?: boolean
  checked?: boolean
  children?: Item[]
  run?: () => void | Promise<void>
}

const now = Date.now()
const snoozed = computed(() => Boolean(props.thread.snoozedUntil && props.thread.snoozedUntil > now))
const regenerating = computed(() => Boolean(sidebar.regenerating[props.thread.id]))

const items = computed<Item[]>(() => {
  const t = props.thread
  const list: Item[] = [
    { id: 'rename', label: 'Rename', hint: 'F2', icon: Pencil, run: () => emit('rename') },
    t.pinned
      ? { id: 'unpin', label: 'Unpin', hint: 'Ctrl+Shift+P', icon: PinOff, run: () => sidebar.unpin(t.id) }
      : { id: 'pin', label: 'Pin', hint: 'Ctrl+Shift+P', icon: Pin, run: () => sidebar.pin(t.id) },
    t.settledAt
      ? { id: 'unsettle', label: 'Un-settle', hint: 'Ctrl+Shift+S', icon: ArchiveRestore, run: () => sidebar.unsettle(t.id) }
      : { id: 'settle', label: 'Settle', hint: 'Ctrl+Shift+S', icon: Archive, run: () => sidebar.settle(t.id) },
    snoozed.value
      ? { id: 'wake', label: 'Wake', icon: Sunrise, run: () => sidebar.wake(t.id) }
      : {
          id: 'snooze',
          label: 'Snooze',
          icon: AlarmClock,
          children: [
            { id: 'snooze-3h', label: '3 hours', hint: wakeLabel(inHours(now, 3), now), run: () => snoozeAt(inHours(now, 3)) },
            { id: 'snooze-tomorrow', label: 'Tomorrow', hint: wakeLabel(tomorrowAt(now), now), run: () => snoozeAt(tomorrowAt(now)) },
            { id: 'snooze-custom', label: 'Custom…', run: () => void openPanel('custom') },
          ],
        },
    {
      id: 'auto-settle',
      label: 'Auto-settle',
      children: [
        { id: 'auto-on', label: 'Enabled', checked: t.autoSettle, run: () => sidebar.setAutoSettle(t.id, true) },
        { id: 'auto-off', label: 'Disabled', checked: !t.autoSettle, run: () => sidebar.setAutoSettle(t.id, false) },
      ],
    },
    {
      id: 'regenerate',
      label: regenerating.value ? 'Writing a title…' : 'Regenerate title',
      icon: RefreshCw,
      disabled: regenerating.value || t.kind !== 'chat' || !t.providerSessionId,
      run: () => sidebar.regenerateTitle(t.id),
    },
    {
      id: 'copy',
      label: 'Copy',
      icon: Copy,
      children: [
        { id: 'copy-ref', label: 'Thread reference', hint: 'Ctrl+Shift+C', run: () => copy(threadReference(t)) },
        { id: 'copy-id', label: 'Thread ID', run: () => copy(t.id) },
      ],
    },
  ]
  if (t.worktreePath) {
    list.push({
      id: 'in-worktree',
      label: 'New thread in this worktree',
      icon: FolderGit2,
      run: () => void store.createThread({ contextId: t.contextId, mode: t.mode, provider: t.provider, worktreePath: t.worktreePath ?? undefined }),
    })
  }
  if (t.kind === 'terminal') {
    list.push({ id: 'terminal', label: 'Open in terminal', icon: SquareTerminal, run: () => store.openTerminal(t.id) })
  }
  list.push({ id: 'delete', label: 'Delete…', icon: Trash2, danger: true, run: () => void openPanel('delete') })
  return list
})

function snoozeAt(at: number) {
  return sidebar.snooze(props.thread.id, at, wakeLabel(at, Date.now()))
}

async function copy(text: string) {
  try {
    await navigator.clipboard.writeText(text)
  } catch {
    sidebar.error = 'Could not copy to the clipboard.'
  }
}

// ── Focus and keys ──
const root = ref<HTMLElement | null>(null)
const active = ref(0)
const open = ref<number | null>(null)
const subActive = ref(0)
const panel = ref<'menu' | 'custom' | 'delete'>('menu')

function enabledFrom(list: Item[], from: number, dir: 1 | -1): number {
  const n = list.length
  for (let i = 0; i < n; i++) {
    const at = (from + dir * i + n * 2) % n
    if (!list[at]?.disabled) return at
  }
  return -1
}

async function choose(item: Item | undefined) {
  if (!item || item.disabled) return
  if (item.children) {
    open.value = items.value.indexOf(item)
    subActive.value = Math.max(0, item.children.findIndex((c) => c.checked))
    return
  }
  const keepOpen = item.id === 'snooze-custom' || item.id === 'delete'
  await item.run?.()
  if (!keepOpen) emit('close')
}

function onKey(e: KeyboardEvent) {
  if (panel.value !== 'menu') {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      panel.value = 'menu'
      void nextTick(() => root.value?.focus())
    }
    return
  }
  const sub = open.value !== null ? items.value[open.value]?.children : undefined
  const list = sub ?? items.value
  const cur = sub ? subActive : active
  if (e.key === 'ArrowDown') {
    e.preventDefault()
    cur.value = enabledFrom(list, cur.value + 1, 1)
  } else if (e.key === 'ArrowUp') {
    e.preventDefault()
    cur.value = enabledFrom(list, cur.value - 1 + list.length, -1)
  } else if (e.key === 'Home') {
    e.preventDefault()
    cur.value = enabledFrom(list, 0, 1)
  } else if (e.key === 'End') {
    e.preventDefault()
    cur.value = enabledFrom(list, list.length - 1, -1)
  } else if (e.key === 'ArrowRight' && !sub && items.value[active.value]?.children) {
    e.preventDefault()
    void choose(items.value[active.value])
  } else if (e.key === 'ArrowLeft' && sub) {
    e.preventDefault()
    open.value = null
  } else if (e.key === 'Enter' || e.key === ' ') {
    e.preventDefault()
    void choose(list[cur.value])
  } else if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    if (sub) open.value = null
    else emit('close')
  } else if (e.key === 'Tab') {
    e.preventDefault()
  }
}

function hover(i: number) {
  active.value = i
  open.value = items.value[i]?.children ? i : null
  if (open.value !== null) subActive.value = -1
}

// ── Position: at the pointer, kept inside the window ──
const pos = ref({ left: props.x, top: props.y })
const flyLeft = ref(false)
function place() {
  const el = root.value
  if (!el) return
  const r = el.getBoundingClientRect()
  const maxX = window.innerWidth - 8
  const maxY = window.innerHeight - 8
  pos.value = {
    left: Math.max(8, Math.min(props.x, maxX - r.width)),
    top: Math.max(8, Math.min(props.y, maxY - r.height)),
  }
  flyLeft.value = pos.value.left + r.width + 200 > maxX
}

function onOutside(e: PointerEvent) {
  if (root.value && !root.value.contains(e.target as Node)) emit('close')
}

onMounted(() => {
  void nextTick(() => {
    place()
    root.value?.focus()
  })
  window.addEventListener('pointerdown', onOutside, true)
  window.addEventListener('blur', close)
  window.addEventListener('resize', close)
})
onBeforeUnmount(() => {
  window.removeEventListener('pointerdown', onOutside, true)
  window.removeEventListener('blur', close)
  window.removeEventListener('resize', close)
})
function close() {
  emit('close')
}

// ── Custom snooze ──
const customText = ref('')
const customAt = ref('')
const customInput = ref<HTMLInputElement | null>(null)
const customError = ref('')
async function openPanel(p: 'custom' | 'delete') {
  panel.value = p
  open.value = null
  await nextTick()
  if (p === 'custom') customInput.value?.focus()
  else root.value?.querySelector<HTMLButtonElement>('.qp-tm-danger-btn')?.focus()
  place()
}
function localInputValue(ms: number) {
  const d = new Date(ms)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`
}
const minAt = localInputValue(Date.now())
async function confirmCustom() {
  customError.value = ''
  let at: number | null = null
  if (customText.value.trim()) {
    const ms = parseDuration(customText.value)
    if (ms === null) {
      customError.value = 'Try 90m, 2h or 3d.'
      return
    }
    at = Date.now() + ms
  } else if (customAt.value) {
    at = new Date(customAt.value).getTime()
  }
  if (!at || Number.isNaN(at) || at <= Date.now()) {
    customError.value = 'Pick a time in the future.'
    return
  }
  await snoozeAt(at)
  emit('close')
}

function confirmDelete() {
  sidebar.deleteDeferred(props.thread.id)
  emit('close')
}
</script>

<template>
  <Teleport to="body">
    <div
      ref="root"
      class="qp-tm"
      role="menu"
      tabindex="-1"
      :aria-label="`Thread: ${thread.title || 'New thread'}`"
      :style="{ left: `${pos.left}px`, top: `${pos.top}px` }"
      @keydown="onKey"
      @contextmenu.prevent
    >
      <template v-if="panel === 'menu'">
        <div v-for="(item, i) in items" :key="item.id" class="qp-tm-wrap">
          <button
            class="qp-tm-row"
            role="menuitem"
            tabindex="-1"
            :class="{ on: active === i && open === null, opened: open === i, danger: item.danger }"
            :disabled="item.disabled"
            :aria-haspopup="item.children ? 'menu' : undefined"
            :aria-expanded="item.children ? open === i : undefined"
            @pointerenter="hover(i)"
            @click="choose(item)"
          >
            <component :is="item.icon" v-if="item.icon" class="qp-tm-icon" :size="13" />
            <span v-else class="qp-tm-icon" />
            <span class="qp-tm-label">{{ item.label }}</span>
            <span v-if="item.hint" class="qp-tm-hint">{{ item.hint }}</span>
            <ChevronRight v-if="item.children" class="qp-tm-chev" :size="12" />
          </button>
          <div v-if="open === i && item.children" class="qp-tm qp-tm-sub" :class="{ left: flyLeft }" role="menu">
            <button
              v-for="(c, j) in item.children"
              :key="c.id"
              class="qp-tm-row"
              role="menuitemradio"
              tabindex="-1"
              :class="{ on: subActive === j }"
              :aria-checked="c.checked"
              :disabled="c.disabled"
              @pointerenter="subActive = j"
              @click="choose(c)"
            >
              <Check v-if="c.checked" class="qp-tm-icon" :size="13" />
              <span v-else class="qp-tm-icon" />
              <span class="qp-tm-label">{{ c.label }}</span>
              <span v-if="c.hint" class="qp-tm-hint">{{ c.hint }}</span>
            </button>
          </div>
        </div>
      </template>

      <form v-else-if="panel === 'custom'" class="qp-tm-form" @submit.prevent="confirmCustom">
        <div class="qp-tm-form-title">Snooze until</div>
        <input
          ref="customInput"
          v-model="customText"
          class="qp-tm-input"
          placeholder="Duration: 90m, 2h, 3d"
          aria-label="Snooze for a duration"
          @input="customAt = ''"
        />
        <div class="qp-tm-or">or</div>
        <input
          v-model="customAt"
          class="qp-tm-input"
          type="datetime-local"
          :min="minAt"
          aria-label="Snooze until a date and time"
          @input="customText = ''"
        />
        <div v-if="customError" class="qp-tm-error">{{ customError }}</div>
        <div class="qp-tm-actions">
          <button type="button" class="qp-tm-btn" @click="panel = 'menu'">Back</button>
          <button type="submit" class="qp-tm-btn primary">Snooze</button>
        </div>
      </form>

      <div v-else class="qp-tm-form">
        <div class="qp-tm-form-title">Delete "{{ thread.title || 'New thread' }}"?</div>
        <div class="qp-tm-note">It leaves QuantPilot; the agent CLI's own transcript stays. You can undo for 5 seconds.</div>
        <div class="qp-tm-actions">
          <button type="button" class="qp-tm-btn" @click="panel = 'menu'">Cancel</button>
          <button type="button" class="qp-tm-btn danger qp-tm-danger-btn" @click="confirmDelete">Delete</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.qp-tm {
  position: fixed;
  z-index: 1000;
  min-width: 196px;
  padding: 4px;
  background: var(--qss-bg-raised, #1f1f25);
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 8px;
  box-shadow: 0 10px 28px rgba(0, 0, 0, 0.45);
  color: var(--qss-text, #d4d4d8);
  font-size: 12.5px;
  outline: none;
}
.qp-tm-wrap { position: relative; }
.qp-tm-sub { position: absolute; top: -5px; left: calc(100% + 4px); min-width: 180px; }
.qp-tm-sub.left { left: auto; right: calc(100% + 4px); }
.qp-tm-row {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 5px 8px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
}
.qp-tm-row.on,
.qp-tm-row.opened { background: var(--qss-bg-hover, #313139); }
.qp-tm-row:disabled { opacity: 0.45; cursor: default; }
.qp-tm-row.danger { color: var(--qss-error, #f87171); }
.qp-tm-icon { width: 13px; height: 13px; flex-shrink: 0; color: var(--qss-text-secondary, #9a9aa5); }
.qp-tm-row.danger .qp-tm-icon { color: inherit; }
.qp-tm-label { flex: 1; min-width: 0; white-space: nowrap; }
.qp-tm-hint { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); white-space: nowrap; }
.qp-tm-chev { color: var(--qss-text-muted, #6e6e7a); }
.qp-tm-form { display: flex; flex-direction: column; gap: 7px; padding: 8px; width: 232px; }
.qp-tm-form-title { font-size: 12.5px; font-weight: 600; overflow-wrap: anywhere; }
.qp-tm-note { font-size: 11.5px; line-height: 1.45; color: var(--qss-text-secondary, #9a9aa5); }
.qp-tm-or { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); text-align: center; }
.qp-tm-input {
  background: var(--qss-bg, #18181e);
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 12px;
  padding: 5px 8px;
  outline: none;
  color-scheme: dark;
}
.qp-tm-input:focus { border-color: var(--qss-accent, #7c8cff); }
.qp-tm-error { font-size: 11px; color: var(--qss-warning, #d29a3f); }
.qp-tm-actions { display: flex; justify-content: flex-end; gap: 6px; }
.qp-tm-btn {
  padding: 4px 10px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}
.qp-tm-btn:hover,
.qp-tm-btn:focus-visible { background: var(--qss-bg-hover, #313139); outline: none; }
.qp-tm-btn.primary { background: var(--qss-accent, #7c8cff); border-color: transparent; color: var(--qss-bg, #18181e); }
.qp-tm-btn.danger { background: var(--qss-error, #f87171); border-color: transparent; color: var(--qss-bg, #18181e); }
</style>
