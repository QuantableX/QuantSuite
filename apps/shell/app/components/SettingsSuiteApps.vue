<script setup lang="ts">
import { ref } from 'vue'
import { apps, appAvailability, isAppEnabled, loadAppAvailability, modulesForApp, setAppEnabled } from '@quantsuite/core'

const choices = apps.filter((app) => app.id !== 'dashboard')
const saving = ref<string | null>(null)
const saveError = ref<string | null>(null)

async function toggle(id: string) {
  if (saving.value) return
  saving.value = id
  saveError.value = null
  try {
    await setAppEnabled(id, !isAppEnabled(id))
  } catch (e) {
    saveError.value = String(e)
  } finally {
    saving.value = null
  }
}
</script>

<template>
  <div>
    <p class="qsu-hint qsa-intro">
      Choose the apps available in this QuantSuite installation. Deactivated apps are hidden
      and their MCP tools are removed from the tools offered to AI agents. Your data is kept.
    </p>
    <div v-for="app in choices" :key="app.id" class="qsu-row">
      <QAppIcon :app-id="app.id" :size="24" />
      <div class="qsu-row-text">
        <p :id="`app-label-${app.id}`" class="qsu-label">{{ app.title }}</p>
        <p class="qsu-hint">{{ modulesForApp(app.id).map((m) => m.title).join(', ') }}</p>
      </div>
      <button
        class="qsu-toggle"
        :class="{ 'is-on': isAppEnabled(app.id) }"
        role="switch"
        :aria-labelledby="`app-label-${app.id}`"
        :aria-checked="isAppEnabled(app.id)"
        :disabled="!appAvailability.ready.value || saving !== null"
        @click="toggle(app.id)"
      />
    </div>
    <p v-if="saveError || appAvailability.error.value" class="qsa-error" role="alert">
      {{ saveError || appAvailability.error.value }}
      <button v-if="!appAvailability.ready.value" @click="loadAppAvailability()">Try again</button>
    </p>
    <p class="qsu-hint qsa-footer">
      Changes apply immediately. Reconnect an AI client if it still shows an older tool list.
      These preferences can be changed here by anyone using this installation.
    </p>
  </div>
</template>

<style scoped>
.qsu-row-text { flex: 1; }
.qsu-row > svg { flex-shrink: 0; }
.qsu-hint { overflow-wrap: anywhere; }
.qsa-intro { margin: 0 0 12px; }
.qsa-footer { margin: 16px 0 0; }
.qsa-error { color: var(--qss-error); font-size: 12px; }
</style>
