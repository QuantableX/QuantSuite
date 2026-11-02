<script setup lang="ts">
/**
 * The context menu of a file reference or a link in a reply (parity 20):
 * right-click, Shift+F10 or the Menu key on it. A file opens here (the Files
 * tab, at its line), in an editor that is installed (VS Code, Cursor) or in
 * Explorer, or its path is copied; a link opens in the system browser or the
 * Browser tab, or is copied. Arrow keys move, Enter runs, Escape closes and
 * gives the focus back. The exit is animated but takes no input: a closed
 * menu never catches a click.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import { pilot } from '#pilot/utils/invoke'
import { openExternal } from '#pilot/utils/external'
import { useInspectorStore } from '#pilot/stores/inspector'

const props = defineProps<{
  x: number
  y: number
  threadId: string | null
  file?: { path: string; line: number | null } | null
  href?: string | null
  from: HTMLElement | null
  /** An embedding module's own open action (pre-1.1.0 embed); replaces the Files tab. */
  hostOpen?: ((path: string, line: number | null) => boolean) | null
  hostLabel?: string
}>()
const emit = defineEmits<{ close: []; notice: [text: string] }>()

interface Row {
  id: string
  label: string
  hint?: string
  disabled?: string
  run: () => void | Promise<void>
}

const editors = useState<{ id: string; label: string }[] | null>('pilot-editors', () => null)
const root = ref<HTMLElement | null>(null)
const active = ref(0)

async function copy(text: string, what: string) {
  try {
    await navigator.clipboard.writeText(text)
    emit('notice', `Copied ${what}`)
  } catch {
    emit('notice', 'The clipboard is not available.')
  }
}
async function openIn(editor: string) {
  if (!props.threadId || !props.file) return
  try {
    localStorage.setItem('pilot.openIn', editor)
  } catch {
    /* private mode */
  }
  await pilot('plugin:pilot|pilot_open_in', { id: props.threadId, path: props.file.path, line: props.file.line, editor }).catch((e) => emit('notice', String(e)))
}

const rows = computed<Row[]>(() => {
  const noThread = props.threadId ? undefined : 'Open a thread first'
  if (props.file) {
    const f = props.file
    const at = f.line ? ` at line ${f.line}` : ''
    return [
      props.hostOpen
        ? { id: 'here', label: props.hostLabel || 'Open', hint: 'Enter', run: () => void props.hostOpen!(f.path, f.line) }
        : { id: 'here', label: 'Open in the Files tab', hint: 'Enter', disabled: noThread, run: () => useInspectorStore().openFile(props.threadId!, f.path, f.line) },
      ...(editors.value ?? [])
        .filter((e) => e.id !== 'explorer')
        .map<Row>((e) => ({ id: `ed-${e.id}`, label: `Open in ${e.label}${at}`, disabled: noThread, run: () => openIn(e.id) })),
      { id: 'explorer', label: 'Show in Explorer', disabled: noThread, run: () => openIn('explorer') },
      { id: 'copy', label: 'Copy path', run: () => copy(f.line ? `${f.path}:${f.line}` : f.path, 'the path') },
    ]
  }
  const href = props.href ?? ''
  const web = /^https?:/i.test(href)
  return [
    { id: 'open', label: web ? 'Open in the system browser' : 'Open', hint: 'Enter', run: () => void openExternal(href) },
    ...(web && !props.hostOpen ? [{ id: 'tab', label: 'Open in the Browser tab', disabled: noThread, run: () => useInspectorStore().openBrowser(props.threadId!, href) }] : []),
    { id: 'copy', label: 'Copy link', run: () => copy(href, 'the link') },
  ]
})

function close(refocus: boolean) {
  emit('close')
  if (refocus) props.from?.focus({ preventScroll: true })
}
async function runAt(i: number) {
  const r = rows.value[i]
  if (!r || r.disabled) return
  close(true)
  await r.run()
}
function onKey(e: KeyboardEvent) {
  const n = rows.value.length
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault()
    const dir = e.key === 'ArrowDown' ? 1 : -1
    for (let k = 1; k <= n; k++) {
      const i = (active.value + dir * k + n) % n
      if (!rows.value[i]!.disabled) {
        active.value = i
        break
      }
    }
    root.value?.querySelectorAll<HTMLElement>('[role="menuitem"]')[active.value]?.focus()
  } else if (e.key === 'Enter' || e.key === ' ') {
    e.preventDefault()
    void runAt(active.value)
  } else if (e.key === 'Escape' || e.key === 'Tab') {
    e.preventDefault()
    close(true)
  }
}
function onOutside(e: PointerEvent) {
  if (!root.value?.contains(e.target as Node)) close(false)
}
const style = computed(() => ({ left: `${Math.min(props.x, window.innerWidth - 260)}px`, top: `${Math.min(props.y, window.innerHeight - 40 - rows.value.length * 30)}px` }))

onMounted(async () => {
  if (props.file && !editors.value) editors.value = (await pilot<{ id: string; label: string }[]>('plugin:pilot|pilot_editors').catch(() => null)) ?? [{ id: 'explorer', label: 'Explorer' }]
  // Input works from the first frame of the entrance.
  await nextTick()
  root.value?.querySelector<HTMLElement>('[role="menuitem"]:not([aria-disabled="true"])')?.focus()
  window.addEventListener('pointerdown', onOutside, true)
  window.addEventListener('blur', () => close(false), { once: true })
})
onBeforeUnmount(() => window.removeEventListener('pointerdown', onOutside, true))
</script>

<template>
  <div ref="root" class="qp-lm" role="menu" :aria-label="file ? `Actions for ${file.path}` : 'Link actions'" :style="style" @keydown="onKey" @contextmenu.prevent>
    <div class="qp-lm-title">{{ file ? file.path + (file.line ? `:${file.line}` : '') : href }}</div>
    <button
      v-for="(r, i) in rows"
      :key="r.id"
      role="menuitem"
      class="qp-lm-row"
      :class="{ on: i === active }"
      :aria-disabled="!!r.disabled"
      :title="r.disabled ?? ''"
      tabindex="-1"
      @pointerenter="!r.disabled && (active = i)"
      @click="runAt(i)"
    >
      <span>{{ r.label }}</span>
      <span v-if="r.disabled" class="qp-lm-why">{{ r.disabled }}</span>
      <span v-else-if="r.hint" class="qp-lm-hint">{{ r.hint }}</span>
    </button>
  </div>
</template>

<style scoped>
.qp-lm { position: fixed; z-index: 1200; min-width: 220px; max-width: 360px; padding: 4px; display: flex; flex-direction: column; border: 1px solid var(--qss-border, #47474f); border-radius: 8px; background: var(--qss-bg-raised, #1f1f25); box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4); font-size: 12.5px; }
.qp-lm-title { padding: 4px 8px 6px; color: var(--qss-text-muted, #6e6e7a); font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-lm-row { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 5px 8px; border: none; border-radius: 5px; background: transparent; color: var(--qss-text, #d4d4d8); font: inherit; text-align: left; cursor: pointer; outline: none; }
.qp-lm-row.on, .qp-lm-row:focus-visible { background: var(--qss-bg-hover, #313139); }
.qp-lm-row[aria-disabled='true'] { color: var(--qss-text-muted, #6e6e7a); cursor: default; }
.qp-lm-hint, .qp-lm-why { color: var(--qss-text-muted, #6e6e7a); font-size: 11px; }
</style>
