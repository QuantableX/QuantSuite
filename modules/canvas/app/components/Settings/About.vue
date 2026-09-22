<script setup lang="ts">
// Module information; application updates belong to the suite (Settings → General).
const runtimeConfig = useRuntimeConfig()
</script>

<template>
  <div>
    <div class="cs-shortcuts">
      <div><strong>Right click</strong> — new window</div>
      <div><strong>Drag header</strong> — move window</div>
      <div><strong>Scroll</strong> — pan canvas</div>
      <div><strong>Ctrl+Scroll</strong> — zoom</div>
      <div><strong>Ctrl+B</strong> — file explorer</div>
      <div><strong>Ctrl+Shift+B</strong> — editor panel</div>
      <div><strong>Ctrl+J</strong> — notes bar</div>
    </div>

    <div class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">QuantSuite v{{ runtimeConfig.public.appVersion }}</p>
      </div>

    </div>
  </div>
</template>

<style scoped>
.cs-shortcuts {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 6px 18px;
  padding-bottom: 12px;
  margin-bottom: 4px;
  border-bottom: 1px solid var(--qss-border-subtle);
  font-size: 12px;
  color: var(--qss-text-secondary);
}
.cs-shortcuts strong {
  color: var(--qss-text);
  font-weight: 600;
}
</style>
