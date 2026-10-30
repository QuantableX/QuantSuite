<script setup lang="ts">
/**
 * Settings → General (card 19): what a new thread starts with, the
 * composer, notifications, threads and the suite as context. "Settings for"
 * picks a project: its rows show what it inherits and what it overrides
 * (ProjectScope.vue); everything else applies to every project. Agents have
 * their own section (Providers), as do source control, appearance and the
 * advanced corner.
 */
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { qs, type ConsoleShell } from '@quantsuite/core'
import { usePilotStore } from '#pilot/stores/pilot'
import type { PilotSettings } from '#pilot/types'

const store = usePilotStore()

/** The fields this section owns; saving sends only these. */
type Owned = Pick<
  PilotSettings,
  | 'shell'
  | 'defaultProvider'
  | 'defaultMode'
  | 'sendShortcut'
  | 'followUp'
  | 'notify'
  | 'notifySound'
  | 'autoSettleDays'
  | 'workingSection'
  | 'newThreadWorkspace'
  | 'worktreePrefix'
  | 'limitResume'
  | 'limitSnooze'
  | 'attachQuantmcp'
  | 'includeGeneralVault'
>
const draft = reactive<Owned>({
  shell: '',
  defaultProvider: 'claude',
  defaultMode: 'full',
  sendShortcut: 'enter',
  followUp: 'queue',
  notify: 'unfocused',
  notifySound: false,
  autoSettleDays: 3,
  workingSection: true,
  newThreadWorkspace: 'local',
  worktreePrefix: 'quantpilot/',
  limitResume: false,
  limitSnooze: false,
  attachQuantmcp: true,
  includeGeneralVault: true,
})
const notice = ref('')
const busy = ref(false)
const shells = ref<ConsoleShell[]>([])
/** `''` = every project; a context id = that project's overrides. */
const scope = ref('')

function pull() {
  const s = store.settings
  if (!s) return
  for (const k of Object.keys(draft) as (keyof Owned)[]) {
    const v = s[k]
    if (v !== undefined) (draft as Record<string, unknown>)[k] = v
  }
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

const projects = computed(() => store.contexts.filter((c) => c.kind !== 'general'))
const agents = computed(() => store.adapters.filter((a) => a.chat ?? (a.id === 'claude' || a.id === 'codex')))

async function save() {
  busy.value = true
  notice.value = ''
  try {
    await store.saveSettings({ ...draft, autoSettleDays: Math.max(0, Math.min(365, Math.round(Number(draft.autoSettleDays) || 0))) })
    notice.value = 'Saved.'
  } catch (e) {
    notice.value = e instanceof Error ? e.message : String(e)
  } finally {
    busy.value = false
  }
}
const status = computed(() => store.status)
</script>

<template>
  <div class="qp-settings" data-module="pilot">
    <label class="qp-settings-scope">
      <span>Settings for</span>
      <select v-model="scope" class="qp-input">
        <option value="">Every project</option>
        <option v-for="p in projects" :key="p.id" :value="p.id">{{ p.name }}</option>
      </select>
    </label>

    <PilotSettingsProjectScope v-if="scope" :project="scope" />

    <template v-else>
      <h3 class="qp-settings-title">New threads</h3>
      <div class="qp-settings-grid">
        <label class="qp-settings-field">
          <span>Agent</span>
          <select v-model="draft.defaultProvider" class="qp-input">
            <option v-for="a in agents" :key="a.id" :value="a.id">{{ a.label }}</option>
          </select>
        </label>
        <label class="qp-settings-field">
          <span>Permissions</span>
          <select v-model="draft.defaultMode" class="qp-input">
            <option value="full">Full access</option>
            <option value="auto">Auto (the agent's own reviewer)</option>
            <option value="edits">Accept edits</option>
            <option value="plan">Plan (read and plan only)</option>
            <option value="ask">Supervised (ask before tools)</option>
          </select>
        </label>
        <label class="qp-settings-field">
          <span>Work in <em>(git projects; Ctrl+Shift+X switches before the first message)</em></span>
          <select v-model="draft.newThreadWorkspace" class="qp-input">
            <option value="local">Local — the project's checkout</option>
            <option value="worktree">Worktree — a checkout of their own</option>
          </select>
        </label>
        <label class="qp-settings-field">
          <span>Worktree branch prefix</span>
          <input v-model="draft.worktreePrefix" class="qp-input" spellcheck="false" placeholder="quantpilot/" />
        </label>
        <label class="qp-settings-field">
          <span>Terminal shell</span>
          <select v-model="draft.shell" class="qp-input">
            <option value="">default{{ status?.shell ? ` (${status.shell})` : '' }}</option>
            <option v-for="sh in shells" :key="sh.path" :value="sh.path">{{ sh.label }}</option>
          </select>
        </label>
      </div>
      <p class="qp-settings-desc">Models, effort and each agent's own options are under Providers; a project can change any of the first three rows.</p>

      <h3 class="qp-settings-title">Composer</h3>
      <div class="qp-settings-grid">
        <label class="qp-settings-field">
          <span>Send with</span>
          <select v-model="draft.sendShortcut" class="qp-input">
            <option value="enter">Enter (Shift+Enter for a new line)</option>
            <option value="modEnterMultiline">Ctrl+Enter once a message has several lines</option>
            <option value="modEnter">Always Ctrl+Enter</option>
          </select>
        </label>
        <label class="qp-settings-field">
          <span>While the agent works <em>(Ctrl+Enter does the other)</em></span>
          <select v-model="draft.followUp" class="qp-input">
            <option value="queue">Queue — send after the turn</option>
            <option value="steer">Steer — into the running turn</option>
          </select>
        </label>
      </div>

      <h3 class="qp-settings-title">When a thread needs you</h3>
      <div class="qp-settings-grid">
        <label class="qp-settings-field">
          <span>Windows notification <em>(click it to open the thread)</em></span>
          <select v-model="draft.notify" class="qp-input">
            <option value="unfocused">Only while QuantSuite is in the background</option>
            <option value="always">Always</option>
            <option value="off">Off</option>
          </select>
        </label>
      </div>
      <label class="qp-settings-check">
        <input v-model="draft.notifySound" type="checkbox" />
        <span>Play a short sound</span>
      </label>

      <h3 class="qp-settings-title">Threads</h3>
      <div class="qp-settings-grid">
        <label class="qp-settings-field">
          <span>Settle inactive threads after <em>(days; 0 = never)</em></span>
          <input v-model.number="draft.autoSettleDays" class="qp-input" type="number" min="0" max="365" step="1" />
        </label>
      </div>
      <label class="qp-settings-check">
        <input v-model="draft.limitResume" type="checkbox" />
        <span>Resume after usage limits <em>(a limit stop continues by itself when the allowance returns)</em></span>
      </label>
      <label class="qp-settings-check">
        <input v-model="draft.limitSnooze" type="checkbox" />
        <span>Snooze until limits reset <em>(a limited thread leaves the list until then)</em></span>
      </label>
      <label class="qp-settings-check">
        <input v-model="draft.workingSection" type="checkbox" />
        <span>Fold working threads into a Working section <em>(they come back on top when they finish, fail or need you)</em></span>
      </label>

      <h3 class="qp-settings-title">The suite as context</h3>
      <label class="qp-settings-check">
        <input v-model="draft.attachQuantmcp" type="checkbox" />
        <span>Attach QuantMCP at launch where the CLI takes it — Claude Code and Codex <em>({{ status?.mcpUrl || 'the suite\'s MCP server' }}); pi, omp, OpenCode and Gemini use their own MCP config</em></span>
      </label>
      <label class="qp-settings-check">
        <input v-model="draft.includeGeneralVault" type="checkbox" />
        <span>Workspace sessions can also read the General memory vault <em>({{ status?.generalPath || '…' }})</em></span>
      </label>

      <div class="qp-settings-actions">
        <button class="qp-btn qp-btn-primary" :disabled="busy" @click="save">{{ busy ? 'Saving…' : 'Save' }}</button>
        <span v-if="notice" class="qp-settings-notice">{{ notice }}</span>
      </div>
    </template>
  </div>
</template>

<style scoped>
.qp-settings { display: flex; flex-direction: column; gap: 10px; font-size: 12.5px; color: var(--qss-text, #d4d4d8); }
.qp-settings-scope { display: flex; align-items: center; gap: 10px; padding-bottom: 10px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); }
.qp-settings-scope > span { font-size: 12px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-settings-scope .qp-input { min-width: 220px; }
.qp-settings-title { margin: 6px 0 0; font-size: 13px; font-weight: 600; }
.qp-settings-desc { margin: 0; color: var(--qss-text-secondary, #9a9aa5); line-height: 1.5; font-size: 12px; }
.qp-settings-field { display: flex; flex-direction: column; gap: 4px; }
.qp-settings-field > span { font-size: 11.5px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-settings-field em { font-style: normal; color: var(--qss-text-muted, #6e6e7a); }
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
.qp-input::placeholder { color: var(--qss-text-muted, #6e6e7a); }
.qp-settings-check { display: flex; align-items: flex-start; gap: 8px; line-height: 1.4; }
.qp-settings-check em { font-style: normal; color: var(--qss-text-muted, #6e6e7a); }
.qp-settings-actions { display: flex; align-items: center; gap: 10px; margin-top: 4px; }
.qp-settings-notice { color: var(--qss-text-secondary, #9a9aa5); }
</style>
