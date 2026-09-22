<script setup lang="ts">
import { useTools } from '#mcp/composables/useTools'
import type { ToolDef } from '#mcp/composables/useTools'

const props = defineProps<{
  modelValue: boolean
  tool?: ToolDef | null
}>()

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
}>()

const { testTool } = useTools()
const inputs = ref<Record<string, any>>({})
const output = ref('')
const running = ref(false)
const hasRun = ref(false)

const properties = computed(() => {
  if (!props.tool?.input_schema?.properties) return {}
  return props.tool.input_schema.properties as Record<string, { type: string; description?: string }>
})

watch(() => props.modelValue, (open) => {
  if (open) {
    inputs.value = {}
    output.value = ''
    hasRun.value = false
    // Set defaults
    for (const [key, schema] of Object.entries(properties.value)) {
      if (schema.type === 'boolean') inputs.value[key] = false
      else if (schema.type === 'number') inputs.value[key] = 0
      else inputs.value[key] = ''
    }
  }
})

function close() {
  emit('update:modelValue', false)
}

async function run() {
  if (!props.tool) return
  running.value = true
  hasRun.value = true
  try {
    output.value = await testTool(props.tool.id, inputs.value)
  } catch (e: any) {
    output.value = `Error: ${e.message || e}`
  } finally {
    running.value = false
  }
}
</script>

<template>
  <Teleport to="body">
    <!-- data-module: teleported out of the module root, the overlay must
         carry the scope itself or it loses the theme-bridge palette. -->
    <div v-if="modelValue && tool" class="modal-overlay" data-module="mcp" @click.self="close">
      <div class="modal-card">
        <h2 class="modal-title">Test: {{ tool.name }}</h2>

        <div class="test-form">
          <div v-for="(schema, key) in properties" :key="key" class="form-group">
            <label class="form-label">{{ key }} <span class="form-hint">({{ schema.type }})</span></label>
            <input
              v-if="schema.type === 'string'"
              v-model="inputs[key]"
              type="text"
              class="form-input"
              :placeholder="schema.description || ''"
            />
            <input
              v-else-if="schema.type === 'number'"
              v-model.number="inputs[key]"
              type="number"
              class="form-input"
            />
            <label v-else-if="schema.type === 'boolean'" class="checkbox-label">
              <input type="checkbox" v-model="inputs[key]" />
              <span>{{ schema.description || key }}</span>
            </label>
            <textarea
              v-else
              v-model="inputs[key]"
              class="form-input form-textarea"
              :placeholder="'JSON ' + schema.type"
              rows="2"
            />
          </div>

          <div v-if="Object.keys(properties).length === 0" class="empty-params">
            No input parameters. Tool will run with empty input.
          </div>

          <button class="btn-run" @click="run" :disabled="running">
            {{ running ? 'Running...' : '▶ Run' }}
          </button>

          <div v-if="hasRun" class="output-section">
            <label class="form-label">Output</label>
            <pre class="output-block">{{ output || '(empty)' }}</pre>
          </div>
        </div>

        <div class="modal-actions">
          <button class="btn-cancel" @click="close">Close</button>
        </div>
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

.modal-title { font-size: 18px; font-weight: 600; margin-bottom: 20px; }
.test-form { display: flex; flex-direction: column; gap: 14px; }
.form-group { display: flex; flex-direction: column; gap: 6px; }
.form-label { font-size: 13px; font-weight: 500; color: var(--text-secondary); }
.form-hint { font-weight: 400; color: var(--text-muted); }

.form-input {
  padding: 10px 12px;
  background: var(--bg-primary);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-primary);
  font-size: 14px;
  font-family: inherit;
  outline: none;
}

.form-input:focus { border-color: var(--accent); }
.form-textarea { resize: vertical; min-height: 50px; }

.checkbox-label {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 14px;
  color: var(--text-primary);
  cursor: pointer;
}

.empty-params {
  font-size: 13px;
  color: var(--text-muted);
  padding: 8px 0;
}

.btn-run {
  padding: 10px 20px;
  background: var(--accent);
  border: none;
  border-radius: var(--radius);
  color: var(--bg-primary);
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
  transition: all var(--transition);
}

.btn-run:hover:not(:disabled) { background: var(--accent-hover); }
.btn-run:disabled { opacity: 0.5; cursor: not-allowed; }

.output-section { margin-top: 4px; }

.output-block {
  padding: 12px;
  background: var(--bg-primary);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  font-family: monospace;
  font-size: 13px;
  color: var(--text-primary);
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 200px;
  overflow-y: auto;
}

.modal-actions { display: flex; justify-content: flex-end; margin-top: 16px; }

.btn-cancel {
  padding: 8px 16px;
  background: transparent;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-secondary);
  font-size: 13px;
  cursor: pointer;
  transition: all var(--transition);
}

.btn-cancel:hover { background: var(--bg-hover); color: var(--text-primary); }
</style>
