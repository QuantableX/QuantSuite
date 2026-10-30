<script setup lang="ts">
/**
 * One project's settings (card 19; T3 Code's project settings): each row
 * shows the value the project's new threads get — inherited from the
 * defaults (a muted layers mark) or set here (an accent mark and Reset).
 * Changing a row overrides it; Reset inherits again.
 */
import { computed, reactive, ref, watch } from 'vue'
import { Layers } from 'lucide-vue-next'
import { usePilotStore } from '#pilot/stores/pilot'
import type { Mode, ProjectPrefs } from '#pilot/types'

const props = defineProps<{ project: string }>()
const store = usePilotStore()

const MODES: { id: Mode; label: string }[] = [
  { id: 'full', label: 'Full access' },
  { id: 'auto', label: 'Auto' },
  { id: 'edits', label: 'Accept edits' },
  { id: 'plan', label: 'Plan' },
  { id: 'ask', label: 'Supervised' },
]
const AGENTS = computed(() => store.adapters.filter((a) => a.id === 'claude' || a.id === 'codex'))
const label = (id: string) => store.adapterLabel(id)

const draft = reactive<Required<ProjectPrefs>>({ provider: null, model: {}, mode: null, workspace: null, enabled: null })
function pull() {
  const p = store.settings?.projects?.[props.project] ?? {}
  draft.provider = p.provider ?? null
  draft.model = { ...(p.model ?? {}) }
  draft.mode = p.mode ?? null
  draft.workspace = p.workspace ?? null
  draft.enabled = p.enabled ? [...p.enabled] : null
}
watch(() => [props.project, store.settings], pull, { immediate: true })

// What the defaults give — the inherited values.
const s = computed(() => store.settings)
const inheritedEnabled = computed(() => AGENTS.value.map((a) => a.id).filter((id) => !s.value?.providers?.[id]?.disabled))
const enabled = computed(() => draft.enabled ?? inheritedEnabled.value)
const inherited = computed(() => ({
  provider: s.value?.defaultProvider ?? 'claude',
  mode: s.value?.defaultMode ?? 'full',
  workspace: s.value?.newThreadWorkspace ?? 'local',
}))

function toggleAgent(id: string, on: boolean) {
  const list = new Set(enabled.value)
  if (on) list.add(id)
  else list.delete(id)
  draft.enabled = AGENTS.value.map((a) => a.id).filter((x) => list.has(x))
}

const busy = ref(false)
const notice = ref('')
async function save() {
  busy.value = true
  notice.value = ''
  try {
    const model = Object.fromEntries(Object.entries(draft.model).filter(([, v]) => v?.trim()))
    const entry: ProjectPrefs = { provider: draft.provider, model, mode: draft.mode, workspace: draft.workspace, enabled: draft.enabled }
    await store.saveSettings({ projects: { ...(s.value?.projects ?? {}), [props.project]: entry } })
    notice.value = 'Saved for this project.'
  } catch (e) {
    notice.value = e instanceof Error ? e.message : String(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="qp-scope">
    <p class="qp-scope-desc">
      New threads in this project start with these. <Layers :size="12" class="qp-mark" /> inherited from Every project ·
      <Layers :size="12" class="qp-mark on" /> set here. The other settings apply to every project.
    </p>

    <div class="qp-row">
      <span class="qp-row-k"><Layers :size="13" class="qp-mark" :class="{ on: draft.provider !== null }" /> Agent</span>
      <select class="qp-input" :value="draft.provider ?? ''" @change="draft.provider = ($event.target as HTMLSelectElement).value || null">
        <option value="">Inherit — {{ label(inherited.provider) }}</option>
        <option v-for="a in AGENTS" :key="a.id" :value="a.id">{{ a.label }}</option>
      </select>
      <button class="qp-reset" :disabled="draft.provider === null" @click="draft.provider = null">Reset</button>
    </div>

    <div v-for="a in AGENTS" :key="a.id" class="qp-row">
      <span class="qp-row-k"><Layers :size="13" class="qp-mark" :class="{ on: !!draft.model[a.id] }" /> {{ a.label }} model</span>
      <input v-model="draft.model[a.id]" class="qp-input" spellcheck="false" :placeholder="`Inherit — ${s?.model?.[a.id] || 'the CLI\'s default'}`" />
      <button class="qp-reset" :disabled="!draft.model[a.id]" @click="draft.model[a.id] = ''">Reset</button>
    </div>

    <div class="qp-row">
      <span class="qp-row-k"><Layers :size="13" class="qp-mark" :class="{ on: draft.mode !== null }" /> Permissions</span>
      <select class="qp-input" :value="draft.mode ?? ''" @change="draft.mode = (($event.target as HTMLSelectElement).value || null) as Mode | null">
        <option value="">Inherit — {{ MODES.find((m) => m.id === inherited.mode)?.label }}</option>
        <option v-for="m in MODES" :key="m.id" :value="m.id">{{ m.label }}</option>
      </select>
      <button class="qp-reset" :disabled="draft.mode === null" @click="draft.mode = null">Reset</button>
    </div>

    <div class="qp-row">
      <span class="qp-row-k"><Layers :size="13" class="qp-mark" :class="{ on: draft.workspace !== null }" /> Work in</span>
      <select class="qp-input" :value="draft.workspace ?? ''" @change="draft.workspace = (($event.target as HTMLSelectElement).value || null) as 'local' | 'worktree' | null">
        <option value="">Inherit — {{ inherited.workspace === 'worktree' ? 'Worktree' : 'Local' }}</option>
        <option value="local">Local</option>
        <option value="worktree">Worktree</option>
      </select>
      <button class="qp-reset" :disabled="draft.workspace === null" @click="draft.workspace = null">Reset</button>
    </div>

    <div class="qp-row">
      <span class="qp-row-k"><Layers :size="13" class="qp-mark" :class="{ on: draft.enabled !== null }" /> Agents offered</span>
      <span class="qp-agents">
        <label v-for="a in AGENTS" :key="a.id" class="qp-check"><input type="checkbox" :checked="enabled.includes(a.id)" @change="toggleAgent(a.id, ($event.target as HTMLInputElement).checked)" /> {{ a.label }}</label>
      </span>
      <button class="qp-reset" :disabled="draft.enabled === null" @click="draft.enabled = null">Reset</button>
    </div>

    <p class="qp-scope-desc">This project's actions are under Project actions.</p>
    <div class="qp-actions">
      <button class="qp-btn qp-btn-primary" :disabled="busy" @click="save">{{ busy ? 'Saving…' : 'Save' }}</button>
      <span v-if="notice" class="qp-notice">{{ notice }}</span>
    </div>
  </div>
</template>

<style scoped>
.qp-scope { display: flex; flex-direction: column; gap: 8px; font-size: 12.5px; }
.qp-scope-desc { margin: 0; display: flex; align-items: center; flex-wrap: wrap; gap: 4px; font-size: 12px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-row { display: grid; grid-template-columns: 170px minmax(0, 1fr) auto; align-items: center; gap: 10px; }
.qp-row-k { display: inline-flex; align-items: center; gap: 6px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-mark { color: var(--qss-text-muted, #6e6e7a); flex-shrink: 0; }
.qp-mark.on { color: var(--qss-accent, #7aa2f7); }
.qp-input {
  background: var(--qss-bg, #18181e);
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 12.5px;
  padding: 6px 9px;
  outline: none;
  min-width: 0;
}
.qp-input:focus { border-color: var(--qss-accent, #a0a0a8); }
.qp-input::placeholder { color: var(--qss-text-muted, #6e6e7a); }
.qp-reset { padding: 3px 9px; border: 1px solid var(--qss-border-subtle, #35353d); border-radius: 6px; background: transparent; color: var(--qss-text-secondary, #9a9aa5); font: inherit; font-size: 11.5px; cursor: pointer; }
.qp-reset:hover:not(:disabled) { background: var(--qss-bg-card, #292930); color: var(--qss-text, #d4d4d8); }
.qp-reset:disabled { opacity: 0.35; cursor: default; }
.qp-agents { display: flex; gap: 14px; }
.qp-check { display: inline-flex; align-items: center; gap: 6px; }
.qp-actions { display: flex; align-items: center; gap: 10px; margin-top: 4px; }
.qp-notice { color: var(--qss-text-secondary, #9a9aa5); }
</style>
