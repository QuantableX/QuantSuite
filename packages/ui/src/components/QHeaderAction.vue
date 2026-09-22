<script setup lang="ts">
/**
 * A small round action beside the header's search field — back/forward, and the
 * toggle for whatever strip the module keeps at its bottom edge.
 *
 * 28px, rounded, quiet until hovered. Sits in `QFileSearch`'s `left`/`right`
 * slots, which position it outside the field so the field stays centred in the
 * header no matter how many of these there are.
 */
withDefaults(
  defineProps<{
    active?: boolean
    disabled?: boolean
    label?: string
  }>(),
  { active: false, disabled: false, label: undefined }
)

defineEmits<{ (e: 'click'): void }>()
</script>

<template>
  <button
    class="qha"
    :class="{ 'is-active': active, 'is-disabled': disabled }"
    :disabled="disabled"
    :title="label"
    :aria-label="label"
    style="-webkit-app-region: no-drag"
    @click="$emit('click')"
  >
    <slot />
  </button>
</template>

<style scoped>
.qha {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  flex-shrink: 0;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text-muted);
  cursor: pointer;
  transition: color 120ms ease, background-color 120ms ease, opacity 120ms ease;
}

.qha:hover:not(:disabled) {
  color: var(--qss-text);
  background: color-mix(in srgb, var(--qss-text) 8%, transparent);
}

.qha:active:not(:disabled) {
  opacity: 0.5;
}

.qha.is-active {
  color: var(--qss-text);
}

.qha.is-disabled {
  opacity: 0.25;
  cursor: default;
}
</style>
