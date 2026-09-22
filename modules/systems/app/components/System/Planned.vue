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
  gap: 14px;
  text-align: center;
  padding: 40px;
}

.qs-planned__mark {
  font-size: 52px;
  color: var(--qs-accent);
}

.qs-planned__title {
  margin: 0;
  font-size: 22px;
  font-weight: 600;
}

.qs-planned__desc {
  margin: 0;
  max-width: 520px;
  color: var(--qs-text-secondary);
}

.qs-planned__note {
  margin: 0;
  max-width: 480px;
  font-size: 13px;
  color: var(--qs-text-muted);
}
</style>
