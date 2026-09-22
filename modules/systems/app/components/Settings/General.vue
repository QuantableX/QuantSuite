<script setup lang="ts">
/**
 * QuantSystems → General — entry in the unified settings modal (V3).
 * Per-system configuration is deep and lives on each system's own settings
 * view; this section routes there rather than duplicating it.
 */
import { useSystemsStore } from '#systems/stores/systems'
import { useAppStore } from '#systems/stores/app'

const systems = useSystemsStore()
const app = useAppStore()

function openSystemSettings(id: string) {
  window.dispatchEvent(new CustomEvent('qss:navigate', { detail: { route: `/algo/manual/${id}/settings` } }))
  window.dispatchEvent(new CustomEvent('qss:settings-close'))
}
</script>

<template>
  <div>
    <p class="qsu-hint" style="margin-bottom: 6px">
      Each evaluation system carries its own configuration — universe, factors,
      rotation rules — on its Settings view.
    </p>
    <div v-for="s in systems.systems" :key="s.id" class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">{{ s.name }}</p>
        <p class="qsu-hint">{{ s.short }}{{ s.status !== 'ready' ? ' · planned' : '' }}</p>
      </div>
      <button class="qsu-btn" :disabled="s.status !== 'ready'" @click="openSystemSettings(s.id)">
        Open settings
      </button>
    </div>
    <div class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">Engine</p>
        <p class="qsu-hint">Status: {{ app.engineStatus?.status ?? 'unknown' }}</p>
      </div>
    </div>
  </div>
</template>
