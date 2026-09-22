<script setup lang="ts">
/**
 * The command palette, v2 (PLAN-V2 E5): one ranked list over everything the
 * suite knows.
 *
 *   actions — navigation and quick capture ("New note" …)
 *   modules — the eight surfaces
 *   entities — core.db search: kanban cards, workspaces, whatever modules
 *     index (client-side fallback over the kanban store in browser dev)
 *   notes / to-dos — the shared documents, searched client-side (they are
 *     documents, not entities, by design)
 *   files — content search in the open workspace via QuantCode's plugin,
 *     debounced and only from 3 characters (it walks the folder)
 *   module actions — whatever modules registered via `registerPaletteActions`
 *     (PLAN-CONSOLE §P6), grouped under their own heading. The palette must not
 *     import from `modules\**`, so these arrive through the core registry.
 *
 * Choosing a row either navigates (module/entity/file), opens the drawer
 * on the right tab (`qss:drawer` window event — the drawer listens), or runs a
 * registered action's own callback.
 */
import { computed, nextTick, onUnmounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import {
  appForModule,
  isModuleEnabled,
  isRouteEnabled,
  bus,
  modules,
  notesStore,
  paletteActions,
  qs,
  todosStore,
  type Entity,
  type ModuleInfo,
  type NoteSection,
  type PaletteAction,
  type TodoSection,
} from '@quantsuite/core'

const props = defineProps<{
  open: boolean
  /** Active workspace folder — enables the file source. */
  workspacePath?: string | null
}>()
const emit = defineEmits<{
  (e: 'close'): void
  (e: 'navigate', route: string): void
}>()

interface ActionRow {
  module?: string
  label: string
  hint: string
  run: () => void
}
type Row =
  | { kind: 'action'; action: ActionRow }
  | { kind: 'registered'; action: PaletteAction }
  | { kind: 'module'; module: ModuleInfo }
  | { kind: 'entity'; entity: Entity }
  | { kind: 'note'; title: string; section: string }
  | { kind: 'todo'; title: string; section: string }
  | { kind: 'file'; name: string; path: string; line: string }

const query = ref('')
const entities = ref<Entity[]>([])
const noteRows = ref<Row[]>([])
const todoRows = ref<Row[]>([])
const fileRows = ref<Row[]>([])
/** Snapshots of the shared documents, taken once per palette session. */
const noteSections = ref<NoteSection[]>([])
const todoSections = ref<TodoSection[]>([])
const selected = ref(0)
const input = ref<HTMLInputElement | null>(null)
/** Set when a registered action threw — see `runRegistered`. */
const actionError = ref<string | null>(null)

function drawer(tab: string, capture = false) {
  window.dispatchEvent(new CustomEvent('qss:drawer', { detail: { tab, capture } }))
  emit('close')
}

const ACTIONS: ActionRow[] = [
  { label: 'Open workspace picker', hint: 'go', run: () => { emit('navigate', '/'); emit('close') } },
  { label: 'Processes', hint: 'go', run: () => { emit('navigate', '/processes'); emit('close') } },
  { label: 'Settings', hint: 'open', run: () => { window.dispatchEvent(new CustomEvent('qss:settings', { detail: { module: null } })); emit('close') } },
  { module: 'notes', label: 'New note', hint: 'capture · Ctrl+Shift+N', run: () => drawer('notes', true) },
  { module: 'flow', label: 'New to-do', hint: 'capture', run: () => drawer('todos') },
  { module: 'mcp', label: 'New kanban card', hint: 'capture', run: () => drawer('kanban') },
  { module: 'mcp', label: 'Open kanban board', hint: 'drawer', run: () => drawer('kanban') },
  { label: 'Open activity feed', hint: 'drawer', run: () => drawer('feed') },
]

const actionRows = computed<Row[]>(() => {
  const q = query.value.trim().toLowerCase()
  return ACTIONS.filter((a) => !a.module || isModuleEnabled(a.module)).filter((a) => !q || a.label.toLowerCase().includes(q)).map(
    (action) => ({ kind: 'action', action }) as Row
  )
})

/**
 * The registered actions that pass their `when()`, snapshotted per palette
 * session. Snapshotted rather than filtered in the computed because `when()` is
 * a plain function and nothing about it is reactive: called from a computed it
 * would run once per keystroke and still show a stale answer, whereas a module
 * documents it as "re-read on every palette open".
 */
const registered = ref<PaletteAction[]>([])

function refreshRegistered() {
  registered.value = paletteActions().filter((action) => {
    if (!action.when) return true
    try {
      return action.when()
    } catch {
      // A broken predicate hides its own row; it does not empty the palette.
      return false
    }
  })
}

// A module that registers while the palette is already open — HMR, or rows that
// arrive from a backend after start — would otherwise be invisible until the
// next open. `paletteActions()` reads the registry ref, so this tracks it.
watch(paletteActions, refreshRegistered)

const registeredRows = computed<Row[]>(() => {
  const q = query.value.trim().toLowerCase()
  return registered.value
    .filter((a) => (q ? true : !a.queryOnly))
    .filter(
      (a) =>
        !q ||
        a.label.toLowerCase().includes(q) ||
        (a.keywords?.toLowerCase().includes(q) ?? false)
    )
    .map((action) => ({ kind: 'registered', action }) as Row)
})

const moduleRows = computed<Row[]>(() => {
  const q = query.value.trim().toLowerCase()
  return modules
    .filter((m) => m.status !== 'planned' && isModuleEnabled(m.id))
    .filter((m) => !q || m.title.toLowerCase().includes(q) || m.id.includes(q))
    .map((module) => ({ kind: 'module', module }) as Row)
})

const entityRows = computed<Row[]>(() =>
  entities.value.filter((entity) => isModuleEnabled(entity.module) && isRouteEnabled(entity.route)).map((entity) => ({ kind: 'entity', entity }) as Row)
)

const rows = computed<Row[]>(() => {
  // Modules and actions lead; the rest are empty without a query, so with no
  // query this is a launcher and with one it is everything. Module actions sit
  // with the suite's actions because that is what they are — after them, so
  // adding a module can never push a suite row off the first screen.
  const base = [...moduleRows.value, ...actionRows.value, ...registeredRows.value]
  return [...base, ...entityRows.value, ...(isModuleEnabled('notes') ? noteRows.value : []), ...(isModuleEnabled('flow') ? todoRows.value : []), ...(isModuleEnabled('code') ? fileRows.value : [])]
})

/**
 * The group heading to draw above row `i`, or null. Headings are derived from
 * `rows` instead of living in it: they are not selectable, and putting them in
 * the list would shift every index the arrow keys and `selected` work with.
 * `paletteActions()` keeps a group's rows contiguous, so one boundary test does
 * it.
 */
function groupHeading(i: number): string | null {
  const row = rows.value[i]
  if (row?.kind !== 'registered') return null
  const prev = rows.value[i - 1]
  if (prev?.kind === 'registered' && prev.action.group === row.action.group) return null
  return row.action.group
}

watch(
  () => props.open,
  async (open) => {
    if (!open) {
      cancelSearch()
      return
    }
    query.value = ''
    entities.value = []
    noteRows.value = []
    todoRows.value = []
    fileRows.value = []
    actionError.value = null
    selected.value = 0
    refreshRegistered()
    await nextTick()
    input.value?.focus()
    void loadDocuments()
    void search()
  }
)

let searchToken = 0
let searchTimer: ReturnType<typeof setTimeout> | null = null
let fileTimer: ReturnType<typeof setTimeout> | null = null

/**
 * Closing inside either debounce window used to leave the searches running —
 * an FTS round-trip and, three characters in, a recursive folder walk for a
 * dialog the user had already dismissed. Bumping the token also drops the
 * answers of calls that are already in flight.
 */
function cancelSearch() {
  searchToken++
  if (searchTimer) {
    clearTimeout(searchTimer)
    searchTimer = null
  }
  if (fileTimer) {
    clearTimeout(fileTimer)
    fileTimer = null
  }
}
onUnmounted(cancelSearch)

/**
 * Notes and to-dos are documents, not entities: one snapshot per palette
 * session, filtered in memory per keystroke. Loading them inside `search()`
 * cost two uncached settings reads per character typed, for documents that
 * cannot change while the palette is open.
 */
async function loadDocuments() {
  noteSections.value = isModuleEnabled('notes') ? (await notesStore.load().catch(() => null)) ?? [] : []
  todoSections.value = isModuleEnabled('flow') ? (await todosStore.load().catch(() => null)) ?? [] : []
  // The first keystrokes can beat the load — re-filter once it lands.
  if (query.value.trim()) void search()
}

async function search() {
  const token = ++searchToken
  const q = query.value.trim()

  // ── entities (core.db FTS; none in the browser — the tables live in Rust) ──
  try {
    const result = await qs.core.searchEntities(q, 20)
    if (token !== searchToken) return
    entities.value = result
  } catch {
    if (token !== searchToken) return
    entities.value = []
  }

  // ── shared notes & to-dos, client-side over the session snapshots ──
  if (q) {
    const ql = q.toLowerCase()
    noteRows.value = noteSections.value
      .flatMap((s) =>
        s.notes
          .filter((n) => n.title.toLowerCase().includes(ql) || n.content.toLowerCase().includes(ql))
          .map((n) => ({ kind: 'note', title: n.title || 'Untitled', section: s.name }) as Row)
      )
      .slice(0, 8)

    todoRows.value = todoSections.value
      .flatMap((s) =>
        s.tasks
          .filter((t) => t.title.toLowerCase().includes(ql))
          .map((t) => ({ kind: 'todo', title: t.title, section: s.name }) as Row)
      )
      .slice(0, 8)
  } else {
    noteRows.value = []
    todoRows.value = []
  }

  // ── workspace files: debounced content search, 3+ chars ──
  if (fileTimer) clearTimeout(fileTimer)
  fileRows.value = []
  if (isModuleEnabled('code') && props.workspacePath && q.length >= 3) {
    fileTimer = setTimeout(async () => {
      try {
        const matches = await invoke<{ path: string; line_number: number; line: string }[]>(
          'plugin:canvas|search_files',
          { path: props.workspacePath, pattern: q }
        )
        if (token !== searchToken) return
        const seen = new Set<string>()
        fileRows.value = matches
          .filter((m) => (seen.has(m.path) ? false : (seen.add(m.path), true)))
          .slice(0, 8)
          .map(
            (m) =>
              ({
                kind: 'file',
                name: m.path.split(/[/\\]/).pop() ?? m.path,
                path: m.path,
                line: m.line.trim().slice(0, 80),
              }) as Row
          )
      } catch {
        // No code plugin (browser) or unreadable folder — files just absent.
      }
    }, 300)
  }
}

watch(query, () => {
  selected.value = 0
  // The last action's failure is about the last query; typing retires it.
  actionError.value = null
  // Debounced: the entity search is an FTS round-trip per keystroke whose
  // result the token guard throws away anyway while you keep typing.
  if (searchTimer) clearTimeout(searchTimer)
  searchTimer = setTimeout(() => void search(), 120)
})

function move(delta: number) {
  const n = rows.value.length
  if (!n) return
  selected.value = (selected.value + delta + n) % n
}

/**
 * Runs a module's action. That callback is foreign code inside suite chrome: an
 * exception escaping it would leave the palette open on a row that appears to do
 * nothing, with the reason only in the console. So it is caught and shown, and
 * the palette deliberately stays open because that is the only place to show it.
 *
 * The result is awaited before closing, which is the one deviation from
 * "closes first": a rejection has nowhere to be reported once the dialog is
 * gone, and every action registered so far returns synchronously, so the
 * visible behaviour is identical.
 */
async function runRegistered(action: PaletteAction) {
  actionError.value = null
  try {
    const keepOpen = await action.run()
    if (keepOpen === false) return
    emit('close')
  } catch (e) {
    actionError.value = e instanceof Error ? e.message : String(e)
  }
}

function choose(row?: Row) {
  const target = row ?? rows.value[selected.value]
  if (!target) return

  switch (target.kind) {
    case 'action':
      target.action.run()
      return
    case 'registered':
      // `runRegistered` owns the close — it may be told to stay open.
      void runRegistered(target.action)
      return
    case 'module':
      emit('navigate', target.module.route)
      break
    case 'entity':
      if (target.entity.kind === 'kanban.card') {
        drawer('kanban')
        return
      }
      emit('navigate', target.entity.route)
      break
    case 'note':
      drawer('notes')
      return
    case 'todo':
      drawer('todos')
      return
    case 'file':
      emit('navigate', '/canvas')
      // QuantCode opens it from the bus (`core.file.open`), keeping the
      // module boundary intact.
      void bus.emit('core.file.open', { path: target.path }).catch(() => {})
      break
  }
  emit('close')
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'ArrowDown') {
    e.preventDefault()
    move(1)
  } else if (e.key === 'ArrowUp') {
    e.preventDefault()
    move(-1)
  } else if (e.key === 'Enter') {
    e.preventDefault()
    choose()
  } else if (e.key === 'Escape') {
    e.preventDefault()
    emit('close')
  }
}
</script>

<template>
  <div v-if="open" class="qss-palette" role="dialog" aria-modal="true" aria-label="Command palette">
    <div class="qss-palette-backdrop" @click="emit('close')" />
    <div class="qss-palette-box">
      <input
        ref="input"
        v-model="query"
        class="qss-palette-input"
        placeholder="Search everything — modules, cards, notes, to-dos, files…"
        autocomplete="off"
        spellcheck="false"
        @keydown="onKeydown"
      >

      <p v-if="actionError" class="qss-palette-error" role="alert">{{ actionError }}</p>

      <ul v-if="rows.length" class="qss-palette-list">
        <template v-for="(row, i) in rows" :key="i">
          <li v-if="groupHeading(i)" class="qss-palette-group">{{ groupHeading(i) }}</li>
          <li>
            <button
              class="qss-palette-row"
              :class="{ 'is-selected': i === selected }"
              @click="choose(row)"
              @mouseenter="selected = i"
            >
              <template v-if="row.kind === 'action'">
                <span class="r-title">{{ row.action.label }}</span>
                <span class="r-kind">action</span>
                <span class="r-mod">{{ row.action.hint }}</span>
              </template>
              <template v-else-if="row.kind === 'registered'">
                <span class="r-title">{{ row.action.label }}</span>
                <span class="r-kind">action</span>
                <span class="r-mod">{{ row.action.hint }}</span>
              </template>
              <template v-else-if="row.kind === 'module'">
                <span class="r-title">{{ row.module.title }}</span>
                <span class="r-kind">{{ row.module.status === 'stub' ? 'soon' : 'module' }}</span>
                <span class="r-mod">{{ appForModule(row.module.id)?.title ?? row.module.id }}</span>
              </template>
              <template v-else-if="row.kind === 'entity'">
                <span class="r-title">{{ row.entity.title }}</span>
                <span class="r-kind">{{ row.entity.kind }}</span>
                <span class="r-mod">{{ row.entity.module }}</span>
              </template>
              <template v-else-if="row.kind === 'note' || row.kind === 'todo'">
                <span class="r-title">{{ row.title }}</span>
                <span class="r-kind">{{ row.kind }}</span>
                <span class="r-mod">{{ row.section }}</span>
              </template>
              <template v-else>
                <span class="r-title">{{ row.name }}</span>
                <span class="r-kind">file</span>
                <span class="r-mod r-line">{{ row.line }}</span>
              </template>
            </button>
          </li>
        </template>
      </ul>

      <div v-else class="qss-palette-note">
        <template v-if="query">Nothing matches “{{ query }}”</template>
        <template v-else>Type to search modules, cards, notes, to-dos and files.</template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.qss-palette {
  position: fixed;
  inset: 0;
  z-index: 100;
}
.qss-palette-backdrop {
  position: absolute;
  inset: 0;
  background: rgb(0 0 0 / 0.5);
}
.qss-palette-box {
  position: relative;
  width: min(640px, calc(100vw - 60px));
  margin: 12vh auto 0;
  background: var(--qss-bg-card);
  border: 1px solid var(--qss-border);
  border-radius: 10px;
  overflow: hidden;
  box-shadow: 0 16px 48px rgb(0 0 0 / 0.45);
}

.qss-palette-input {
  width: 100%;
  padding: 14px 16px;
  background: none;
  border: none;
  border-bottom: 1px solid var(--qss-border);
  outline: none;
  color: var(--qss-text);
  font: 400 14px/1.4 var(--qss-font-sans);
}
.qss-palette-input::placeholder {
  color: var(--qss-text-muted);
}

/* A module action's failure. Above the list, so the row that failed is still
   visible underneath it and the message names something the user can see. */
.qss-palette-error {
  margin: 0;
  padding: 8px 16px;
  border-bottom: 1px solid var(--qss-border);
  background: color-mix(in srgb, var(--qss-error) 12%, transparent);
  color: var(--qss-error);
  font-size: 11.5px;
}

.qss-palette-list {
  max-height: 380px;
  overflow-y: auto;
  padding: 6px;
  margin: 0;
  list-style: none;
}
/* Heading above a module's block of rows; the QSettingsModal nav-label voice. */
.qss-palette-group {
  padding: 8px 10px 2px;
  color: var(--qss-text-muted);
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.05em;
  text-transform: uppercase;
}
.qss-palette-row {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 8px 10px;
  border: none;
  border-radius: 6px;
  background: none;
  color: var(--qss-text);
  font: 400 12.5px/1.4 var(--qss-font-sans);
  text-align: left;
  cursor: pointer;
}
.qss-palette-row.is-selected {
  background: var(--qss-bg-hover);
}
.r-title {
  flex-shrink: 0;
}
.r-kind {
  color: var(--qss-text-muted);
  font-size: 10.5px;
  flex-shrink: 0;
}
.r-mod {
  margin-left: auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--qss-text-muted);
  font: 400 10.5px/1 var(--qss-font-mono);
}
.r-line {
  max-width: 45%;
}

.qss-palette-note {
  padding: 24px;
  text-align: center;
  color: var(--qss-text-muted);
  font-size: 12.5px;
}
</style>
