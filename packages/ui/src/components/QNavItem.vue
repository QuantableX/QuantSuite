<script setup lang="ts">
/**
 * One row in a module's QSidebar nav (V3) — icon + label + optional count,
 * the shape the good sidebars already shared. Icons are 24×24 stroke paths
 * (the QRail language); no emoji, no glyph characters in chrome.
 */
defineProps<{
  label: string
  /** 24×24 stroke path(s), space-separated `M…` commands. */
  icon?: string
  active?: boolean
  count?: string | number
  disabled?: boolean
}>()
const emit = defineEmits<{
  (e: 'click'): void
}>()
</script>

<template>
  <button
    class="qni"
    :class="{ 'is-active': active }"
    :disabled="disabled"
    :aria-current="active ? 'page' : undefined"
    @click="emit('click')"
  >
    <svg v-if="icon" class="qni-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path :d="icon" />
    </svg>
    <span class="qni-label"><slot>{{ label }}</slot></span>
    <span v-if="count !== undefined" class="qni-count">{{ count }}</span>
    <slot name="trailing" />
  </button>
</template>

<style scoped>
.qni {
  display: flex;
  align-items: center;
  gap: 10px;
  width: calc(100% - 16px);
  margin: 3px 8px;
  padding: 9px 10px;
  min-height: 36px;
  border: 1px solid transparent;
  border-radius: 9px;
  background: transparent;
  color: var(--qss-text-secondary);
  font-family: inherit;
  font-size: 13px;
  font-weight: 500;
  text-align: left;
  cursor: pointer;
  transition: background var(--qss-dur-instant) var(--qss-ease-out), color var(--qss-dur-instant) var(--qss-ease-out);
}
.qni:hover:not(:disabled) {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}
.qni.is-active {
  background: linear-gradient(110deg, var(--qss-bg-card), color-mix(in srgb, var(--qss-bg-card) 45%, transparent));
  border-color: var(--qss-border);
  color: var(--qss-text);
}
.qni:focus-visible { outline: 2px solid var(--qss-accent); outline-offset: 2px; }
.qni:disabled {
  opacity: 0.45;
  cursor: default;
}

.qni-icon {
  flex-shrink: 0;
}

.qni-label {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.qni-count {
  padding: 1px 7px;
  border-radius: 999px;
  background: var(--qss-bg-hover);
  color: var(--qss-text-muted);
  font-size: 10.5px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}
</style>
