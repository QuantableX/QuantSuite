<script setup lang="ts">
/**
 * The bottom dock: problems, terminal, search — one strip of tabs and one body.
 *
 * The terminal is `ConsolePane`, the same emulator QuantConsole and QuantCanvas
 * use; QuantCode does not grow a second one. It stays mounted while another
 * dock tab is in front, or the shell would die every time you looked at the
 * problems list.
 */
import { useEditorStore } from '#code-root/stores/editor'
import { qs } from '@quantsuite/core'
import type { SearchHit } from '#code-root/shared/types'

const props = defineProps<{
  root: string | null
  /** The dock is on a visible surface — the terminal pane needs to know. */
  visible?: boolean
  /** Which pane is in front. A prop, not the code store: every QuantSpace
   *  module hosts this dock (CodeDock), and each owns its own selection. */
  panel: 'problems' | 'terminal' | 'search'
}>()

const emit = defineEmits<{
  (e: 'reveal', target: { path: string; line: number }): void
}>()

const store = useEditorStore()

// ── Search ──
const query = ref('')
const searching = ref(false)
const hits = ref<SearchHit[]>([])
const searchError = ref<string | null>(null)
const searchInput = ref<HTMLInputElement | null>(null)

async function runSearch() {
  const q = query.value.trim()
  if (!q || !props.root) {
    hits.value = []
    return
  }
  searching.value = true
  searchError.value = null
  try {
    // `search_files` is the same gitignore-aware walk the explorer uses, so a
    // hit list never contains node_modules and never has to be filtered here.
    const raw = await invokeSearch(props.root, q)
    hits.value = raw
  } catch (e) {
    searchError.value = e instanceof Error ? e.message : String(e)
    hits.value = []
  } finally {
    searching.value = false
  }
}

/**
 * The backend's search command. Shapes vary between plugin versions, so the
 * result is normalised here rather than trusted — a shape mismatch would
 * otherwise render an empty list with no explanation.
 */
async function invokeSearch(root: string, pattern: string): Promise<SearchHit[]> {
  const { invoke } = await import('@tauri-apps/api/core')
  const result = await invoke<unknown>('plugin:canvas|search_files', { path: root, pattern })
  if (!Array.isArray(result)) return []
  return result
    .map((row) => {
      const r = row as Record<string, unknown>
      const path = typeof r.path === 'string' ? r.path : typeof r.file === 'string' ? r.file : ''
      const line = typeof r.line === 'number' ? r.line : typeof r.lineNumber === 'number' ? r.lineNumber : 1
      const text = typeof r.text === 'string' ? r.text : typeof r.content === 'string' ? r.content : ''
      return { path, line, text: text.trim().slice(0, 200) }
    })
    .filter((h) => !!h.path)
}

function openHit(hit: SearchHit) {
  emit('reveal', { path: hit.path, line: hit.line })
}

watch(
  () => props.panel,
  (tab) => {
    if (tab === 'search') nextTick(() => searchInput.value?.focus())
  }
)

// ── Terminal ──
/** Opened lazily: a shell should start when the dock is first shown, not on
 *  every visit to the module. */
const terminalStarted = ref(false)
watch(
  () => props.panel,
  (tab) => {
    if (tab === 'terminal') terminalStarted.value = true
  },
  { immediate: true }
)

const shellPath = ref<string | null>(null)
onMounted(async () => {
  try {
    const shells = await qs.console.listShells()
    shellPath.value = (shells.find((s) => s.isDefault) ?? shells[0])?.path ?? null
  } catch {
    // Browser development — the pane renders its own empty state.
  }
})

function relative(path: string): string {
  const p = path.replace(/\\/g, '/')
  const r = props.root?.replace(/\\/g, '/') ?? ''
  return r && p.toLowerCase().startsWith(r.toLowerCase()) ? p.slice(r.length + 1) : p
}

const SEVERITY_LABEL = { error: 'E', warning: 'W', info: 'I', hint: 'H' } as const
</script>

<template>
  <!-- The strip (tabs, badges, close) moved to the shared QDock chrome the
       page wraps around this component — what remains here is the CONTENT of
       the dock: the three panes and their state. -->
  <div class="dock-body">
      <!-- Problems -->
      <div v-show="panel === 'problems'" class="pane scroll">
        <p v-if="!store.allMarkers.length" class="pane-empty">No problems detected.</p>
        <button
          v-for="(marker, i) in store.allMarkers"
          :key="`${marker.path}:${marker.line}:${i}`"
          class="row"
          @click="emit('reveal', { path: marker.path, line: marker.line })"
        >
          <span class="sev" :class="`sev-${marker.severity}`">{{ SEVERITY_LABEL[marker.severity] }}</span>
          <span class="row-msg">{{ marker.message }}</span>
          <span class="row-where">{{ relative(marker.path) }}:{{ marker.line }}</span>
        </button>
      </div>

      <!-- Search -->
      <div v-show="panel === 'search'" class="pane">
        <form class="search-bar" @submit.prevent="runSearch">
          <input
            ref="searchInput"
            v-model="query"
            class="search-input"
            type="text"
            placeholder="Search in workspace…"
            spellcheck="false"
          />
          <button class="search-go" type="submit" :disabled="!query.trim() || !root">
            {{ searching ? 'Searching…' : 'Search' }}
          </button>
        </form>
        <p v-if="!root" class="pane-empty">Open a workspace to search in it.</p>
        <p v-else-if="searchError" class="pane-empty is-error">{{ searchError }}</p>
        <p v-else-if="!hits.length && !searching" class="pane-empty">
          {{ query.trim() ? 'No matches.' : 'Type a query and press Enter.' }}
        </p>
        <div v-else class="scroll search-results">
          <button v-for="(hit, i) in hits" :key="`${hit.path}:${hit.line}:${i}`" class="row" @click="openHit(hit)">
            <span class="row-msg mono">{{ hit.text }}</span>
            <span class="row-where">{{ relative(hit.path) }}:{{ hit.line }}</span>
          </button>
        </div>
      </div>

      <!-- Terminal: mounted once, kept alive behind the other tabs -->
      <div v-if="terminalStarted" v-show="panel === 'terminal'" class="pane pane-term">
        <ConsolePane
          :shell="shellPath"
          :cwd="root ?? '.'"
          :visible="visible && panel === 'terminal'"
          :chrome="false"
          owner="canvas"
          label="QuantCode"
        />
      </div>
  </div>
</template>

<style scoped>
.dock-body {
  flex: 1;
  min-height: 0;
  position: relative;
}

.pane {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.pane-term {
  padding: 4px;
}

.scroll {
  overflow-y: auto;
}

.pane-empty {
  padding: 12px 14px;
  font-size: 12px;
  color: var(--qss-text-muted);
}
.pane-empty.is-error {
  color: #f87171;
}

.row {
  display: flex;
  align-items: baseline;
  gap: 8px;
  width: 100%;
  padding: 4px 14px;
  border: none;
  background: transparent;
  color: var(--qss-text-secondary);
  font-size: 12px;
  text-align: left;
  cursor: pointer;
}
.row:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}

.row-msg {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.row-msg.mono {
  font-family: var(--qss-font-mono);
  font-size: 11px;
}
.row-where {
  flex-shrink: 0;
  font-size: 10.5px;
  color: var(--qss-text-muted);
}

.sev {
  flex-shrink: 0;
  width: 15px;
  height: 15px;
  border-radius: 4px;
  font-size: 9.5px;
  font-weight: 700;
  line-height: 15px;
  text-align: center;
}
.sev-error {
  background: color-mix(in srgb, #ef4444 30%, transparent);
  color: #fca5a5;
}
.sev-warning {
  background: color-mix(in srgb, #f59e0b 28%, transparent);
  color: #fcd34d;
}
.sev-info,
.sev-hint {
  background: var(--qss-bg-hover);
  color: var(--qss-text-muted);
}

.search-bar {
  display: flex;
  gap: 6px;
  flex-shrink: 0;
  padding: 8px 12px;
}
.search-input {
  flex: 1;
  min-width: 0;
  height: 26px;
  padding: 0 9px;
  border: 1px solid var(--qss-border);
  border-radius: 6px;
  background: var(--qss-bg);
  color: var(--qss-text);
  font-family: var(--qss-font-mono);
  font-size: 11.5px;
  outline: none;
}
.search-input:focus {
  border-color: color-mix(in srgb, var(--qss-accent) 50%, var(--qss-border));
}
.search-go {
  height: 26px;
  padding: 0 12px;
  border: 1px solid var(--qss-border);
  border-radius: 6px;
  background: var(--qss-bg);
  color: var(--qss-text-secondary);
  font-size: 11.5px;
  cursor: pointer;
}
.search-go:hover:not(:disabled) {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}
.search-go:disabled {
  opacity: 0.4;
  cursor: default;
}

.search-results {
  flex: 1;
  min-height: 0;
}
</style>
