<script setup lang="ts">
/**
 * Browse GitHub (parity 10): the repositories the signed-in `gh` can reach —
 * owned, collaborator and organization ones, newest first, 50 a page, or a
 * search across the account and its organizations. A repository that already
 * is a workspace opens it; any other one is cloned into a folder you pick
 * (never into a non-empty one), with progress and Stop, then registered as a
 * workspace through the suite's workspace service and opened here. A clone
 * that fails or is stopped removes only what it created.
 */
import { computed, onMounted, ref } from 'vue'
import { Archive, FolderOpen, GitFork, Lock, LoaderCircle, X } from 'lucide-vue-next'
import { openWorkspacePath, useTauriEvent } from '@quantsuite/core'
import { pilot } from '#pilot/utils/invoke'
import { timeAgo } from '#pilot/utils/format'
import { hostWords, type HostId } from '#pilot/utils/github'

interface Remote {
  slug: string
  owner: string
  name: string
  description: string
  private: boolean
  archived: boolean
  fork: boolean
  access: string
  updatedAt: string
  local: [string, string, string][]
}
interface CloneEvent {
  job: string
  phase: string
  percent: number | null
  done: boolean
  error: string | null
  path: string
  existing: boolean
}

const props = withDefaults(defineProps<{ host?: HostId }>(), { host: 'github' })
const site = computed(() => hostWords(props.host).site)
const emit = defineEmits<{ close: []; opened: [path: string] }>()
const query = ref('')
const items = ref<Remote[]>([])
const page = ref(1)
const more = ref(false)
const loading = ref(false)
const error = ref('')
const target = ref<Remote | null>(null)
const parent = ref('')
const folder = ref('')
const job = ref<string | null>(null)
const progress = ref<CloneEvent | null>(null)
const result = ref('')

async function load(next = false) {
  loading.value = true
  error.value = ''
  try {
    const p = await pilot<{ items: Remote[]; page: number; more: boolean }>('plugin:pilot|pilot_gh_browse', { query: query.value.trim() || null, page: next ? page.value + 1 : 1, host: props.host })
    items.value = next ? [...items.value, ...(p?.items ?? [])] : (p?.items ?? [])
    page.value = p?.page ?? 1
    more.value = p?.more ?? false
  } catch (e) {
    error.value = String(e)
  } finally {
    loading.value = false
  }
}
onMounted(() => void load())

function chooseClone(r: Remote) {
  target.value = r
  folder.value = r.name
  result.value = ''
  progress.value = null
}

async function pickParent() {
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const chosen = await open({ directory: true, title: 'Clone into this folder' })
    if (typeof chosen === 'string') parent.value = chosen
  } catch {
    /* browser build */
  }
}

const dest = computed(() => (parent.value && folder.value ? `${parent.value.replace(/[\\/]+$/, '')}\\${folder.value}` : ''))

async function clone() {
  if (!target.value || !parent.value.trim()) return
  error.value = ''
  result.value = ''
  progress.value = { job: '', phase: 'Starting', percent: 0, done: false, error: null, path: '', existing: false }
  try {
    job.value = await pilot<string>('plugin:pilot|pilot_gh_clone', { slug: target.value.slug, parent: parent.value.trim(), folder: folder.value.trim() || null, host: props.host })
  } catch (e) {
    progress.value = null
    error.value = String(e)
  }
}

async function stop() {
  if (job.value) await pilot('plugin:pilot|pilot_gh_clone_cancel', { job: job.value }).catch(() => undefined)
}

useTauriEvent<CloneEvent>('pilot-gh-clone', async (e) => {
  if (!job.value || e.job !== job.value) return
  progress.value = e
  if (!e.done) return
  job.value = null
  if (e.error) {
    error.value = e.error
    return
  }
  try {
    // The suite's one write path for workspaces; it also opens it.
    await openWorkspacePath(e.path)
    result.value = e.existing ? `${e.path} already was this repository — registered it as a workspace.` : `Cloned into ${e.path} and registered it as a workspace.`
    emit('opened', e.path)
  } catch (err) {
    error.value = `Cloned into ${e.path}, but registering it failed (${String(err)}). The clone stays; open the folder as a workspace from the suite.`
  }
})
</script>

<template>
  <section class="qp-ghb">
    <header class="qp-ghb-bar">
      <span class="qp-ghb-title">Browse {{ site }}</span>
      <input v-model="query" class="qp-ghb-search" placeholder="Search your and your organizations' repositories — Enter" spellcheck="false" @keydown.enter="load()" />
      <button class="qp-ghb-x" aria-label="Close" @click="emit('close')"><X :size="14" /></button>
    </header>
    <div v-if="error" class="qp-ghb-err" role="alert">{{ error }}</div>
    <div v-if="target" class="qp-ghb-clone">
      <div class="qp-ghb-clone-head">Clone <b>{{ target.slug }}</b><span v-if="target.private" class="qp-ghb-tag"><Lock :size="10" /> private</span></div>
      <div class="qp-ghb-row">
        <input v-model="parent" class="qp-ghb-input" placeholder="Into this folder" aria-label="Parent folder" />
        <button class="qp-btn qp-btn-sm" title="Choose a folder" @click="pickParent"><FolderOpen :size="13" /></button>
        <input v-model="folder" class="qp-ghb-input qp-ghb-folder" aria-label="Folder name" />
      </div>
      <div v-if="dest" class="qp-ghb-dim">{{ dest }} — must not exist yet or be empty; nothing in an existing folder is changed.</div>
      <div v-if="progress && !progress.done" class="qp-ghb-progress">
        <LoaderCircle :size="12" class="qp-spin" /> {{ progress.phase }}<template v-if="progress.percent != null"> · {{ progress.percent }}%</template>
        <div class="qp-ghb-track"><div class="qp-ghb-fill" :style="{ width: `${progress.percent ?? 0}%` }" /></div>
      </div>
      <div v-if="result" class="qp-ghb-ok" role="status">{{ result }}</div>
      <div class="qp-ghb-row qp-ghb-actions">
        <button v-if="job" class="qp-btn qp-btn-sm" @click="stop">Stop</button>
        <button class="qp-btn qp-btn-sm" :disabled="!!job" @click="target = null">Close</button>
        <button class="qp-btn qp-btn-sm qp-btn-primary" :disabled="!!job || !parent.trim() || !folder.trim()" @click="clone">Clone</button>
      </div>
    </div>
    <div class="qp-ghb-list">
      <div v-if="loading && !items.length" class="qp-ghb-dim qp-ghb-pad">Asking {{ site }}…</div>
      <div v-else-if="!items.length && !error" class="qp-ghb-dim qp-ghb-pad">No repositories{{ query ? ' match' : '' }}.</div>
      <div v-for="r in items" :key="r.slug" class="qp-ghb-item">
        <div class="qp-ghb-main">
          <div class="qp-ghb-line">
            <span class="qp-ghb-name">{{ r.slug }}</span>
            <span v-if="r.private" class="qp-ghb-tag"><Lock :size="10" /> private</span>
            <span v-if="r.archived" class="qp-ghb-tag"><Archive :size="10" /> archived</span>
            <span v-if="r.fork" class="qp-ghb-tag"><GitFork :size="10" /> fork</span>
            <span class="qp-ghb-tag">{{ r.access }}</span>
          </div>
          <div v-if="r.description" class="qp-ghb-dim">{{ r.description }}</div>
          <div class="qp-ghb-dim">
            Updated {{ r.updatedAt ? timeAgo(Date.parse(r.updatedAt)) : '—' }}
            <template v-for="l in r.local" :key="l[0]"> · workspace <b>{{ l[1] }}</b> ({{ l[2] }})</template>
          </div>
        </div>
        <button v-if="r.local.length" class="qp-btn qp-btn-sm" @click="emit('opened', r.local[0]![2])">Open</button>
        <button class="qp-btn qp-btn-sm" :disabled="!!job" @click="chooseClone(r)">{{ r.local.length ? 'Clone again…' : 'Clone…' }}</button>
      </div>
      <button v-if="more" class="qp-ghb-more" :disabled="loading" @click="load(true)">More</button>
    </div>
  </section>
</template>

<style scoped>
.qp-ghb { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 8px; padding: 10px 14px; font-size: 12.5px; color: var(--qss-text, #d4d4d8); }
.qp-ghb-bar { display: flex; align-items: center; gap: 8px; }
.qp-ghb-title { font-weight: 600; font-size: 13px; }
.qp-ghb-search { flex: 1; height: 28px; padding: 0 9px; border-radius: 7px; border: 1px solid var(--qss-border, #47474f); background: var(--qss-bg, #18181e); color: inherit; font: inherit; outline: none; }
.qp-ghb-x { border: none; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; }
.qp-ghb-err { color: var(--qss-error, #e06c75); font-size: 12px; }
.qp-ghb-ok { color: var(--qss-success, #34d399); font-size: 12px; }
.qp-ghb-dim { color: var(--qss-text-muted, #6e6e7a); font-size: 11.5px; }
.qp-ghb-pad { padding: 20px; text-align: center; }
.qp-ghb-clone { display: flex; flex-direction: column; gap: 6px; padding: 10px; border-radius: 10px; border: 1px solid var(--qss-border, #47474f); background: var(--qss-bg-raised, #1f1f25); }
.qp-ghb-clone-head { display: flex; align-items: center; gap: 6px; }
.qp-ghb-row { display: flex; align-items: center; gap: 6px; }
.qp-ghb-actions { justify-content: flex-end; }
.qp-ghb-input { flex: 1; height: 28px; padding: 0 9px; border-radius: 7px; border: 1px solid var(--qss-border, #47474f); background: var(--qss-bg, #18181e); color: inherit; font: inherit; }
.qp-ghb-folder { flex: 0 0 200px; }
.qp-ghb-progress { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-ghb-track { height: 3px; border-radius: 2px; background: var(--qss-bg-hover, #313139); overflow: hidden; }
.qp-ghb-fill { height: 100%; background: var(--qss-accent, #7aa2f7); transition: width 0.2s; }
.qp-ghb-list { flex: 1; min-height: 0; overflow: auto; display: flex; flex-direction: column; gap: 2px; }
.qp-ghb-item { display: flex; align-items: center; gap: 8px; padding: 7px 8px; border-radius: 8px; }
.qp-ghb-item:hover { background: var(--qss-bg-hover, #313139); }
.qp-ghb-main { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
.qp-ghb-line { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
.qp-ghb-name { font-weight: 500; }
.qp-ghb-tag { display: inline-flex; align-items: center; gap: 3px; padding: 0 6px; border-radius: 8px; border: 1px solid var(--qss-border, #47474f); color: var(--qss-text-secondary, #9a9aa5); font-size: 10.5px; }
.qp-ghb-more { align-self: center; margin: 6px; border: none; background: transparent; color: var(--qss-text-secondary, #9a9aa5); font: inherit; cursor: pointer; }
</style>
