<script setup lang="ts">
import type { AgentRecipient, CustomRecipient } from '../types/agentRecipients'

const props = defineProps<{
  mode?: 'instructions' | 'mcp'
  modelValue: string[]
  agents: AgentRecipient[]
  scanning: boolean
  busy: boolean
  error: string
  canImport: boolean
  importLabel: string
  saveCustom?: (recipient: CustomRecipient) => Promise<void>
  removeCustom?: (id: string) => Promise<void>
}>()
const emit = defineEmits<{
  'update:modelValue': [ids: string[]]
  refresh: []
  import: []
}>()
const dialog = ref<HTMLDialogElement | null>(null)
const detailPanel = ref<HTMLElement | null>(null)
const query = ref('')
const filter = ref<'available' | 'all' | 'custom' | 'manual'>('available')
const mcpMode = computed(() => props.mode === 'mcp')
const manualSetup = ref(false)
const titleId = computed(() => mcpMode.value ? 'mcp-agent-picker-title' : 'agent-picker-title')
const activeId = ref('')
const editing = ref(false)
const editId = ref('')
const customName = ref('')
const customPath = ref('')
const customError = ref('')
const writing = ref(false)
const customSupported = computed(() => !mcpMode.value && !!props.saveCustom && !!props.removeCustom && props.agents.length > 0 && props.agents.every(agent => typeof agent.custom === 'boolean'))
const locked = computed(() => props.busy || props.scanning || writing.value)
const detected = computed(() => props.agents.filter(agent => agent.installed && !agent.custom))
const customs = computed(() => props.agents.filter(agent => agent.custom))
const selected = computed(() => props.agents.filter(agent => props.modelValue.includes(agent.id)))
const automatic = computed(() => props.agents.filter(agent => agent.installed && !isManual(agent)))
const visible = computed(() => props.agents
  .filter(agent => filter.value === 'all' || (filter.value === 'custom' ? agent.custom : filter.value === 'manual' ? isManual(agent) : agent.installed))
  .filter(agent => `${agent.name} ${agent.id}`.toLowerCase().includes(query.value.toLowerCase().trim()))
  .sort((a, b) => Number(b.installed) - Number(a.installed) || a.name.localeCompare(b.name)))
const active = computed(() => props.agents.find(agent => agent.id === activeId.value))
const detailPaths = computed(() => mcpMode.value ? (active.value?.config_path ? [active.value.config_path] : []) : active.value?.instructions_paths ?? [])

function isManual(agent: AgentRecipient) {
  return mcpMode.value ? agent.manual !== false : agent.instructions_manual
}
function showDetails(id: string) {
  activeId.value = id
  editing.value = false
  manualSetup.value = false
}
function description(agent: AgentRecipient) {
  if (mcpMode.value) return agent.note || 'Adds QuantMCP to this agent’s MCP configuration. Existing settings and other servers are kept.'
  return agent.instructions_note || (agent.instructions_manual ? 'Follow the setup steps after importing.' : 'Receives your General AgentOS instructions automatically. Existing instructions are kept.')
}

function initials(agent: AgentRecipient) {
  return ({ pi: 'π', omp: 'OMP', 'codex-cli': 'CX', 'claude-code': 'CL', 'claude-desktop': 'CL', 'kilo-cli': 'K', 'kilo-code': 'K', opencode: 'OC' } as Record<string, string>)[agent.id]
    ?? agent.name.split(/\s+/).slice(0, 2).map(word => word[0]).join('').toUpperCase()
}
function status(agent: AgentRecipient) {
  if (mcpMode.value && isManual(agent)) return 'Manual setup'
  if (!agent.installed) return agent.custom ? 'File unavailable' : 'Not detected'
  if (mcpMode.value) return agent.configured ? 'Already configured' : 'Ready to connect'
  return agent.instructions_manual ? 'Manual setup' : agent.custom ? 'Custom file' : 'Ready to import'
}
function openPicker() {
  if (props.busy) return
  editing.value = false
  manualSetup.value = false
  customError.value = ''
  query.value = ''
  filter.value = 'available'
  activeId.value = selected.value[0]?.id ?? detected.value[0]?.id ?? props.agents[0]?.id ?? ''
  dialog.value?.showModal()
}
function closePicker() {
  if (!writing.value) dialog.value?.close()
}
function toggle(agent: AgentRecipient) {
  activeId.value = agent.id
  if (locked.value || !agent.installed) return
  const ids = props.modelValue.includes(agent.id)
    ? props.modelValue.filter(id => id !== agent.id)
    : [...props.modelValue, agent.id]
  emit('update:modelValue', ids)
}
function editCustom(agent?: AgentRecipient) {
  if (!customSupported.value) return
  editing.value = true
  editId.value = agent?.id ?? `custom:${crypto.randomUUID()}`
  customName.value = agent?.name ?? ''
  customPath.value = agent?.instructions_paths?.[0] ?? ''
  customError.value = ''
}
async function browse() {
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const path = await open({ title: 'Choose an instruction file', multiple: false, filters: [{ name: 'Markdown instructions', extensions: ['md'] }] })
    if (typeof path === 'string') customPath.value = path
  } catch (error) { customError.value = String(error) }
}
async function saveCustom() {
  if (!props.saveCustom) return
  writing.value = true
  customError.value = ''
  try {
    await props.saveCustom({ id: editId.value, name: customName.value, path: customPath.value })
    activeId.value = editId.value
    filter.value = 'custom'
    query.value = ''
    editing.value = false
  } catch (error) { customError.value = String(error) }
  finally { writing.value = false }
}
async function removeCustom() {
  if (!active.value?.custom || !props.removeCustom) return
  writing.value = true
  customError.value = ''
  try {
    await props.removeCustom(active.value.id)
    activeId.value = visible.value[0]?.id ?? ''
  } catch (error) { customError.value = String(error) }
  finally { writing.value = false }
}
watch(visible, rows => {
  if (!rows.some(agent => agent.id === activeId.value)) activeId.value = rows[0]?.id ?? ''
})
watch([activeId, editing, manualSetup], async () => {
  await nextTick()
  detailPanel.value?.scrollTo(0, 0)
})
onBeforeUnmount(() => dialog.value?.close())
</script>

<template>
  <section class="recipient-summary" :aria-label="mcpMode ? 'MCP agents' : 'AgentOS recipients'">
    <div class="summary-mark" aria-hidden="true">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M8 4H5a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-4M12 3v10m-4-4 4 4 4-4" /></svg>
    </div>
    <div class="summary-copy">
      <div class="summary-title">{{ mcpMode ? 'MCP agents' : 'Agent recipients' }} <span class="summary-count">{{ selected.length ? `${selected.length} selected` : scanning ? 'Detecting…' : `${detected.length} detected` }}</span></div>
      <p>{{ selected.length ? selected.map(agent => agent.name).join(', ') : mcpMode ? 'Choose which agents should use QuantMCP.' : 'Choose where your shared instructions should go.' }}</p>
    </div>
    <div class="summary-actions">
      <button class="picker-button" :disabled="busy" @click="openPicker">Choose agents</button>
      <button class="picker-button primary" :disabled="!canImport || writing" @click="emit('import')">{{ importLabel }}</button>
    </div>
  </section>
  <p v-if="error" class="picker-error" role="alert">{{ error }}</p>

  <Teleport to="body">
    <dialog ref="dialog" class="agent-dialog" data-module="mcp" :aria-labelledby="titleId" @cancel.prevent="closePicker" @click="event => { if (event.target === dialog) closePicker() }">
      <div class="picker-shell">
        <header class="picker-header">
          <div><span class="picker-eyebrow">{{ mcpMode ? 'CONNECT QUANTMCP' : 'GENERAL AGENTOS' }}</span><h2 :id="titleId">Choose your agents</h2><p>{{ mcpMode ? 'Enable QuantMCP in the agents you choose.' : 'Send your shared instructions to the agents you use.' }}</p></div>
          <button class="icon-button" aria-label="Close agent picker" :disabled="writing" @click="closePicker">×</button>
        </header>
        <div class="picker-toolbar">
          <label class="picker-search"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><circle cx="10.5" cy="10.5" r="6.5" /><path d="m16 16 4 4" /></svg><input v-model="query" type="search" placeholder="Search agents…" aria-label="Search agents"></label>
          <button class="picker-button" :disabled="locked" @click="emit('refresh')">{{ scanning ? 'Scanning…' : 'Rescan' }}</button>
          <button v-if="mcpMode" class="picker-button" @click="manualSetup = true">Manual setup</button>
          <button v-else class="picker-button" :disabled="locked || !customSupported" :title="customSupported ? 'Add an agent by its instruction file' : 'Reload the updated backend to add custom agents'" @click="editCustom()">+ Add custom</button>
        </div>
        <div class="picker-filters" aria-label="Filter agents">
          <button :class="{ active: filter === 'available' }" :aria-pressed="filter === 'available'" @click="filter = 'available'">Available <span>{{ agents.filter(agent => agent.installed).length }}</span></button>
          <button :class="{ active: filter === 'all' }" :aria-pressed="filter === 'all'" @click="filter = 'all'">All agents <span>{{ agents.length }}</span></button>
          <button v-if="mcpMode" :class="{ active: filter === 'manual' }" :aria-pressed="filter === 'manual'" @click="filter = 'manual'; manualSetup = false">Manual <span>{{ agents.filter(isManual).length }}</span></button>
          <button v-else :class="{ active: filter === 'custom' }" :aria-pressed="filter === 'custom'" @click="filter = 'custom'">Custom <span>{{ customs.length }}</span></button>
        </div>
        <p v-if="error" class="picker-error picker-inline-error" role="alert">{{ error }}</p>
        <div class="picker-content">
          <div class="agent-list" aria-label="Agent recipients">
            <div v-for="agent in visible" :key="agent.id" class="agent-row" :class="{ chosen: modelValue.includes(agent.id), focused: activeId === agent.id, unavailable: !agent.installed }">
              <label @click="showDetails(agent.id)" @focusin="showDetails(agent.id)">
                <input type="checkbox" :checked="modelValue.includes(agent.id)" :disabled="locked || !agent.installed" :aria-label="agent.name" @change="toggle(agent)">
                <span class="agent-avatar" :class="{ pi: agent.id === 'pi', omp: agent.id === 'omp' }" aria-hidden="true">{{ initials(agent) }}</span>
                <span class="agent-name"><strong>{{ agent.name }}</strong><span>{{ status(agent) }}</span></span>
              </label>
              <button class="row-details" :aria-label="`Details for ${agent.name}`" :aria-pressed="activeId === agent.id && !manualSetup" @click="showDetails(agent.id)">›</button>
            </div>
            <div v-if="!visible.length" class="picker-empty"><strong>{{ query ? 'No matching agents' : filter === 'custom' ? 'Make room for your agent' : 'No agents found' }}</strong><p>{{ mcpMode ? 'Try another name, rescan, or use manual setup for any MCP client.' : query ? 'Try another name or add a custom agent.' : 'Rescan your machine or add an instruction file yourself.' }}</p></div>
          </div>
          <aside ref="detailPanel" class="agent-detail" aria-label="Agent details">
            <template v-if="mcpMode && manualSetup"><span class="picker-eyebrow">ANY MCP CLIENT</span><h3 class="manual-heading">Manual setup</h3><slot name="manual-setup" /></template>
            <form v-else-if="editing" class="custom-form" @submit.prevent="saveCustom">
              <span class="picker-eyebrow">CUSTOM AGENT</span><h3>{{ agents.some(agent => agent.id === editId) ? 'Edit agent' : 'Add your agent' }}</h3>
              <p>Choose a Markdown file your agent loads as instructions.</p>
              <label>Agent name<input v-model="customName" maxlength="80" placeholder="e.g. My coding agent" required :disabled="writing"></label>
              <label>Instruction file<textarea v-model="customPath" rows="3" placeholder="Absolute path to AGENTS.md" required :disabled="writing" /></label>
              <button type="button" class="picker-button browse-button" :disabled="writing" @click="browse">Browse files…</button>
              <p class="form-hint">You can also enter a new .md file in an existing folder. Existing instructions are preserved.</p>
              <p v-if="customError" class="picker-error" role="alert">{{ customError }}</p>
              <div class="custom-actions"><button type="button" class="picker-button" :disabled="writing" @click="editing = false; customError = ''">Cancel</button><button class="picker-button primary" :disabled="writing || !customName.trim() || !customPath.trim()">{{ writing ? 'Saving…' : 'Save agent' }}</button></div>
            </form>
            <template v-else-if="active">
              <span class="agent-avatar detail-avatar" :class="{ pi: active.id === 'pi', omp: active.id === 'omp' }" aria-hidden="true">{{ initials(active) }}</span>
              <h3>{{ active.name }}</h3>
              <span class="detail-status" :class="{ ready: active.installed && !isManual(active) }"><i />{{ status(active) }}</span>
              <p class="detail-description">{{ description(active) }}</p>
              <div v-if="detailPaths.length" class="detail-section"><h4>{{ mcpMode || active.id.startsWith('kilo') ? 'Configuration file' : 'Instruction file' }}</h4><code v-for="path in detailPaths" :key="path">{{ path }}</code></div>
              <div class="detail-section"><h4>{{ active.custom ? 'Custom target' : 'Detection' }}</h4><p>{{ active.detection || (mcpMode ? 'No local installation found. Rescan after installing, or use manual setup.' : 'No installation found. Rescan after installing, or add a custom path.') }}</p></div>
              <template v-if="mcpMode">
                <div v-if="!isManual(active)" class="detail-section"><h4>After connecting</h4><p>{{ active.restart ? 'Restart this agent to load QuantMCP.' : 'Reload the agent’s MCP connections if QuantMCP does not appear.' }} A saved configuration is separate from a live connection.</p></div>
                <div v-if="isManual(active) || !active.installed" class="manual-setup"><slot name="manual-setup" /></div>
              </template>
              <div v-else class="detail-section"><h4>MCP connection</h4><p>{{ active.mcp_support === 'extension' ? 'Requires an extension. Instruction import works without MCP.' : active.custom ? 'Managed separately in your agent.' : 'Supported separately from instruction import.' }}</p></div>
              <div v-if="active.custom && !mcpMode" class="custom-actions"><button class="picker-button" :disabled="locked" @click="editCustom(active)">Edit</button><button class="text-button remove-button" :disabled="locked" title="Remove this saved target; the instruction file is kept" @click="removeCustom">Remove target</button></div>
              <p v-if="active.custom && !mcpMode" class="form-hint">Removing a target keeps its instruction file.</p>
              <p v-if="customError" class="picker-error" role="alert">{{ customError }}</p>
            </template>
            <div v-else-if="mcpMode"><h3>Any MCP client</h3><slot name="manual-setup" /></div>
            <div v-else class="picker-empty"><strong>Instructions for any agent</strong><p>Select an agent to see how its instructions are loaded, or add your own file.</p></div>
          </aside>
        </div>
        <footer class="picker-footer">
          <div><strong>{{ modelValue.length }} selected</strong><button class="text-button" :disabled="locked || !automatic.length" @click="emit('update:modelValue', automatic.map(agent => agent.id))">Select automatic</button><button class="text-button" :disabled="locked || !modelValue.length" @click="emit('update:modelValue', [])">Clear</button></div>
          <button class="picker-button primary" :disabled="writing" @click="closePicker">Done</button>
        </footer>
      </div>
    </dialog>
  </Teleport>
</template>

<style scoped>
.recipient-summary { display: flex; align-items: center; gap: 14px; flex-shrink: 0; padding: 16px 18px; margin-bottom: 16px; background: var(--bg-secondary); border: 1px solid var(--border); border-radius: 12px; color: var(--text-primary); }
.summary-mark { width: 40px; height: 40px; display: grid; place-items: center; flex-shrink: 0; border-radius: 10px; background: color-mix(in srgb, var(--accent) 12%, transparent); color: var(--accent); }
.summary-mark svg { width: 23px; height: 23px; }
.summary-copy { min-width: 0; flex: 1; }
.summary-title { display: flex; align-items: center; flex-wrap: wrap; gap: 10px; font-size: 13px; font-weight: 600; }
.summary-count { color: var(--text-muted); font-size: 11px; font-weight: 400; }
.summary-copy p { margin: 5px 0 0; color: var(--text-secondary); font-size: 12px; line-height: 1.5; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.summary-actions { display: flex; flex-wrap: wrap; gap: 8px; }
.picker-button { min-height: 32px; padding: 7px 12px; border: 1px solid var(--border); border-radius: 7px; background: var(--bg-hover); color: var(--text-primary); font: inherit; font-size: 12px; font-weight: 500; cursor: pointer; white-space: nowrap; }
.picker-button:hover:not(:disabled) { border-color: var(--accent); }
.picker-button.primary { border-color: var(--text-primary); background: var(--text-primary); color: var(--bg-primary); }
button:disabled { opacity: .4; cursor: not-allowed; }
button:focus-visible, input:focus-visible, textarea:focus-visible { outline: 2px solid var(--accent); outline-offset: 3px; }
/* Set modal geometry explicitly: the app reset removes dialog auto margins,
   and the module's page styles otherwise give this theme root a 100vh height. */
.agent-dialog { position: fixed; inset: 0; margin: auto; padding: 0; box-sizing: border-box; width: min(920px, calc(100vw - 40px)); height: min(760px, calc(100dvh - 40px)); max-width: none; max-height: calc(100dvh - 40px); overflow: hidden; border: 1px solid var(--border); border-radius: 16px; background: var(--bg-primary); color: var(--text-primary); box-shadow: 0 24px 80px #0007; font-family: inherit; }
.agent-dialog::backdrop { background: #0009; backdrop-filter: blur(4px); }
.picker-shell { display: flex; flex-direction: column; height: 100%; min-height: 0; }
.picker-shell > :not(.picker-content) { flex-shrink: 0; }
.picker-header { display: flex; align-items: flex-start; justify-content: space-between; padding: 24px 24px 18px; gap: 16px; }
.picker-eyebrow { color: var(--text-muted); font-size: 10px; font-weight: 600; letter-spacing: .12em; }
.picker-header h2 { margin: 7px 0 6px; font-size: 22px; font-weight: 600; letter-spacing: -.025em; }
.picker-header p { margin: 0; color: var(--text-secondary); font-size: 13px; line-height: 1.5; }
.icon-button { border: none; background: transparent; color: var(--text-muted); font-size: 25px; cursor: pointer; line-height: 1; padding: 3px 6px; }
.picker-toolbar { display: flex; gap: 8px; padding: 0 24px 16px; }
.picker-search { display: flex; align-items: center; min-width: 0; flex: 1; gap: 8px; padding: 0 10px; border: 1px solid var(--border); border-radius: 7px; background: var(--bg-secondary); }
.picker-search svg { width: 16px; height: 16px; flex-shrink: 0; color: var(--text-muted); }
.picker-search input { min-width: 0; width: 100%; border: 0; background: transparent; color: var(--text-primary); padding: 8px 0; font: inherit; font-size: 12px; }
.picker-search:focus-within { border-color: var(--accent); }
.picker-search input:focus { outline: none; }
.picker-filters { display: flex; gap: 18px; padding: 0 24px; border-bottom: 1px solid var(--border); }
.picker-filters button { display: flex; align-items: center; gap: 6px; padding: 0 0 12px; border: 0; border-bottom: 2px solid transparent; background: none; color: var(--text-muted); font: inherit; font-size: 12px; cursor: pointer; }
.picker-filters button.active { border-bottom-color: var(--accent); color: var(--text-primary); }
.picker-filters span { font-size: 10px; background: var(--bg-hover); border-radius: 4px; padding: 1px 5px; }
.picker-content { display: grid; grid-template-columns: minmax(0, 1.15fr) minmax(0, 1fr); flex: 1; min-height: 0; }
.agent-list { overflow-y: auto; min-height: 0; padding: 10px; }
.agent-row { display: flex; align-items: center; border: 1px solid transparent; border-radius: 9px; margin-bottom: 4px; }
.agent-row.focused { background: var(--bg-hover); }
.agent-row.chosen { background: color-mix(in srgb, var(--accent) 9%, transparent); border-color: color-mix(in srgb, var(--accent) 25%, transparent); }
.agent-row label { display: flex; align-items: center; gap: 12px; flex: 1; min-width: 0; padding: 11px 8px 11px 12px; cursor: pointer; }
.agent-row input { appearance: none; width: 16px; height: 16px; margin: 0; flex-shrink: 0; display: grid; place-items: center; border: 1px solid var(--text-muted); border-radius: 4px; background: transparent; cursor: pointer; }
.agent-row input:checked { background: var(--text-primary); border-color: var(--text-primary); }
.agent-row input:checked::after { content: ''; width: 7px; height: 4px; border-left: 2px solid var(--bg-primary); border-bottom: 2px solid var(--bg-primary); transform: translateY(-1px) rotate(-45deg); }
.agent-row input:disabled { cursor: not-allowed; opacity: .4; }
.agent-avatar { display: grid; place-items: center; width: 34px; height: 34px; flex-shrink: 0; border: 1px solid var(--border); border-radius: 9px; background: var(--bg-secondary); color: var(--text-secondary); font-size: 11px; font-weight: 600; letter-spacing: -.03em; }
.agent-avatar.pi { color: #f0ad75; font-size: 23px; }
.agent-avatar.omp { color: #8cc3ba; font-size: 10px; }
.agent-name { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
.agent-name strong { font-size: 13px; font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.agent-name > span { font-size: 11px; color: var(--text-muted); }
.agent-row.unavailable label { cursor: default; opacity: .6; }
.row-details { border: 0; background: none; color: var(--text-muted); font-size: 22px; padding: 10px 14px; cursor: pointer; }
.agent-detail { min-height: 0; padding: 22px; overflow-y: auto; border-left: 1px solid var(--border); background: var(--bg-secondary); }
.detail-avatar { width: 42px; height: 42px; margin-bottom: 14px; }
.agent-detail h3 { font-size: 17px; font-weight: 600; margin: 0 0 9px; letter-spacing: -.02em; }
.detail-status { display: inline-flex; align-items: center; gap: 6px; color: var(--text-secondary); font-size: 11px; }
.detail-status i { width: 5px; height: 5px; border-radius: 50%; background: var(--text-muted); }
.detail-status.ready i { background: #7db895; }
.detail-description, .custom-form > p { font-size: 12px; line-height: 1.7; color: var(--text-secondary); margin: 16px 0 20px; }
.detail-section { margin-top: 18px; }
.manual-setup { margin-top: 20px; }
.agent-detail .manual-heading { margin-top: 8px; }
.detail-section h4 { margin: 0 0 7px; font-size: 10px; font-weight: 500; color: var(--text-muted); text-transform: uppercase; letter-spacing: .07em; }
.detail-section p { margin: 0; font-size: 11px; color: var(--text-secondary); overflow-wrap: anywhere; line-height: 1.7; }
.detail-section code { display: block; overflow-wrap: anywhere; font-size: 11px; line-height: 1.7; color: var(--text-secondary); margin-bottom: 5px; }
.picker-empty { padding: 24px 16px; font-size: 13px; color: var(--text-secondary); line-height: 1.7; }
.picker-empty strong { color: var(--text-primary); font-weight: 500; }
.picker-empty p { margin: 6px 0 0; font-size: 12px; }
.picker-footer { display: flex; align-items: center; justify-content: space-between; gap: 12px; border-top: 1px solid var(--border); padding: 14px 24px; }
.picker-footer > div { display: flex; align-items: center; gap: 14px; flex-wrap: wrap; }
.picker-footer strong { font-size: 12px; font-weight: 500; }
.text-button { border: 0; padding: 0; background: none; color: var(--text-secondary); cursor: pointer; font: inherit; font-size: 11px; }
.text-button:hover:not(:disabled) { color: var(--text-primary); }
.picker-error { font-size: 12px; line-height: 1.5; color: var(--error, #ef7777); overflow-wrap: anywhere; margin: 0 0 12px; }
.custom-form .picker-error { color: var(--error, #ef7777); }
.picker-inline-error { margin: 12px 24px; }
.custom-form h3 { margin-top: 8px; }
.custom-form > label { display: flex; flex-direction: column; gap: 7px; margin-top: 16px; font-size: 11px; color: var(--text-secondary); }
.custom-form input, .custom-form textarea { box-sizing: border-box; width: 100%; background: var(--bg-primary); border: 1px solid var(--border); border-radius: 6px; padding: 9px; color: var(--text-primary); font: inherit; font-size: 12px; }
.custom-form textarea { resize: vertical; line-height: 1.5; }
.browse-button { margin-top: 8px; }
.custom-form .form-hint, .form-hint { color: var(--text-muted); font-size: 11px; line-height: 1.6; margin: 10px 0; }
.custom-actions { display: flex; align-items: center; gap: 10px; margin-top: 20px; }
.remove-button { color: var(--text-muted); }
@media (max-width: 640px) {
  .recipient-summary { flex-wrap: wrap; }
  .summary-actions { width: 100%; justify-content: flex-end; }
  .agent-dialog { width: calc(100vw - 24px); height: min(760px, calc(100dvh - 24px)); max-height: calc(100dvh - 24px); }
  .picker-header { padding: 18px 16px 14px; }
  .picker-toolbar { padding: 0 16px 12px; flex-wrap: wrap; }
  .picker-search { flex-basis: 100%; }
  .picker-filters { padding: 0 16px; }
  .picker-content { grid-template-columns: 1fr; grid-template-rows: minmax(0, 1fr) minmax(0, 1.2fr); }
  .agent-detail { border-left: 0; border-top: 1px solid var(--border); padding: 18px; }
  .picker-footer { padding: 12px 16px; }
  .picker-footer > div { gap: 10px; }
}
</style>
