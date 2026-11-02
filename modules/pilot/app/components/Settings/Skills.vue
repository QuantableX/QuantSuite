<script setup lang="ts">
/**
 * The skill library (parity 09): import a skill folder once, then turn it on
 * per provider and scope — for you everywhere (user) or for one thread's
 * checkout (project). QuantPilot copies it where the CLI itself looks and
 * records every file it wrote, so it can tell an active copy from an outdated
 * one (the library is newer), a drifted one (someone edited it) or a missing
 * one — and never overwrites or deletes an edit or a skill it did not write.
 * Providers without skill folders are listed as unsupported, never as
 * installed. Rules for agents stay in AgentOS (QuantMCP).
 */
import { computed, onMounted, ref, watch } from 'vue'
import { FolderOpen } from 'lucide-vue-next'
import { pilot } from '#pilot/utils/invoke'
import { usePilotStore } from '#pilot/stores/pilot'

interface Cell {
  provider: string
  scope: 'user' | 'project'
  state: 'active' | 'outdated' | 'drifted' | 'missing' | 'off'
  dir: string | null
  drifted: string[]
}
interface ItemView {
  id: string
  name: string
  description: string
  source: string
  hash: string
  files: string[]
  importedAt: number
  cells: Cell[]
}
interface LibraryView {
  providers: { id: string; name: string; installed: boolean; supported: boolean }[]
  items: ItemView[]
  unmanaged: [string, string, string, string][]
  project: [string, string] | null
}
interface Change {
  written: string[]
  kept: string[]
  dir: string
}

const store = usePilotStore()
const view = ref<LibraryView | null>(null)
const thread = ref<string>('')
const path = ref('')
const notice = ref('')
const error = ref('')
const busy = ref(false)

const threads = computed(() => store.sessions.filter((s) => s.kind === 'chat').slice(0, 200))
const supported = computed(() => view.value?.providers.filter((p) => p.supported) ?? [])
const unsupported = computed(() => view.value?.providers.filter((p) => !p.supported) ?? [])
const STATE: Record<Cell['state'], string> = {
  active: 'Active',
  outdated: 'Outdated — the library has a newer version',
  drifted: 'Edited outside QuantPilot',
  missing: 'Missing — its folder is gone',
  off: 'Off',
}

async function load() {
  try {
    view.value = await pilot<LibraryView>('plugin:pilot|pilot_skill_library', { threadId: thread.value || null })
    error.value = ''
  } catch (e) {
    error.value = String(e)
  }
}
onMounted(() => {
  thread.value = store.activeId ?? ''
  void load()
})
watch(thread, () => void load())

async function pick() {
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const chosen = await open({ directory: true, title: 'A skill folder (with a SKILL.md)' })
    if (typeof chosen === 'string') path.value = chosen
  } catch {
    /* browser build */
  }
}

async function run<T>(what: () => Promise<T>, ok: (r: T) => string) {
  busy.value = true
  notice.value = ''
  error.value = ''
  try {
    notice.value = ok(await what())
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
    await load()
  }
}

function importIt(replace = false) {
  if (!path.value.trim()) return
  void run(
    () => pilot<ItemView>('plugin:pilot|pilot_skill_import', { path: path.value.trim(), replace }),
    (i) => (i ? `Imported "${i.name}" (${i.files.length} file${i.files.length === 1 ? '' : 's'}). Turn it on below.` : '')
  )
}

function changed(c: Change | null, verb: string) {
  if (!c) return ''
  const kept = c.kept.length ? ` Kept ${c.kept.length} edited file(s): ${c.kept.join(', ')}.` : ''
  return `${verb} ${c.dir}.${kept}`
}

function toggle(item: ItemView, cell: Cell, keepEdited = false) {
  const args = { id: item.id, provider: cell.provider, scope: cell.scope, threadId: thread.value || null }
  if (cell.state === 'off' || cell.state === 'outdated' || (cell.state === 'drifted' && keepEdited) || cell.state === 'missing') {
    void run(() => pilot<Change>('plugin:pilot|pilot_skill_activate', { ...args, keepEdited }), (c) => changed(c, 'Wrote'))
  } else {
    void run(() => pilot<Change>('plugin:pilot|pilot_skill_deactivate', args), (c) => changed(c, 'Removed from'))
  }
}

function remove(item: ItemView) {
  void run(() => pilot('plugin:pilot|pilot_skill_remove', { id: item.id }), () => `Removed "${item.name}" from the library.`)
}

const cellOf = (item: ItemView, provider: string, scope: string) => item.cells.find((c) => c.provider === provider && c.scope === scope) ?? null
const active = (item: ItemView) => item.cells.some((c) => c.state !== 'off')
</script>

<template>
  <div class="qp-settings" data-module="pilot">
    <p class="qp-settings-desc">
      Import a skill folder once, then turn it on for Claude Code or Codex — for you everywhere, or for one thread's project. QuantPilot copies it where the CLI looks and
      never overwrites or removes a file someone else edited or a skill it did not install. Rules for agents live in AgentOS.
    </p>

    <div class="qp-sk-import">
      <input v-model="path" class="qp-settings-input" placeholder="Skill folder (with a SKILL.md)" aria-label="Skill folder" @keydown.enter="importIt()" />
      <button class="qp-btn qp-btn-sm" title="Choose a folder" @click="pick"><FolderOpen :size="13" /></button>
      <button class="qp-btn qp-btn-sm qp-btn-primary" :disabled="busy || !path.trim()" @click="importIt()">Import</button>
    </div>
    <label class="qp-settings-field">
      <span>Project scope means the checkout of</span>
      <select v-model="thread" class="qp-settings-input">
        <option value="">— no thread (user scope only) —</option>
        <option v-for="t in threads" :key="t.id" :value="t.id">{{ t.title || 'Untitled' }} · {{ t.contextName }}</option>
      </select>
    </label>
    <div v-if="view?.project" class="qp-sk-dim">{{ view.project[1] }}</div>

    <div v-if="notice" class="qp-sk-ok" role="status">{{ notice }}</div>
    <div v-if="error" class="qp-sk-err" role="alert">
      {{ error }}
      <button v-if="error.includes('already has')" class="qp-link" @click="importIt(true)">Replace its source</button>
    </div>

    <h4 class="qp-settings-title">Library</h4>
    <div v-if="view && !view.items.length" class="qp-sk-dim">No skills imported yet.</div>
    <table v-else-if="view" class="qp-sk-table">
      <thead>
        <tr>
          <th>Skill</th>
          <template v-for="p in supported" :key="p.id">
            <th>{{ p.name }} · user</th>
            <th>{{ p.name }} · project</th>
          </template>
          <th />
        </tr>
      </thead>
      <tbody>
        <tr v-for="item in view.items" :key="item.id">
          <td class="qp-sk-name">
            <div>{{ item.name }}</div>
            <div class="qp-sk-dim" :title="item.source">{{ item.description }}</div>
            <div class="qp-sk-dim qp-sk-mono" :title="`From ${item.source}`">{{ item.hash.slice(0, 10) }} · {{ item.files.length }} files</div>
          </td>
          <template v-for="p in supported" :key="p.id">
            <td v-for="scope in ['user', 'project']" :key="scope">
              <template v-if="cellOf(item, p.id, scope)">
                <button
                  class="qp-sk-cell"
                  :data-state="cellOf(item, p.id, scope)!.state"
                  :title="`${STATE[cellOf(item, p.id, scope)!.state]}\n${cellOf(item, p.id, scope)!.dir ?? ''}`"
                  :disabled="busy || (scope === 'project' && !thread) || (cellOf(item, p.id, scope)!.state === 'drifted')"
                  @click="toggle(item, cellOf(item, p.id, scope)!)"
                >
                  {{ cellOf(item, p.id, scope)!.state === 'off' ? 'Turn on' : cellOf(item, p.id, scope)!.state === 'outdated' ? 'Update' : cellOf(item, p.id, scope)!.state === 'missing' ? 'Restore' : STATE[cellOf(item, p.id, scope)!.state].split(' ')[0] }}
                </button>
                <div v-if="cellOf(item, p.id, scope)!.state === 'drifted'" class="qp-sk-drift">
                  <span>{{ cellOf(item, p.id, scope)!.drifted.join(', ') }} edited</span>
                  <button class="qp-link" :disabled="busy" @click="toggle(item, cellOf(item, p.id, scope)!, true)">Update, keep edits</button>
                  <button class="qp-link" :disabled="busy" @click="toggle(item, { ...cellOf(item, p.id, scope)!, state: 'active' })">Turn off, keep edits</button>
                </div>
              </template>
              <span v-else class="qp-sk-dim">pick a thread</span>
            </td>
          </template>
          <td>
            <button class="qp-link" :disabled="busy || active(item)" :title="active(item) ? 'Turn it off everywhere first' : 'Remove from the library'" @click="remove(item)">Remove</button>
          </td>
        </tr>
      </tbody>
    </table>

    <template v-if="view?.unmanaged.length">
      <h4 class="qp-settings-title">Installed by hand</h4>
      <p class="qp-settings-desc">Skills in these folders were not installed by QuantPilot; they keep working and are never changed here.</p>
      <ul class="qp-sk-list">
        <li v-for="u in view.unmanaged" :key="u[3]"><b>{{ u[2] }}</b> <span class="qp-sk-dim">{{ u[0] === 'codex' ? 'Codex' : 'Claude Code' }} · {{ u[1] }} · {{ u[3] }}</span></li>
      </ul>
    </template>
    <p v-if="unsupported.length" class="qp-sk-dim">
      No skill folders QuantPilot can manage for {{ unsupported.map((p) => p.name).join(', ') }} — they are not shown as installed.
    </p>
  </div>
</template>

<style scoped>
.qp-settings { display: flex; flex-direction: column; gap: 10px; font-size: 12.5px; color: var(--qss-text, #d4d4d8); }
.qp-settings-title { margin: 6px 0 0; font-size: 13px; font-weight: 600; }
.qp-settings-desc { margin: 0; color: var(--qss-text-secondary, #9a9aa5); line-height: 1.5; }
.qp-settings-field { display: flex; flex-direction: column; gap: 4px; }
.qp-settings-field > span { font-size: 11.5px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-settings-input { height: 28px; padding: 0 9px; border-radius: 7px; border: 1px solid var(--qss-border, #47474f); background: var(--qss-bg, #18181e); color: inherit; font: inherit; }
.qp-sk-import { display: flex; gap: 6px; align-items: center; }
.qp-sk-import .qp-settings-input { flex: 1; }
.qp-sk-dim { color: var(--qss-text-muted, #6e6e7a); font-size: 11.5px; }
.qp-sk-mono { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; }
.qp-sk-ok { color: var(--qss-success, #34d399); font-size: 12px; }
.qp-sk-err { color: var(--qss-error, #e06c75); font-size: 12px; }
.qp-sk-table { width: 100%; border-collapse: collapse; }
.qp-sk-table th { text-align: left; font-weight: 500; font-size: 11.5px; color: var(--qss-text-secondary, #9a9aa5); padding: 4px 6px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); }
.qp-sk-table td { padding: 6px; vertical-align: top; border-bottom: 1px solid var(--qss-border-subtle, #35353d); }
.qp-sk-name { max-width: 240px; }
.qp-sk-cell { border: 1px solid var(--qss-border, #47474f); border-radius: 6px; background: transparent; color: inherit; font: inherit; font-size: 11.5px; padding: 2px 8px; cursor: pointer; }
.qp-sk-cell[data-state='active'] { border-color: color-mix(in srgb, var(--qss-success, #34d399) 60%, transparent); color: var(--qss-success, #34d399); }
.qp-sk-cell[data-state='outdated'], .qp-sk-cell[data-state='missing'] { border-color: color-mix(in srgb, var(--qss-warning, #d29a3f) 60%, transparent); color: var(--qss-warning, #d29a3f); }
.qp-sk-cell[data-state='drifted'] { border-color: color-mix(in srgb, var(--qss-error, #e06c75) 60%, transparent); color: var(--qss-error, #e06c75); }
.qp-sk-cell:disabled { cursor: default; opacity: 0.8; }
.qp-sk-drift { display: flex; flex-direction: column; gap: 2px; margin-top: 4px; font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-sk-list { margin: 0; padding-left: 16px; display: flex; flex-direction: column; gap: 3px; }
</style>
