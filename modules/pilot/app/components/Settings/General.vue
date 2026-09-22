<script setup lang="ts">
/**
 * QuantPilot's section in the unified settings modal: which agents are
 * found, where a CLI is when PATH is not enough, what a new session starts
 * with, whether the suite is attached, and the user's own adapters.
 */
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { qs, type ConsoleShell } from '@quantsuite/core'
import { usePilotStore } from '#pilot/stores/pilot'
import type { CustomAdapter, PilotSettings } from '#pilot/types'

const store = usePilotStore()
const draft = reactive<PilotSettings>({
  version: 2,
  shell: '',
  defaultMode: 'full',
  exe: {},
  model: {},
  effort: '',
  attachQuantmcp: true,
  includeGeneralVault: true,
  custom: [],
})
const notice = ref('')
const busy = ref(false)
const shells = ref<ConsoleShell[]>([])

function pull() {
  if (!store.settings) return
  const s = store.settings
  draft.version = s.version
  draft.shell = s.shell
  draft.defaultMode = s.defaultMode
  draft.exe = { ...s.exe }
  draft.model = { ...s.model }
  draft.effort = s.effort
  draft.attachQuantmcp = s.attachQuantmcp
  draft.includeGeneralVault = s.includeGeneralVault
  draft.custom = s.custom.map((c) => ({ ...c }))
}
onMounted(async () => {
  if (!store.settings) await store.loadSettings()
  pull()
  if (!store.status) void store.refreshStatus()
  try {
    shells.value = await qs.console.listShells()
  } catch {
    shells.value = []
  }
})
watch(() => store.settings, pull)

async function save() {
  busy.value = true
  notice.value = ''
  try {
    await store.saveSettings({
      ...draft,
      exe: { ...draft.exe },
      model: { ...draft.model },
      custom: draft.custom.map((c) => ({ ...c })),
    })
    notice.value = 'Saved.'
  } catch (e) {
    notice.value = e instanceof Error ? e.message : String(e)
  } finally {
    busy.value = false
  }
}

async function browse(id: string) {
  try {
    const dialog = await import('@tauri-apps/plugin-dialog')
    const picked = await dialog.open({ multiple: false, title: 'Pick the executable' })
    if (typeof picked === 'string') draft.exe[id] = picked
  } catch {
    /* outside Tauri the text field still works */
  }
}

function addCustom() {
  draft.custom.push({ id: '', label: '', exe: '', args: '', resumeArgs: '' } satisfies CustomAdapter)
}
function removeCustom(i: number) {
  draft.custom.splice(i, 1)
}

const status = computed(() => store.status)
const adapters = computed(() => store.adapters)
const builtIn = computed(() => adapters.value.filter((a) => !a.custom))
const signalsLabel = (s: string | null) =>
  s === 'hooks' ? 'hooks' : s === 'transcript' ? 'session file' : s === 'activity' ? 'activity only' : ''
</script>

<template>
  <div class="qp-settings">
    <h3 class="qp-settings-title">Agents</h3>
    <p class="qp-settings-desc">
      A session is a terminal; the agents found here are commands in it (<code>claude</code>, <code>codex</code>, …)
      that run as that session. Log in once outside the suite where a CLI needs it: <code>claude auth login</code> ·
      <code>codex login</code>.
    </p>

    <div v-if="status" class="qp-settings-status">
      <div v-for="a in adapters" :key="a.id" class="qp-settings-row">
        <span class="qp-settings-dot" :class="{ ok: store.adapterReady(a.id), bad: !store.adapterReady(a.id) }" />
        <span class="qp-settings-name">{{ a.label }}</span>
        <span class="qp-settings-detail">
          {{ a.version || (a.found ? 'found' : 'not found') }}<template v-if="a.detail"> · {{ a.detail }}</template>
          <template v-if="a.found && a.signals"> · {{ signalsLabel(a.signals) }}</template>
        </span>
      </div>
      <button class="qp-btn qp-btn-sm" :disabled="store.statusBusy" @click="store.refreshStatus()">{{ store.statusBusy ? 'Checking…' : 'Check again' }}</button>
    </div>

    <h3 class="qp-settings-title">Executables <em>(empty = the usual install or PATH)</em></h3>
    <div class="qp-settings-grid">
      <label v-for="a in builtIn" :key="a.id" class="qp-settings-field">
        <span>{{ a.label }}</span>
        <div class="qp-settings-inline">
          <input v-model="draft.exe[a.id]" class="qp-input" :placeholder="a.path || a.id" spellcheck="false" />
          <button class="qp-btn qp-btn-sm" @click="browse(a.id)">Browse</button>
        </div>
      </label>
    </div>

    <h3 class="qp-settings-title">New sessions</h3>
    <div class="qp-settings-grid">
      <label class="qp-settings-field">
        <span>Terminal shell</span>
        <select v-model="draft.shell" class="qp-input">
          <option value="">default{{ status?.shell ? ` (${status.shell})` : '' }}</option>
          <option v-for="sh in shells" :key="sh.path" :value="sh.path">{{ sh.label }}</option>
        </select>
      </label>
      <label class="qp-settings-field">
        <span>Permissions <em>(what the wrappers pass on)</em></span>
        <select v-model="draft.defaultMode" class="qp-input">
          <option value="full">Full access</option>
          <option value="auto">Auto</option>
          <option value="ask">Ask before tools</option>
        </select>
      </label>
      <label v-for="a in builtIn" :key="a.id" class="qp-settings-field">
        <span>{{ a.label }} model <em>(empty = the CLI's default)</em></span>
        <input v-model="draft.model[a.id]" class="qp-input" spellcheck="false" />
      </label>
      <label class="qp-settings-field">
        <span>Claude effort</span>
        <select v-model="draft.effort" class="qp-input">
          <option value="">default</option>
          <option value="low">low</option>
          <option value="medium">medium</option>
          <option value="high">high</option>
          <option value="xhigh">xhigh</option>
          <option value="max">max</option>
        </select>
      </label>
    </div>

    <h3 class="qp-settings-title">The suite as context</h3>
    <label class="qp-settings-check">
      <input v-model="draft.attachQuantmcp" type="checkbox" />
      <span>Attach QuantMCP at launch where the CLI takes it — Claude Code and Codex <em>({{ status?.mcpUrl || 'the suite\'s MCP server' }}); pi, omp, OpenCode and Gemini use their own MCP config</em></span>
    </label>
    <label class="qp-settings-check">
      <input v-model="draft.includeGeneralVault" type="checkbox" />
      <span>Workspace sessions can also read the General memory vault <em>({{ status?.generalPath || '…' }})</em></span>
    </label>

    <h3 class="qp-settings-title">Your own agents</h3>
    <p class="qp-settings-desc">
      Any CLI with a terminal UI. Placeholders in the arguments: <code>{cwd}</code> <code>{session}</code>
      <code>{title}</code> <code>{mode}</code> <code>{model}</code> <code>{vault}</code>. Such an agent gets the
      terminal, the session row and a face driven by its output and bell; hooks and files need an adapter.
    </p>
    <div v-for="(c, i) in draft.custom" :key="i" class="qp-custom">
      <div class="qp-settings-grid">
        <label class="qp-settings-field"><span>Id</span><input v-model="c.id" class="qp-input" placeholder="my-agent" spellcheck="false" /></label>
        <label class="qp-settings-field"><span>Name</span><input v-model="c.label" class="qp-input" placeholder="My Agent" spellcheck="false" /></label>
      </div>
      <label class="qp-settings-field"><span>Executable</span><input v-model="c.exe" class="qp-input" placeholder="my-agent or C:\path\to\my-agent.exe" spellcheck="false" /></label>
      <label class="qp-settings-field"><span>Arguments for a new session</span><input v-model="c.args" class="qp-input" placeholder="--cwd {cwd} --name &quot;{title}&quot;" spellcheck="false" /></label>
      <label class="qp-settings-field"><span>Arguments when reopened <em>(empty = the same)</em></span><input v-model="c.resumeArgs" class="qp-input" placeholder="--resume {session}" spellcheck="false" /></label>
      <button class="qp-btn qp-btn-sm qp-btn-danger" @click="removeCustom(i)">Remove</button>
    </div>
    <div>
      <button class="qp-btn qp-btn-sm" @click="addCustom">Add an agent</button>
    </div>

    <div class="qp-settings-actions">
      <button class="qp-btn qp-btn-primary" :disabled="busy" @click="save">{{ busy ? 'Saving…' : 'Save' }}</button>
      <span v-if="notice" class="qp-settings-notice">{{ notice }}</span>
    </div>
  </div>
</template>

<style scoped>
.qp-settings { display: flex; flex-direction: column; gap: 10px; font-size: 12.5px; color: var(--qss-text, #d4d4d8); }
.qp-settings-title { margin: 6px 0 0; font-size: 13px; font-weight: 600; }
.qp-settings-title em { font-style: normal; font-weight: 400; color: var(--qss-text-muted, #6e6e7a); font-size: 11.5px; }
.qp-settings-desc { margin: 0; color: var(--qss-text-secondary, #9a9aa5); line-height: 1.5; }
.qp-settings-desc code { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11.5px; background: var(--qss-bg-card, #292930); padding: 1px 5px; border-radius: 4px; }
.qp-settings-status { display: flex; flex-direction: column; gap: 5px; padding: 8px 10px; border: 1px solid var(--qss-border-subtle, #35353d); border-radius: var(--qss-radius, 6px); background: var(--qss-bg-raised, #1f1f25); align-items: flex-start; }
.qp-settings-row { display: flex; align-items: center; gap: 8px; font-size: 12px; }
.qp-settings-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--qss-border, #47474f); flex-shrink: 0; }
.qp-settings-dot.ok { background: var(--qss-success, #34d399); }
.qp-settings-dot.bad { background: var(--qss-error, #f87171); }
.qp-settings-name { font-weight: 600; min-width: 90px; }
.qp-settings-detail { color: var(--qss-text-secondary, #9a9aa5); }
.qp-settings-field { display: flex; flex-direction: column; gap: 4px; }
.qp-settings-field > span { font-size: 11.5px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-settings-field em { font-style: normal; color: var(--qss-text-muted, #6e6e7a); }
.qp-settings-inline { display: flex; gap: 6px; }
.qp-settings-inline .qp-input { flex: 1; }
.qp-settings-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
.qp-input {
  background: var(--qss-bg, #18181e);
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 12.5px;
  padding: 6px 9px;
  outline: none;
}
.qp-input:focus { border-color: var(--qss-accent, #a0a0a8); }
.qp-settings-check { display: flex; align-items: flex-start; gap: 8px; line-height: 1.4; }
.qp-settings-check em { font-style: normal; color: var(--qss-text-muted, #6e6e7a); }
.qp-custom { display: flex; flex-direction: column; gap: 8px; padding: 10px; border: 1px solid var(--qss-border-subtle, #35353d); border-radius: var(--qss-radius, 6px); align-items: flex-start; }
.qp-custom .qp-settings-grid, .qp-custom .qp-settings-field { width: 100%; }
.qp-settings-actions { display: flex; align-items: center; gap: 10px; margin-top: 4px; }
.qp-settings-notice { color: var(--qss-text-secondary, #9a9aa5); }
</style>
