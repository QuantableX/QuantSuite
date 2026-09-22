<script setup lang="ts">
import { useScripts } from '#mcp/composables/useScripts'
import type { ToolDef, ToolFormData } from '#mcp/composables/useTools'

interface ParamRow {
  name: string
  type: string
  description: string
  required: boolean
}

const props = defineProps<{
  modelValue: boolean
  editTool?: ToolDef | null
}>()

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
  submit: [data: ToolFormData]
}>()

const { scripts, refresh: refreshScripts } = useScripts()

const name = ref('')
const description = ref('')
const scriptPath = ref('')
const interpreter = ref('')
// '' is the empty box: the crate's default applies, nothing is stored.
const timeoutSecs = ref<number | ''>('')
const params = ref<ParamRow[]>([])
const scriptMode = ref<'external' | 'internal'>('external')
const selectedScriptId = ref('')

const isEdit = computed(() => !!props.editTool)

watch(() => props.modelValue, async (open) => {
  if (open) {
    await refreshScripts()
    if (props.editTool) {
      const t = props.editTool
      name.value = t.name
      description.value = t.description
      scriptPath.value = t.script_path
      interpreter.value = t.interpreter || ''
      timeoutSecs.value = t.timeout_secs ?? ''
      scriptMode.value = 'external'
      selectedScriptId.value = ''
      // Parse input_schema back to param rows
      const schema = t.input_schema || {}
      const properties = schema.properties || {}
      const required = schema.required || []
      params.value = Object.entries(properties).map(([key, val]: [string, any]) => ({
        name: key,
        type: val.type || 'string',
        description: val.description || '',
        required: required.includes(key),
      }))
    } else {
      name.value = ''
      description.value = ''
      scriptPath.value = ''
      interpreter.value = ''
      timeoutSecs.value = ''
      params.value = []
      scriptMode.value = 'external'
      selectedScriptId.value = ''
    }
  }
})

watch(selectedScriptId, async (id) => {
  if (scriptMode.value === 'internal' && id) {
    const script = scripts.value.find(s => s.id === id)
    if (script) {
      // Auto-set interpreter from language
      const langInterp: Record<string, string> = {
        Python: 'python',
        Javascript: 'node',
        Bash: 'bash',
        Powershell: 'powershell',
      }
      interpreter.value = langInterp[script.language] || ''
      // Resolve path via backend
      try {
        if (window.__TAURI_INTERNALS__) {
          const { invoke } = await import('@tauri-apps/api/core')
          scriptPath.value = await invoke<string>('plugin:mcp|get_script_path', { id })
        }
      } catch (e) {
        console.error('Failed to get script path:', e)
      }
    }
  }
})

watch(scriptMode, (mode) => {
  if (mode === 'external') {
    selectedScriptId.value = ''
  }
})

function addParam() {
  params.value.push({ name: '', type: 'string', description: '', required: false })
}

function removeParam(index: number) {
  params.value.splice(index, 1)
}

function buildSchema(): Record<string, any> {
  const properties: Record<string, any> = {}
  const required: string[] = []
  for (const p of params.value) {
    if (!p.name.trim()) continue
    properties[p.name.trim()] = { type: p.type, description: p.description }
    if (p.required) required.push(p.name.trim())
  }
  return { type: 'object', properties, required }
}

function close() {
  emit('update:modelValue', false)
}

function handleSubmit() {
  if (!name.value.trim() || !scriptPath.value.trim()) return
  // Whole seconds or nothing; the crate clamps whatever it is sent.
  const secs = Math.round(Number(timeoutSecs.value))
  emit('submit', {
    name: name.value.trim(),
    description: description.value.trim(),
    script_path: scriptPath.value.trim(),
    interpreter: interpreter.value.trim() || null,
    timeout_secs: secs > 0 ? secs : null,
    input_schema: buildSchema(),
  })
  emit('update:modelValue', false)
}
</script>

<template>
  <Teleport to="body">
    <!-- data-module: teleported out of the module root, the overlay must
         carry the scope itself or it loses the theme-bridge palette. -->
    <div v-if="modelValue" class="modal-overlay" data-module="mcp" @click.self="close">
      <div class="modal-card">
        <h2 class="modal-title">{{ isEdit ? 'Edit Tool' : 'Add Tool' }}</h2>

        <form @submit.prevent="handleSubmit" class="modal-form">
          <div class="form-group">
            <label class="form-label">Name</label>
            <input v-model="name" type="text" class="form-input" placeholder="get_weather" autofocus />
          </div>

          <div class="form-group">
            <label class="form-label">Description</label>
            <textarea v-model="description" class="form-input form-textarea" placeholder="What does this tool do?" rows="2" />
          </div>

          <div class="form-group">
            <label class="form-label">Script Source</label>
            <div class="transport-toggle">
              <button
                type="button"
                class="toggle-btn"
                :class="{ active: scriptMode === 'external' }"
                @click="scriptMode = 'external'"
              >External File</button>
              <button
                type="button"
                class="toggle-btn"
                :class="{ active: scriptMode === 'internal' }"
                @click="scriptMode = 'internal'"
              >Internal Script</button>
            </div>
          </div>

          <div v-if="scriptMode === 'external'" class="form-group">
            <label class="form-label">Script Path</label>
            <input v-model="scriptPath" type="text" class="form-input" placeholder="/path/to/script.py" />
          </div>

          <div v-else class="form-group">
            <label class="form-label">Internal Script</label>
            <select v-model="selectedScriptId" class="form-input">
              <option value="" disabled>Select a script...</option>
              <option v-for="s in scripts" :key="s.id" :value="s.id">
                {{ s.name }} ({{ s.language }})
              </option>
            </select>
            <span v-if="scriptPath && selectedScriptId" class="form-hint-path">→ {{ scriptPath }}</span>
          </div>

          <div class="form-group">
            <label class="form-label">Interpreter <span class="form-hint">(auto-detect if empty)</span></label>
            <input v-model="interpreter" type="text" class="form-input" placeholder="python, node, bash..." />
          </div>

          <div class="form-group">
            <label class="form-label">Timeout <span class="form-hint">(seconds — default 120, max 600)</span></label>
            <input v-model.number="timeoutSecs" type="number" min="1" max="600" step="1" class="form-input" placeholder="120" />
          </div>

          <div class="form-group">
            <label class="form-label">Input Parameters</label>
            <div v-for="(param, i) in params" :key="i" class="param-row">
              <input v-model="param.name" type="text" class="form-input param-name" placeholder="name" />
              <select v-model="param.type" class="form-input param-type">
                <option value="string">string</option>
                <option value="number">number</option>
                <option value="boolean">boolean</option>
                <option value="object">object</option>
                <option value="array">array</option>
              </select>
              <input v-model="param.description" type="text" class="form-input param-desc" placeholder="description" />
              <label class="param-required" title="Required">
                <input type="checkbox" v-model="param.required" />
                <span>*</span>
              </label>
              <button type="button" class="btn-param-remove" @click="removeParam(i)">&#10005;</button>
            </div>
            <button type="button" class="btn-param-add" @click="addParam">+ Add Parameter</button>
          </div>

          <div class="modal-actions">
            <button type="button" class="btn-cancel" @click="close">Cancel</button>
            <button type="submit" class="btn-save" :disabled="!name.trim() || !scriptPath.trim()">
              {{ isEdit ? 'Save' : 'Add' }}
            </button>
          </div>
        </form>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.modal-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.6);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}

.modal-card {
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 28px;
  width: 100%;
  max-width: 560px;
  margin: 16px;
  max-height: 85vh;
  overflow-y: auto;
}

.modal-title { font-size: 18px; font-weight: 600; margin-bottom: 20px; }
.modal-form { display: flex; flex-direction: column; gap: 14px; }
.form-group { display: flex; flex-direction: column; gap: 6px; }
.form-label { font-size: 13px; font-weight: 500; color: var(--text-secondary); }
.form-hint { font-weight: 400; color: var(--text-muted); }

.form-hint-path {
  font-size: 11px;
  color: var(--text-muted);
  font-family: monospace;
  margin-top: 2px;
}

.form-input {
  padding: 10px 12px;
  background: var(--bg-primary);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-primary);
  font-size: 14px;
  font-family: inherit;
  outline: none;
  transition: border-color var(--transition);
}

.form-input:focus { border-color: var(--accent); }
.form-input::placeholder { color: var(--text-muted); }
.form-textarea { resize: vertical; min-height: 60px; }

.transport-toggle {
  display: flex;
  gap: 0;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  overflow: hidden;
  width: fit-content;
}

.toggle-btn {
  padding: 8px 16px;
  background: transparent;
  border: none;
  color: var(--text-secondary);
  font-size: 13px;
  cursor: pointer;
  transition: all var(--transition);
}

.toggle-btn + .toggle-btn { border-left: 1px solid var(--border); }
.toggle-btn.active { background: var(--accent); color: var(--bg-primary); font-weight: 600; }
.toggle-btn:not(.active):hover { background: var(--bg-hover); color: var(--text-primary); }

.param-row {
  display: flex;
  gap: 6px;
  align-items: center;
}

.param-name { flex: 2; }
.param-type { flex: 1; min-width: 0; }
.param-desc { flex: 3; }

.param-required {
  display: flex;
  align-items: center;
  gap: 2px;
  color: var(--text-muted);
  font-size: 14px;
  cursor: pointer;
  flex-shrink: 0;
}

.param-required input { display: none; }
.param-required:has(input:checked) { color: var(--error); }

.btn-param-remove {
  padding: 6px 8px;
  background: transparent;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-muted);
  font-size: 12px;
  cursor: pointer;
  flex-shrink: 0;
}

.btn-param-remove:hover { color: var(--error); border-color: var(--error); }

.btn-param-add {
  padding: 6px 12px;
  background: transparent;
  border: 1px dashed var(--border);
  border-radius: var(--radius);
  color: var(--text-muted);
  font-size: 13px;
  cursor: pointer;
  transition: all var(--transition);
}

.btn-param-add:hover { color: var(--text-secondary); border-color: var(--text-muted); }

.modal-actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 4px; }

.btn-cancel {
  padding: 8px 16px;
  background: transparent;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-secondary);
  font-size: 13px;
  transition: all var(--transition);
}

.btn-cancel:hover { background: var(--bg-hover); color: var(--text-primary); }

.btn-save {
  padding: 8px 20px;
  background: var(--accent);
  border: none;
  border-radius: var(--radius);
  color: var(--bg-primary);
  font-size: 13px;
  font-weight: 600;
  transition: all var(--transition);
}

.btn-save:hover:not(:disabled) { background: var(--accent-hover); }
.btn-save:disabled { opacity: 0.4; cursor: not-allowed; }
</style>
