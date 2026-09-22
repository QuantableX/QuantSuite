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
.qes { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; min-height: 240px; padding: 40px 24px; text-align: center; }
.qes-icon { display: grid; place-items: center; width: 56px; height: 56px; margin-bottom: 4px; border: 1px solid var(--qss-border); border-radius: 16px; background: linear-gradient(145deg, var(--qss-bg-card), var(--qss-bg)); color: var(--qss-text-secondary); }
.qes h2 { margin: 0; color: var(--qss-text); font-size: 20px; font-weight: 500; letter-spacing: -.025em; }
.qes p { max-width: 340px; margin: 0; color: var(--qss-text-secondary); font-size: 13px; line-height: 1.65; }
.qes-actions { display: flex; justify-content: center; flex-wrap: wrap; gap: 8px; margin-top: 8px; }
.qes--compact { min-height: 140px; padding: 24px 16px; }
.qes--compact h2 { font-size: 14px; }
</style>
