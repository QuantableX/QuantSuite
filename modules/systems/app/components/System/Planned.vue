<script setup lang="ts">
import type { SystemMeta } from '#systems/types'

defineProps<{ system?: SystemMeta }>()
</script>

<template>
  <div class="qs-planned">
    <span class="qs-planned__mark">◇</span>
    <h1 class="qs-planned__title">{{ system?.name ?? 'System' }}</h1>
    <span class="pill">Planned</span>
    <p class="qs-planned__desc">{{ system?.description }}</p>
    <p class="qs-planned__note">
      This system is on the roadmap. The compute engine and UI are shared with LCES, so it will slot
      into this same workspace once its slice of the market-cap ranking is wired up.
    </p>
  </div>
</template>

<style scoped>
.qs-planned {
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--qss-gap-tight);
  text-align: center;
  padding: var(--qss-page-edge);
}

.qs-planned__mark {
  font-size: 20px;
  color: var(--qs-accent);
}

.qs-planned__title {
  margin: 0;
  font-size: var(--qss-text-title);
  font-weight: 600;
}

.qs-planned__desc {
  margin: 0;
  max-width: 80ch;
  font-size: var(--qss-text-section);
  color: var(--qs-text-secondary);
}

.qs-planned__note {
  margin: 0;
  max-width: 80ch;
  font-size: var(--qss-text-label);
  color: var(--qs-text-muted);
}
</style>
