<script setup lang="ts">
defineProps<{ title: string; description?: string; compact?: boolean }>()
</script>

<template>
  <div class="qes" :class="{ 'qes--compact': compact }">
    <div v-if="$slots.icon" class="qes-icon" aria-hidden="true"><slot name="icon" /></div>
    <h2>{{ title }}</h2>
    <p v-if="description">{{ description }}</p>
    <div v-if="$slots.default" class="qes-actions"><slot /></div>
  </div>
</template>

<style scoped>
.qes { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: var(--qss-empty-gap, var(--qss-gap-tight)); min-height: var(--qss-empty-min, 140px); padding: var(--qss-empty-pad, 24px 16px); text-align: center; }
.qes-icon { display: grid; place-items: center; margin-bottom: 4px; color: var(--qss-text-muted); }
.qes h2 { margin: 0; color: var(--qss-text); font-size: var(--qss-empty-title, 14px); font-weight: var(--qss-empty-title-weight, 600); letter-spacing: var(--qss-empty-title-tracking, -.01em); }
.qes p { max-width: 340px; margin: 0; color: var(--qss-text-secondary); font-size: 13px; line-height: 1.65; }
.qes-actions { display: flex; justify-content: center; flex-wrap: wrap; gap: 8px; margin-top: 8px; }
.qes--compact { min-height: 140px; padding: 24px 16px; }
.qes--compact h2 { font-size: 14px; }
</style>
