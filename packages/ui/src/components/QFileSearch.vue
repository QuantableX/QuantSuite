<script setup lang="ts">
/**
 * The workspace file search — the module header's centre piece.
 *
 * Written for QuantCanvas' header and shared since 2026-08-26 (user decision):
 * QuantCode and QuantConsole show the same bar, so a file is one Ctrl+P away
 * wherever you are in QuantSpace. One implementation, one set of keys, one
 * result list.
 *
 * The bar owns the search and nothing else. It walks the workspace, filters,
 * and emits the path it was asked to open — where that file lands is the
 * host's business (a canvas tab, an editor group, the bus).
 *
 * `left` and `right` slots reserve space beside the flexible field for
 * whatever the host wants beside it — QuantCanvas keeps its back/forward
 * buttons and its notes toggle there.
 */
import { computed, nextTick, onMounted, onUnmounted, ref } from 'vue'
import { onClickOutside } from '@vueuse/core'
import { qs, terminalHasFocus, type FileEntry } from '@quantsuite/core'

const props = withDefaults(
  defineProps<{
    /** Workspace root to search. Nothing is listed without one. */
    root?: string | null
    placeholder?: string
    /** Shown in the field while it is idle, and bound as the focus key. */
    shortcut?: string
    /** Cap on rendered rows — the list is for picking, not for browsing. */
    limit?: number
  }>(),
  {
    root: null,
    placeholder: 'Search files...',
    shortcut: 'Ctrl+P',
    limit: 50,
  }
)

const emit = defineEmits<{
  /** A result was picked. The host reads and opens it. */
  (e: 'open', path: string): void
}>()

interface Hit {
  name: string
  path: string
  relativePath: string
}

const query = ref('')
const open = ref(false)
const inputRef = ref<HTMLInputElement | null>(null)
const containerRef = ref<HTMLElement | null>(null)
const files = ref<Hit[]>([])
const selected = ref(0)

onClickOutside(containerRef, () => close())

function flatten(nodes: FileEntry[], base: string, out: Hit[] = []): Hit[] {
  const root = base.replace(/\\/g, '/')
  for (const node of nodes) {
    if (!node.isDirectory) {
      const path = node.path.replace(/\\/g, '/')
      out.push({
        name: node.name,
        path: node.path,
        relativePath: path.startsWith(root + '/') ? path.slice(root.length + 1) : path,
      })
    }
    if (node.children) flatten(node.children, base, out)
  }
  return out
}

/**
 * Walk the workspace once per opening of the list. Gitignore-aware, or a real
 * project returns a hundred thousand entries and the dropdown stops responding.
 */
async function loadFiles() {
  if (!props.root) {
    files.value = []
    return
  }
  try {
    const tree = await qs.files.readDirTree(props.root, true)
    files.value = flatten(tree, props.root)
  } catch {
    files.value = []
  }
}

const results = computed(() => {
  const q = query.value.trim().toLowerCase()
  if (!q) return files.value.slice(0, props.limit)
  return files.value
    .filter((f) => f.relativePath.toLowerCase().includes(q) || f.name.toLowerCase().includes(q))
    .slice(0, props.limit)
})

function onFocus() {
  open.value = true
  selected.value = 0
  void loadFiles()
}

function onInput() {
  open.value = true
  selected.value = 0
}

function onBlur(e: FocusEvent) {
  const next = e.relatedTarget as Node | null
  if (next && containerRef.value?.contains(next)) return
  close()
}

function close() {
  open.value = false
  query.value = ''
  inputRef.value?.blur()
}

function pick(hit: Hit) {
  close()
  emit('open', hit.path)
}

function scrollSelectedIntoView() {
  void nextTick(() => {
    containerRef.value?.querySelector('.qfs-item.is-selected')?.scrollIntoView({ block: 'nearest' })
  })
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.preventDefault()
    close()
    return
  }
  if (e.key === 'ArrowDown') {
    e.preventDefault()
    selected.value = Math.min(selected.value + 1, results.value.length - 1)
    scrollSelectedIntoView()
    return
  }
  if (e.key === 'ArrowUp') {
    e.preventDefault()
    selected.value = Math.max(selected.value - 1, 0)
    scrollSelectedIntoView()
    return
  }
  if (e.key === 'Enter') {
    e.preventDefault()
    const hit = results.value[selected.value]
    if (hit) pick(hit)
  }
}

/**
 * Ctrl+P focuses the field — except while a terminal has the keyboard. Ctrl+P
 * is "previous command" in PSReadLine and readline both, and pulling the caret
 * out of a half-typed command into a file search is the most disruptive thing
 * this shortcut could do.
 */
function onGlobalKeydown(e: KeyboardEvent) {
  if (!(e.ctrlKey || e.metaKey) || e.key !== 'p') return
  if (terminalHasFocus(e.target)) return
  e.preventDefault()
  inputRef.value?.focus()
}

/**
 * Bound by the host, not here: the module stages stay mounted while another
 * module is on screen (V3 warm cache), so a listener armed on mount would keep
 * swallowing Ctrl+P from inside every other module. `arm()`/`disarm()` are
 * exposed for the host's onActivated/onDeactivated.
 */
let armed = false

function arm() {
  if (armed) return
  armed = true
  window.addEventListener('keydown', onGlobalKeydown)
}

function disarm() {
  if (!armed) return
  armed = false
  window.removeEventListener('keydown', onGlobalKeydown)
  close()
}

onUnmounted(disarm)

defineExpose({ arm, disarm, focus: () => inputRef.value?.focus() })
</script>

<template>
  <div ref="containerRef" class="qfs" style="-webkit-app-region: no-drag">
    <div class="qfs-wrap">
      <div class="qfs-left"><slot name="left" /></div>

      <div class="qfs-bar" :class="{ 'is-active': open }">
        <svg class="qfs-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <circle cx="11" cy="11" r="8" /><line x1="21" y1="21" x2="16.65" y2="16.65" />
        </svg>
        <input
          ref="inputRef"
          v-model="query"
          type="text"
          class="qfs-input"
          spellcheck="false"
          :placeholder="placeholder"
          :aria-label="placeholder"
          @focus="onFocus"
          @input="onInput"
          @keydown="onKeydown"
          @blur="onBlur"
        />
        <kbd v-if="!open" class="qfs-kbd">{{ shortcut }}</kbd>
        <button v-if="query" class="qfs-clear" title="Clear" @click="close">
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none" aria-hidden="true">
            <path d="M2 2l6 6M8 2l-6 6" stroke="currentColor" stroke-width="1.5" />
          </svg>
        </button>
      </div>

      <div class="qfs-right"><slot name="right" /></div>

      <div v-if="open && (query || results.length)" class="qfs-results">
        <div v-if="!root" class="qfs-empty">No workspace open</div>
        <div v-else-if="!results.length" class="qfs-empty">No files found</div>
        <template v-else>
          <button
            v-for="(hit, i) in results"
            :key="hit.path"
            class="qfs-item"
            :class="{ 'is-selected': i === selected }"
            @click="pick(hit)"
            @mouseenter="selected = i"
          >
            <span class="qfs-item-name">{{ hit.name }}</span>
            <span class="qfs-item-path">{{ hit.relativePath }}</span>
          </button>
        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.qfs {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  position: relative;
  padding: 0 16px;
}

.qfs-wrap {
  position: relative;
  display: grid;
  grid-template-columns: minmax(max-content, 1fr) minmax(0, 480px) minmax(max-content, 1fr);
  align-items: center;
  column-gap: 8px;
  width: 100%;
  min-width: 0;
}

/* Reserve each action group's real width before sizing the field. Wide
   headers still centre the search; narrow ones shrink it between controls. */
.qfs-left {
  grid-column: 1;
  justify-self: end;
  display: flex;
  align-items: center;
  gap: 2px;
}
.qfs-right {
  grid-column: 3;
  justify-self: start;
  display: flex;
  align-items: center;
  gap: 2px;
}

.qfs-bar {
  grid-column: 2;
  min-width: 0;
  container-type: inline-size;
  position: relative;
  display: flex;
  align-items: center;
  height: 32px;
  border: 1px solid var(--qss-border);
  border-radius: 8px;
  background: var(--qss-bg-raised);
  transition: border-color 150ms ease, background-color 150ms ease, box-shadow 150ms ease;
}
.qfs-bar:hover {
  border-color: color-mix(in srgb, var(--qss-text) 20%, transparent);
}
.qfs-bar.is-active {
  border-color: color-mix(in srgb, var(--qss-text) 30%, transparent);
  background: color-mix(in srgb, var(--qss-text) 4%, var(--qss-bg-raised));
  box-shadow: 0 2px 8px rgb(0 0 0 / 0.15);
}

.qfs-icon {
  position: absolute;
  left: 10px;
  color: var(--qss-text-muted);
  opacity: 0.5;
  pointer-events: none;
}

.qfs-input {
  min-width: 0;
  width: 100%;
  height: 100%;
  padding: 0 32px;
  border: none;
  outline: none;
  background: transparent;
  color: var(--qss-text);
  font-family: var(--qss-font-mono);
  font-size: 12px;
}
.qfs-input::placeholder {
  color: var(--qss-text-muted);
  opacity: 0.5;
}

.qfs-kbd {
  position: absolute;
  right: 8px;
  padding: 1px 5px;
  border: 1px solid var(--qss-border);
  border-radius: 4px;
  background: var(--qss-bg);
  color: var(--qss-text-muted);
  font-family: var(--qss-font-mono);
  font-size: 10px;
  line-height: 1.4;
  opacity: 0.5;
  pointer-events: none;
}

.qfs-clear {
  position: absolute;
  right: 6px;
  display: grid;
  place-items: center;
  width: 18px;
  height: 18px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--qss-text-muted);
  cursor: pointer;
  opacity: 0.5;
}
.qfs-clear:hover {
  opacity: 1;
}

.qfs-results {
  position: absolute;
  grid-column: 2 / 3;
  top: calc(100% + 4px);
  left: 0;
  width: 100%;
  max-height: 320px;
  overflow-y: auto;
  padding: 4px;
  border: 1px solid var(--qss-border);
  border-radius: 8px;
  background: var(--qss-bg-raised);
  box-shadow: 0 8px 32px rgb(0 0 0 / 0.3);
  z-index: 9999;
}

.qfs-empty {
  padding: 12px 16px;
  font-size: 11px;
  color: var(--qss-text-muted);
  text-align: center;
}

.qfs-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 6px 10px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--qss-text);
  font-size: 12px;
  text-align: left;
  cursor: pointer;
}
.qfs-item:hover,
.qfs-item.is-selected {
  background: var(--qss-bg-hover);
}

.qfs-item-name {
  flex-shrink: 0;
  font-weight: 500;
}

.qfs-item-path {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  text-align: right;
  font-size: 11px;
  color: var(--qss-text-muted);
  opacity: 0.6;
}

@container (max-width: 180px) {
  .qfs-input {
    padding-inline: 28px;
  }
  .qfs-kbd {
    display: none;
  }
}
</style>
