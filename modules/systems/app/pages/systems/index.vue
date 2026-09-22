<script setup lang="ts">
definePageMeta({ layout: 'systems', path: '/algo/manual' })

import { useAppStore } from '#systems/stores/app'

const app = useAppStore()

onMounted(() => {
  navigateTo(`/algo/manual/${app.activeSystemId || 'lces'}/live`, { replace: true })
})
</script>

<template>
  <div class="qs-redirect">
    <span class="qs-redirect__mark">⌖</span>
    <p>Loading QuantAlgo…</p>
  </div>
</template>

<style scoped>
.qs-redirect {
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  color: var(--qs-text-muted);
}

.qs-redirect__mark {
  font-size: 40px;
  color: var(--qs-accent);
}
</style>
