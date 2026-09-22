<script setup lang="ts">
const props = defineProps<{
  checked: boolean
  disabled?: boolean
  label: string
  groupOff?: boolean
}>()
const emit = defineEmits<{ change: [enabled: boolean] }>()

function change(event: Event) {
  // Keep the control on the saved value until the write succeeds (also on failure).
  const input = event.target as HTMLInputElement
  const next = input.checked
  input.checked = props.checked
  emit('change', next)
}
</script>

<template>
  <label class="tool-toggle" @click.stop>
    <span class="toggle-status">{{ groupOff ? 'Group off' : checked ? 'Enabled' : 'Disabled' }}</span>
    <input
      type="checkbox"
      role="switch"
      :aria-label="label"
      :checked="checked"
      :disabled="disabled || groupOff"
      @change="change"
    />
    <span class="toggle-track" aria-hidden="true"></span>
  </label>
</template>

<style scoped>
.tool-toggle { display: inline-flex; align-items: center; gap: 8px; position: relative; flex-shrink: 0; cursor: pointer; margin-left: auto; }
.toggle-status { font-size: 11px; color: var(--text-muted); font-weight: 400; text-transform: none; letter-spacing: normal; }
input { position: absolute; right: 0; width: 34px; height: 20px; opacity: 0; cursor: pointer; }
.toggle-track { width: 34px; height: 20px; background: var(--bg-hover); border: 1px solid var(--border); border-radius: 12px; pointer-events: none; }
.toggle-track::after { content: ''; display: block; width: 14px; height: 14px; margin: 2px; border-radius: 50%; background: var(--text-muted); transition: transform 0.15s; }
input:checked + .toggle-track { background: var(--accent); border-color: var(--accent); }
input:checked + .toggle-track::after { transform: translateX(14px); background: var(--bg-primary); }
input:focus-visible + .toggle-track { outline: 2px solid var(--accent); outline-offset: 3px; }
input:disabled + .toggle-track { opacity: 0.4; }
input:disabled { cursor: default; }
</style>
