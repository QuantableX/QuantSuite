<script setup lang="ts">
/**
 * Label + tabular value + optional delta tone (DESIGN.md §6, shipped with
 * the V3 dashboard). Numbers are tabular so live updates never jitter.
 */
defineProps<{
  label: string
  value: string | number
  tone?: 'up' | 'down' | 'neutral'
}>()
</script>

<template>
  <div class="qmt">
    <span class="qmt-k">{{ label }}</span>
    <span class="qmt-v" :class="tone">{{ value }}</span>
  </div>
</template>

<style scoped>
.qmt {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.qmt-k {
  font-size: var(--qss-text-meta);
  color: var(--qss-text-muted);
}

.qmt-v {
  font-size: var(--qss-text-title);
  font-weight: 600;
  line-height: 1.3;
  letter-spacing: -.01em;
  font-variant-numeric: tabular-nums;
  color: var(--qss-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.qmt-v.up {
  color: var(--qss-success);
}
.qmt-v.down {
  color: var(--qss-error);
}
</style>
