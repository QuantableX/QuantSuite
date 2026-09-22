<script setup lang="ts">
defineProps<{ progress: number }>()
</script>

<template>
  <div v-if="progress > 0" class="w-full h-0.5 flex-shrink-0" :style="{ background: 'var(--qc-border)' }">
    <div class="h-full transition-all duration-300 ease-out" :style="{ width: progress + '%', background: '#f97316' }" />
  </div>
</template>
