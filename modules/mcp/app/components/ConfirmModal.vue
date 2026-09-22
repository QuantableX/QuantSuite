<script setup lang="ts">
const props = defineProps<{
  modelValue: boolean
  title?: string
  message: string
  submessage?: string
  confirmLabel?: string
  cancelLabel?: string
  danger?: boolean
}>()

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
  confirm: []
  cancel: []
}>()

function close() {
  emit('update:modelValue', false)
  emit('cancel')
}

function handleConfirm() {
  emit('update:modelValue', false)
  emit('confirm')
}
</script>

<template>
  <Teleport to="body">
    <Transition name="confirm-fade">
      <!-- data-module: teleported out of the module root, the overlay must
           carry the scope itself or it loses the theme-bridge palette. -->
      <div v-if="modelValue" class="confirm-overlay" data-module="mcp" @click.self="close">
        <Transition name="confirm-scale" appear>
          <div v-if="modelValue" class="confirm-card">
            <div class="confirm-header">
              <h3 class="confirm-title">{{ title || 'Confirm' }}</h3>
            </div>
            <div class="confirm-body">
              <p class="confirm-message">{{ message }}</p>
              <p v-if="submessage" class="confirm-submessage">{{ submessage }}</p>
            </div>
            <div class="confirm-actions">
              <button class="btn-cancel" @click="close">
                {{ cancelLabel || 'Cancel' }}
              </button>
              <button
                :class="['btn-confirm', { 'btn-danger': danger }]"
                @click="handleConfirm"
              >
                {{ confirmLabel || 'Confirm' }}
              </button>
            </div>
          </div>
        </Transition>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.confirm-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.6);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1100;
}

.confirm-card {
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 24px;
  width: 100%;
  max-width: 380px;
  margin: 16px;
}

.confirm-header {
  margin-bottom: 12px;
}

.confirm-title {
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary);
  margin: 0;
}

.confirm-body {
  margin-bottom: 20px;
}

.confirm-message {
  font-size: 14px;
  color: var(--text-secondary);
  margin: 0;
  line-height: 1.5;
}

.confirm-submessage {
  font-size: 13px;
  color: var(--text-muted);
  margin: 6px 0 0;
  line-height: 1.5;
}

.confirm-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}

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

.btn-cancel:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.btn-confirm {
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

.btn-confirm:hover {
  background: var(--accent-hover);
}

.btn-danger {
  background: rgba(239, 68, 68, 0.15);
  color: var(--error);
  border: 1px solid rgba(239, 68, 68, 0.3);
}

.btn-danger:hover {
  background: rgba(239, 68, 68, 0.25);
  border-color: rgba(239, 68, 68, 0.5);
}

/* Overlay fade */
.confirm-fade-enter-active,
.confirm-fade-leave-active {
  transition: opacity 150ms ease;
}

.confirm-fade-enter-from,
.confirm-fade-leave-to {
  opacity: 0;
}

/* Card scale */
.confirm-scale-enter-active {
  transition: all 150ms ease;
}

.confirm-scale-leave-active {
  transition: all 100ms ease;
}

.confirm-scale-enter-from {
  opacity: 0;
  transform: scale(0.95);
}

.confirm-scale-leave-to {
  opacity: 0;
  transform: scale(0.95);
}
</style>
