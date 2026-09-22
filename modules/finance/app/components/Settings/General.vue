<script setup lang="ts">
/**
 * QuantFinance → General — entry in the unified settings modal (V3). The full
 * configuration lives on the module's Settings page.
 */
import { usePlanStore } from '#finance/stores/plan'
import { useAppStore } from '#finance/stores/app'

const plan = usePlanStore()
const app = useAppStore()

function openFullSettings() {
  window.dispatchEvent(new CustomEvent('qss:navigate', { detail: { route: '/finance/settings' } }))
  window.dispatchEvent(new CustomEvent('qss:settings-close'))
}
</script>

<template>
  <div>
    <div class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">Currency &amp; data</p>
        <p class="qsu-hint">
          {{ plan.items.length }} plan line(s) · {{ app.settings.currency }}
        </p>
      </div>
      <button class="qsu-btn qsu-btn--primary" @click="openFullSettings">Open settings</button>
    </div>
  </div>
</template>
