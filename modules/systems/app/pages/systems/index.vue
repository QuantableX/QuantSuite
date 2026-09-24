<script setup lang="ts">
definePageMeta({ layout: 'systems', path: '/algo/manual' })

import { useSystemsStore } from '#systems/stores/systems'
const systems = useSystemsStore()
</script>

<template>
  <div class="qs-redirect">
    <template v-if="systems.loaded && !systems.systems.length">
      <h1>Create your first strategy</h1>
      <p>Choose your own name, universe, signals and rotation settings.</p>
      <button class="btn btn-primary" @click="navigateTo('/algo/manual/new')">New strategy</button>
    </template>
    <p v-else>Loading strategy…</p>
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
