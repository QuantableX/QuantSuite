<script setup lang="ts">
import type { ScriptEntry, ScriptFormData, ScriptLanguage } from '#mcp/composables/useScripts'

const props = defineProps<{
  modelValue: boolean
  editScript?: ScriptEntry | null
}>()

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
  submit: [data: ScriptFormData]
}>()

const name = ref('')
const language = ref<ScriptLanguage>('Python')
const content = ref('')

const isEdit = computed(() => !!props.editScript)

const LANG_DEFAULTS: Record<ScriptLanguage, string> = {
  Python: `import json, sys\n\ndata = json.loads(sys.stdin.read())\n\n# Your code here\nprint("Hello from Python!")\n`,
  Javascript: `const data = JSON.parse(require('fs').readFileSync('/dev/stdin', 'utf8'));\n\n// Your code here\nconsole.log('Hello from JavaScript!');\n`,
  Bash: `#!/bin/bash\n\nINPUT=$(cat)\n\n# Your code here\necho "Hello from Bash!"\n`,
  Powershell: `$input_data = $input | ConvertFrom-Json\n\n# Your code here\nWrite-Output "Hello from PowerShell!"\n`,
}

watch(() => props.modelValue, (open) => {
  if (open && props.editScript) {
    const s = props.editScript
    name.value = s.name
    language.value = s.language
    content.value = s.content
  } else if (open) {
    name.value = ''
    language.value = 'Python'
    content.value = LANG_DEFAULTS['Python']
  }
})

watch(language, (lang) => {
  // Only set default if content hasn't been customized
  if (!isEdit.value && (!content.value.trim() || Object.values(LANG_DEFAULTS).includes(content.value))) {
    content.value = LANG_DEFAULTS[lang]
  }
})

function close() {
  emit('update:modelValue', false)
}

function handleSubmit() {
  if (!name.value.trim()) return
  emit('submit', {
    name: name.value.trim(),
    language: language.value,
    content: content.value,
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
        <h2 class="modal-title">{{ isEdit ? 'Edit Script' : 'New Script' }}</h2>

        <form @submit.prevent="handleSubmit" class="modal-form">
          <div class="form-row">
            <div class="form-group form-group-grow">
              <label class="form-label">Name</label>
              <input v-model="name" type="text" class="form-input" placeholder="my_script" autofocus />
            </div>
            <div class="form-group">
              <label class="form-label">Language</label>
              <select v-model="language" class="form-input form-select">
                <option value="Python">Python</option>
                <option value="Javascript">JavaScript</option>
                <option value="Bash">Bash</option>
                <option value="Powershell">PowerShell</option>
              </select>
            </div>
          </div>

          <div class="form-group">
            <label class="form-label">Content</label>
            <textarea
              v-model="content"
              class="form-input script-textarea"
              spellcheck="false"
              placeholder="Script content..."
            />
          </div>

          <div class="modal-actions">
            <button type="button" class="btn-cancel" @click="close">Cancel</button>
            <button type="submit" class="btn-save" :disabled="!name.trim()">
              {{ isEdit ? 'Save' : 'Create' }}
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
  max-width: 680px;
  margin: 16px;
  max-height: 90vh;
  display: flex;
  flex-direction: column;
  gap: 0;
}

.modal-title { font-size: 18px; font-weight: 600; margin-bottom: 20px; }
.modal-form { display: flex; flex-direction: column; gap: 14px; flex: 1; min-height: 0; }

.form-row { display: flex; gap: 12px; align-items: flex-end; }
.form-group { display: flex; flex-direction: column; gap: 6px; }
.form-group-grow { flex: 1; }

.form-label { font-size: 13px; font-weight: 500; color: var(--text-secondary); }

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
.form-select { min-width: 130px; }

.script-textarea {
  min-height: 280px;
  font-family: 'Consolas', 'Monaco', monospace;
  font-size: 13px;
  line-height: 1.6;
  resize: vertical;
  tab-size: 2;
}

.modal-actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 4px; }

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

.btn-save {
  padding: 8px 20px;
  background: var(--accent);
  border: none;
  border-radius: var(--radius);
  color: var(--bg-primary);
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: all var(--transition);
}

.btn-save:hover:not(:disabled) { background: var(--accent-hover); }
.btn-save:disabled { opacity: 0.4; cursor: not-allowed; }
</style>
