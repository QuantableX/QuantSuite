<script setup lang="ts">
/**
 * The tracker — the year grid is the page; everything else is chrome.
 */
definePageMeta({ layout: 'flow' })
</script>

<template>
  <div class="qh-page">
    <HabitGridYearGrid />
  </div>
</template>

<style scoped>
.qh-page {
  height: 100%;
  min-height: 0;
}
</style>
