<script setup lang="ts">
/**
 * Settings → Providers (card 19): each agent CLI as a card — found where,
 * which version, signed in (the CLI's own `auth status` / `login status`,
 * no prompt, nothing started), and what QuantPilot launches it with: the
 * executable, the default model, extra arguments, environment variables,
 * the user's own model ids, whether it is offered. Update runs the
 * installer that put the CLI there (npm, winget, Claude's native installer)
 * or the CLI's own `update`, after showing exactly what will run, and reads
 * the version again. "Prefer saved Claude login" drops inherited API keys,
 * tokens and endpoints from new Claude processes.
 */
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { RefreshCw } from 'lucide-vue-next'
import { usePilotStore } from '#pilot/stores/pilot'
import { pilot } from '#pilot/utils/invoke'
import type { CustomAdapter, PilotSettings, ProviderPrefs } from '#pilot/types'

const store = usePilotStore()
/** Agents that run chat threads get the launch options (card 20 adds ACP agents, pi and omp). */
const chatIds = computed(() => store.adapters.filter((a) => a.chat ?? (a.id === 'claude' || a.id === 'codex')).map((a) => a.id))
const CHAT = { includes: (id: string) => chatIds.value.includes(id) }

type Owned = Pick<PilotSettings, 'exe' | 'model' | 'effort' | 'providers' | 'env' | 'preferSavedClaudeLogin' | 'custom'>
const blank = (): ProviderPrefs => ({ args: '', env: {}, models: [], disabled: false })
const draft = reactive<Owned>({ exe: {}, model: {}, effort: '', providers: { claude: blank(), codex: blank() }, env: {}, preferSavedClaudeLogin: false, custom: [] })
/** Own model ids as typed, one per line. */
const modelText = reactive<Record<string, string>>({})
/** Bumped on pull so the env editors start from the saved rows. */
const epoch = ref(0)

function prefs(id: string): ProviderPrefs {
  // Assign first, then read: `??=` would hand back the raw object, not the reactive one.
  if (!draft.providers[id]) draft.providers[id] = blank()
  return draft.providers[id]
}
function pull() {
  const s = store.settings
  if (!s) return
  draft.exe = { ...s.exe }
  draft.model = { ...s.model }
  draft.effort = s.effort
  draft.providers = JSON.parse(JSON.stringify(s.providers ?? {}))
  // The chat agents' rows edit these directly.
  for (const id of chatIds.value) draft.providers[id] ??= blank()
  draft.env = { ...(s.env ?? {}) }
  draft.preferSavedClaudeLogin = s.preferSavedClaudeLogin ?? false
  draft.custom = (s.custom ?? []).map((c) => ({ ...c }))
  for (const a of store.adapters) modelText[a.id] = (s.providers?.[a.id]?.models ?? []).join('\n')
  epoch.value++
}
onMounted(async () => {
  if (!store.settings) await store.loadSettings()
  pull()
  if (!store.status) void store.refreshStatus()
})
watch(() => store.settings, pull)
// Agents whose status arrives later get their rows too (before the next render).
watch(chatIds, (ids) => ids.forEach((id) => (draft.providers[id] ? undefined : (draft.providers[id] = blank()))), { immediate: true })

const adapters = computed(() => store.adapters.filter((a) => !a.custom))

const busy = ref(false)
const notice = ref('')
async function save() {
  busy.value = true
  notice.value = ''
  try {
    for (const a of store.adapters) {
      const models = (modelText[a.id] ?? '').split(/[\n,]/).map((m) => m.trim()).filter(Boolean)
      if (models.length || draft.providers[a.id]) prefs(a.id).models = models
    }
    await store.saveSettings({ ...draft, providers: JSON.parse(JSON.stringify(draft.providers)), custom: draft.custom.map((c) => ({ ...c })) })
    notice.value = 'Saved. New agent processes use it; running ones keep theirs until their next start.'
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

// ── Update ──
interface UpdateResult {
  plan: { method: string; display: string }
  ran: boolean
  ok: boolean
  before: string | null
  after: string | null
  output: string
}
const METHOD: Record<string, string> = { npm: 'npm global install', winget: 'winget', native: 'Claude native installer', cli: "the CLI's own updater" }
const update = reactive<Record<string, { busy: boolean; result: UpdateResult | null; error: string }>>({})
async function checkUpdate(id: string, run: boolean) {
  if (!update[id]) update[id] = { busy: false, result: null, error: '' }
  const u = update[id]
  u.busy = true
  u.error = ''
  try {
    u.result = (await pilot<UpdateResult>('plugin:pilot|pilot_provider_update', { id, dryRun: !run })) ?? null
    if (run) void store.refreshStatus()
  } catch (e) {
    u.error = e instanceof Error ? e.message : String(e)
  } finally {
    u.busy = false
  }
}

function addCustom() {
  draft.custom.push({ id: '', label: '', exe: '', args: '', resumeArgs: '', protocol: 'acp' } satisfies CustomAdapter)
}
</script>

<template>
  <div class="qp-settings" data-module="pilot">
    <p class="qp-settings-desc">
      The agents QuantPilot runs. Sign in once outside the suite where a CLI needs it: <code>claude auth login</code> · <code>codex login</code>.
      <button class="qp-link" :disabled="store.statusBusy" @click="store.refreshStatus()"><RefreshCw :size="11" /> {{ store.statusBusy ? 'Checking…' : 'Check again' }}</button>
    </p>

    <section v-for="a in adapters" :key="a.id" class="qp-prov" :class="{ off: draft.providers[a.id]?.disabled }">
      <header class="qp-prov-head">
        <span class="qp-dot" :class="{ ok: store.adapterReady(a.id), bad: !store.adapterReady(a.id) }" />
        <span class="qp-prov-name">{{ a.label }}</span>
        <span class="qp-prov-ver">{{ a.version || (a.found ? 'found' : 'not found') }}</span>
        <label v-if="CHAT.includes(a.id)" class="qp-check qp-prov-on">
          <input type="checkbox" :checked="!draft.providers[a.id]?.disabled" @change="prefs(a.id).disabled = !($event.target as HTMLInputElement).checked" /> Offered for threads
        </label>
        <button v-if="a.found && a.update" class="qp-btn qp-btn-sm" :disabled="update[a.id]?.busy" @click="checkUpdate(a.id, false)">Update…</button>
      </header>
      <div class="qp-prov-status">
        <span v-if="a.path" class="qp-prov-path" :title="a.path">{{ a.path }}</span>
        <span v-if="a.detail" :class="{ warn: a.loggedIn === false }">{{ a.detail }}</span>
      </div>

      <div v-if="update[a.id]?.result || update[a.id]?.error" class="qp-update" role="status">
        <template v-if="update[a.id]?.error">{{ update[a.id]?.error }}</template>
        <template v-else-if="update[a.id]?.result && !update[a.id]?.result?.ran">
          Update runs <code>{{ update[a.id]?.result?.plan.display }}</code> ({{ METHOD[update[a.id]?.result?.plan.method ?? ''] }}). Now: {{ update[a.id]?.result?.before || 'unknown' }}.
          <span class="qp-update-actions">
            <button class="qp-btn qp-btn-sm" @click="update[a.id]!.result = null">Cancel</button>
            <button class="qp-btn qp-btn-sm qp-btn-primary" :disabled="update[a.id]?.busy" @click="checkUpdate(a.id, true)">{{ update[a.id]?.busy ? 'Updating…' : 'Run update' }}</button>
          </span>
        </template>
        <template v-else-if="update[a.id]?.result">
          <template v-if="update[a.id]?.result?.ok">
            {{ update[a.id]?.result?.before === update[a.id]?.result?.after ? `Already the newest: ${update[a.id]?.result?.after}` : `Updated: ${update[a.id]?.result?.before} → ${update[a.id]?.result?.after}` }}
          </template>
          <template v-else>The update failed.</template>
          <pre v-if="update[a.id]?.result?.output && !update[a.id]?.result?.ok" class="qp-update-out">{{ update[a.id]?.result?.output }}</pre>
          <button class="qp-btn qp-btn-sm" @click="update[a.id]!.result = null">Close</button>
        </template>
      </div>

      <div class="qp-settings-grid">
        <label class="qp-settings-field">
          <span>Executable <em>(empty = the usual install or PATH)</em></span>
          <span class="qp-settings-inline">
            <input v-model="draft.exe[a.id]" class="qp-input" :placeholder="a.path || a.id" spellcheck="false" />
            <button class="qp-btn qp-btn-sm" @click="browse(a.id)">Browse</button>
          </span>
        </label>
        <label class="qp-settings-field">
          <span>Default model <em>(empty = the CLI's default)</em></span>
          <input v-model="draft.model[a.id]" class="qp-input" spellcheck="false" />
        </label>
        <template v-if="CHAT.includes(a.id)">
          <label v-if="a.id === 'claude'" class="qp-settings-field">
            <span>Effort</span>
            <select v-model="draft.effort" class="qp-input">
              <option value="">default</option>
              <option v-for="e in ['low', 'medium', 'high', 'xhigh', 'max']" :key="e" :value="e">{{ e }}</option>
            </select>
          </label>
          <label class="qp-settings-field">
            <span>Extra arguments <em>({{ a.id === 'codex' ? 'before app-server, e.g. -c key=value' : 'after QuantPilot\'s own' }})</em></span>
            <input v-model="draft.providers[a.id]!.args" class="qp-input" spellcheck="false" />
          </label>
          <label class="qp-settings-field">
            <span>Your models <em>(one id per line; they join the picker)</em></span>
            <textarea v-model="modelText[a.id]" class="qp-input" rows="2" spellcheck="false" />
          </label>
        </template>
      </div>
      <template v-if="CHAT.includes(a.id)">
        <div class="qp-settings-field">
          <span>Environment variables</span>
          <PilotSettingsEnvEditor :key="`${a.id}-${epoch}`" :model-value="draft.providers[a.id]!.env" @update:model-value="(v: Record<string, string>) => (draft.providers[a.id]!.env = v)" />
        </div>
        <label v-if="a.id === 'claude'" class="qp-check">
          <input v-model="draft.preferSavedClaudeLogin" type="checkbox" />
          <span>Prefer the saved Claude login <em>— new Claude processes ignore inherited ANTHROPIC_API_KEY, ANTHROPIC_AUTH_TOKEN, CLAUDE_CODE_OAUTH_TOKEN, custom endpoints and model overrides (variables set above still apply)</em></span>
        </label>
      </template>
    </section>

    <h3 class="qp-settings-title">Every agent's environment</h3>
    <p class="qp-settings-desc">Variables for every agent process; an agent's own win.</p>
    <PilotSettingsEnvEditor :key="`global-${epoch}`" :model-value="draft.env" @update:model-value="(v: Record<string, string>) => (draft.env = v)" />

    <h3 class="qp-settings-title">Your own agents</h3>
    <p class="qp-settings-desc">
      An agent that speaks the Agent Client Protocol (ACP) runs chat threads like the built-ins: the command and the arguments that start it in
      ACP mode (e.g. <code>my-agent --acp</code>); its environment is set above once it is saved. A terminal agent (from QuantPilot V2) runs in a
      terminal session; placeholders in its arguments: <code>{cwd}</code> <code>{session}</code> <code>{title}</code> <code>{mode}</code>
      <code>{model}</code> <code>{vault}</code>.
    </p>
    <div v-for="(c, i) in draft.custom" :key="i" class="qp-prov">
      <div class="qp-settings-grid">
        <label class="qp-settings-field"><span>Id</span><input v-model="c.id" class="qp-input" placeholder="my-agent" spellcheck="false" /></label>
        <label class="qp-settings-field"><span>Name</span><input v-model="c.label" class="qp-input" placeholder="My Agent" spellcheck="false" /></label>
        <label class="qp-settings-field">
          <span>Speaks</span>
          <select v-model="c.protocol" class="qp-input">
            <option value="acp">ACP — chat threads</option>
            <option value="terminal">Terminal — a terminal session (V2)</option>
          </select>
        </label>
        <label class="qp-settings-field"><span>Executable</span><input v-model="c.exe" class="qp-input" placeholder="my-agent or C:\path\to\my-agent.exe" spellcheck="false" /></label>
        <label class="qp-settings-field"><span>Arguments</span><input v-model="c.args" class="qp-input" placeholder="--cwd {cwd}" spellcheck="false" /></label>
        <label v-if="c.protocol !== 'acp'" class="qp-settings-field"><span>Arguments when reopened <em>(empty = the same)</em></span><input v-model="c.resumeArgs" class="qp-input" placeholder="--resume {session}" spellcheck="false" /></label>
      </div>
      <button class="qp-btn qp-btn-sm qp-btn-danger" @click="draft.custom.splice(i, 1)">Remove</button>
    </div>
    <div><button class="qp-btn qp-btn-sm" @click="addCustom">Add an agent</button></div>

    <div class="qp-settings-actions">
      <button class="qp-btn qp-btn-primary" :disabled="busy" @click="save">{{ busy ? 'Saving…' : 'Save' }}</button>
      <span v-if="notice" class="qp-settings-notice">{{ notice }}</span>
    </div>
  </div>
</template>

<style scoped>
.qp-settings { display: flex; flex-direction: column; gap: 10px; font-size: 12.5px; color: var(--qss-text, #d4d4d8); }
.qp-settings-title { margin: 6px 0 0; font-size: 13px; font-weight: 600; }
.qp-settings-desc { margin: 0; color: var(--qss-text-secondary, #9a9aa5); line-height: 1.5; font-size: 12px; }
.qp-settings-desc code,
.qp-update code { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11.5px; background: var(--qss-bg-card, #292930); padding: 1px 5px; border-radius: 4px; }
.qp-link { display: inline-flex; align-items: center; gap: 4px; margin-left: 6px; padding: 1px 6px; border: none; border-radius: 5px; background: transparent; color: var(--qss-text-secondary, #9a9aa5); font: inherit; font-size: 11.5px; cursor: pointer; }
.qp-link:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-prov { display: flex; flex-direction: column; gap: 8px; padding: 10px 12px; border: 1px solid var(--qss-border-subtle, #35353d); border-radius: 8px; background: var(--qss-bg-raised, #1f1f25); align-items: stretch; }
.qp-prov.off { opacity: 0.7; }
.qp-prov-head { display: flex; align-items: center; gap: 8px; }
.qp-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--qss-border, #47474f); flex-shrink: 0; }
.qp-dot.ok { background: var(--qss-success, #34d399); }
.qp-dot.bad { background: var(--qss-error, #f87171); }
.qp-prov-name { font-weight: 600; }
.qp-prov-ver { font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-prov-on { margin-left: auto; font-size: 12px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-prov-status { display: flex; flex-wrap: wrap; gap: 4px 12px; font-size: 11.5px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-prov-status .warn { color: var(--qss-warning, #d29a3f); }
.qp-prov-path { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; color: var(--qss-text-muted, #6e6e7a); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 100%; }
.qp-update { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 10px; padding: 7px 9px; border-radius: 6px; border: 1px solid var(--qss-border, #47474f); font-size: 12px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-update-actions { display: inline-flex; gap: 6px; margin-left: auto; }
.qp-update-out { width: 100%; max-height: 140px; overflow: auto; margin: 0; font-size: 11px; white-space: pre-wrap; }
.qp-settings-field { display: flex; flex-direction: column; gap: 4px; }
.qp-settings-field > span { font-size: 11.5px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-settings-field em, .qp-check em { font-style: normal; color: var(--qss-text-muted, #6e6e7a); }
.qp-settings-inline { display: flex; gap: 6px; }
.qp-settings-inline .qp-input { flex: 1; min-width: 0; }
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
  resize: vertical;
}
.qp-input:focus { border-color: var(--qss-accent, #a0a0a8); }
.qp-input::placeholder { color: var(--qss-text-muted, #6e6e7a); }
.qp-check { display: flex; align-items: flex-start; gap: 8px; line-height: 1.4; }
.qp-settings-actions { display: flex; align-items: center; gap: 10px; margin-top: 4px; }
.qp-settings-notice { color: var(--qss-text-secondary, #9a9aa5); }
</style>
