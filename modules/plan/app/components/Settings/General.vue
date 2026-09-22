<script setup lang="ts">
/**
 * QuantPlan → General — entry in the unified settings modal (V3). QuantPlan's
 * full configuration lives on its Settings page; this section routes there.
 */
import { useCalendarsStore } from '#plan/stores/calendars'
import { useAppStore } from '#plan/stores/app'

const calendars = useCalendarsStore()
const app = useAppStore()

function openFullSettings() {
  window.dispatchEvent(new CustomEvent('qss:navigate', { detail: { route: '/flow/settings' } }))
  window.dispatchEvent(new CustomEvent('qss:settings-close'))
}
</script>

<template>
  <div>
    <div class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">Calendars & grid settings</p>
        <p class="qsu-hint">
          {{ calendars.list.length }} calendar(s) ·
          opens on the {{ app.settings.defaultView }} view
        </p>
      </div>
      <button class="qsu-btn qsu-btn--primary" @click="openFullSettings">Open settings</button>
    </div>
  </div>
</template>
