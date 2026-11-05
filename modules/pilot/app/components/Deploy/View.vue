<script setup lang="ts">
/**
 * Deploy (round 4, card 4): a project's targets (Netlify, Vercel or your own
 * commands), a preview first, then production after you confirm, and the history.
 * The backend holds the rules: this page never decides what may go live.
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { Plus, Rocket } from 'lucide-vue-next'
import { usePilotStore } from '#pilot/stores/pilot'
import { useInspectorStore } from '#pilot/stores/inspector'
import { pilot } from '#pilot/utils/invoke'

interface Target { id: string; name: string; kind: string; buildCommand: string; folder: string; site: string; previewCommand: string; productionCommand: string; hasToken?: boolean; cliFound?: boolean }
interface Run { id: string; targetId: string; targetName: string; stage: string; commitId: string; dirty: boolean; status: string; url: string; log: string; startedAt: number; finishedAt: number | null }
interface View { targets: Target[]; runs: Run[]; head: { commit: string; dirty: boolean } | null }

const store = usePilotStore()
const inspector = useInspectorStore()
const projects = computed(() => store.contexts.filter((c) => c.kind !== 'general'))
const project = ref('')
const view = ref<View | null>(null)
const selectedId = ref('')
const notice = ref('')
const busy = ref(false)
const editing = ref<Target | null>(null)
const token = ref('')
const confirming = ref(false)
const withoutPreview = ref(false)
const openLog = ref('')
let timer: ReturnType<typeof setInterval> | null = null

const KINDS = [
  { id: 'netlify', label: 'Netlify' },
  { id: 'vercel', label: 'Vercel' },
  { id: 'custom', label: 'My own commands' },
]
const kindLabel = (k: string) => KINDS.find((x) => x.id === k)?.label ?? k
const blank = (): Target => ({ id: '', name: '', kind: 'netlify', buildCommand: '', folder: 'dist', site: '', previewCommand: '', productionCommand: '' })

const selected = computed(() => view.value?.targets.find((t) => t.id === selectedId.value) ?? null)
const runs = computed(() => (view.value?.runs ?? []).filter((r) => r.targetId === selectedId.value))
const running = computed(() => runs.value.some((r) => r.status === 'running'))
const head = computed(() => view.value?.head ?? null)
const short = (c: string) => c.slice(0, 7)
const headText = computed(() => (head.value ? `${short(head.value.commit)}${head.value.dirty ? ' with uncommitted changes' : ''}` : 'not a git repository'))
/** A good preview of exactly what would go live. */
const previewed = computed(() => !!head.value && !head.value.dirty && runs.value.some((r) => r.stage === 'preview' && r.status === 'ok' && r.commitId === head.value?.commit && !r.dirty))
const installHint = computed(() => {
  const t = selected.value
  if (!t || t.cliFound !== false) return ''
  return t.kind === 'vercel' ? 'npm install -g vercel' : 'npm install -g netlify-cli'
})
const tokenName = computed(() => ({ netlify: 'NETLIFY_AUTH_TOKEN', vercel: 'VERCEL_TOKEN', custom: 'DEPLOY_TOKEN' })[editing.value?.kind ?? 'netlify'])

async function load() {
  if (!project.value) return
  try {
    const next = await pilot<View>('plugin:pilot|pilot_deploy_view', { project: project.value })
    if (next) {
      view.value = next
      if (!next.targets.some((t) => t.id === selectedId.value)) selectedId.value = next.targets[0]?.id ?? ''
    }
  } catch (e) {
    notice.value = String(e)
  }
}

async function save() {
  if (!editing.value) return
  busy.value = true
  notice.value = ''
  try {
    const next = await pilot<View>('plugin:pilot|pilot_deploy_save', { project: project.value, target: editing.value, token: token.value || null })
    if (next) {
      const name = editing.value.name.trim()
      view.value = next
      selectedId.value = next.targets.find((t) => t.name === name)?.id ?? selectedId.value
    }
    editing.value = null
    token.value = ''
  } catch (e) {
    notice.value = String(e)
  } finally {
    busy.value = false
  }
}

async function remove() {
  if (!selected.value) return
  busy.value = true
  try {
    const next = await pilot<View>('plugin:pilot|pilot_deploy_delete', { project: project.value, id: selected.value.id })
    if (next) view.value = next
    selectedId.value = view.value?.targets[0]?.id ?? ''
  } catch (e) {
    notice.value = String(e)
  } finally {
    busy.value = false
  }
}

async function deploy(stage: 'preview' | 'production') {
  if (!selected.value) return
  busy.value = true
  notice.value = ''
  try {
    await pilot('plugin:pilot|pilot_deploy_run', { project: project.value, targetId: selected.value.id, stage, confirmed: stage === 'production', withoutPreview: stage === 'production' && withoutPreview.value })
    confirming.value = false
    withoutPreview.value = false
    await load()
  } catch (e) {
    notice.value = String(e)
  } finally {
    busy.value = false
  }
}

const canOpenInTab = computed(() => !!store.activeId)
function openUrl(url: string) {
  if (store.activeId) inspector.openBrowser(store.activeId, url)
}

const when = (ms: number) => new Date(ms).toLocaleString('en-US', { dateStyle: 'medium', timeStyle: 'short', hour12: false })
const statusText = (r: Run) => (r.status === 'running' ? 'Running' : r.status === 'ok' ? 'Done' : 'Failed')

watch(project, () => {
  selectedId.value = ''
  view.value = null
  editing.value = null
  confirming.value = false
  void load()
})
onMounted(async () => {
  if (!store.contexts.length) await store.refreshContexts()
  const active = store.sessions.find((s) => s.id === store.activeId)?.contextId
  project.value = projects.value.find((p) => p.id === active)?.id ?? projects.value[0]?.id ?? ''
  // A deploy runs in the background: the history follows it.
  timer = setInterval(() => {
    if (running.value) void load()
  }, 1500)
})
onBeforeUnmount(() => {
  if (timer) clearInterval(timer)
})
</script>

<template>
  <div class="qp-dep">
    <header class="qp-dep-head">
      <div>
        <h1>Deploy</h1>
        <p>A preview first. Production only after you confirm.</p>
      </div>
      <select v-model="project" class="qp-input qp-dep-project" aria-label="Project">
        <option v-for="p in projects" :key="p.id" :value="p.id">{{ p.name }}</option>
      </select>
      <button class="qp-btn qp-btn-primary" :disabled="!project" @click="editing = blank(); token = ''"><Plus :size="13" /> New target</button>
    </header>

    <div v-if="!projects.length" class="qp-dep-empty">Deploy targets belong to a project. Add a workspace first.</div>
    <div v-else class="qp-dep-body">
      <aside class="qp-dep-list">
        <button v-for="t in view?.targets ?? []" :key="t.id" class="qp-dep-item" :class="{ on: t.id === selectedId }" @click="selectedId = t.id; confirming = false; editing = null">
          <span class="qp-dep-item-name">{{ t.name }}</span>
          <span class="qp-dep-item-line">{{ kindLabel(t.kind) }}</span>
        </button>
        <p v-if="view && !view.targets.length" class="qp-dep-none">No targets yet.</p>
      </aside>

      <main class="qp-dep-main">
        <form v-if="editing" class="qp-dep-form" @submit.prevent="save" @keydown.esc="editing = null">
          <h2>{{ editing.id ? 'Edit target' : 'New target' }}</h2>
          <label class="qp-dep-field"><span>Name</span><input v-model="editing.name" class="qp-input" placeholder="e.g. Website" spellcheck="false" /></label>
          <label class="qp-dep-field">
            <span>Where</span>
            <select v-model="editing.kind" class="qp-input"><option v-for="k in KINDS" :key="k.id" :value="k.id">{{ k.label }}</option></select>
          </label>
          <label class="qp-dep-field"><span>Build command <em>(optional, runs first; a failed build stops the deploy)</em></span><input v-model="editing.buildCommand" class="qp-input qp-mono" placeholder="npm run build" spellcheck="false" /></label>
          <template v-if="editing.kind !== 'custom'">
            <label class="qp-dep-field"><span>Folder to publish <em>(inside the project{{ editing.kind === 'vercel' ? '; empty = the project' : '' }})</em></span><input v-model="editing.folder" class="qp-input qp-mono" placeholder="dist" spellcheck="false" /></label>
          </template>
          <label v-if="editing.kind === 'netlify'" class="qp-dep-field"><span>Site <em>(optional; empty = what the folder is linked to)</em></span><input v-model="editing.site" class="qp-input qp-mono" placeholder="my-site" spellcheck="false" /></label>
          <template v-if="editing.kind === 'custom'">
            <label class="qp-dep-field"><span>Preview command <em>(print the address, it is picked up)</em></span><input v-model="editing.previewCommand" class="qp-input qp-mono" placeholder="./deploy.sh preview" spellcheck="false" /></label>
            <label class="qp-dep-field"><span>Production command</span><input v-model="editing.productionCommand" class="qp-input qp-mono" placeholder="./deploy.sh live" spellcheck="false" /></label>
          </template>
          <label class="qp-dep-field">
            <span>Token <em>(optional if you are signed in to the CLI; goes to the vault, is never shown again, and reaches the deploy step only, as {{ tokenName }})</em></span>
            <input v-model="token" class="qp-input qp-mono" type="password" autocomplete="off" :placeholder="editing.hasToken ? 'A token is saved. Paste another to replace it.' : ''" />
          </label>
          <div class="qp-dep-actions">
            <button type="button" class="qp-btn" @click="editing = null">Cancel</button>
            <button type="submit" class="qp-btn qp-btn-primary" :disabled="busy">Save</button>
          </div>
        </form>

        <template v-else-if="selected">
          <div class="qp-dep-title">
            <h2>{{ selected.name }}</h2>
            <span class="qp-dep-sub">{{ kindLabel(selected.kind) }}<template v-if="selected.hasToken"> · token saved</template></span>
            <span class="qp-dep-spacer" />
            <button class="qp-btn" @click="editing = { ...selected }; token = ''">Edit</button>
            <button class="qp-btn" :disabled="busy || running" @click="remove">Delete</button>
          </div>
          <p class="qp-dep-sub">Now in the project: <code>{{ headText }}</code></p>
          <p v-if="installHint" class="qp-dep-warn">The {{ kindLabel(selected.kind) }} CLI is not installed. Run <code>{{ installHint }}</code></p>

          <div class="qp-dep-actions">
            <button class="qp-btn" :disabled="busy || running" @click="deploy('preview')"><Rocket :size="13" /> Deploy a preview</button>
            <button class="qp-btn" :disabled="busy || running" @click="confirming = !confirming">Promote to production…</button>
          </div>

          <div v-if="confirming" class="qp-dep-confirm" role="alertdialog" aria-label="Deploy to production">
            <p v-if="previewed"><b>Deploy {{ headText }} to production?</b> A good preview of this commit exists. Real users will see this.</p>
            <p v-else><b>There is no good preview of what is in the project now ({{ headText }}).</b> Deploy a preview and check it first, or go without one if you mean to. Real users will see this.</p>
            <label v-if="!previewed" class="qp-check"><input v-model="withoutPreview" type="checkbox" /> Deploy to production without a preview</label>
            <div class="qp-dep-actions">
              <button class="qp-btn" @click="confirming = false; withoutPreview = false">Cancel</button>
              <button class="qp-btn qp-btn-danger" :disabled="busy || (!previewed && !withoutPreview)" @click="deploy('production')">Deploy to production</button>
            </div>
          </div>

          <h3 class="qp-dep-h3">History</h3>
          <p v-if="!runs.length" class="qp-dep-none">Nothing deployed yet.</p>
          <div v-for="r in runs" :key="r.id" class="qp-dep-run">
            <div class="qp-dep-run-top">
              <span class="qp-dep-stage" :class="`is-${r.stage}`">{{ r.stage === 'production' ? 'Production' : 'Preview' }}</span>
              <span :class="`qp-dep-status is-${r.status}`">{{ statusText(r) }}</span>
              <code v-if="r.commitId">{{ short(r.commitId) }}{{ r.dirty ? ' + changes' : '' }}</code>
              <span class="qp-dep-spacer" />
              <span class="qp-dep-sub">{{ when(r.startedAt) }}</span>
              <button class="qp-btn qp-btn-sm" @click="openLog = openLog === r.id ? '' : r.id">{{ openLog === r.id ? 'Hide log' : 'Log' }}</button>
            </div>
            <div v-if="r.url" class="qp-dep-url">
              <code>{{ r.url }}</code>
              <button v-if="canOpenInTab" class="qp-btn qp-btn-sm" @click="openUrl(r.url)">Open in the Browser tab</button>
            </div>
            <pre v-if="openLog === r.id" class="qp-dep-log">{{ r.log || 'Nothing yet.' }}</pre>
          </div>
        </template>
        <p v-else-if="view" class="qp-dep-empty">Add a target to deploy this project.</p>
        <p v-if="notice" class="qp-dep-bad" role="alert">{{ notice }}</p>
      </main>
    </div>
  </div>
</template>

<style scoped>
.qp-dep { flex: 1; min-height: 0; display: flex; flex-direction: column; font-size: 12.5px; color: var(--qss-text, #e8e8ec); }
.qp-dep-head { display: flex; align-items: center; gap: var(--qss-gap); padding: var(--qss-gap) var(--qss-page-edge); border-bottom: 1px solid var(--qss-border-subtle, #35353d); }
.qp-dep-head > div { flex: 1; min-width: 0; }
.qp-dep-head h1 { margin: 0; font-size: var(--qss-text-title); font-weight: 600; }
.qp-dep-head p { margin: 2px 0 0; color: var(--qss-text-secondary, #9a9aa5); }
.qp-dep-project { flex: none; max-width: 220px; }
.qp-dep-body { flex: 1; min-height: 0; display: flex; container: qp-dep / inline-size; }
.qp-dep-list { flex: none; width: 240px; padding: 10px; overflow-y: auto; border-right: 1px solid var(--qss-border-subtle, #35353d); }
.qp-dep-item { display: block; width: 100%; padding: 8px 10px; margin-bottom: 4px; border: none; border-radius: 6px; background: none; color: inherit; font: inherit; text-align: left; cursor: pointer; }
.qp-dep-item:hover, .qp-dep-item.on { background: var(--qss-bg-card, #292930); }
.qp-dep-item-name { display: block; font-weight: 500; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
.qp-dep-item-line { display: block; color: var(--qss-text-secondary, #9a9aa5); font-size: 11.5px; }
@container qp-dep (max-width: 600px) {
  .qp-dep-list { width: 170px; padding: 6px; }
  .qp-dep-main { padding: 12px !important; }
}
.qp-dep-main { flex: 1; min-width: 0; padding: 14px 20px; overflow-y: auto; display: flex; flex-direction: column; gap: 10px; }
.qp-dep-main h2 { margin: 0; font-size: 14px; font-weight: 600; }
.qp-dep-h3 { margin: 8px 0 0; font-size: 13px; font-weight: 600; }
.qp-dep-title { display: flex; align-items: center; gap: 8px; }
.qp-dep-spacer { flex: 1; }
.qp-dep-sub, .qp-dep-none { margin: 0; color: var(--qss-text-secondary, #9a9aa5); }
.qp-dep-empty { padding: 30px 20px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-dep code, .qp-mono { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 12px; }
.qp-dep-warn { margin: 0; color: var(--qss-warning, #fbbf24); }
.qp-dep-bad { margin: 0; color: var(--qss-error, #f87171); }
.qp-dep-actions { display: flex; flex-wrap: wrap; gap: 8px; }
.qp-dep-form { display: flex; flex-direction: column; gap: 10px; max-width: 620px; }
.qp-dep-field { display: flex; flex-direction: column; gap: 4px; }
.qp-dep-field > span { font-size: 11.5px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-dep-field em { font-style: normal; color: var(--qss-text-muted, #6e6e7a); }
.qp-dep-confirm { display: flex; flex-direction: column; gap: 8px; padding: 12px; border: 1px solid var(--qss-warning, #fbbf24); border-radius: 8px; }
.qp-dep-confirm p { margin: 0; line-height: 1.45; }
.qp-dep-run { display: flex; flex-direction: column; gap: 6px; padding: 8px 10px; border: 1px solid var(--qss-border-subtle, #35353d); border-radius: 6px; }
.qp-dep-run-top { display: flex; align-items: center; gap: 8px; }
.qp-dep-stage { padding: 1px 7px; border: 1px solid var(--qss-border, #47474f); border-radius: 10px; font-size: 11px; }
.qp-dep-stage.is-production { border-color: var(--qss-warning, #fbbf24); color: var(--qss-warning, #fbbf24); }
.qp-dep-status.is-ok { color: var(--qss-success, #34d399); }
.qp-dep-status.is-failed { color: var(--qss-error, #f87171); }
.qp-dep-status.is-running { color: var(--qss-text-secondary, #9a9aa5); }
.qp-dep-url { display: flex; align-items: center; gap: 8px; overflow-wrap: anywhere; }
.qp-dep-log { margin: 0; max-height: 260px; overflow: auto; padding: 8px; border-radius: 6px; background: var(--qss-bg, #18181e); font: 11.5px/1.45 var(--qss-font-mono, ui-monospace, monospace); white-space: pre-wrap; overflow-wrap: anywhere; }
.qp-input { min-width: 0; background: var(--qss-bg, #18181e); border: 1px solid var(--qss-border, #47474f); border-radius: 6px; color: var(--qss-text, #d4d4d8); font: inherit; padding: 6px 9px; outline: none; }
.qp-input:focus { border-color: var(--qss-accent, #a0a0a8); }
.qp-input::placeholder { color: var(--qss-text-muted, #6e6e7a); }
.qp-btn { display: inline-flex; align-items: center; gap: 6px; flex: none; padding: 5px 10px; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; background: none; color: inherit; font: inherit; cursor: pointer; }
.qp-btn:hover:not(:disabled) { background: var(--qss-bg-hover, rgba(255, 255, 255, 0.06)); }
.qp-btn:disabled { opacity: 0.5; cursor: default; }
.qp-btn-sm { padding: 2px 8px; font-size: 11.5px; }
.qp-btn-primary { background: var(--qss-text, #e8e8ec); border-color: var(--qss-text, #e8e8ec); color: var(--qss-bg, #18181e); font-weight: 600; }
.qp-btn-primary:hover:not(:disabled) { background: var(--qss-text, #e8e8ec); }
.qp-btn-danger { border-color: var(--qss-warning, #fbbf24); color: var(--qss-warning, #fbbf24); font-weight: 600; }
.qp-check { display: flex; align-items: center; gap: 7px; cursor: pointer; }
</style>
