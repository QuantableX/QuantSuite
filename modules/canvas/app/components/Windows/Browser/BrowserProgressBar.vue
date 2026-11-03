<script setup lang="ts">
// The page is loading (its own load events, not a timer). A sliding bar —
// transform only, so it never costs the main thread; a still one under
// reduced motion.
defineProps<{ loading: boolean }>()
</script>

<template>
  <div class="qcb-progress" :class="{ on: loading }" aria-hidden="true">
    <div v-if="loading" class="qcb-progress-bar" />
  </div>
</template>

<style scoped>
.qcb-progress {
  position: relative;
  height: 2px;
  flex-shrink: 0;
  overflow: hidden;
}
.qcb-progress.on {
  background: color-mix(in srgb, #f97316 18%, transparent);
}
.qcb-progress-bar {
  position: absolute;
  inset: 0;
  width: 40%;
  background: #f97316;
  animation: qcb-slide 1.1s ease-in-out infinite;
}
@keyframes qcb-slide {
  from { transform: translateX(-100%); }
  to { transform: translateX(250%); }
}
@media (prefers-reduced-motion: reduce) {
  .qcb-progress-bar {
    width: 100%;
    animation: none;
    opacity: 0.6;
  }
}
</style>
