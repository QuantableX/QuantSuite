<script setup lang="ts">
/**
 * QuantNotes → General — entry in the unified settings modal (V3). QuantNotes'
 * full configuration lives on its Settings page; this section routes there.
 */
import { useNotesStore } from '#notes/stores/notes'
import { useSchemaStore } from '#notes/stores/schema'

const notes = useNotesStore()
const schema = useSchemaStore()

function openFullSettings() {
  window.dispatchEvent(new CustomEvent('qss:navigate', { detail: { route: '/notes/settings' } }))
  window.dispatchEvent(new CustomEvent('qss:settings-close'))
}
</script>

<template>
  <div>
    <div class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">Collection & editor settings</p>
        <p class="qsu-hint">
          {{ notes.list.length }} note(s) ·
          {{ schema.properties.length }} propert{{ schema.properties.length === 1 ? 'y' : 'ies' }} ·
          {{ schema.views.length }} view(s)
        </p>
      </div>
      <button class="qsu-btn qsu-btn--primary" @click="openFullSettings">Open settings</button>
    </div>
  </div>
</template>
