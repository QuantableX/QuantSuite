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
  font-size: 9.5px;
  color: var(--qss-text-muted);
  text-transform: uppercase;
  letter-spacing: 0.07em;
}

.qmt-v {
  margin-top: 2px;
  font: 500 13.5px/18px var(--qss-font-mono);
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
