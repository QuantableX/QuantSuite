<script setup lang="ts">
/**
 * Status dot (DESIGN.md §6, shipped with the V3 dashboard): idle / working /
 * running / error. The label sits beside it so state is never colour-only.
 */
defineProps<{
  state: 'idle' | 'working' | 'running' | 'error'
  label?: string
}>()
</script>

<template>
  <span class="qsd">
    <span class="qsd-dot" :data-state="state" />
    <span v-if="label" class="qsd-label">{{ label }}</span>
  </span>
</template>

<style scoped>
.qsd {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.qsd-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex: none;
}
.qsd-dot[data-state='running'] {
  background: var(--qss-success);
  box-shadow: 0 0 8px color-mix(in srgb, var(--qss-success) 60%, transparent);
}
.qsd-dot[data-state='working'] {
  background: var(--qss-accent);
  animation: qsd-pulse 1.8s ease-in-out infinite;
}
.qsd-dot[data-state='error'] {
  background: var(--qss-error);
  box-shadow: 0 0 8px color-mix(in srgb, var(--qss-error) 60%, transparent);
}
.qsd-dot[data-state='idle'] {
  background: var(--qss-text-muted);
}

.qsd-label {
  font-size: 11px;
  color: var(--qss-text-secondary);
}

@keyframes qsd-pulse {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.35;
  }
}

@media (prefers-reduced-motion: reduce) {
  .qsd-dot[data-state='working'] {
    animation: none;
  }
}
</style>
