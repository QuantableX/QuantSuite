<script setup lang="ts">
/**
 * Import conversations from the Claude Code and Codex CLIs (parity 08).
 * Lists the session logs of every CLI home on this computer, newest first —
 * searchable, 50 at a time — with what is already in QuantPilot, what cannot
 * be imported and why. The chosen ones become threads bound to their native
 * session, so the next message resumes it. Nothing is sent to a model and
 * the logs are never changed; importing one again adds only what is new.
 */
import { computed, onMounted, ref, watch } from 'vue'
import { Download, X } from 'lucide-vue-next'
import { pilot } from '#pilot/utils/invoke'
import { usePilotStore } from '#pilot/stores/pilot'

interface Listed {
  provider: string
  home: string
  path: string
  sessionId: string
  cwd: string
  title: string
  createdAt: number
  updatedAt: number
  messages: number
  model: string | null
  problems: string[]
  fromQuantpilot: boolean
  threadId: string | null
  state: 'imported' | 'native' | null
  unavailable: string | null
}
interface Page {
  items: Listed[]
  total: number
  problems: string[]
}
interface Outcome {
  imported: string[]
  existing: [string, number][]
  failed: [string, string][]
  cancelled: boolean
}

const emit = defineEmits<{ close: [] }>()
const store = usePilotStore()
const provider = ref<'claude' | 'codex'>('claude')
const search = ref('')
const items = ref<Listed[]>([])
const total = ref(0)
const problems = ref<string[]>([])
const loading = ref(false)
const error = ref<string | null>(null)
const picked = ref<Set<string>>(new Set())
const context = ref('general')
const busy = ref(false)
const outcome = ref<Outcome | null>(null)

const options = computed(() => store.contexts.map((c) => ({ id: c.id, name: c.name })))
const homes = computed(() => [...new Set(items.value.map((i) => i.home))])

async function load(more = false) {
  loading.value = true
  error.value = null
  try {
    const page = await pilot<Page>('plugin:pilot|pilot_import_list', { provider: provider.value, search: search.value || null, offset: more ? items.value.length : 0 })
    items.value = more ? [...items.value, ...(page?.items ?? [])] : (page?.items ?? [])
    total.value = page?.total ?? 0
    problems.value = page?.problems ?? []
  } catch (e) {
    error.value = String(e)
  } finally {
    loading.value = false
  }
}

let timer: ReturnType<typeof setTimeout> | undefined
watch(search, () => {
  clearTimeout(timer)
  timer = setTimeout(() => void load(), 250)
})
watch(provider, () => {
  picked.value = new Set()
  outcome.value = null
  void load()
})
onMounted(() => void load())

function toggle(i: Listed) {
  if (i.unavailable) return
  const next = new Set(picked.value)
  if (next.has(i.path)) next.delete(i.path)
  else if (next.size < 50) next.add(i.path)
  picked.value = next
}

async function run() {
  if (!picked.value.size || busy.value) return
  busy.value = true
  outcome.value = null
  try {
    outcome.value = await pilot<Outcome>('plugin:pilot|pilot_import', { provider: provider.value, paths: [...picked.value], context: context.value })
    picked.value = new Set()
    await store.refreshSessions()
    await load()
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}

async function cancel() {
  await pilot('plugin:pilot|pilot_import_cancel').catch(() => undefined)
}

function open(id: string | null) {
  if (!id) return
  void store.openSession(id)
  emit('close')
}

const date = (ms: number) =>
  ms ? new Date(ms).toLocaleString('en-US', { year: 'numeric', month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', hour12: false }) : '—'
const short = (p: string) => (p.length > 64 ? `…${p.slice(-63)}` : p)
</script>

<template>
  <Teleport to="body">
    <div class="qp-im-veil" @pointerdown.self="!busy && emit('close')">
      <div class="qp-im" role="dialog" aria-label="Import conversations" @keydown.esc.stop="!busy && emit('close')">
        <div class="qp-im-head">
          <Download :size="15" />
          <span class="qp-im-title">Import conversations</span>
          <div class="qp-im-tabs" role="tablist">
            <button role="tab" :aria-selected="provider === 'claude'" :class="{ on: provider === 'claude' }" @click="provider = 'claude'"><PilotAgentIcon agent="claude" :size="12" decorative /> Claude Code</button>
            <button role="tab" :aria-selected="provider === 'codex'" :class="{ on: provider === 'codex' }" @click="provider = 'codex'"><PilotAgentIcon agent="codex" :size="12" decorative /> Codex</button>
          </div>
          <button class="qp-im-x" aria-label="Close" :disabled="busy" @click="emit('close')"><X :size="14" /></button>
        </div>
        <p class="qp-im-note">
          The CLI's own logs on this computer. Importing reads them only — nothing is sent to a model, the logs stay as they are — and your next message continues the same session.
        </p>
        <div class="qp-im-bar">
          <input v-model="search" class="qp-im-search" placeholder="Search title, folder or session id" aria-label="Search conversations" />
          <span class="qp-im-count">{{ total }} conversation{{ total === 1 ? '' : 's' }}<template v-if="homes.length > 1"> in {{ homes.length }} homes</template></span>
        </div>
        <div class="qp-im-list" role="listbox" aria-multiselectable="true">
          <div v-if="loading && !items.length" class="qp-im-empty">Reading the logs…</div>
          <div v-else-if="!items.length" class="qp-im-empty">No {{ provider === 'codex' ? 'Codex' : 'Claude Code' }} conversations found{{ search ? ' for this search' : '' }}.</div>
          <div
            v-for="i in items"
            :key="i.path"
            class="qp-im-row"
            role="option"
            :aria-selected="picked.has(i.path)"
            :aria-disabled="!!i.unavailable"
            :class="{ on: picked.has(i.path), off: !!i.unavailable }"
            tabindex="0"
            @click="toggle(i)"
            @keydown.space.prevent="toggle(i)"
          >
            <input type="checkbox" :checked="picked.has(i.path)" :disabled="!!i.unavailable" tabindex="-1" @click.stop="toggle(i)" />
            <div class="qp-im-main">
              <div class="qp-im-line">
                <span class="qp-im-name" :title="i.title">{{ i.title }}</span>
                <span v-if="i.state === 'imported'" class="qp-im-tag ok">imported<template v-if="!i.unavailable"> · import again adds new messages</template></span>
                <span v-else-if="i.state === 'native'" class="qp-im-tag">already a QuantPilot thread</span>
              </div>
              <div class="qp-im-sub">
                <span :title="i.cwd">{{ short(i.cwd) || 'no folder recorded' }}</span>
                <span>{{ date(i.updatedAt) }}</span>
                <span>{{ i.messages }} messages</span>
                <span v-if="i.model">{{ i.model }}</span>
              </div>
              <div v-if="i.unavailable" class="qp-im-warn">{{ i.unavailable }}</div>
              <div v-for="p in i.problems" :key="p" class="qp-im-warn">{{ p }}</div>
            </div>
            <button v-if="i.threadId" class="qp-im-open" @click.stop="open(i.threadId)">Open</button>
          </div>
          <button v-if="items.length < total" class="qp-im-more" :disabled="loading" @click="load(true)">Show more ({{ total - items.length }})</button>
        </div>
        <div v-if="problems.length" class="qp-im-warn qp-im-problems" :title="problems.join('\n')">{{ problems.length }} log(s) could not be read.</div>
        <div v-if="error" class="qp-im-err" role="alert">{{ error }}</div>
        <div v-if="outcome" class="qp-im-result" role="status">
          <span v-if="outcome.imported.length">{{ outcome.imported.length }} imported.</span>
          <span v-if="outcome.existing.length">{{ outcome.existing.length }} already here ({{ outcome.existing.reduce((n, e) => n + e[1], 0) }} new messages added).</span>
          <span v-if="outcome.cancelled">Stopped — what was imported so far is complete.</span>
          <span v-for="f in outcome.failed" :key="f[0]" class="qp-im-err">{{ f[1] }}</span>
        </div>
        <div class="qp-im-foot">
          <label class="qp-im-ctx">
            Folders outside your projects go to
            <select v-model="context">
              <option v-for="o in options" :key="o.id" :value="o.id">{{ o.name }}</option>
            </select>
          </label>
          <button v-if="busy" class="qp-btn qp-btn-sm" @click="cancel">Stop</button>
          <button class="qp-btn qp-btn-sm qp-btn-primary" :disabled="!picked.size || busy" @click="run">
            {{ busy ? 'Importing…' : `Import ${picked.size || ''}`.trim() }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.qp-im-veil { position: fixed; inset: 0; z-index: 1100; display: flex; align-items: center; justify-content: center; background: rgba(0, 0, 0, 0.45); }
.qp-im {
  width: min(880px, calc(100vw - 64px));
  height: min(720px, calc(100vh - 64px));
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 14px 16px;
  border-radius: 12px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg-raised, #1f1f25);
  color: var(--qss-text, #d4d4d8);
  font-size: 12.5px;
}
.qp-im-head { display: flex; align-items: center; gap: 8px; }
.qp-im-title { font-size: 13.5px; font-weight: 600; }
.qp-im-tabs { display: flex; gap: 2px; margin-left: 12px; padding: 2px; border-radius: 8px; background: var(--qss-bg, #18181e); }
.qp-im-tabs button { border: none; background: transparent; color: var(--qss-text-secondary, #9a9aa5); padding: 3px 10px; border-radius: 6px; font: inherit; cursor: pointer; }
.qp-im-tabs button.on { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-im-x { margin-left: auto; border: none; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; }
.qp-im-note { margin: 0; color: var(--qss-text-muted, #6e6e7a); font-size: 12px; }
.qp-im-bar { display: flex; align-items: center; gap: 10px; }
.qp-im-search { flex: 1; height: 28px; padding: 0 9px; border-radius: 7px; border: 1px solid var(--qss-border, #47474f); background: var(--qss-bg, #18181e); color: inherit; font: inherit; outline: none; }
.qp-im-search:focus { border-color: var(--qss-accent, #7aa2f7); }
.qp-im-count { color: var(--qss-text-muted, #6e6e7a); font-size: 12px; white-space: nowrap; }
.qp-im-list { flex: 1; min-height: 0; overflow: auto; display: flex; flex-direction: column; gap: 2px; padding-right: 2px; }
.qp-im-empty { padding: 24px; text-align: center; color: var(--qss-text-muted, #6e6e7a); }
.qp-im-row { display: flex; align-items: flex-start; gap: 9px; padding: 7px 8px; border-radius: 8px; cursor: pointer; outline: none; }
.qp-im-row:hover, .qp-im-row:focus-visible { background: var(--qss-bg-hover, #313139); }
.qp-im-row.on { background: color-mix(in srgb, var(--qss-accent, #7aa2f7) 14%, transparent); }
.qp-im-row.off { cursor: default; opacity: 0.7; }
.qp-im-row input { margin-top: 2px; }
.qp-im-main { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
.qp-im-line { display: flex; align-items: center; gap: 8px; min-width: 0; }
.qp-im-name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 500; }
.qp-im-tag { flex-shrink: 0; padding: 0 6px; border-radius: 8px; border: 1px solid var(--qss-border, #47474f); color: var(--qss-text-secondary, #9a9aa5); font-size: 11px; }
.qp-im-tag.ok { border-color: color-mix(in srgb, var(--qss-success, #34d399) 50%, transparent); color: var(--qss-success, #34d399); }
.qp-im-sub { display: flex; flex-wrap: wrap; gap: 4px 12px; color: var(--qss-text-muted, #6e6e7a); font-size: 11.5px; }
.qp-im-sub span:first-child { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 420px; }
.qp-im-warn { color: var(--qss-warning, #d29a3f); font-size: 11.5px; }
.qp-im-err { color: var(--qss-error, #e06c75); font-size: 12px; }
.qp-im-open { flex-shrink: 0; align-self: center; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; background: transparent; color: inherit; font: inherit; padding: 2px 8px; cursor: pointer; }
.qp-im-more { align-self: center; margin: 6px; border: none; background: transparent; color: var(--qss-text-secondary, #9a9aa5); font: inherit; cursor: pointer; }
.qp-im-result { display: flex; flex-wrap: wrap; gap: 4px 12px; font-size: 12px; }
.qp-im-foot { display: flex; align-items: center; gap: 8px; justify-content: flex-end; }
.qp-im-ctx { margin-right: auto; display: inline-flex; align-items: center; gap: 6px; color: var(--qss-text-secondary, #9a9aa5); font-size: 12px; }
.qp-im-ctx select { height: 26px; border-radius: 6px; border: 1px solid var(--qss-border, #47474f); background: var(--qss-bg, #18181e); color: inherit; font: inherit; }
</style>
