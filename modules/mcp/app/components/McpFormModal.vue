<script setup lang="ts">
import type { McpEntry, McpTransportInput, McpFormData } from '#mcp/composables/useMcps'

const props = defineProps<{
  modelValue: boolean
  editEntry?: McpEntry | null
}>()

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
  submit: [data: McpFormData]
}>()

const name = ref('')
const description = ref('')
const transportType = ref<'stdio' | 'http'>('stdio')
const httpPort = ref(3000)
const command = ref('')
const argsText = ref('')
const envPairs = ref<{ key: string; val: string }[]>([])
const workingDir = ref('')
const showProcess = ref(false)

const isEdit = computed(() => !!props.editEntry)

watch(() => props.modelValue, (open) => {
  if (open && props.editEntry) {
    const e = props.editEntry
    name.value = e.name
    description.value = e.description
    if (typeof e.transport === 'string') {
      transportType.value = 'stdio'
    } else {
      transportType.value = 'http'
      httpPort.value = e.transport.http.port
    }
    command.value = e.command || ''
    argsText.value = (e.args || []).join('\n')
    envPairs.value = Object.entries(e.env || {}).map(([key, val]) => ({ key, val }))
    workingDir.value = e.working_dir || ''
    showProcess.value = !!e.command
  } else if (open) {
    name.value = ''
    description.value = ''
    transportType.value = 'stdio'
    httpPort.value = 3000
    command.value = ''
    argsText.value = ''
    envPairs.value = []
    workingDir.value = ''
    showProcess.value = false
  }
})

function addEnvPair() {
  envPairs.value.push({ key: '', val: '' })
}

function removeEnvPair(index: number) {
  envPairs.value.splice(index, 1)
}

function close() {
  emit('update:modelValue', false)
}

function handleSubmit() {
  if (!name.value.trim()) return
  const transport: McpTransportInput =
    transportType.value === 'stdio' ? 'stdio' : { http: { port: httpPort.value } }

  const args = argsText.value.split('\n').map(s => s.trim()).filter(Boolean)
  const env: Record<string, string> = {}
  for (const pair of envPairs.value) {
    if (pair.key.trim()) env[pair.key.trim()] = pair.val
  }

  emit('submit', {
    name: name.value.trim(),
    description: description.value.trim(),
    transport,
    command: command.value.trim() || null,
    args,
    env,
    working_dir: workingDir.value.trim() || null,
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
        <h2 class="modal-title">{{ isEdit ? 'Edit MCP' : 'Add MCP' }}</h2>

        <form @submit.prevent="handleSubmit" class="modal-form">
          <div class="form-group">
            <label class="form-label">Name</label>
            <input v-model="name" type="text" class="form-input" placeholder="My MCP Server" autofocus />
          </div>

          <div class="form-group">
            <label class="form-label">Description</label>
            <textarea v-model="description" class="form-input form-textarea" placeholder="What does this MCP do?" rows="2" />
          </div>

          <div class="form-group">
            <label class="form-label">Transport</label>
            <div class="transport-options">
              <label class="transport-option" :class="{ active: transportType === 'stdio' }">
                <input v-model="transportType" type="radio" value="stdio" />
                <span>Stdio</span>
              </label>
              <label class="transport-option" :class="{ active: transportType === 'http' }">
                <input v-model="transportType" type="radio" value="http" />
                <span>HTTP</span>
              </label>
            </div>
          </div>

          <div v-if="transportType === 'http'" class="form-group">
            <label class="form-label">Port</label>
            <input v-model.number="httpPort" type="number" class="form-input" min="1" max="65535" placeholder="3000" />
          </div>

          <!-- Process Configuration -->
          <button type="button" class="section-toggle" @click="showProcess = !showProcess">
            {{ showProcess ? '▾' : '▸' }} Process Configuration
          </button>

          <template v-if="showProcess">
            <div class="form-group">
              <label class="form-label">Command</label>
              <input v-model="command" type="text" class="form-input" placeholder="python, node, npx..." />
            </div>

            <div class="form-group">
              <label class="form-label">Arguments <span class="form-hint">(one per line)</span></label>
              <textarea v-model="argsText" class="form-input form-textarea" placeholder="server.py&#10;--port&#10;3001" rows="3" />
            </div>

            <div class="form-group">
              <label class="form-label">Environment Variables</label>
              <div v-for="(pair, i) in envPairs" :key="i" class="env-row">
                <input v-model="pair.key" type="text" class="form-input env-key" placeholder="KEY" />
                <input v-model="pair.val" type="text" class="form-input env-val" placeholder="value" />
                <button type="button" class="btn-env-remove" @click="removeEnvPair(i)">&#10005;</button>
              </div>
              <button type="button" class="btn-env-add" @click="addEnvPair">+ Add Variable</button>
            </div>

            <div class="form-group">
              <label class="form-label">Working Directory</label>
              <input v-model="workingDir" type="text" class="form-input" placeholder="/path/to/project" />
            </div>
          </template>

          <div class="modal-actions">
            <button type="button" class="btn-cancel" @click="close">Cancel</button>
            <button type="submit" class="btn-save" :disabled="!name.trim()">
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
  max-width: 500px;
  margin: 16px;
  max-height: 85vh;
  overflow-y: auto;
}

.modal-title {
  font-size: 18px;
  font-weight: 600;
  margin-bottom: 20px;
}

.modal-form {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.form-group {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.form-label {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-secondary);
}

.form-hint {
  font-weight: 400;
  color: var(--text-muted);
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

.form-input:focus {
  border-color: var(--accent);
}

.form-input::placeholder {
  color: var(--text-muted);
}

.form-textarea {
  resize: vertical;
  min-height: 60px;
}

.transport-options {
  display: flex;
  gap: 8px;
}

.transport-option {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 10px;
  background: var(--bg-primary);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-secondary);
  font-size: 14px;
  cursor: pointer;
  transition: all var(--transition);
}

.transport-option input { display: none; }

.transport-option.active {
  border-color: var(--accent);
  color: var(--text-primary);
}

.section-toggle {
  background: none;
  border: none;
  color: var(--text-secondary);
  font-size: 13px;
  font-weight: 500;
  text-align: left;
  padding: 4px 0;
  cursor: pointer;
  transition: color var(--transition);
}

.section-toggle:hover {
  color: var(--text-primary);
}

.env-row {
  display: flex;
  gap: 6px;
  align-items: center;
}

.env-key { flex: 1; }
.env-val { flex: 2; }

.btn-env-remove {
  padding: 6px 8px;
  background: transparent;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-muted);
  font-size: 12px;
  cursor: pointer;
  flex-shrink: 0;
}

.btn-env-remove:hover {
  color: var(--error);
  border-color: var(--error);
}

.btn-env-add {
  padding: 6px 12px;
  background: transparent;
  border: 1px dashed var(--border);
  border-radius: var(--radius);
  color: var(--text-muted);
  font-size: 13px;
  cursor: pointer;
  transition: all var(--transition);
}

.btn-env-add:hover {
  color: var(--text-secondary);
  border-color: var(--text-muted);
}

.modal-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 4px;
}

.btn-cancel {
  padding: 8px 16px;
  background: transparent;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-secondary);
  font-size: 13px;
  transition: all var(--transition);
}

.btn-cancel:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

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

.btn-save:hover:not(:disabled) {
  background: var(--accent-hover);
}

.btn-save:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
</style>
