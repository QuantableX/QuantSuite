<script setup lang="ts">
/**
 * QuantAlgo → General — entry in the unified settings modal (V3). The
 * module's full configuration (trading defaults, risk, Python runtime,
 * notifications) lives on its Settings page; this section routes there.
 * The old sidebar-footer modal was a duplicate of that page and is gone.
 */
import { useAppStore } from '#algo/stores/app'

const app = useAppStore()

function openFullSettings() {
  window.dispatchEvent(new CustomEvent('qss:navigate', { detail: { route: '/algo/settings' } }))
  window.dispatchEvent(new CustomEvent('qss:settings-close'))
}
</script>

<template>
  <div>
    <div class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">Trading & runtime settings</p>
        <p class="qsu-hint">
          Default pair {{ app.settings.default_pair }} · {{ app.settings.default_timeframe }} ·
          risk {{ app.settings.risk_per_trade }}% per trade
        </p>
      </div>
      <button class="qsu-btn qsu-btn--primary" @click="openFullSettings">Open settings</button>
    </div>
  </div>
</template>
