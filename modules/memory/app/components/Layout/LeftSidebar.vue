<script setup lang="ts">
/** Knowledge navigation, with the file tree available when needed. */
import { BookOpen, Star, Network, Clock3, Hash, FolderTree, ChevronRight, RefreshCw } from 'lucide-vue-next'
import { useAppStore } from '#memory/stores/app'
import { useVaultStore } from '#memory/stores/vault'
import type { MemoryMeta, VaultFolder } from '#memory/types'

const vault = useVaultStore()
const app = useAppStore()
const router = useRouter()
const route = useRoute()

const activeId = computed(() => {
  const match = route.path.match(/^\/memory\/m\/(.+)$/)
  return match ? decodeURIComponent(match[1]!) : null
})

function openMemory(id: string) {
  void router.push(`/memory/m/${id}`)
}

// ── The tree ─────────────────────────────────────────────────────────────

/** Grow one scope's directory tree into `root`. Folder paths get a `prefix`
 *  so collapse state stays unique across scope groups. */
function buildInto(memories: MemoryMeta[], root: VaultFolder, prefix: string) {
  const byPath = new Map<string, VaultFolder>([['', root]])
  const folderFor = (path: string): VaultFolder => {
    const existing = byPath.get(path)
    if (existing) return existing
    const cut = path.lastIndexOf('/')
    const parent = folderFor(cut === -1 ? '' : path.slice(0, cut))
    const node: VaultFolder = {
      name: path.slice(cut + 1),
      path: prefix + path,
      folders: [],
      files: [],
    }
    parent.folders.push(node)
    byPath.set(path, node)
    return node
  }
  for (const m of memories) {
    const cut = m.relPath.lastIndexOf('/')
    folderFor(cut === -1 ? '' : m.relPath.slice(0, cut)).files.push(m)
  }
  const sortNode = (n: VaultFolder) => {
    n.folders.sort((a, b) => a.name.localeCompare(b.name))
    n.files.sort((a, b) => a.relPath.localeCompare(b.relPath))
    n.folders.forEach(sortNode)
  }
  sortNode(root)
}

/** Directory tree over `filtered` (the kind/tag chips prune it), shaped the
 *  way the QuantSpace explorer shapes its General mode: every scope is a
 *  ROOT ROW with its files nested (and guide-lined) beneath it. A workspace
 *  view shows one root; the gigabrain view shows General first, then every
 *  workspace that has memories. */
const tree = computed<VaultFolder>(() => {
  const root: VaultFolder = { name: '', path: '', folders: [], files: [] }
  const visible = vault.filtered as MemoryMeta[]
  const addScopeRoot = (scope: string, name: string, members: MemoryMeta[], evenIfEmpty: boolean) => {
    if (!members.length && !evenIfEmpty) return
    const node: VaultFolder = { name, path: `scope:${scope}`, folders: [], files: [] }
    buildInto(members, node, `scope:${scope}/`)
    root.folders.push(node)
  }
  if (vault.scope !== 'general') {
    addScopeRoot(vault.scope, vault.scopeName(vault.scope), visible, true)
    return root
  }
  addScopeRoot('general', 'General', visible.filter((m) => m.scope === 'general'), true)
  for (const s of vault.scopes.filter((s) => s.scope !== 'general')) {
    addScopeRoot(s.scope, s.name, visible.filter((m) => m.scope === s.scope), false)
  }
  return root
})

function refresh() {
  void vault.loadAll()
  void vault.loadHygiene()
  if (route.path === '/memory/graph') void vault.loadGraph()
}

/** Folders default to open; only the collapsed ones are remembered. */
const collapsed = ref<Set<string>>(new Set())
try {
  collapsed.value = new Set(JSON.parse(localStorage.getItem('qm-tree-collapsed') ?? '[]'))
} catch {
  /* fresh profile */
}

function toggleFolder(path: string) {
  const next = new Set(collapsed.value)
  if (next.has(path)) {
    next.delete(path)
  } else {
    next.add(path)
  }
  collapsed.value = next
  try {
    localStorage.setItem('qm-tree-collapsed', JSON.stringify([...next]))
  } catch {
    /* private mode */
  }
}

const graphOpen = computed(() => route.path === '/memory/graph')
const view = computed(() => typeof route.query.view === 'string' ? route.query.view : 'library')
const starredCount = computed(() => vault.scopeMemories.filter(m => app.starred.includes(m.id)).length)
const topics = computed(() => {
  const counts = new Map<string, number>()
  for (const m of vault.scopeMemories) for (const tag of m.tags) counts.set(tag, (counts.get(tag) ?? 0) + 1)
  return [...counts].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).slice(0, 12)
})
function browse(view = 'library', tag: string | null = null) {
  vault.filterTag = tag
  vault.filterKind = null
  vault.query = ''
  void router.push({ path: '/memory', query: view === 'library' ? {} : { view } })
}
</script>



<template>
  <div class="qm-side">
    <div class="qm-side-top"><MemoryLayoutScopePicker /><button class="qm-side-icon" aria-label="Refresh vault" @click="refresh"><RefreshCw :size="13" /></button></div>
    <nav class="qm-side-nav" aria-label="Your knowledge">
      <button :class="{ active: !graphOpen && !activeId && view === 'library' && !vault.filterTag }" @click="browse()"><BookOpen :size="15" /> All memories <span>{{ vault.scopeMemories.length }}</span></button>
      <button :class="{ active: !graphOpen && !activeId && view === 'starred' }" @click="browse('starred')"><Star :size="15" /> Starred <span>{{ starredCount }}</span></button>
      <button :class="{ active: graphOpen }" @click="router.push('/memory/graph')"><Network :size="15" /> Memory map <ChevronRight :size="12" class="qm-side-arrow" /></button>
      <button :class="{ active: !graphOpen && !activeId && view === 'review' }" @click="browse('review')"><Clock3 :size="15" /> Review memories</button>
    </nav>
    <div class="qm-side-list">
      <template v-if="!activeId">
        <div v-if="topics.length" class="qm-side-topics"><h2>Topics</h2><button v-for="[tag, count] in topics" :key="tag" :class="{ active: vault.filterTag === tag }" @click="browse('library', tag)"><Hash :size="13" /><span>{{ tag }}</span><small>{{ count }}</small></button></div>
        <details class="qm-file-disclosure"><summary><FolderTree :size="14" /> Browse files <ChevronRight :size="12" /></summary><MemoryTreeNode :folder="tree" :depth="0" root :collapsed="collapsed" :active-id="activeId" @open="openMemory" @toggle="toggleFolder" /><p v-if="!vault.filtered.length" class="qm-side-empty">No files in this view.</p></details>
      </template>
      <template v-else>
        <div class="qm-side-search"><input v-model="vault.query" aria-label="Search memory vault" placeholder="Find another memory…" spellcheck="false" /></div>
        <template v-if="vault.query.trim()">
          <p v-if="vault.searching" class="qm-side-empty" role="status">Searching…</p>
          <p v-else-if="vault.searchError" class="qm-side-empty" role="alert">{{ vault.searchError }}</p>
          <p v-else-if="!vault.hits.length" class="qm-side-empty">No matching memories.</p>
          <button v-for="hit in vault.hits" :key="hit.id" class="qm-side-result" :class="{ active: hit.id === activeId }" @click="openMemory(hit.id)"><span>{{ hit.title }}</span><small>{{ vault.scopeName(hit.scope) }}</small><p v-if="hit.snippet">{{ hit.snippet }}</p></button>
        </template>
        <MemoryTreeNode v-else :folder="tree" :depth="0" root :collapsed="collapsed" :active-id="activeId" @open="openMemory" @toggle="toggleFolder" />
      </template>
    </div>
  </div>
</template>

<style scoped>
.qm-side { display: flex; flex-direction: column; height: 100%; min-height: 0; }
.qm-side-top { display: flex; align-items: center; gap: 4px; padding: 15px 10px 12px; }
.qm-side-icon { display: grid; place-items: center; width: 25px; height: 25px; flex-shrink: 0; border: 0; border-radius: 5px; background: none; color: var(--qm-text-muted); cursor: pointer; }
.qm-side-nav { display: grid; gap: 4px; padding: 4px 10px 20px; border-bottom: 1px solid var(--qm-border-subtle); }
.qm-side-nav button { display: flex; align-items: center; gap: 9px; width: 100%; padding: 10px; border: 1px solid transparent; border-radius: 7px; background: none; color: var(--qm-text-secondary); font-size: 12px; text-align: left; cursor: pointer; }
.qm-side-nav button svg { color: var(--qm-text-muted); flex-shrink: 0; }.qm-side-nav button span, .qm-side-arrow { margin-left: auto; font-size: 10px; color: var(--qm-text-muted); }
.qm-side-nav button:hover, .qm-side-topics button:hover, .qm-side-icon:hover { background: var(--qm-bg-hover); color: var(--qm-text); }
.qm-side-nav button.active { border-color: color-mix(in srgb, #ae9bd8 13%, transparent); background: color-mix(in srgb, #ae9bd8 8%, transparent); color: var(--qm-text); }.qm-side-nav button.active > svg { color: #ad9ccf; }
.qm-side-list { flex: 1; min-height: 0; overflow-y: auto; padding: 8px 10px 20px; }
.qm-side-topics { padding: 10px 0 20px; }.qm-side-topics h2 { margin: 0 10px 10px; color: var(--qm-text-muted); font-size: 10px; font-weight: 500; letter-spacing: .6px; }
.qm-side-topics button { display: flex; align-items: center; gap: 8px; width: 100%; padding: 8px 10px; border: 0; border-radius: 6px; background: none; color: var(--qm-text-secondary); text-align: left; font-size: 11px; cursor: pointer; }.qm-side-topics button svg { color: #9585b4; flex-shrink: 0; }.qm-side-topics button span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }.qm-side-topics button small { margin-left: auto; color: var(--qm-text-muted); font-size: 10px; }.qm-side-topics button.active { background: color-mix(in srgb, #ae9bd8 10%, transparent); color: var(--qm-text); }
.qm-file-disclosure { border-top: 1px solid var(--qm-border-subtle); padding-top: 12px; }.qm-file-disclosure summary { display: flex; align-items: center; gap: 8px; list-style: none; padding: 8px 10px 14px; color: var(--qm-text-muted); font-size: 11px; cursor: pointer; }.qm-file-disclosure summary::-webkit-details-marker { display: none; }.qm-file-disclosure summary svg:last-child { margin-left: auto; }.qm-file-disclosure[open] summary svg:last-child { transform: rotate(90deg); }
.qm-side-search { padding: 4px 0 12px; }.qm-side-search input { width: 100%; min-width: 0; padding: 8px 10px; border: 1px solid var(--qm-border); border-radius: 7px; background: transparent; color: var(--qm-text); font: inherit; font-size: 11px; }.qm-side-search input::placeholder { color: var(--qm-text-muted); }
.qm-side-result { display: block; width: 100%; padding: 10px; border: 0; border-radius: 6px; background: none; color: var(--qm-text-secondary); text-align: left; cursor: pointer; }.qm-side-result:hover, .qm-side-result.active { background: var(--qm-bg-hover); }.qm-side-result > span { display: block; font-size: 12px; }.qm-side-result small { display: block; margin-top: 4px; color: var(--qm-text-muted); font-size: 10px; }.qm-side-result p { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; margin: 6px 0 0; color: var(--qm-text-muted); font-size: 11px; }
.qm-side-empty { padding: 12px 10px; color: var(--qm-text-muted); font-size: 11px; line-height: 1.6; }
.qm-side button:focus-visible, .qm-side summary:focus-visible, .qm-side input:focus-visible { outline: 2px solid var(--qm-link); outline-offset: 1px; }
</style>
