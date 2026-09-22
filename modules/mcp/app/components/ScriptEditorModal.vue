<script setup lang="ts">
const props = defineProps<{
  modelValue: boolean
  scriptPath: string
  toolName: string
}>()

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
}>()

const content = ref('')
const original = ref('')
const saving = ref(false)
const saveStatus = ref<'idle' | 'saved' | 'error'>('idle')
const errorMsg = ref('')

const hasChanges = computed(() => content.value !== original.value)

watch(() => props.modelValue, async (open) => {
  if (open && props.scriptPath) {
    content.value = ''
    original.value = ''
    saveStatus.value = 'idle'
    errorMsg.value = ''
    await loadFile()
  }
})

async function loadFile() {
  try {
    if (window.__TAURI_INTERNALS__) {
      const { invoke } = await import('@tauri-apps/api/core')
      const text = await invoke<string>('plugin:mcp|read_file', { path: props.scriptPath })
      content.value = text
      original.value = text
    }
  } catch (e: any) {
    errorMsg.value = e?.toString() || 'Failed to load file'
  }
}

async function save() {
  saving.value = true
  saveStatus.value = 'idle'
  try {
    if (window.__TAURI_INTERNALS__) {
      const { invoke } = await import('@tauri-apps/api/core')
      await invoke('plugin:mcp|write_file', { path: props.scriptPath, content: content.value })
      original.value = content.value
      saveStatus.value = 'saved'
      setTimeout(() => { saveStatus.value = 'idle' }, 2000)
    }
  } catch (e: any) {
    saveStatus.value = 'error'
    errorMsg.value = e?.toString() || 'Failed to save file'
  } finally {
    saving.value = false
  }
}

function close() {
  emit('update:modelValue', false)
}
</script>

<template>
  <Teleport to="body">
    <!-- data-module: teleported out of the module root, the overlay must
         carry the scope itself or it loses the theme-bridge palette. -->
    <div v-if="modelValue" class="modal-overlay" data-module="mcp" @click.self="close">
      <div class="modal-card">
        <div class="modal-header">
          <div>
            <h2 class="modal-title">Edit Script</h2>
            <p class="modal-subtitle">{{ toolName }} — <code class="path-code">{{ scriptPath }}</code></p>
          </div>
          <button class="btn-close" @click="close">&times;</button>
        </div>

        <div v-if="errorMsg && !content" class="error-msg">{{ errorMsg }}</div>

        <textarea
          v-model="content"
          class="script-editor"
          spellcheck="false"
          placeholder="Script content..."
        />

        <div class="modal-footer">
          <span v-if="saveStatus === 'saved'" class="save-status saved">Saved</span>
          <span v-else-if="saveStatus === 'error'" class="save-status error">{{ errorMsg }}</span>
          <div class="footer-actions">
            <button class="btn-cancel" @click="close">Close</button>
            <button
              class="btn-save"
              @click="save"
              :disabled="!hasChanges || saving"
            >
              {{ saving ? 'Saving...' : 'Save' }}
            </button>
          </div>
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
  padding: 24px;
  width: 100%;
  max-width: 720px;
  margin: 16px;
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-height: 90vh;
}

.modal-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}

.modal-title { font-size: 18px; font-weight: 600; }
.modal-subtitle { font-size: 12px; color: var(--text-muted); margin-top: 4px; }

.path-code {
  font-family: monospace;
  background: var(--bg-hover);
  padding: 1px 5px;
  border-radius: 4px;
  color: var(--accent);
  font-size: 11px;
}

.btn-close {
  background: transparent;
  border: none;
  color: var(--text-muted);
  font-size: 16px;
  cursor: pointer;
  padding: 4px;
  flex-shrink: 0;
}

.btn-close:hover { color: var(--text-primary); }

.error-msg {
  padding: 10px 14px;
  background: rgba(255, 80, 80, 0.1);
  border: 1px solid var(--error);
  border-radius: var(--radius);
  font-size: 13px;
  color: var(--error);
}

.script-editor {
  flex: 1;
  min-height: 320px;
  padding: 14px 16px;
  background: var(--bg-primary);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-primary);
  font-size: 13px;
  font-family: 'Consolas', 'Monaco', monospace;
  line-height: 1.6;
  resize: vertical;
  outline: none;
  transition: border-color var(--transition);
  tab-size: 2;
}

.script-editor:focus { border-color: var(--accent); }

.modal-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.save-status { font-size: 13px; }
.save-status.saved { color: #2ed573; }
.save-status.error { color: var(--error); }

.footer-actions { display: flex; gap: 8px; margin-left: auto; }

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
