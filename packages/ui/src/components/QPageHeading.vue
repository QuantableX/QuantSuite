<script setup lang="ts">
defineProps<{ title: string; eyebrow?: string; meta?: string }>()
</script>

<template>
  <header class="qph">
    <div class="qph-copy">
      <p v-if="eyebrow" class="qph-eyebrow">{{ eyebrow }}</p>
      <div class="qph-title-row">
        <h1>{{ title }}</h1>
        <span v-if="meta" class="qph-meta">{{ meta }}</span>
      </div>
    </div>
    <div v-if="$slots.default" class="qph-actions"><slot /></div>
  </header>
</template>

<style scoped>
.qph { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 16px; margin-bottom: 26px; }
.qph-copy { min-width: 0; }
.qph-eyebrow { margin: 0 0 8px; color: var(--qss-text-secondary); font-size: 10px; font-weight: 600; letter-spacing: .16em; text-transform: uppercase; }
.qph-title-row { display: flex; align-items: baseline; flex-wrap: wrap; gap: 12px; }
.qph h1 { margin: 0; color: var(--qss-text); font-size: clamp(22px, 2vw, 28px); line-height: 1.25; font-weight: 500; letter-spacing: -.035em; }
.qph-meta { color: var(--qss-text-secondary); font-size: 12px; font-variant-numeric: tabular-nums; }
.qph-actions { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; }
</style>
